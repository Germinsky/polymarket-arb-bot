// src/strategies/arbitrage.rs — Multi-asset divergence detection and signal generation
//
// Uses a log-normal fair probability model (Black-Scholes style) to estimate
// what a binary option SHOULD be priced at vs what Polymarket shows.
// Supports: BTC, ETH, Crude Oil (CL), Gold (GC), Fed rate decisions.
// When divergence > threshold → signal generated.

use crate::api::polymarket::Side;
use crate::state::{AppState, Asset, MarketSnapshot};

/// An actionable arbitrage signal.
#[derive(Debug, Clone)]
pub struct ArbitrageSignal {
    pub condition_id: String,
    pub question: String,
    pub token_id: String,
    pub side: Side,
    pub market_price: f64,
    pub fair_price: f64,
    pub divergence: f64,      // signed: positive = underpriced, negative = overpriced
    pub abs_divergence: f64,
    pub entry_price: f64,     // price we'd pay/receive
    pub asset: Asset,
}

// ── Fair probability model ───────────────────────────────────────────────────

const SECONDS_PER_YEAR: f64 = 365.25 * 86400.0;

/// Base annualized vol per asset class (moderate baseline).
fn base_vol(asset: Asset) -> f64 {
    match asset {
        Asset::Btc => 0.60,       // BTC: 50-80% realized
        Asset::Eth => 0.70,       // ETH: typically higher than BTC
        Asset::CrudeOil => 0.35,  // CL: 25-45% typically
        Asset::Gold => 0.15,      // GC: 12-20% typically
        Asset::FedRate => 0.10,   // Fed rate implieds: very low
    }
}

/// Apply a term-structure adjustment per asset.
fn adjusted_vol(asset: Asset, seconds_to_expiry: f64) -> f64 {
    let t_years = seconds_to_expiry / SECONDS_PER_YEAR;
    let bv = base_vol(asset);

    let term_mult = match asset {
        Asset::Btc | Asset::Eth => {
            // Crypto: short-term vol bump, long-term mean reversion
            if t_years < 1.0 / 12.0 {
                1.3 - 0.3 * (t_years * 12.0)
            } else if t_years < 0.25 {
                1.0 + 0.1 * (1.0 - (t_years - 1.0 / 12.0) * 6.0)
            } else {
                0.95
            }
        }
        Asset::CrudeOil => {
            // Oil: geopolitical spikes make short-term vol higher
            if t_years < 1.0 / 12.0 {
                1.4 - 0.4 * (t_years * 12.0)
            } else if t_years < 0.25 {
                1.05
            } else {
                0.90  // strong mean reversion in oil
            }
        }
        Asset::Gold => {
            // Gold: relatively flat term structure
            if t_years < 1.0 / 12.0 {
                1.15
            } else {
                1.0
            }
        }
        Asset::FedRate => {
            // Fed: vol increases with time (more meetings = more uncertainty)
            if t_years < 0.1 {
                0.5   // next meeting: well-telegraphed
            } else {
                1.0 + 0.3 * t_years.min(1.0)  // increases over time
            }
        }
    };

    bv * term_mult.max(0.5).min(2.0)
}

/// Estimate the probability that `spot` will be above `threshold` in `seconds_to_expiry`.
/// Uses lognormal model: P(S_T > K) = Φ(d2) where d2 = (ln(S/K) - σ²T/2) / (σ√T)
pub fn fair_probability(spot: f64, threshold: f64, seconds_to_expiry: f64, asset: Asset) -> f64 {
    if spot <= 0.0 || threshold <= 0.0 || seconds_to_expiry <= 0.0 {
        return if spot >= threshold { 1.0 } else { 0.0 };
    }
    let t = seconds_to_expiry / SECONDS_PER_YEAR;
    let vol = adjusted_vol(asset, seconds_to_expiry);
    let sigma_sqrt_t = vol * t.sqrt();
    if sigma_sqrt_t < 1e-12 {
        return if spot >= threshold { 1.0 } else { 0.0 };
    }
    let d2 = ((spot / threshold).ln() - 0.5 * vol * vol * t) / sigma_sqrt_t;
    normal_cdf(d2)
}

