// src/config.rs — All configuration loaded from .env
use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    // ── Polymarket CLOB API ───────────────────────────────────────────────
    pub polymarket_private_key: String,
    pub polymarket_clob_url: String,
    pub polymarket_gamma_url: String,

    // ── Price feeds ──────────────────────────────────────────────────────
    pub binance_ws_url: String,
    pub coinbase_ws_url: String,
    pub coingecko_url: String,

    // ── Risk parameters ──────────────────────────────────────────────────
    pub max_risk_per_trade: f64,   // fraction of balance (e.g. 0.005 = 0.5%)
    pub daily_risk_cap: f64,       // fraction of balance (e.g. 0.02 = 2%)
    pub min_divergence: f64,       // e.g. 0.003 = 0.3%
    pub max_position_usd: f64,    // hard cap per position
    pub initial_balance: f64,

    // ── Misc ─────────────────────────────────────────────────────────────
    pub dry_run: bool,
    pub poll_interval_ms: u64,
    pub scan_interval_ms: u64,
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        dotenvy::dotenv().ok(); // ignore if .env is missing

        Ok(Self {
            polymarket_private_key:    env_or("POLYMARKET_PRIVATE_KEY", ""),
            polymarket_clob_url:       env_or("POLYMARKET_CLOB_URL", "https://clob.polymarket.com"),
            polymarket_gamma_url:      env_or("POLYMARKET_GAMMA_URL", "https://gamma-api.polymarket.com"),

            binance_ws_url:  env_or("BINANCE_WS_URL", "wss://stream.binance.com:9443/ws/btcusdt@trade"),
            coinbase_ws_url: env_or("COINBASE_WS_URL", "wss://ws-feed.exchange.coinbase.com"),
            coingecko_url:   env_or("COINGECKO_URL", "https://api.coingecko.com/api/v3"),

            max_risk_per_trade: parse_env("MAX_RISK_PER_TRADE", 0.005),
            daily_risk_cap:     parse_env("DAILY_RISK_CAP", 0.02),
            min_divergence:     parse_env("MIN_DIVERGENCE", 0.003),
            max_position_usd:   parse_env("MAX_POSITION_USD", 100.0),
            initial_balance:    parse_env("INITIAL_BALANCE", 1000.0),

            dry_run:           env_bool("DRY_RUN", true),
            poll_interval_ms:  parse_env("POLL_INTERVAL_MS", 3000),
            scan_interval_ms:  parse_env("SCAN_INTERVAL_MS", 100),
        })
    }
}

fn env_or(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}

fn parse_env<T: std::str::FromStr>(key: &str, default: T) -> T {
    env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn env_bool(key: &str, default: bool) -> bool {
    env::var(key)
        .map(|v| matches!(v.as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(default)
}
