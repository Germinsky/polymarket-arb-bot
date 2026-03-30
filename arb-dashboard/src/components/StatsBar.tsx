"use client";

// src/components/StatsBar.tsx — Top stats bar matching TUI header
import { cn } from "@/lib/utils";
import type { BotStatus } from "@/lib/types";
import {
  formatCurrency,
  formatPnl,
  formatPercent,
  formatLatency,
  pnlColor,
  latencyColor,
} from "@/lib/utils";

interface StatsBarProps {
  status: BotStatus | null;
  connected: boolean;
}

function Stat({ label, value, colorClass }: { label: string; value: string; colorClass?: string }) {
  return (
    <div className="flex items-center gap-1.5">
      <span className="text-dim text-xs uppercase tracking-wider">{label}</span>
      <span className={cn("text-sm font-bold", colorClass || "text-cyan")}>{value}</span>
    </div>
  );
}

function Divider() {
  return <div className="mx-2 h-4 w-px bg-[var(--card-border)]" />;
}

export function StatsBar({ status, connected }: StatsBarProps) {
  if (!status) {
    return (
      <div className="card-cyber flex items-center justify-center px-4 py-3">
        <div className="flex items-center gap-2">
          <div className="h-2 w-2 animate-pulse rounded-full bg-amber" />
          <span className="text-dim text-sm">
            {connected ? "Loading..." : "Waiting for bot connection on :3001..."}
          </span>
        </div>
      </div>
    );
  }

  return (
    <div className="card-cyber flex flex-wrap items-center gap-y-2 px-4 py-2.5">
      {/* Status indicator */}
      <div className="flex items-center gap-2">
        <div
          className={cn(
            "h-2 w-2 rounded-full",
            status.is_paused ? "bg-neon-red animate-pulse-glow" : "bg-neon-green animate-pulse-glow"
          )}
        />
        <span className={cn("text-xs font-bold uppercase", status.is_paused ? "text-neon-red" : "text-neon-green")}>
          {status.is_paused ? "Paused" : "Active"}
        </span>
      </div>
      <Divider />

      <Stat label="Balance" value={formatCurrency(status.balance)} colorClass="text-cyan glow-cyan" />
      <Divider />

      <Stat label="Daily" value={formatPnl(status.daily_pnl)} colorClass={pnlColor(status.daily_pnl)} />
      <Divider />

      <Stat label="Total" value={formatPnl(status.total_pnl)} colorClass={pnlColor(status.total_pnl)} />
      <Divider />

      <Stat
        label="Win"
        value={`${formatPercent(status.win_rate)} (${status.wins}/${status.total_trades})`}
        colorClass={status.win_rate >= 60 ? "text-neon-green" : "text-amber"}
      />
      <Divider />

      <Stat label="Lat" value={formatLatency(status.latency_us)} colorClass={latencyColor(status.latency_us)} />
      <Divider />

      <Stat label="Orders" value={String(status.orders)} />
      <Divider />

      <Stat label="BTC" value={formatCurrency(status.btc_price)} colorClass="text-amber font-bold" />
      <Divider />

      <Stat label="ETH" value={formatCurrency(status.eth_price)} />
      <Divider />

      <Stat label="CL" value={formatCurrency(status.oil_price)} />
      <Divider />

      <Stat label="GC" value={formatCurrency(status.gold_price)} />
      <Divider />

      <Stat label="Markets" value={String(status.markets_count)} />
    </div>
  );
}
