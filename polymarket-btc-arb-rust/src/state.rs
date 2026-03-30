// src/state.rs — Shared application state behind Arc for multi-task access
use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use ratatui::style::Color;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

// ── Asset classes ────────────────────────────────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Asset {
    Btc,
    Eth,
    CrudeOil,
    Gold,
    FedRate,
}

impl Asset {
    pub fn label(self) -> &'static str {
        match self {
            Self::Btc => "BTC",
            Self::Eth => "ETH",
            Self::CrudeOil => "CL",
            Self::Gold => "GC",
            Self::FedRate => "FED",
        }
    }
}

// ── Log levels ───────────────────────────────────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Info,
    Exec,
    Fill,
    Warn,
    Error,
    Slip,
}

impl LogLevel {
    pub fn label(self) -> &'static str {
        match self {
            Self::Info  => "INFO",
            Self::Exec  => "EXEC",
            Self::Fill  => "FILL",
            Self::Warn  => "WARN",
            Self::Error => "ERR ",
            Self::Slip  => "SLIP",
        }
    }

    pub fn color(self) -> Color {
        match self {
            Self::Info  => Color::Rgb(0, 217, 255),   // cyan
            Self::Exec  => Color::Rgb(255, 184, 0),   // amber
            Self::Fill  => Color::Rgb(0, 255, 136),    // green
            Self::Warn  => Color::Rgb(255, 184, 0),    // amber
            Self::Error => Color::Rgb(255, 51, 102),   // red
            Self::Slip  => Color::Rgb(255, 51, 102),   // red
        }
    }
}

// ── Log entry ────────────────────────────────────────────────────────────────
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub ts: DateTime<Utc>,
    pub level: LogLevel,
    pub message: String,
}

// ── Equity point ─────────────────────────────────────────────────────────────
#[derive(Debug, Clone)]
pub struct EquityPoint {
    pub ts: DateTime<Utc>,
    pub equity: f64,
}

// ── Polymarket market snapshot ───────────────────────────────────────────────
#[derive(Debug, Clone)]
pub struct MarketSnapshot {
    pub condition_id: String,
    pub question: String,
    pub token_id_yes: String,
    pub token_id_no: String,
    pub yes_bid: f64,
    pub yes_ask: f64,
    pub no_bid: f64,
    pub no_ask: f64,
    pub end_date: Option<String>,
    pub updated_at: DateTime<Utc>,
    pub asset: Asset,
}

// ── Main shared state ────────────────────────────────────────────────────────
#[derive(Clone)]
pub struct AppState {
    // Live BTC prices
    pub btc_binance:  Arc<RwLock<f64>>,
    pub btc_coinbase: Arc<RwLock<f64>>,
    pub btc_coingecko: Arc<RwLock<f64>>,

    // Live commodity prices
    pub oil_price:  Arc<RwLock<f64>>,
    pub gold_price: Arc<RwLock<f64>>,
    pub eth_price:  Arc<RwLock<f64>>,
    pub fed_rate:   Arc<RwLock<f64>>,   // current upper bound fed funds rate

    // Polymarket snapshots: condition_id → snapshot
    pub markets: Arc<dashmap::DashMap<String, MarketSnapshot>>,

    // Account
    pub balance:   Arc<RwLock<f64>>,
    pub daily_pnl: Arc<RwLock<f64>>,
    pub total_pnl: Arc<RwLock<f64>>,

    // Counters
    pub wins:   Arc<AtomicU64>,
    pub losses: Arc<AtomicU64>,
    pub orders: Arc<AtomicU64>,

    // Latency tracking (microseconds)
    pub last_latency_us: Arc<AtomicU64>,

    // Execution log (ring buffer, newest last)
    pub log: Arc<RwLock<VecDeque<LogEntry>>>,
    pub log_capacity: usize,

    // Equity history
    pub equity: Arc<RwLock<VecDeque<EquityPoint>>>,
    pub equity_capacity: usize,

