// src/api/price_feeds.rs — Multi-asset price feeds: BTC, ETH, Crude Oil, Gold
use crate::state::{AppState, LogLevel};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use std::sync::atomic::Ordering;
use tokio_tungstenite::connect_async;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Binance WebSocket — fastest public BTC/USDT trade stream
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[derive(Deserialize)]
struct BinanceTrade {
    #[serde(rename = "p")]
    price: String,
}

pub async fn binance_feed(state: AppState, url: String) {
    // Try multiple Binance endpoints (geo-fallback)
    let urls = [
        url.clone(),
        "wss://stream.binance.us:9443/ws/btcusdt@trade".to_string(),
        "wss://data-stream.binance.vision/ws/btcusdt@trade".to_string(),
    ];
    let mut url_idx = 0;
    let mut consecutive_fails = 0u32;

    loop {
        if state.is_shutdown.load(Ordering::Relaxed) {
            return;
        }
        let current_url = &urls[url_idx % urls.len()];
        state.push_log(LogLevel::Info, format!("Binance WS connecting to {}...", current_url.split('/').nth(2).unwrap_or("?")));
        match connect_async(current_url).await {
            Ok((ws, _)) => {
                state.push_log(LogLevel::Info, "Binance WS connected ✓");
                consecutive_fails = 0;
                let (_, mut read) = ws.split();
                while let Some(msg) = read.next().await {
                    if state.is_shutdown.load(Ordering::Relaxed) {
                        return;
                    }
                    match msg {
                        Ok(tokio_tungstenite::tungstenite::Message::Text(txt)) => {
                            if let Ok(trade) = serde_json::from_str::<BinanceTrade>(&txt) {
                                if let Ok(p) = trade.price.parse::<f64>() {
                                    *state.btc_binance.write() = p;
                                }
                            }
                        }
                        Ok(tokio_tungstenite::tungstenite::Message::Ping(d)) => {
                            // auto-pong handled by tungstenite
                            let _ = d;
                        }
                        Err(e) => {
                            state.push_log(LogLevel::Warn, format!("Binance WS error: {e}"));
                            break;
                        }
                        _ => {}
                    }
                }
            }
            Err(e) => {
                state.push_log(LogLevel::Warn, format!("Binance WS connect failed: {e}"));
                consecutive_fails += 1;
                // Rotate to next URL after failure
                url_idx += 1;
                if consecutive_fails >= urls.len() as u32 {
                    state.push_log(LogLevel::Warn, "All Binance endpoints failed — retrying cycle in 10s");
                    tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                    consecutive_fails = 0;
                    continue;
                }
            }
        }
        state.push_log(LogLevel::Warn, "Binance WS disconnected — reconnecting in 3s");
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Coinbase WebSocket — secondary BTC/USD feed for redundancy
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[derive(Deserialize)]
struct CoinbaseMsg {
    #[serde(rename = "type")]
    msg_type: Option<String>,
    price: Option<String>,
}

pub async fn coinbase_feed(state: AppState, url: String) {
    let subscribe = serde_json::json!({
        "type": "subscribe",
        "product_ids": ["BTC-USD"],
        "channels": ["ticker"]
    });

    loop {
        if state.is_shutdown.load(Ordering::Relaxed) {
            return;
        }
        state.push_log(LogLevel::Info, "Coinbase WS connecting...");
        match connect_async(&url).await {
            Ok((ws, _)) => {
                state.push_log(LogLevel::Info, "Coinbase WS connected ✓");
                let (mut write, mut read) = ws.split();
                let sub_msg = tokio_tungstenite::tungstenite::Message::Text(
                    subscribe.to_string(),
                );
                if write.send(sub_msg).await.is_err() {
                    continue;
                }
                while let Some(msg) = read.next().await {
                    if state.is_shutdown.load(Ordering::Relaxed) {
                        return;
                    }
                    match msg {
                        Ok(tokio_tungstenite::tungstenite::Message::Text(txt)) => {
                            if let Ok(m) = serde_json::from_str::<CoinbaseMsg>(&txt) {
                                if m.msg_type.as_deref() == Some("ticker") {
                                    if let Some(ps) = m.price {
                                        if let Ok(p) = ps.parse::<f64>() {
                                            *state.btc_coinbase.write() = p;
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            state.push_log(LogLevel::Warn, format!("Coinbase WS error: {e}"));
                            break;
                        }
                        _ => {}
                    }
                }
            }
            Err(e) => {
                state.push_log(LogLevel::Warn, format!("Coinbase WS connect failed: {e}"));
            }
        }
        state.push_log(LogLevel::Warn, "Coinbase WS disconnected — reconnecting in 3s");
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// CoinGecko REST fallback — polled every 10s as a safety net
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[derive(Deserialize)]
struct CoinGeckoResponse {
    bitcoin: Option<CoinGeckoPrice>,
    ethereum: Option<CoinGeckoPrice>,
}

#[derive(Deserialize)]
struct CoinGeckoPrice {
    usd: Option<f64>,
}

pub async fn coingecko_feed(state: AppState, base_url: String) {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent("Mozilla/5.0 (compatible; ArbBot/1.0)")
        .build()
        .unwrap();
    let url = format!("{base_url}/simple/price?ids=bitcoin,ethereum&vs_currencies=usd");

    loop {
        if state.is_shutdown.load(Ordering::Relaxed) {
            return;
        }
        match client.get(&url).send().await {
            Ok(resp) => {
                match resp.error_for_status() {
                    Ok(resp) => {
                        match resp.json::<CoinGeckoResponse>().await {
                            Ok(data) => {
                                if let Some(btc) = data.bitcoin {
                                    if let Some(p) = btc.usd {
                                        *state.btc_coingecko.write() = p;
                                    }
                                }
                                if let Some(eth) = data.ethereum {
                                    if let Some(p) = eth.usd {
                                        *state.eth_price.write() = p;
                                    }
                                }
                            }
                            Err(e) => {
                                state.push_log(LogLevel::Warn, format!("CoinGecko parse error: {e}"));
                            }
                        }
                    }
                    Err(e) => {
                        state.push_log(LogLevel::Warn, format!("CoinGecko HTTP error: {e}"));
                    }
                }
            }
            Err(e) => {
                state.push_log(LogLevel::Warn, format!("CoinGecko poll failed: {e}"));
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Commodity price feeds: Crude Oil (CL) and Gold (GC)
// Uses free public APIs — polled every 15s
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Metals.dev free API for gold price.
#[derive(Deserialize)]
struct MetalsResponse {
    metals: Option<MetalsPrices>,
}

#[derive(Deserialize)]
struct MetalsPrices {
    gold: Option<f64>,
}

/// Combined commodity feed: fetches oil + gold from public APIs.
pub async fn commodity_feed(state: AppState) {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent("Mozilla/5.0 (compatible; ArbBot/1.0)")
        .build()
        .unwrap();

    // Initial delay
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    loop {
        if state.is_shutdown.load(Ordering::Relaxed) {
            return;
        }

        // ── Crude Oil: use Yahoo Finance CSV endpoint ────────────────────
        match fetch_yahoo_price(&client, "CL=F").await {
            Ok(p) if p > 0.0 => {
                *state.oil_price.write() = p;
            }
            Ok(_) => {}
            Err(e) => {
                state.push_log(LogLevel::Warn, format!("Oil price fetch failed: {e}"));
            }
        }

        // ── Gold: use Yahoo Finance CSV endpoint ─────────────────────────
        match fetch_yahoo_price(&client, "GC=F").await {
            Ok(p) if p > 0.0 => {
                *state.gold_price.write() = p;
            }
            Ok(_) => {}
            Err(e) => {
                state.push_log(LogLevel::Warn, format!("Gold price fetch failed: {e}"));
            }
        }

        tokio::time::sleep(std::time::Duration::from_secs(15)).await;
    }
}

/// Fetch the latest price for a Yahoo Finance ticker using the v8 chart API.
async fn fetch_yahoo_price(client: &reqwest::Client, ticker: &str) -> anyhow::Result<f64> {
    let url = format!(
        "https://query1.finance.yahoo.com/v8/finance/chart/{}?interval=1m&range=1d",
        ticker,
    );
    let resp = client.get(&url).send().await?.error_for_status()?;
    let body: serde_json::Value = resp.json().await?;

    // Navigate: chart.result[0].meta.regularMarketPrice
    let price = body
        .get("chart")
        .and_then(|c| c.get("result"))
        .and_then(|r| r.get(0))
        .and_then(|r| r.get("meta"))
        .and_then(|m| m.get("regularMarketPrice"))
        .and_then(|p| p.as_f64())
        .unwrap_or(0.0);

    Ok(price)
}
