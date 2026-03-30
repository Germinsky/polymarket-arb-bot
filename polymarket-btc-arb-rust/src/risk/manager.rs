// src/risk/manager.rs — Pre-trade risk checks and position sizing
use crate::state::AppState;
use std::sync::atomic::Ordering;

pub struct RiskManager {
    max_risk_per_trade: f64,   // e.g. 0.005 = 0.5%
    daily_risk_cap: f64,       // e.g. 0.02 = 2%
    max_position_usd: f64,
}

impl RiskManager {
    pub fn new(max_risk_per_trade: f64, daily_risk_cap: f64, max_position_usd: f64) -> Self {
        Self {
            max_risk_per_trade,
            daily_risk_cap,
            max_position_usd,
        }
    }

    /// Check whether the trade is allowed and return a position size in USD.
    /// Returns `None` if the trade is blocked by a risk rule.
    pub fn check_and_size(&self, state: &AppState, entry_price: f64) -> Option<f64> {
        // 1. Daily cap check
        if state.daily_cap_hit.load(Ordering::Relaxed) {
            return None;
        }
        if state.is_paused.load(Ordering::Relaxed) {
            return None;
        }

        let balance = *state.balance.read();
        let daily_pnl = *state.daily_pnl.read();

        // 2. If daily loss exceeds cap, pause
        let daily_cap_usd = balance * self.daily_risk_cap;
        if daily_pnl <= -daily_cap_usd {
            state.daily_cap_hit.store(true, Ordering::Relaxed);
            state.is_paused.store(true, Ordering::Relaxed);
            state.push_log(
                crate::state::LogLevel::Warn,
                format!("⚠ Daily risk cap hit (${:.2} loss) — auto-paused", daily_pnl.abs()),
            );
            return None;
        }

        // 3. Sanity check entry price
        if entry_price <= 0.01 || entry_price >= 0.99 {
            return None;
        }

        // 4. Position sizing: risk = max_risk_fraction * balance
        let risk_usd = balance * self.max_risk_per_trade;
        let size = (risk_usd / entry_price).min(self.max_position_usd);

        if size < 0.01 {
            return None;
        }

        Some(size)
    }

    /// Reset daily counters — call at midnight or session start.
    pub fn reset_daily(&self, state: &AppState) {
        *state.daily_pnl.write() = 0.0;
        state.daily_cap_hit.store(false, Ordering::Relaxed);
        state.is_paused.store(false, Ordering::Relaxed);
        state.push_log(crate::state::LogLevel::Info, "Daily risk counters reset");
    }
}
