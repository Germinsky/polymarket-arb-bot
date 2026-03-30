# ⚡ Polymarket BTC Arbitrage Bot

Ultra low-latency Rust bot that detects price divergences between fast BTC feeds (Binance, Coinbase, CoinGecko) and Polymarket's slower BTC prediction contract prices, then executes arbitrage trades when divergence exceeds the configured threshold.

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                        MAIN THREAD                                  │
│                     TUI Dashboard (~30fps)                          │
│  ┌─────────────────────────┬───────────────────────────────────┐   │
│  │   📈 Equity Curve       │   📋 Execution Log               │   │
│  │   (Braille line chart)  │   (Real-time scrolling)           │   │
│  └─────────────────────────┴───────────────────────────────────┘   │
├─────────────────────────────────────────────────────────────────────┤
│                      TOKIO RUNTIME (4 workers)                      │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐              │
│  │ Binance  │ │ Coinbase │ │CoinGecko │ │ Market   │              │
│  │ WS Feed  │ │ WS Feed  │ │ REST     │ │ Poller   │              │
│  └────┬─────┘ └────┬─────┘ └────┬─────┘ └────┬─────┘              │
│       └──────┬──────┘            │             │                    │
│              ▼                   ▼             ▼                    │
│       ┌────────────┐    ┌──────────────┐ ┌──────────┐              │
│       │ AppState   │◄───│ Arb Scanner  │ │ Risk Mgr │              │
│       │ (shared)   │    │ (~10 scans/s)│ │ (checks) │              │
│       └────────────┘    └──────┬───────┘ └──────────┘              │
│                                │                                    │
│                         ┌──────▼───────┐                            │
│                         │ CLOB Client  │──→ Polymarket API          │
│                         └──────┬───────┘                            │
│                                │                                    │
│                         ┌──────▼───────┐                            │
│                         │Trade Journal │──→ SQLite + CSV             │
│                         └──────────────┘                            │
└─────────────────────────────────────────────────────────────────────┘
```

## Quick Start

```bash
# 1. Clone / create project
git clone <this-repo> && cd polymarket-btc-arb-rust

# 2. Configure
cp .env.example .env
# Edit .env with your Polymarket API credentials

# 3. Build (optimised)
cargo build --release

# 4. Run
cargo run --release
```

## Getting API Keys

### Polymarket CLOB API
1. Go to [polymarket.com](https://polymarket.com)
2. Connect your wallet
3. Navigate to **Settings → API Keys**
4. Generate a new key pair
5. Copy the API key, secret, and passphrase into `.env`
6. Your wallet private key goes in `POLYMARKET_PRIVATE_KEY`

### Price Feeds
Binance and Coinbase WebSocket feeds are **free and require no API key**. CoinGecko free tier is used as a fallback.

## TUI Keybindings

| Key | Action |
|-----|--------|
| `q` / `Esc` | Quit (graceful shutdown) |
| `p` | Toggle pause/resume |
| `r` | Reset daily PnL counters |
| `Ctrl+C` | Force quit |

## Risk Management

| Parameter | Default | Description |
|-----------|---------|-------------|
| `MAX_RISK_PER_TRADE` | 0.5% | Max fraction of balance risked per trade |
| `DAILY_RISK_CAP` | 2.0% | Daily loss limit — bot auto-pauses when hit |
| `MIN_DIVERGENCE` | 0.3% | Minimum price divergence to trigger a trade |
| `MAX_POSITION_USD` | $100 | Hard cap on any single position |

## Strategy

The bot uses a **log-normal fair probability model** (Black-Scholes style) to estimate fair prices for Polymarket BTC binary options:

1. **Pull fast BTC prices** from Binance (primary), Coinbase (secondary), CoinGecko (fallback)
2. **Poll Polymarket markets** for all active BTC prediction contracts
3. **Calculate fair value** using the lognormal model: `P(BTC > K) = Φ(d₂)` with 65% annualised vol
4. **Detect divergence** between fair price and Polymarket's market price
5. **Execute trade** when divergence > threshold, expecting convergence
6. **Log everything** to TUI, CSV, and SQLite

## Project Structure

```
src/
├── main.rs              # Entry point, task orchestration
├── config.rs            # .env configuration loader
├── state.rs             # Shared application state (Arc-based)
├── api/
│   ├── mod.rs
│   ├── price_feeds.rs   # Binance, Coinbase, CoinGecko feeds
│   └── polymarket.rs    # CLOB REST client with HMAC auth
├── strategies/
│   ├── mod.rs
│   └── arbitrage.rs     # Fair probability model + divergence scanner
├── risk/
│   ├── mod.rs
│   └── manager.rs       # Pre-trade risk checks + position sizing
├── dashboard/
│   ├── mod.rs
│   └── tui.rs           # ratatui TUI dashboard
└── journal/
    ├── mod.rs
    └── logger.rs        # CSV + SQLite trade journal
```

## Build Optimisations

| Setting | Value | Purpose |
|---------|-------|---------|
| `opt-level` | 3 | Maximum optimisation |
| `lto` | fat | Full link-time optimisation |
| `codegen-units` | 1 | Single codegen unit for best optimisation |
| `panic` | abort | No unwinding overhead |
| `strip` | true | Strip debug symbols from binary |

## Data Files

- `data/trades.csv` — CSV trade log
- `data/trades.db` — SQLite trade database
- Queryable: `sqlite3 data/trades.db "SELECT * FROM trades ORDER BY timestamp DESC LIMIT 20;"`

## Safety

- **DRY_RUN=true** by default — no real money at risk
- Daily loss kill-switch auto-pauses when daily cap is hit
- All API keys loaded from `.env` — never hardcoded
- Graceful shutdown saves state on Ctrl+C or `q`
