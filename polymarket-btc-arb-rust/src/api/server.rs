// src/api/server.rs — Lightweight axum HTTP + WebSocket server
// Exposes bot state to the web frontend on port 3001.

use crate::state::{AppState, LogLevel};
use crate::strategies::arbitrage;
use axum::{
    extract::{State, WebSocketUpgrade},
    http::Method,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::Serialize;
use std::sync::atomic::Ordering;
use tower_http::cors::{Any, CorsLayer};

// ── JSON payloads ────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct StatusPayload {
    balance: f64,
    daily_pnl: f64,
    total_pnl: f64,
    wins: u64,
    losses: u64,
    total_trades: u64,
    win_rate: f64,
    orders: u64,
    latency_us: u64,
    btc_price: f64,
    btc_binance: f64,
    btc_coinbase: f64,
    btc_coingecko: f64,
    eth_price: f64,
    oil_price: f64,
    gold_price: f64,
    fed_rate: f64,
    is_paused: bool,
    daily_cap_hit: bool,
    markets_count: usize,
}

#[derive(Serialize)]
struct LogEntryPayload {
    ts: String,
    level: String,
    message: String,
}

#[derive(Serialize)]
struct EquityPointPayload {
    ts: String,
    equity: f64,
}

#[derive(Serialize)]
struct MarketPayload {
    condition_id: String,
    question: String,
    asset: String,
    yes_bid: f64,
    yes_ask: f64,
    no_bid: f64,
    no_ask: f64,
    end_date: Option<String>,
}

// ── Router ───────────────────────────────────────────────────────────────────

pub fn build_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST])
        .allow_headers(Any);

    Router::new()
        .route("/api/status", get(get_status))
        .route("/api/logs", get(get_logs))
        .route("/api/equity", get(get_equity))
        .route("/api/markets", get(get_markets))
        .route("/api/control/pause", get(toggle_pause))
        .route("/api/control/reset-daily", get(reset_daily))
        .route("/api/diagnostics", get(get_diagnostics))
        .route("/api/ws", get(ws_handler))
        .layer(cors)
        .with_state(state)
}

// ── Handlers ─────────────────────────────────────────────────────────────────

async fn get_status(State(state): State<AppState>) -> Json<StatusPayload> {
    Json(StatusPayload {
        balance: *state.balance.read(),
        daily_pnl: *state.daily_pnl.read(),
        total_pnl: *state.total_pnl.read(),
        wins: state.wins.load(Ordering::Relaxed),
        losses: state.losses.load(Ordering::Relaxed),
        total_trades: state.total_trades(),
        win_rate: state.win_rate(),
        orders: state.orders.load(Ordering::Relaxed),
        latency_us: state.last_latency_us.load(Ordering::Relaxed),
        btc_price: state.best_btc_price(),
        btc_binance: *state.btc_binance.read(),
        btc_coinbase: *state.btc_coinbase.read(),
        btc_coingecko: *state.btc_coingecko.read(),
        eth_price: *state.eth_price.read(),
        oil_price: *state.oil_price.read(),
        gold_price: *state.gold_price.read(),
        fed_rate: *state.fed_rate.read(),
        is_paused: state.is_paused.load(Ordering::Relaxed),
        daily_cap_hit: state.daily_cap_hit.load(Ordering::Relaxed),
        markets_count: state.markets.len(),
    })
}

async fn get_logs(State(state): State<AppState>) -> Json<Vec<LogEntryPayload>> {
    let log = state.log.read();
    let entries: Vec<LogEntryPayload> = log
        .iter()
        .map(|e| LogEntryPayload {
            ts: e.ts.to_rfc3339(),
            level: e.level.label().trim().to_string(),
            message: e.message.clone(),
        })
        .collect();
    Json(entries)
}

async fn get_equity(State(state): State<AppState>) -> Json<Vec<EquityPointPayload>> {
    let equity = state.equity.read();
    let points: Vec<EquityPointPayload> = equity
        .iter()
        .map(|p| EquityPointPayload {
            ts: p.ts.to_rfc3339(),
            equity: p.equity,
        })
        .collect();
    Json(points)
}

async fn get_markets(State(state): State<AppState>) -> Json<Vec<MarketPayload>> {
    let markets: Vec<MarketPayload> = state
        .markets
        .iter()
        .map(|entry| {
            let s = entry.value();
            MarketPayload {
                condition_id: s.condition_id.clone(),
                question: s.question.clone(),
                asset: s.asset.label().to_string(),
                yes_bid: s.yes_bid,
                yes_ask: s.yes_ask,
                no_bid: s.no_bid,
                no_ask: s.no_ask,
                end_date: s.end_date.clone(),
            }
        })
        .collect();
    Json(markets)
}

async fn get_diagnostics(State(state): State<AppState>) -> Json<Vec<arbitrage::MarketDiag>> {
    Json(arbitrage::scan_diagnostics(&state, &state.markets))
}

async fn toggle_pause(State(state): State<AppState>) -> Json<serde_json::Value> {
    let was_paused = state.is_paused.load(Ordering::Relaxed);
    state.is_paused.store(!was_paused, Ordering::Relaxed);
    let new_state = if was_paused { "resumed" } else { "paused" };
    state.push_log(
        LogLevel::Info,
        format!("Bot {} via web UI", new_state),
    );
    Json(serde_json::json!({ "paused": !was_paused }))
}

async fn reset_daily(State(state): State<AppState>) -> Json<serde_json::Value> {
    *state.daily_pnl.write() = 0.0;
    state.daily_cap_hit.store(false, Ordering::Relaxed);
    state.is_paused.store(false, Ordering::Relaxed);
    state.push_log(LogLevel::Info, "Daily counters reset via web UI");
    Json(serde_json::json!({ "reset": true }))
}

// ── WebSocket: streams status updates at ~2Hz ────────────────────────────────

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |mut socket| async move {
        loop {
            if state.is_shutdown.load(Ordering::Relaxed) {
                break;
            }

            let payload = serde_json::json!({
                "balance": *state.balance.read(),
                "daily_pnl": *state.daily_pnl.read(),
                "total_pnl": *state.total_pnl.read(),
                "wins": state.wins.load(Ordering::Relaxed),
                "losses": state.losses.load(Ordering::Relaxed),
                "win_rate": state.win_rate(),
                "orders": state.orders.load(Ordering::Relaxed),
                "latency_us": state.last_latency_us.load(Ordering::Relaxed),
                "btc_price": state.best_btc_price(),
                "is_paused": state.is_paused.load(Ordering::Relaxed),
                "markets_count": state.markets.len(),
            });

            let msg = axum::extract::ws::Message::Text(payload.to_string().into());
            if socket.send(msg).await.is_err() {
                break;
            }

            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    })
}

// ── Start server ─────────────────────────────────────────────────────────────

pub async fn start_server(state: AppState, port: u16) {
    let app = build_router(state.clone());
    let addr = format!("0.0.0.0:{port}");
    state.push_log(LogLevel::Info, format!("Web API server listening on {addr}"));

    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            state.push_log(LogLevel::Error, format!("API server bind failed: {e}"));
            return;
        }
    };

    if let Err(e) = axum::serve(listener, app).await {
        state.push_log(LogLevel::Error, format!("API server error: {e}"));
    }
}