    // Control flags
    pub is_shutdown: Arc<AtomicBool>,
    pub is_paused:   Arc<AtomicBool>,
    pub daily_cap_hit: Arc<AtomicBool>,
}

impl AppState {
    pub fn new(initial_balance: f64) -> Self {
        Self {
            btc_binance:   Arc::new(RwLock::new(0.0)),
            btc_coinbase:  Arc::new(RwLock::new(0.0)),
            btc_coingecko: Arc::new(RwLock::new(0.0)),
            oil_price:     Arc::new(RwLock::new(0.0)),
            gold_price:    Arc::new(RwLock::new(0.0)),
            eth_price:     Arc::new(RwLock::new(0.0)),
            fed_rate:      Arc::new(RwLock::new(4.50)),  // current upper bound as of 2026
            markets:       Arc::new(dashmap::DashMap::new()),
            balance:       Arc::new(RwLock::new(initial_balance)),
            daily_pnl:     Arc::new(RwLock::new(0.0)),
            total_pnl:     Arc::new(RwLock::new(0.0)),
            wins:          Arc::new(AtomicU64::new(0)),
            losses:        Arc::new(AtomicU64::new(0)),
            orders:        Arc::new(AtomicU64::new(0)),
            last_latency_us: Arc::new(AtomicU64::new(0)),
            log:           Arc::new(RwLock::new(VecDeque::with_capacity(1000))),
            log_capacity:  1000,
            equity:        Arc::new(RwLock::new(VecDeque::with_capacity(7200))),
            equity_capacity: 7200,
            is_shutdown:   Arc::new(AtomicBool::new(false)),
            is_paused:     Arc::new(AtomicBool::new(false)),
            daily_cap_hit: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Best available BTC price — prefers Binance, then Coinbase, then CoinGecko.
    pub fn best_btc_price(&self) -> f64 {
        let b = *self.btc_binance.read();
        if b > 0.0 { return b; }
        let c = *self.btc_coinbase.read();
        if c > 0.0 { return c; }
        *self.btc_coingecko.read()
    }

    /// Best available spot price for any tracked asset.
    pub fn best_price(&self, asset: Asset) -> f64 {
        match asset {
            Asset::Btc => self.best_btc_price(),
            Asset::Eth => *self.eth_price.read(),
            Asset::CrudeOil => *self.oil_price.read(),
            Asset::Gold => *self.gold_price.read(),
            Asset::FedRate => *self.fed_rate.read(),
        }
    }

    pub fn win_rate(&self) -> f64 {
        let w = self.wins.load(Ordering::Relaxed) as f64;
        let l = self.losses.load(Ordering::Relaxed) as f64;
        let total = w + l;
        if total == 0.0 { 0.0 } else { (w / total) * 100.0 }
    }

    pub fn total_trades(&self) -> u64 {
        self.wins.load(Ordering::Relaxed) + self.losses.load(Ordering::Relaxed)
    }

    pub fn push_log(&self, level: LogLevel, message: impl Into<String>) {
        let entry = LogEntry {
            ts: Utc::now(),
            level,
            message: message.into(),
        };
        let mut log = self.log.write();
        if log.len() >= self.log_capacity {
            log.pop_front();
        }
        log.push_back(entry);
    }

    pub fn snapshot_equity(&self) {
        let eq = *self.balance.read();
        let point = EquityPoint { ts: Utc::now(), equity: eq };
        let mut hist = self.equity.write();
        if hist.len() >= self.equity_capacity {
            hist.pop_front();
        }
        hist.push_back(point);
    }

    pub fn apply_pnl(&self, pnl: f64) {
        {
            let mut b = self.balance.write();
            *b += pnl;
        }
        {
            let mut d = self.daily_pnl.write();
            *d += pnl;
        }
        {
            let mut t = self.total_pnl.write();
            *t += pnl;
        }
        if pnl >= 0.0 {
            self.wins.fetch_add(1, Ordering::Relaxed);
        } else {
            self.losses.fetch_add(1, Ordering::Relaxed);
        }
    }
}