/// Touch/barrier probability that S touches barrier K at any point before T.
/// Uses the reflection principle for GBM with zero drift.
///
/// For up-touch (K > S): P(max S_t ≥ K) = Φ(d₂) + (S/K)·Φ(d₁)
/// For down-touch (K < S): P(min S_t ≤ K) = Φ(-d₂') + (S/K)·Φ(-d₁')
///
/// Always returns the probability that the barrier is hit (regardless of direction).
pub fn touch_probability(spot: f64, barrier: f64, seconds_to_expiry: f64, asset: Asset) -> f64 {
    if spot <= 0.0 || barrier <= 0.0 || seconds_to_expiry <= 0.0 {
        return if (barrier > spot && spot >= barrier) || (barrier < spot && spot <= barrier) { 1.0 } else { 0.0 };
    }
    if (spot - barrier).abs() / spot < 1e-6 {
        return 1.0; // already at barrier
    }
    let t = seconds_to_expiry / SECONDS_PER_YEAR;
    let vol = adjusted_vol(asset, seconds_to_expiry);
    let sigma_sqrt_t = vol * t.sqrt();
    if sigma_sqrt_t < 1e-12 {
        return if barrier <= spot { 1.0 } else { 0.0 };
    }
    let vol_sq_t = vol * vol * t;

    if barrier > spot {
        // Up-touch: P(max S_t ≥ K)
        let ln_s_k = (spot / barrier).ln(); // negative
        let d1 = (ln_s_k + vol_sq_t / 2.0) / sigma_sqrt_t;
        let d2 = (ln_s_k - vol_sq_t / 2.0) / sigma_sqrt_t;
        let p = normal_cdf(d2) + (spot / barrier) * normal_cdf(d1);
        p.min(1.0).max(0.0)
    } else {
        // Down-touch: P(min S_t ≤ K)
        let ln_s_k = (spot / barrier).ln(); // positive
        let d1_prime = (ln_s_k + vol_sq_t / 2.0) / sigma_sqrt_t;
        let d2_prime = (ln_s_k - vol_sq_t / 2.0) / sigma_sqrt_t;
        let p = normal_cdf(-d2_prime) + (spot / barrier) * normal_cdf(-d1_prime);
        p.min(1.0).max(0.0)
    }
}

/// Standard normal CDF using Abramowitz & Stegun approximation (max error 7.5e-8).
fn normal_cdf(x: f64) -> f64 {
    let a1 = 0.254829592;
    let a2 = -0.284496736;
    let a3 = 1.421413741;
    let a4 = -1.453152027;
    let a5 = 1.061405429;
    let p  = 0.3275911;

    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let x_abs = x.abs();
    let t = 1.0 / (1.0 + p * x_abs);
    let y = 1.0 - (((((a5 * t + a4) * t) + a3) * t + a2) * t + a1) * t * (-x_abs * x_abs / 2.0).exp();
    0.5 * (1.0 + sign * y)
}

// ── Market type detection ────────────────────────────────────────────────────

/// Whether the market settles as a barrier (touch at ANY point) or
/// European (level at a specific date).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketStyle {
    Touch,     // "hit", "reach", "dip to" — barrier touched at any point
    European,  // "above X on [date]", "be X% at end of" — level at specific date
}

/// Determine if a market is touch/barrier or European-style from question text.
pub fn parse_market_style(question: &str) -> MarketStyle {
    let q = question.to_lowercase();
    // European: "above $X on [date]" or "be X% at the end of"
    if (q.contains("above") || q.contains("below")) && q.contains(" on ") {
        return MarketStyle::European;
    }
    if q.contains(" be ") && (q.contains("at the end") || q.contains("at end")) {
        return MarketStyle::European;
    }
    // Everything else is touch: "hit", "reach", "dip to"
    MarketStyle::Touch
}

// ── Question parser ──────────────────────────────────────────────────────────

