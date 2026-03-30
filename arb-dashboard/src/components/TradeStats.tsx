"use client";

// src/components/TradeStats.tsx — Detailed stats card
import { cn } from "@/lib/utils";
import { formatCurrency, formatPnl, formatPercent, pnlColor } from "@/lib/utils";
import type { BotStatus } from "@/lib/types";

interface TradeStatsProps {
  status: BotStatus | null;
}

function Row({ label, value, colorClass }: { label: string; value: string; colorClass?: string }) {
  return (
    <div className="flex items-center justify-between py-1">
      <span className="text-xs text-dim">{label}</span>
      <span className={cn("text-xs font-mono font-semibold", colorClass || "text-foreground")}>{value}</span>
    </div>
  );
}

export function TradeStats({ status }: TradeStatsProps) {
  if (!status) return null;

  return (
    <div className="card-cyber p-3">
      <h2 className="mb-2 text-sm font-bold text-cyan">📊 Trade Statistics</h2>
      <Row label="Total Trades" value={String(status.total_trades)} colorClass="text-cyan" />
      <Row label="Wins" value={String(status.wins)} colorClass="text-neon-green" />
      <Row label="Losses" value={String(status.losses)} colorClass="text-neon-red" />
      <Row label="Win Rate" value={formatPercent(status.win_rate)} colorClass={status.win_rate >= 60 ? "text-neon-green" : "text-amber"} />
      <div className="my-1.5 border-t border-[var(--card-border)]" />
      <Row label="Daily PnL" value={formatPnl(status.daily_pnl)} colorClass={pnlColor(status.daily_pnl)} />
      <Row label="Total PnL" value={formatPnl(status.total_pnl)} colorClass={pnlColor(status.total_pnl)} />
      <Row label="Balance" value={formatCurrency(status.balance)} colorClass="text-cyan" />
      <div className="my-1.5 border-t border-[var(--card-border)]" />
      <Row
        label="Daily Cap"
        value={status.daily_cap_hit ? "⚠ HIT" : "OK"}
        colorClass={status.daily_cap_hit ? "text-neon-red" : "text-neon-green"}
      />
      <Row label="Total Orders" value={String(status.orders)} colorClass="text-amber" />
    </div>
  );
}
