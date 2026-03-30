// src/main.rs — Entry point: spawns async tasks, runs TUI on main thread
//
// Architecture:
//   main thread  → TUI render loop (blocking, ~30fps)
//   task 1       → Binance WebSocket BTC price feed
//   task 2       → Coinbase WebSocket BTC price feed
//   task 3       → CoinGecko REST fallback (every 10s)
//   task 4       → Polymarket market poll loop (every poll_interval_ms)
//   task 5       → Arbitrage scan + execution loop (every scan_interval_ms)
//   task 6       → Equity snapshot ticker (every 1s)

mod api;
mod config;
mod dashboard;
mod journal;
mod positions;
mod risk;
mod state;
mod strategies;

use crate::api::polymarket::PolymarketClient;
use crate::config::Config;
use crate::journal::logger::{TradeJournal, TradeRecord};
use crate::positions::PositionTracker;
use crate::risk::manager::RiskManager;
use crate::state::{AppState, LogLevel};
use crate::strategies::arbitrage;
use std::str::FromStr;
use std::sync::atomic::Ordering;
use std::sync::Arc;

fn main() {
    // ── Load config ──────────────────────────────────────────────────────
    let config = match Config::load() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("❌ Config error: {e}");
            eprintln!("   Copy .env.example → .env and fill in your keys.");
            std::process::exit(1);
        }
    };

    let dry_run = config.dry_run;
    let initial_balance = config.initial_balance;
    let poll_interval = std::time::Duration::from_millis(config.poll_interval_ms);
    let scan_interval = std::time::Duration::from_millis(config.scan_interval_ms);
    let min_divergence = config.min_divergence;
    let binance_url = config.binance_ws_url.clone();
    let coinbase_url = config.coinbase_ws_url.clone();
    let coingecko_url = config.coingecko_url.clone();

    // ── Create shared state ──────────────────────────────────────────────
    let state = AppState::new(initial_balance);

    state.push_log(LogLevel::Info, "═══════════════════════════════════════════");
    state.push_log(LogLevel::Info, " ⚡ Polymarket BTC Arb Bot — Starting up");
    state.push_log(LogLevel::Info, format!(" Balance: ${:.2} | Mode: {}", initial_balance, if dry_run { "DRY RUN" } else { "LIVE" }));
    state.push_log(LogLevel::Info, format!(" Min divergence: {:.2}% | Max risk/trade: {:.2}%", min_divergence * 100.0, config.max_risk_per_trade * 100.0));
    state.push_log(LogLevel::Info, "═══════════════════════════════════════════");

    // ── Open trade journal ───────────────────────────────────────────────
    let journal = match TradeJournal::open("data") {
        Ok(j) => Arc::new(parking_lot::Mutex::new(j)),
        Err(e) => {
            eprintln!("❌ Journal error: {e}");
            std::process::exit(1);
        }
    };

    // ── Build Polymarket client ──────────────────────────────────────────
    let mut poly_base = PolymarketClient::new(&config);

    // For live trading: authenticate the SDK client inside a temporary tokio runtime
    if !dry_run {
        let pk = config.polymarket_private_key.clone();
        let clob_url = config.polymarket_clob_url.clone();

        // Build a temporary single-threaded runtime just for authentication
        let auth_rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to create auth runtime");

        match auth_rt.block_on(async {
            use polymarket_client_sdk::POLYGON;
            use polymarket_client_sdk::auth::Signer as _;
            use alloy_signer_local::PrivateKeySigner;
            use polymarket_client_sdk::clob::{Client as ClobClient, Config as SdkConfig};

            let signer = PrivateKeySigner::from_str(&pk)
                .map_err(|e| anyhow::anyhow!("Invalid private key: {e}"))?
                .with_chain_id(Some(POLYGON));

            let client = ClobClient::new(&clob_url, SdkConfig::default())
                .map_err(|e| anyhow::anyhow!("SDK client creation failed: {e}"))?
                .authentication_builder(&signer)
                .authenticate()
                .await
                .map_err(|e| anyhow::anyhow!("SDK authentication failed: {e}"))?;

            state.push_log(LogLevel::Info, format!("🔑 SDK authenticated: {}", signer.address()));
            Ok::<_, anyhow::Error>((client, signer))
        }) {
            Ok((sdk_client, signer)) => {
                poly_base = poly_base.with_sdk(sdk_client, signer);
                state.push_log(LogLevel::Info, "✅ Live trading enabled via Polymarket SDK");
            }
            Err(e) => {
                state.push_log(LogLevel::Error, format!("⚠️  SDK auth failed (running dry-run): {e}"));
            }
        }
    }

    let poly_client = Arc::new(poly_base);

    // ── Build risk manager ───────────────────────────────────────────────
    let risk_mgr = Arc::new(RiskManager::new(
        config.max_risk_per_trade,
        config.daily_risk_cap,
        config.max_position_usd,
    ));

    // ── Build position tracker ───────────────────────────────────────────
    let positions = PositionTracker::new();

    // ── Tokio runtime with tuned thread pool ─────────────────────────────
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .thread_name("arb-worker")
        .enable_all()
        .build()
        .expect("failed to create tokio runtime");

    // ── Spawn async tasks ────────────────────────────────────────────────

    // Task 1: Binance WS feed
    {
        let s = state.clone();
        rt.spawn(async move {
            api::price_feeds::binance_feed(s, binance_url).await;
        });
    }

    // Task 2: Coinbase WS feed
    {
        let s = state.clone();
        rt.spawn(async move {
            api::price_feeds::coinbase_feed(s, coinbase_url).await;
        });
    }

    // Task 3: CoinGecko REST fallback (BTC + ETH)
    {
        let s = state.clone();
        rt.spawn(async move {
            api::price_feeds::coingecko_feed(s, coingecko_url).await;
        });
    }

    // Task 3b: Commodity feeds (Crude Oil + Gold via Yahoo Finance)
    {
        let s = state.clone();
        rt.spawn(async move {
            api::price_feeds::commodity_feed(s).await;
        });
    }

    // Task 4: Polymarket market poll
    {
        let s = state.clone();
        let client = poly_client.clone();
        rt.spawn(async move {
            market_poll_loop(s, client, poll_interval).await;
        });
    }

    // Task 5: Arbitrage scan + execution
    {
        let s = state.clone();
        let client = poly_client.clone();
        let rm = risk_mgr.clone();
        let j = journal.clone();
        let pos = positions.clone();
        rt.spawn(async move {
            arb_scan_loop(s, client, rm, j, pos, scan_interval, min_divergence, dry_run).await;
        });
    }

    // Task 6: Equity snapshot every second
    {
        let s = state.clone();
        rt.spawn(async move {
            loop {
                if s.is_shutdown.load(Ordering::Relaxed) { return; }
                s.snapshot_equity();
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        });
    }

    // Task 7: Web API server (for frontend dashboard)
    {
        let s = state.clone();
        let api_port: u16 = std::env::var("API_PORT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(3001);
        rt.spawn(async move {
            api::server::start_server(s, api_port).await;
        });
    }

    // ── Ctrl-C handler ───────────────────────────────────────────────────
    {
        let s = state.clone();
        rt.spawn(async move {
            let _ = tokio::signal::ctrl_c().await;
            s.is_shutdown.store(true, Ordering::Relaxed);
        });
    }

    // ── Run TUI or headless mode ─────────────────────────────────────────
    let headless = std::env::var("HEADLESS").map(|v| v == "1" || v.eq_ignore_ascii_case("true")).unwrap_or(false);

    if headless {
        // Headless mode: keep running until Ctrl-C / shutdown signal
        state.push_log(LogLevel::Info, "Running in headless mode (web dashboard only)");
        state.push_log(LogLevel::Info, format!("API server on port {}", std::env::var("API_PORT").unwrap_or_else(|_| "3001".into())));
        rt.block_on(async {
            loop {
                if state.is_shutdown.load(Ordering::Relaxed) { break; }
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
        });
    } else {
        // Interactive TUI mode
        if let Err(e) = dashboard::tui::run_tui(state.clone(), dry_run) {
            eprintln!("TUI error: {e}");
        }
    }

    // ── Shutdown ─────────────────────────────────────────────────────────
    state.is_shutdown.store(true, Ordering::Relaxed);
    rt.shutdown_timeout(std::time::Duration::from_secs(5));

    // Print session summary
    println!("\n═══════════════════════════════════════════");
    println!(" ⚡ Session Summary");
    println!("───────────────────────────────────────────");
    println!("   Balance:   ${:.2}", *state.balance.read());
    println!("   Daily PnL: ${:.2}", *state.daily_pnl.read());
    println!("   Total PnL: ${:.2}", *state.total_pnl.read());
    println!("   Trades:    {} (W:{} / L:{})",
             state.total_trades(),
             state.wins.load(Ordering::Relaxed),
             state.losses.load(Ordering::Relaxed));
    println!("   Win Rate:  {:.1}%", state.win_rate());
    println!("   Orders:    {}", state.orders.load(Ordering::Relaxed));
    println!("═══════════════════════════════════════════\n");
}

// ── Market poll loop ─────────────────────────────────────────────────────────

async fn market_poll_loop(
    state: AppState,
    client: Arc<PolymarketClient>,
    interval: std::time::Duration,
) {
    // Wait a few seconds for price feeds to warm up
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    loop {
        if state.is_shutdown.load(Ordering::Relaxed) { return; }

        state.push_log(LogLevel::Info, "POLLING Polymarket markets...");
        match client.build_snapshots(&state).await {
            Ok(n) => {
                state.push_log(LogLevel::Info, format!("Loaded {n} markets from Polymarket (multi-asset)"));
            }
            Err(e) => {
                state.push_log(LogLevel::Error, format!("Market poll failed: {e:#}"));
            }
        }

        tokio::time::sleep(interval).await;
    }
}

// ── Arbitrage scan + execution loop ──────────────────────────────────────────

async fn arb_scan_loop(
    state: AppState,
    client: Arc<PolymarketClient>,
    risk_mgr: Arc<RiskManager>,
    journal: Arc<parking_lot::Mutex<TradeJournal>>,
    tracker: PositionTracker,
    interval: std::time::Duration,
    min_divergence: f64,
    dry_run: bool,
) {
    // Wait for markets to load
    tokio::time::sleep(std::time::Duration::from_secs(8)).await;

    // Track recently-traded condition IDs to avoid hammering the same market
    let mut cooldowns: std::collections::HashMap<String, std::time::Instant> = std::collections::HashMap::new();
    let cooldown_secs = 300u64; // 5 minutes between trades on same market

    loop {
        if state.is_shutdown.load(Ordering::Relaxed) { return; }
        if state.is_paused.load(Ordering::Relaxed) {
            tokio::time::sleep(interval).await;
            continue;
        }

        // Clean up expired cooldowns
        let now = std::time::Instant::now();
        cooldowns.retain(|_, ts| now.duration_since(*ts).as_secs() < cooldown_secs);

        // Scan for divergences across all asset classes
        let n_markets = state.markets.len();
        let signals = arbitrage::scan_divergences(&state, &state.markets, min_divergence);

        // Log scan summary with price overview
        let btc = state.best_btc_price();
        let oil = *state.oil_price.read();
        let gold = *state.gold_price.read();
        let eth = *state.eth_price.read();
        state.push_log(
            LogLevel::Info,
            format!("📊 Scanned {n_markets} mkts → {} signals | BTC=${btc:.0} ETH=${eth:.0} CL=${oil:.1} GC=${gold:.0}", signals.len()),
        );

        // Find best signal that's not on cooldown
        let top = signals.iter().find(|s| !cooldowns.contains_key(&s.condition_id));

        if let Some(top) = top {
            let spot = state.best_price(top.asset);
            state.push_log(
                LogLevel::Info,
                format!(
                    "🔍 [{}] DIV {:.2}% on \"{}\" (fair={:.4} vs mkt={:.4}) spot={:.2}",
                    top.asset.label(),
                    top.divergence * 100.0,
                    truncate_str(&top.question, 50),
                    top.fair_price,
                    top.market_price,
                    spot,
                ),
            );

            // Risk check
            if let Some(size) = risk_mgr.check_and_size(&state, top.entry_price) {
                let start = std::time::Instant::now();

                // Execute
                match client.place_order(
                    &state,
                    &top.token_id,
                    top.entry_price,
                    size,
                    top.side,
                ).await {
                    Ok(Some(_resp)) => {
                        let latency_us = start.elapsed().as_micros() as u64;

                        // Compute PnL based on mode
                        let pnl = if dry_run {
                            // Dry-run: edge-based simulation with realistic win/loss
                            PositionTracker::estimate_dry_run_pnl(top.abs_divergence, size)
                        } else {
                            // Live: estimate PnL from edge (entry vs fair value)
                            let order_id = _resp.order_id.as_deref().unwrap_or("unknown");
                            tracker.open_position(
                                &top.condition_id,
                                &top.token_id,
                                top.side.as_str(),
                                top.entry_price,
                                size,
                                order_id,
                            );
                            PositionTracker::estimate_live_pnl(
                                top.entry_price,
                                top.fair_price,
                                size,
                                top.side.as_str(),
                            )
                        };

                        state.apply_pnl(pnl);

                        let pnl_str = if pnl >= 0.0 {
                            format!("+${:.2}", pnl)
                        } else {
                            format!("-${:.2}", pnl.abs())
                        };

                        if pnl >= 0.0 {
                            state.push_log(
                                LogLevel::Fill,
                                format!("FILLED {pnl_str} // market converged | {:.1}ms", latency_us as f64 / 1000.0),
                            );
                        } else {
                            state.push_log(
                                LogLevel::Slip,
                                format!("SLIPPED {pnl_str} // adverse fill | {:.1}ms", latency_us as f64 / 1000.0),
                            );
                        }

                        // Journal
                        let record = TradeRecord {
                            timestamp: chrono::Utc::now().to_rfc3339(),
                            condition_id: top.condition_id.clone(),
                            question: top.question.clone(),
                            side: top.side.as_str().to_string(),
                            token_id: top.token_id.clone(),
                            entry_price: top.entry_price,
                            size,
                            pnl,
                            divergence: top.divergence,
                            latency_us,
                            dry_run,
                        };
                        if let Err(e) = journal.lock().record(&record) {
                            state.push_log(LogLevel::Error, format!("Journal write failed: {e}"));
                        }

                        // Cooldown: don't re-trade this market for 5 minutes
                        cooldowns.insert(top.condition_id.clone(), std::time::Instant::now());
                    }
                    Ok(None) => {
                        state.push_log(LogLevel::Warn, "Order rejected — no fill");
                    }
                    Err(e) => {
                        state.push_log(LogLevel::Error, format!("Execution error: {e}"));
                    }
                }
            }
        }

        tokio::time::sleep(interval).await;
    }
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max])
    }
}

