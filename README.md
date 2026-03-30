# Polymarket Multi-Asset Arbitrage Bot

A high-performance Rust arbitrage bot that detects price divergences between real-time spot feeds and Polymarket prediction market prices, with a Next.js web dashboard for monitoring.

## How It Works

```
 Binance WS ──┐                                    ┌── Polymarket CLOB
 Coinbase WS ─┤   ┌────────────┐  ┌───────────┐   │   (SDK v0.4.4)
 CoinGecko   ─┼──▶│  AppState  │──│ Arb Scan  │───┤
 Yahoo Fin.  ─┤   │  (shared)  │  │ (~10/sec) │   │   ┌─────────────┐
               │   └────────────┘  └─────┬─────┘   └──▶│ Trade       │
               │                         │             │ Journal     │
               │                    ┌────▼────┐        │ (SQLite+CSV)│
               │                    │ Risk Mgr│        └─────────────┘
               │                    └─────────┘
               │
               │   ┌──────────┐    ┌──────────────┐
               └──▶│ TUI      │    │ Next.js      │
                   │ Dashboard │    │ Dashboard    │
                   │ (ratatui) │    │ :3000        │
                   └──────────┘    └──────────────┘
```

1. **Ingests spot prices** via Binance + Coinbase WebSockets, CoinGecko REST, and Yahoo Finance
2. **Polls Polymarket** Gamma API for active markets across BTC, ETH, Crude Oil, and Gold
3. **Computes fair value** using touch probability (reflection principle) and European (Black-Scholes d2) models
4. **Detects divergence** between fair price and market price across all asset classes
5. **Executes trades** when divergence exceeds threshold, with per-trade risk limits and cooldowns
6. **Signs orders** via the official `polymarket-client-sdk` (EIP-712 + HMAC L2 auth)

## Supported Assets

| Asset | Spot Source | Model |
|-------|------------|-------|
| BTC | Binance WS, Coinbase WS, CoinGecko | Touch + European, 65% vol |
| ETH | Binance WS, Coinbase WS, CoinGecko | Touch + European, 75% vol |
| Crude Oil (CL) | Yahoo Finance | Touch + European, 35% vol |
| Gold (GC) | Yahoo Finance | Touch + European, 20% vol |

## Project Structure

```
├── polymarket-btc-arb-rust/    # Rust bot + API server
│   ├── src/
│   │   ├── main.rs             # Entry point, task orchestration
│   │   ├── config.rs           # .env config loader
│   │   ├── state.rs            # Shared state (Arc-based)
│   │   ├── positions.rs        # Live position tracker
│   │   ├── api/
│   │   │   ├── polymarket.rs   # CLOB client + Gamma market discovery
│   │   │   ├── price_feeds.rs  # Binance, Coinbase, CoinGecko, Yahoo
│   │   │   └── server.rs       # Axum API server (:3001)
│   │   ├── strategies/
│   │   │   └── arbitrage.rs    # Fair probability models + scanner
│   │   ├── risk/
│   │   │   └── manager.rs      # Pre-trade risk checks + sizing
│   │   ├── journal/
│   │   │   └── logger.rs       # SQLite + CSV trade journal
│   │   └── dashboard/
│   │       └── tui.rs          # Terminal UI (ratatui)
│   └── data/                   # Trade logs (trades.csv, trades.db)
│
└── arb-dashboard/              # Next.js web frontend
    └── src/
        ├── app/page.tsx        # Main dashboard page
        ├── components/         # EquityChart, MarketsTable, ExecutionLog, etc.
        ├── hooks/useBotData.ts # Polling hook for API data
        └── lib/api.ts          # API client for Rust server
```

## Quick Start

### Prerequisites

- Rust 1.88+ (`rustup update`)
- Node.js 18+ (for the web dashboard)

### 1. Configure

```bash
cd polymarket-btc-arb-rust
cp .env.example .env
```

Edit `.env`:
```bash
# Only your private key is needed — SDK derives API credentials automatically
POLYMARKET_PRIVATE_KEY=0xYOUR_PRIVATE_KEY_HERE

# Safety first
DRY_RUN=true
```

### 2. Build & Run the Bot

```bash
cargo build --release
cargo run --release
```

The bot starts in **TUI mode** by default. For headless (web-only):
```bash
HEADLESS=1 cargo run --release
```

### 3. Run the Web Dashboard

```bash
cd arb-dashboard
npm install
npm run dev
```

Open [http://localhost:3000](http://localhost:3000) — connects to the Rust API on port 3001.

## Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `POLYMARKET_PRIVATE_KEY` | — | Ethereum private key (0x-prefixed hex) |
| `DRY_RUN` | `true` | `false` for live trading |
| `MAX_RISK_PER_TRADE` | `0.005` | 0.5% of balance per trade |
| `DAILY_RISK_CAP` | `0.02` | 2% daily loss → auto-pause |
| `MIN_DIVERGENCE` | `0.003` | 0.3% min divergence to trigger |
| `MAX_POSITION_USD` | `100` | Hard cap per position |
| `INITIAL_BALANCE` | `1000` | Starting balance for tracking |
| `POLL_INTERVAL_MS` | `3000` | Market refresh interval |
| `SCAN_INTERVAL_MS` | `100` | Arb scan frequency (~10/sec) |

## Going Live

1. Set `POLYMARKET_PRIVATE_KEY` to your real wallet key
2. Ensure the wallet has **USDC on Polygon** and has approved Polymarket contracts
3. Set `DRY_RUN=false`
4. Start with small `MAX_POSITION_USD` and `INITIAL_BALANCE`

The SDK authenticates automatically — no manual API key management needed.

## TUI Keybindings

| Key | Action |
|-----|--------|
| `q` / `Esc` | Quit |
| `p` | Pause / resume |
| `r` | Reset daily PnL |

## Tech Stack

- **Rust** — tokio async runtime, 4 worker threads
- **polymarket-client-sdk v0.4.4** — official SDK for EIP-712 signing + CLOB API
- **ratatui** — terminal dashboard
- **axum** — API server for web frontend
- **Next.js 16** / React 19 / Tailwind v4 / Recharts — web dashboard

## License

MIT