/// Extract the dollar threshold from a Polymarket question (any asset).
///
/// Handles patterns like:
///   "Will Bitcoin hit $150k by March 31, 2026?"  → 150_000
///   "Will BTC be above $200K on April 2?"        → 200_000
///   "Will bitcoin hit $1m before GTA VI?"         → 1_000_000
///   "Bitcoin above $150,000 by end of year?"      → 150_000
///   "Will Crude Oil (CL) hit (HIGH) $120 ..."    → 120
///   "Will Gold (GC) hit (HIGH) $3,400 ..."       → 3_400
///   "Will the price of Bitcoin be above $66,000?" → 66_000
pub fn parse_threshold(question: &str, asset: Asset) -> Option<f64> {
    let q = question.replace(',', "");

    // For Fed rate: look for percentage patterns
    if asset == Asset::FedRate {
        return parse_fed_rate_threshold(&q);
    }

    let dollar_idx = q.find('$')?;
    let after = &q[dollar_idx + 1..];

    // Extract the numeric part (digits and decimal point)
    let num_str: String = after
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    if num_str.is_empty() {
        return None;
    }
    let base: f64 = num_str.parse().ok()?;

    // Check for suffix multiplier immediately after the number
    let suffix_char = after.chars().nth(num_str.len());
    let multiplier = match suffix_char {
        Some('k' | 'K') => 1_000.0,
        Some('m' | 'M') => 1_000_000.0,
        Some('b' | 'B') => 1_000_000_000.0,
        _ => 1.0,
    };

    let result = base * multiplier;

    // Sanity check per asset
    let valid = match asset {
        Asset::Btc => result >= 1_000.0 && result <= 10_000_000.0,
        Asset::Eth => result >= 10.0 && result <= 1_000_000.0,
        Asset::CrudeOil => result >= 10.0 && result <= 500.0,
        Asset::Gold => result >= 500.0 && result <= 100_000.0,
        Asset::FedRate => false, // handled above
    };

    if valid { Some(result) } else { None }
}

/// Parse Fed rate threshold from question text.
/// E.g. "Will the upper bound ... be 3.0%?" → 3.0
///      "decrease by 25 bps" → current_rate - 0.25
fn parse_fed_rate_threshold(question: &str) -> Option<f64> {
    let q = question.to_lowercase();

    // Pattern: "X.XX%" or "X%" — direct rate target
    if let Some(pct_idx) = q.find('%') {
        let before = &q[..pct_idx];
        // Walk backwards to find the number
        let num_str: String = before.chars().rev()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect::<String>()
            .chars().rev().collect();
        if let Ok(rate) = num_str.parse::<f64>() {
            if rate >= 0.0 && rate <= 20.0 {
                return Some(rate);
            }
        }
    }

    // Pattern: "by 25 bps" or "by 50+ bps" — delta from current rate
    if q.contains("bps") {
        // We'll return the bps as a threshold (handled specially in the scanner)
        if let Some(bps_idx) = q.find("bps") {
            let before = q[..bps_idx].trim();
            let num_str: String = before.chars().rev()
                .take_while(|c| c.is_ascii_digit() || *c == '+')
                .collect::<String>()
                .chars().rev()
                .filter(|c| c.is_ascii_digit())
                .collect();
            if let Ok(bps) = num_str.parse::<f64>() {
                // Return as percentage (25 bps = 0.25%)
                return Some(bps / 100.0);
            }
        }
    }

    None
}

/// Backward-compatible alias for BTC-specific parsing.
pub fn parse_btc_threshold(question: &str) -> Option<f64> {
    parse_threshold(question, Asset::Btc)
}

// ── Direction parsing ────────────────────────────────────────────────────────

/// Whether the market pays out when the asset goes ABOVE or BELOW the threshold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Above, // "above", "reach", "hit (HIGH)", "increase"
    Below, // "dip to", "hit (LOW)", "decrease", "lower", "cut"
}

/// Detect whether a market question is asking about going above or below a threshold.
pub fn parse_direction(question: &str) -> Direction {
    let q = question.to_lowercase();

    // Explicit below patterns (checked first — more specific)
    if q.contains("dip to") || q.contains("dip below") {
        return Direction::Below;
    }
    if q.contains("hit (low)") || q.contains("hit(low)") {
        return Direction::Below;
    }
    if q.contains("decrease") || q.contains("cut") {
        return Direction::Below;
    }
    if q.contains("lower bound reach") && (q.contains("or lower") || q.contains("or below")) {
        return Direction::Below;
    }
    // "≤" or "or lower" in Fed rate context
    if q.contains('≤') || (q.contains("or lower") && !q.contains("lower bound reach")) {
        return Direction::Below;
    }

    // Above patterns (default for most markets)
    // "above", "reach", "hit (HIGH)", "increase", "or higher"
    Direction::Above
}

