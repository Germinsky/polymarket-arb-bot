// src/api/polymarket.rs — Polymarket CLOB REST client using official SDK for live orders
use crate::config::Config;
use crate::state::{AppState, Asset, LogLevel, MarketSnapshot};
use anyhow::{Context, Result};
use chrono::Utc;
use serde::Deserialize;
use std::sync::Arc;
use std::time::Instant;

// SDK re-exports for live order execution
use alloy_signer_local::PrivateKeySigner;
use polymarket_client_sdk::auth::state::Authenticated;
use polymarket_client_sdk::auth::Normal;
use polymarket_client_sdk::clob::Client as ClobClient;
use polymarket_client_sdk::clob::types::Side as SdkSide;
use polymarket_client_sdk::types::{Decimal, U256};

// ── API response types ───────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct GammaEvent {
    pub title: Option<String>,
    pub slug: Option<String>,
    pub markets: Option<Vec<GammaMarket>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GammaMarket {
    #[serde(alias = "conditionId")]
    pub condition_id: Option<String>,
    pub question: Option<String>,
    pub tokens: Option<Vec<GammaToken>>,
    #[serde(alias = "endDateIso")]
    pub end_date_iso: Option<String>,
    pub active: Option<bool>,
    /// Inline token IDs from the event's nested market list
    #[serde(alias = "clobTokenIds")]
    pub clob_token_ids: Option<String>,
    pub outcomes: Option<String>,
    /// Gamma-provided best bid for the Yes token
    #[serde(alias = "bestBid")]
    pub best_bid: Option<f64>,
    /// Gamma-provided best ask for the Yes token
    #[serde(alias = "bestAsk")]
    pub best_ask: Option<f64>,
    /// Outcome prices as JSON string e.g. '["0.57", "0.43"]'
    #[serde(alias = "outcomePrices")]
    pub outcome_prices: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GammaToken {
    pub token_id: Option<String>,
    pub outcome: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BookResponse {
    bids: Option<Vec<BookLevel>>,
    asks: Option<Vec<BookLevel>>,
}

#[derive(Debug, Deserialize)]
struct BookLevel {
    price: Option<String>,
    size: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Buy,
    Sell,
}

impl Side {
    pub fn as_str(&self) -> &'static str {
        match self {
            Side::Buy => "BUY",
            Side::Sell => "SELL",
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct OrderResponse {
    pub order_id: Option<String>,
    pub status: Option<String>,
}

/// Type alias for the authenticated SDK CLOB client (Normal = standard user, not builder)
pub type AuthenticatedClobClient = ClobClient<Authenticated<Normal>>;

// ── Client ───────────────────────────────────────────────────────────────────

pub struct PolymarketClient {
    http: reqwest::Client,
    gamma_url: String,
    dry_run: bool,
    /// SDK CLOB client for authenticated order execution (None in dry_run)
    sdk_client: Option<Arc<AuthenticatedClobClient>>,
    /// Wallet signer for signing orders (None in dry_run)
    signer: Option<Arc<PrivateKeySigner>>,
}

impl PolymarketClient {
    /// Create a new client for market discovery only (no live trading).
    /// Call `with_sdk` to attach the authenticated SDK client for live orders.
    pub fn new(config: &Config) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .pool_max_idle_per_host(8)
            .user_agent("Mozilla/5.0 (compatible; ArbBot/1.0)")
            .build()
            .expect("failed to build HTTP client");

        Self {
            http,
            gamma_url: config.polymarket_gamma_url.clone(),
            dry_run: config.dry_run,
            sdk_client: None,
            signer: None,
        }
    }

    /// Attach an authenticated SDK CLOB client + signer for live order execution.
    pub fn with_sdk(mut self, client: AuthenticatedClobClient, signer: PrivateKeySigner) -> Self {
        self.sdk_client = Some(Arc::new(client));
        self.signer = Some(Arc::new(signer));
        self
    }

    /// Fetch active BTC price-threshold markets from Gamma API.
    /// Uses the events endpoint with text search, then collects child markets.
    pub async fn fetch_btc_markets(&self) -> Result<Vec<GammaMarket>> {
        self.fetch_markets_for_asset(Asset::Btc).await
    }

    /// Classify an event title + question into an asset, if any.
    fn classify_market(title: &str, question: &str) -> Option<Asset> {
        let t = title.to_lowercase();
        let q = question.to_lowercase();

        // Bitcoin / BTC
        if t.contains("bitcoin") || t.contains("btc") || q.contains("bitcoin") || q.contains("btc") {
            // Skip "MicroStrategy sells" unless it also has a $ threshold
            if (q.contains("microstrategy") || q.contains("micro strategy")) && !q.contains('$') {
                return None;
            }
            return Some(Asset::Btc);
        }
        // Ethereum / ETH
        if t.contains("ethereum") || t.contains(" eth ") || q.contains("ethereum") || q.contains(" eth ") {
            return Some(Asset::Eth);
        }
        // Crude Oil
        if t.contains("crude oil") || t.contains("(cl)") || q.contains("crude oil") || q.contains("(cl)") {
            return Some(Asset::CrudeOil);
        }
        // Gold
        if t.contains("gold") || t.contains("(gc)") || q.contains("gold") || q.contains("(gc)") {
            return Some(Asset::Gold);
        }
        // Fed rate
        if t.contains("fed ") || t.contains("interest rate") || t.contains("federal fund")
            || q.contains("fed ") || q.contains("interest rate") || q.contains("federal fund")
        {
            return Some(Asset::FedRate);
        }
        None
    }

    /// Fetch all active markets for a specific asset class from Gamma API.
    pub async fn fetch_markets_for_asset(&self, asset: Asset) -> Result<Vec<GammaMarket>> {
        // We fetch ALL events and filter locally — avoids fragile text-based API filters
        let events_url = format!(
            "{}/events?active=true&closed=false&limit=100&order=volume24hr&ascending=false",
            self.gamma_url
        );
        let resp = self.http.get(&events_url).send().await?.error_for_status()?;
        let body = resp.text().await?;
        let events: Vec<GammaEvent> = match serde_json::from_str(&body) {
            Ok(e) => e,
            Err(e) => {
                anyhow::bail!("Events parse error at line {} col {}: {} | body[..200]: {}",
                    e.line(), e.column(), e, &body[..body.len().min(200)]);
            }
        };

        let mut out = Vec::new();
        for event in &events {
            let title = event.title.as_deref().unwrap_or("");
            if let Some(markets) = &event.markets {
                for m in markets {
                    let q = m.question.as_deref().unwrap_or("");
                    if Self::classify_market(title, q) == Some(asset) {
                        out.push(m.clone());
                    }
                }
            }
        }
        Ok(out)
    }

    /// Fetch ALL markets across every supported asset class.
    pub async fn fetch_all_asset_markets(&self) -> Result<Vec<(Asset, GammaMarket)>> {
        let events_url = format!(
            "{}/events?active=true&closed=false&limit=100&order=volume24hr&ascending=false",
            self.gamma_url
        );
        let resp = self.http.get(&events_url).send().await?.error_for_status()?;
        let body = resp.text().await?;
        let events: Vec<GammaEvent> = match serde_json::from_str(&body) {
            Ok(e) => e,
            Err(e) => {
                anyhow::bail!("Events parse error at line {} col {}: {} | body[..200]: {}",
                    e.line(), e.column(), e, &body[..body.len().min(200)]);
            }
        };

        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for event in &events {
            let title = event.title.as_deref().unwrap_or("");
            if let Some(markets) = &event.markets {
                for m in markets {
                    let q = m.question.as_deref().unwrap_or("");
                    if let Some(asset) = Self::classify_market(title, q) {
                        let cid = m.condition_id.as_deref().unwrap_or("");
                        if !cid.is_empty() && seen.insert(cid.to_string()) {
                            out.push((asset, m.clone()));
                        }
                    }
                }
            }
        }
        Ok(out)
    }

    /// Fetch best bid/ask for a token from the CLOB order book.
    pub async fn fetch_token_price(&self, token_id: &str) -> Result<(f64, f64)> {
        let clob_url = self.sdk_client.as_ref()
            .map(|c| c.host().to_string())
            .unwrap_or_else(|| "https://clob.polymarket.com/".to_string());
        let url = format!("{}book?token_id={}", clob_url, token_id);
        let resp = self.http.get(&url).send().await?.error_for_status()?;
        let book: BookResponse = resp.json().await?;

        let best_bid = book
            .bids
            .as_ref()
            .and_then(|b| b.first())
            .and_then(|l| l.price.as_ref())
            .and_then(|p| p.parse::<f64>().ok())
            .unwrap_or(0.0);

        let best_ask = book
            .asks
            .as_ref()
            .and_then(|a| a.first())
            .and_then(|l| l.price.as_ref())
            .and_then(|p| p.parse::<f64>().ok())
            .unwrap_or(1.0);

        Ok((best_bid, best_ask))
    }

    /// Build snapshots from Gamma markets using inline prices (all asset classes).
    /// Uses Gamma's bestBid/bestAsk/outcomePrices instead of individual CLOB book calls.
    pub async fn build_snapshots(&self, state: &AppState) -> Result<usize> {
        let tagged_markets = self.fetch_all_asset_markets().await?;
        let mut count = 0usize;

        for (asset, m) in &tagged_markets {
            let cid = match &m.condition_id {
                Some(c) => c.clone(),
                None => continue,
            };
            let question = m.question.clone().unwrap_or_default();
            // Try to get token IDs from embedded tokens array or clobTokenIds string
            let (yes_tid, no_tid) = if let Some(tokens) = &m.tokens {
                if tokens.len() >= 2 {
                    let mut yes = String::new();
                    let mut no = String::new();
                    for tok in tokens {
                        match tok.outcome.as_deref() {
                            Some("Yes") => yes = tok.token_id.clone().unwrap_or_default(),
                            Some("No")  => no = tok.token_id.clone().unwrap_or_default(),
                            _ => {}
                        }
                    }
                    if yes.is_empty() || no.is_empty() {
                        continue;
                    }
                    (yes, no)
                } else {
                    continue;
                }
            } else if let Some(clob_ids_json) = &m.clob_token_ids {
                // Parse from JSON array string like ["id1", "id2"]
                match serde_json::from_str::<Vec<String>>(clob_ids_json) {
                    Ok(ids) if ids.len() >= 2 => (ids[0].clone(), ids[1].clone()),
                    _ => continue,
                }
            } else {
                continue;
            };

            // Use Gamma's inline prices — vastly more reliable than CLOB /book
            let (yes_bid, yes_ask) = match (m.best_bid, m.best_ask) {
                (Some(bid), Some(ask)) if bid > 0.0 && ask > 0.0 => (bid, ask),
                _ => {
                    // Fallback: try outcomePrices midpoint with a synthetic spread
                    if let Some(ref prices_json) = m.outcome_prices {
                        if let Ok(prices) = serde_json::from_str::<Vec<String>>(prices_json) {
                            if let Some(yes_mid) = prices.first().and_then(|p| p.parse::<f64>().ok()) {
                                let half_spread = 0.005; // synthetic 1¢ spread
                                ((yes_mid - half_spread).max(0.001), (yes_mid + half_spread).min(0.999))
                            } else {
                                continue;
                            }
                        } else {
                            continue;
                        }
                    } else {
                        continue;
                    }
                }
            };

            // Derive No prices from Yes prices (binary market: Yes + No = 1)
            let no_bid = (1.0 - yes_ask).max(0.0);
            let no_ask = (1.0 - yes_bid).min(1.0);

            let snap = MarketSnapshot {
                condition_id: cid.clone(),
                question,
                token_id_yes: yes_tid,
                token_id_no: no_tid,
                yes_bid,
                yes_ask,
                no_bid,
                no_ask,
                end_date: m.end_date_iso.clone(),
                updated_at: Utc::now(),
                asset: *asset,
            };
            state.markets.insert(cid, snap);
            count += 1;
        }
        Ok(count)
    }

    /// Place an order on the CLOB (or simulate in dry_run mode).
    /// Live orders use the official Polymarket SDK for EIP-712 signing + L2 auth.
    pub async fn place_order(
        &self,
        state: &AppState,
        token_id: &str,
        price: f64,
        size: f64,
        side: Side,
    ) -> Result<Option<OrderResponse>> {
        state.orders.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let start = Instant::now();

        if self.dry_run {
            // Simulate latency
            let latency = start.elapsed().as_micros() as u64 + 500; // ~0.5ms simulated
            state.last_latency_us.store(latency, std::sync::atomic::Ordering::Relaxed);
            state.push_log(
                LogLevel::Exec,
                format!(
                    "[DRY] {} {} @ ${:.4} × {:.2} | {:.1}ms",
                    side.as_str(),
                    &token_id[..8.min(token_id.len())],
                    price,
                    size,
                    latency as f64 / 1000.0,
                ),
            );
            return Ok(Some(OrderResponse {
                order_id: Some(uuid::Uuid::new_v4().to_string()),
                status: Some("DRY_FILLED".into()),
            }));
        }

        // ── Live order via Polymarket SDK ────────────────────────────────
        let sdk = self.sdk_client.as_ref()
            .context("Cannot place live orders: no authenticated SDK client (check POLYMARKET_PRIVATE_KEY)")?;
        let signer = self.signer.as_ref()
            .context("Cannot place live orders: no signer available")?;

        let sdk_side = match side {
            Side::Buy => SdkSide::Buy,
            Side::Sell => SdkSide::Sell,
        };

        let dec_price = Decimal::try_from(price)
            .context("Invalid price for order")?;
        let dec_size = Decimal::try_from(size)
            .context("Invalid size for order")?;
        let token_u256 = U256::from_str_radix(token_id, 10)
            .or_else(|_| token_id.parse::<U256>())
            .context("Invalid token_id — expected numeric string")?;

        // Build → Sign → Post (SDK handles EIP-712, HMAC L2 auth, nonce, etc.)
        let order = sdk
            .limit_order()
            .token_id(token_u256)
            .price(dec_price)
            .size(dec_size)
            .side(sdk_side)
            .build()
            .await
            .context("SDK order build failed")?;

        let signed = sdk
            .sign(signer.as_ref(), order)
            .await
            .context("SDK order signing failed")?;

        let resp = sdk
            .post_order(signed)
            .await
            .context("SDK post_order failed")?;

        let latency = start.elapsed().as_micros() as u64;
        state.last_latency_us.store(latency, std::sync::atomic::Ordering::Relaxed);

        let order_id = resp.order_id.clone();
        state.push_log(
            LogLevel::Fill,
            format!(
                "FILLED {} @ ${:.4} × {:.2} | {:.1}ms | id={}",
                side.as_str(),
                price,
                size,
                latency as f64 / 1000.0,
                &order_id,
            ),
        );

        Ok(Some(OrderResponse {
            order_id: Some(order_id),
            status: Some(resp.status.to_string()),
        }))
    }
}
