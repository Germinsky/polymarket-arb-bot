"use client";

// src/components/ControlBar.tsx — Bottom control bar with pause/reset/mode
import { cn } from "@/lib/utils";
import type { BotStatus } from "@/lib/types";

interface ControlBarProps {
  status: BotStatus | null;
  connected: boolean;
  onTogglePause: () => void;
  onResetDaily: () => void;
}

export function ControlBar({ status, connected, onTogglePause, onResetDaily }: ControlBarProps) {
  return (
    <div className="card-cyber flex flex-wrap items-center justify-between gap-2 px-4 py-2">
      <div className="flex items-center gap-3">
        <button
          onClick={onTogglePause}
          disabled={!connected}
          className={cn(
            "rounded-md px-3 py-1 text-xs font-bold uppercase tracking-wider transition-all",
            "border border-cyan/30 hover:border-cyan hover:bg-cyan/10 text-cyan",
            "disabled:opacity-30 disabled:cursor-not-allowed"
          )}
        >
          {status?.is_paused ? "▶ Resume" : "⏸ Pause"}
        </button>
        <button
          onClick={onResetDaily}
          disabled={!connected}
          className={cn(
            "rounded-md px-3 py-1 text-xs font-bold uppercase tracking-wider transition-all",
            "border border-amber/30 hover:border-amber hover:bg-amber/10 text-amber",
            "disabled:opacity-30 disabled:cursor-not-allowed"
          )}
        >
          ↺ Reset Daily
        </button>
      </div>

      <div className="flex items-center gap-3 text-xs">
        <span className="text-dim">
          Markets: <span className="text-cyan">{status?.markets_count ?? 0}</span>
        </span>
        <span className="text-dim">│</span>
        <span className="text-dim">
          BTC feeds:{" "}
          <span className={status?.btc_binance ? "text-neon-green" : "text-neon-red"}>Binance</span>
          {" / "}
          <span className={status?.btc_coinbase ? "text-neon-green" : "text-neon-red"}>Coinbase</span>
          {" / "}
          <span className={status?.btc_coingecko ? "text-neon-green" : "text-neon-red"}>CoinGecko</span>
        </span>
        <span className="text-dim">│</span>
        {connected ? (
          <span className="font-bold text-amber">🔒 DRY RUN</span>
        ) : (
          <span className="font-bold text-neon-red animate-pulse">⚠ DISCONNECTED</span>
        )}
      </div>
    </div>
  );
}