/// Estimate seconds to expiry from the end_date string.
/// Returns None for already-expired markets.
pub fn seconds_to_expiry(end_date: Option<&str>) -> Option<f64> {
    if let Some(date_str) = end_date {
        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(date_str) {
            let diff = dt.signed_duration_since(chrono::Utc::now());
            let secs = diff.num_seconds();
            return if secs > 0 { Some(secs as f64) } else { None };
        }
        // Try ISO date without time (YYYY-MM-DD)
        if let Ok(nd) = chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
            let dt = nd.and_hms_opt(23, 59, 59)
                .unwrap()
                .and_utc();
            let diff = dt.signed_duration_since(chrono::Utc::now());
            let secs = diff.num_seconds();
            return if secs > 0 { Some(secs as f64) } else { None };
        }
    }
    None
}

// ── Scanner ──────────────────────────────────────────────────────────────────

/// Scan all known snapshots for divergence against live asset prices.
/// Returns signals sorted by |divergence| descending.
pub fn scan_divergences(
    state: &AppState,
    markets: &dashmap::DashMap<String, MarketSnapshot>,
    min_divergence: f64,
) -> Vec<ArbitrageSignal> {
    let mut signals = Vec::new();

    for entry in markets.iter() {
        let snap = entry.value();
        let asset = snap.asset;

        // Get spot price for this asset
        let spot = state.best_price(asset);
        if spot <= 0.0 && asset != Asset::FedRate {
            continue;
        }

        // Skip Fed rate markets — lognormal model is inappropriate for discrete
        // rate decisions; these markets ask about specific rate targets, not above/below
        if asset == Asset::FedRate {
            continue;
        }

        // Parse threshold — skip markets with no parseable threshold
        let threshold = match parse_threshold(&snap.question, asset) {
            Some(t) => t,
            None => continue,
        };

        // Parse expiry — skip expired or undated markets
        let ttx = match seconds_to_expiry(snap.end_date.as_deref()) {
            Some(t) => t,
            None => continue,
        };

        // Skip markets expiring within 1 hour (too close, high gamma risk)
        if ttx < 3600.0 {
            continue;
        }

        // Detect question characteristics
        let direction = parse_direction(&snap.question);
        let style = parse_market_style(&snap.question);

        // Choose probability model based on market style
        let prob_above = match style {
            MarketStyle::European => {
                // European: P(S_T > K) at expiry
                fair_probability(spot, threshold, ttx, asset)
            }
            MarketStyle::Touch => {
                // Touch: P(S touches K at any point before expiry)
                // Direction determines whether it's an up-touch or down-touch
                match direction {
                    Direction::Above => {
                        // "reach $X", "hit (HIGH) $X" — up-touch barrier
                        if threshold > spot {
                            touch_probability(spot, threshold, ttx, asset)
                        } else {
                            1.0 // already above barrier
                        }
                    }
                    Direction::Below => {
                        // "dip to $X", "hit (LOW) $X" — down-touch barrier
                        if threshold < spot {
                            touch_probability(spot, threshold, ttx, asset)
                        } else {
                            1.0 // already below barrier
                        }
                    }
                }
            }
        };

        // fair_yes: probability the "Yes" outcome occurs
        let fair_yes = match direction {
            Direction::Above => match style {
                MarketStyle::European => prob_above, // P(S > K at T)
                MarketStyle::Touch => prob_above,    // P(S touches K) — already computed correctly
            },
            Direction::Below => match style {
                MarketStyle::European => 1.0 - prob_above, // P(S < K at T) = 1 - P(S > K)
                MarketStyle::Touch => prob_above,          // already gives P(touches below)
            },
        };
        let fair_no = 1.0 - fair_yes;

        let market_yes = (snap.yes_bid + snap.yes_ask) / 2.0;
        let market_no = (snap.no_bid + snap.no_ask) / 2.0;

        let yes_spread = snap.yes_ask - snap.yes_bid;
        let no_spread = snap.no_ask - snap.no_bid;

        // Compare YES side — only if market is reasonably liquid
        if market_yes > 0.02 && market_yes < 0.98 && yes_spread < 0.10 {
            let div = fair_yes - market_yes;
            if div.abs() > min_divergence {
                let (side, entry_price, token_id) = if div > 0.0 {
                    (Side::Buy, snap.yes_ask, snap.token_id_yes.clone())
                } else {
                    (Side::Buy, snap.no_ask, snap.token_id_no.clone())
                };
                signals.push(ArbitrageSignal {
                    condition_id: snap.condition_id.clone(),
                    question: snap.question.clone(),
                    token_id,
                    side,
                    market_price: market_yes,
                    fair_price: fair_yes,
                    divergence: div,
                    abs_divergence: div.abs(),
                    entry_price,
                    asset,
                });
            }
        }

        // Compare NO side — avoid duplicate for same condition
        if market_no > 0.02 && market_no < 0.98 && no_spread < 0.10 {
            let div = fair_no - market_no;
            if div.abs() > min_divergence {
                let (side, entry_price, token_id) = if div > 0.0 {
                    (Side::Buy, snap.no_ask, snap.token_id_no.clone())
                } else {
                    (Side::Buy, snap.yes_ask, snap.token_id_yes.clone())
                };
                if !signals.iter().any(|s| s.condition_id == snap.condition_id) {
                    signals.push(ArbitrageSignal {
                        condition_id: snap.condition_id.clone(),
                        question: snap.question.clone(),
                        token_id,
                        side,
                        market_price: market_no,
                        fair_price: fair_no,
                        divergence: div,
                        abs_divergence: div.abs(),
                        entry_price,
                        asset,
                    });
                }
            }
        }
    }

    signals.sort_by(|a, b| b.abs_divergence.partial_cmp(&a.abs_divergence).unwrap_or(std::cmp::Ordering::Equal));
    signals
}

