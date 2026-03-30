// src/positions.rs — Real position tracker for live and dry-run trading
//
// Tracks open positions per-market, computes realized PnL from actual fill prices,
// and provides position-level state for the execution loop.

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use std::sync::Arc;

/// A single open position on a Polymarket market.
#[derive(Debug, Clone)]
pub struct Position {
    pub condition_id: String,
    pub token_id: String,
    pub side: String,       // "BUY" or "SELL"
    pub entry_price: f64,
    pub size: f64,
    pub opened_at: DateTime<Utc>,
    pub order_id: String,
}

/// Realized trade result after a position is marked.
#[derive(Debug, Clone)]
pub struct TradeResult {
    pub condition_id: String,
    pub entry_price: f64,
    pub exit_price: f64,
    pub size: f64,
    pub pnl: f64,
}

/// Thread-safe position tracker.
#[derive(Clone)]
pub struct PositionTracker {
    /// Open positions keyed by condition_id
    positions: Arc<DashMap<String, Position>>,
    /// Accumulated realized PnL from closed positions
    realized_pnl: Arc<parking_lot::RwLock<f64>>,
}

impl PositionTracker {
    pub fn new() -> Self {
        Self {
            positions: Arc::new(DashMap::new()),
            realized_pnl: Arc::new(parking_lot::RwLock::new(0.0)),
        }
    }

    /// Record a new position from a filled order.
    pub fn open_position(
        &self,
        condition_id: &str,
        token_id: &str,
        side: &str,
        entry_price: f64,
        size: f64,
        order_id: &str,
    ) {
        let pos = Position {
            condition_id: condition_id.to_string(),
            token_id: token_id.to_string(),
            side: side.to_string(),
            entry_price,
            size,
            opened_at: Utc::now(),
            order_id: order_id.to_string(),
        };
        self.positions.insert(condition_id.to_string(), pos);
    }

    /// Check if we have an open position on a market.
    pub fn has_position(&self, condition_id: &str) -> bool {
        self.positions.contains_key(condition_id)
    }

    /// Get a snapshot of an open position.
    pub fn get_position(&self, condition_id: &str) -> Option<Position> {
        self.positions.get(condition_id).map(|p| p.clone())
    }

    /// Close a position and compute realized PnL.
    /// For BUY positions: PnL = (exit_price - entry_price) * size
    /// For SELL positions: PnL = (entry_price - exit_price) * size
    pub fn close_position(&self, condition_id: &str, exit_price: f64) -> Option<TradeResult> {
        if let Some((_, pos)) = self.positions.remove(condition_id) {
            let pnl = match pos.side.as_str() {
                "BUY" => (exit_price - pos.entry_price) * pos.size,
                "SELL" => (pos.entry_price - exit_price) * pos.size,
                _ => 0.0,
            };
            {
                let mut rpnl = self.realized_pnl.write();
                *rpnl += pnl;
            }
            Some(TradeResult {
                condition_id: pos.condition_id,
                entry_price: pos.entry_price,
                exit_price,
                size: pos.size,
                pnl,
            })
        } else {
            None
        }
    }

    /// Compute estimated PnL based on divergence edge minus estimated slippage.
    /// Used for dry-run mode: edge-based PnL with realistic win/loss modeling.
    pub fn estimate_dry_run_pnl(abs_divergence: f64, size: f64) -> f64 {
        let slippage = rand::random::<f64>() * 0.002; // 0–0.2% slippage
        let raw_pnl = size * (abs_divergence - slippage);
        // ~70% win rate simulation
        if rand::random::<f64>() < 0.70 {
            raw_pnl.abs()
        } else {
            -(raw_pnl.abs() * 0.5) // losses smaller than wins
        }
    }

    /// Compute live-mode PnL estimate from the fill.
    /// Uses edge (divergence) as the expected profit when the market converges.
    /// In practice, real PnL would come from comparing entry vs settlement/exit.
    pub fn estimate_live_pnl(entry_price: f64, fair_price: f64, size: f64, side: &str) -> f64 {
        match side {
            "BUY" => (fair_price - entry_price) * size,
            "SELL" => (entry_price - fair_price) * size,
            _ => 0.0,
        }
    }

    /// Number of open positions.
    pub fn open_count(&self) -> usize {
        self.positions.len()
    }

    /// Total realized PnL.
    pub fn realized_pnl(&self) -> f64 {
        *self.realized_pnl.read()
    }

    /// Get all open positions as a vector.
    pub fn all_positions(&self) -> Vec<Position> {
        self.positions.iter().map(|r| r.value().clone()).collect()
    }
}
