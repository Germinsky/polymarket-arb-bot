"use client";

// src/hooks/useBotData.ts — Polls the Rust bot API every 500ms for real-time data
import { useCallback, useEffect, useRef, useState } from "react";
import type { BotStatus, LogEntry, EquityPoint, MarketSnapshot } from "@/lib/types";

const API_BASE = process.env.NEXT_PUBLIC_API_URL || "http://localhost:3001";

export function useBotData(intervalMs = 500) {
  const [status, setStatus] = useState<BotStatus | null>(null);
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [equity, setEquity] = useState<EquityPoint[]>([]);
  const [markets, setMarkets] = useState<MarketSnapshot[]>([]);
  const [connected, setConnected] = useState(false);
  const mountedRef = useRef(true);

  const fetchAll = useCallback(async () => {
    try {
      const [statusRes, logsRes, equityRes, marketsRes] = await Promise.all([
        fetch(`${API_BASE}/api/status`, { cache: "no-store" }),
        fetch(`${API_BASE}/api/logs`, { cache: "no-store" }),
        fetch(`${API_BASE}/api/equity`, { cache: "no-store" }),
        fetch(`${API_BASE}/api/markets`, { cache: "no-store" }),
      ]);

      if (!mountedRef.current) return;

      if (statusRes.ok) {
        setStatus(await statusRes.json());
        setConnected(true);
      }
      if (logsRes.ok) setLogs(await logsRes.json());
      if (equityRes.ok) setEquity(await equityRes.json());
      if (marketsRes.ok) setMarkets(await marketsRes.json());
    } catch {
      if (mountedRef.current) setConnected(false);
    }
  }, []);

  useEffect(() => {
    mountedRef.current = true;
    fetchAll();
    const id = setInterval(fetchAll, intervalMs);
    return () => {
      mountedRef.current = false;
      clearInterval(id);
    };
  }, [fetchAll, intervalMs]);

  const togglePause = useCallback(async () => {
    try {
      await fetch(`${API_BASE}/api/control/pause`);
      await fetchAll();
    } catch { /* ignore */ }
  }, [fetchAll]);

  const resetDaily = useCallback(async () => {
    try {
      await fetch(`${API_BASE}/api/control/reset-daily`);
      await fetchAll();
    } catch { /* ignore */ }
  }, [fetchAll]);

  return { status, logs, equity, markets, connected, togglePause, resetDaily };
}