/// Diagnostic info for a single market — shows why it was included or skipped.
#[derive(Debug, Clone, serde::Serialize)]
pub struct MarketDiag {
    pub question: String,
    pub asset: Asset,
    pub threshold: Option<f64>,
    pub spot_price: f64,
    pub seconds_to_expiry: Option<f64>,
    pub fair_yes: Option<f64>,
    pub market_yes: f64,
    pub yes_spread: f64,
    pub skip_reason: Option<String>,
}

/// Run diagnostics on all markets — shows filtering decisions for debugging.
pub fn scan_diagnostics(
    state: &AppState,
    markets: &dashmap::DashMap<String, MarketSnapshot>,
) -> Vec<MarketDiag> {
    let mut diags = Vec::new();
    for entry in markets.iter() {
        let snap = entry.value();
        let asset = snap.asset;
        let spot = state.best_price(asset);
        let threshold = parse_threshold(&snap.question, asset);
        let ttx = seconds_to_expiry(snap.end_date.as_deref());
        let yes_spread = snap.yes_ask - snap.yes_bid;
        let market_yes = (snap.yes_bid + snap.yes_ask) / 2.0;

        let skip_reason = if threshold.is_none() {
            Some("no_threshold".into())
        } else if spot <= 0.0 && asset != Asset::FedRate {
            Some("no_spot_price".into())
        } else if ttx.is_none() {
            Some("expired_or_no_date".into())
        } else if ttx.unwrap() < 3600.0 {
            Some("expiry_too_close".into())
        } else if yes_spread >= 0.10 {
            Some(format!("illiquid_spread_{:.3}", yes_spread))
        } else if market_yes <= 0.02 || market_yes >= 0.98 {
            Some("extreme_price".into())
        } else {
            None
        };

        let fair_yes = if let (Some(t), Some(ttx_val)) = (threshold, ttx) {
            let direction = parse_direction(&snap.question);
            let style = parse_market_style(&snap.question);
            if spot <= 0.0 && asset != Asset::FedRate {
                None
            } else {
                let p = match style {
                    MarketStyle::European => {
                        let pa = fair_probability(spot, t, ttx_val, asset);
                        match direction {
                            Direction::Above => pa,
                            Direction::Below => 1.0 - pa,
                        }
                    }
                    MarketStyle::Touch => {
                        match direction {
                            Direction::Above => {
                                if t > spot { touch_probability(spot, t, ttx_val, asset) } else { 1.0 }
                            }
                            Direction::Below => {
                                if t < spot { touch_probability(spot, t, ttx_val, asset) } else { 1.0 }
                            }
                        }
                    }
                };
                Some(p)
            }
        } else {
            None
        };

        diags.push(MarketDiag {
            question: snap.question.clone(),
            asset,
            threshold,
            spot_price: spot,
            seconds_to_expiry: ttx,
            fair_yes,
            market_yes,
            yes_spread,
            skip_reason,
        });
    }
    diags
}
