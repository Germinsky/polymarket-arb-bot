import type { BotStatus, LogEntry, EquityPoint, MarketSnapshot } from "./types";

const API_BASE = process.env.NEXT_PUBLIC_API_URL || "http://localhost:3001";

async function fetchApi<T>(path: string): Promise<T> {
  const res = await fetch(`${API_BASE}${path}`, { cache: "no-store" });
  if (!res.ok) throw new Error(`API ${path}: ${res.status}`);
  return res.json() as Promise<T>;
}

export async function getStatus() {
  return fetchApi<BotStatus>("/api/status");
}

export async function getLogs() {
  return fetchApi<LogEntry[]>("/api/logs");
}

export async function getEquity() {
  return fetchApi<EquityPoint[]>("/api/equity");
}

export async function getMarkets() {
  return fetchApi<MarketSnapshot[]>("/api/markets");
}

export async function togglePause() {
  return fetchApi<{ paused: boolean }>("/api/control/pause");
}

export async function resetDaily() {
  return fetchApi<{ reset: boolean }>("/api/control/reset-daily");
}

export { API_BASE };
