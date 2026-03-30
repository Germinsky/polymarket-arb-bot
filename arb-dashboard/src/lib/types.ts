// src/lib/types.ts — Shared types matching the Rust bot's API payloads

export interface BotStatus {
  balance: number;
  daily_pnl: number;
  total_pnl: number;
  wins: number;
  losses: number;
  total_trades: number;
  win_rate: number;
  orders: number;
  latency_us: number;
  btc_price: number;
  btc_binance: number;
  btc_coinbase: number;
  btc_coingecko: number;
  eth_price: number;
  oil_price: number;
  gold_price: number;
  fed_rate: number;
  is_paused: boolean;
  daily_cap_hit: boolean;
  markets_count: number;
}

export interface LogEntry {
  ts: string;
  level: string;
  message: string;
}

export interface EquityPoint {
  ts: string;
  equity: number;
}

export interface MarketSnapshot {
  condition_id: string;
  question: string;
  yes_bid: number;
  yes_ask: number;
  no_bid: number;
  no_ask: number;
  end_date: string | null;
  asset: string;
}

// Level → color mapping matching the TUI
export const LEVEL_COLORS: Record<string, string> = {
  INFO: "text-cyan",
  EXEC: "text-amber",
  FILL: "text-neon-green",
  WARN: "text-amber",
  ERR: "text-neon-red",
  SLIP: "text-neon-red",
};
