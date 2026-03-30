"use client";

// src/components/MarketsTable.tsx — Active BTC markets from Polymarket
import type { MarketSnapshot } from "@/lib/types";

interface MarketsTableProps {
  markets: MarketSnapshot[];
}

export function MarketsTable({ markets }: MarketsTableProps) {
  if (markets.length === 0) {
    return (
      <div className="card-cyber flex items-center justify-center p-6">
        <span className="text-dim text-sm">No markets loaded yet...</span>
      </div>
    );
  }

  return (
    <div className="card-cyber overflow-hidden">
      <h2 className="px-3 py-2 text-sm font-bold text-neon-purple border-b border-[var(--card-border)]">
        🎯 Active Markets ({markets.length})
      </h2>
      <div className="overflow-x-auto">
        <table className="w-full text-xs">
          <thead>
            <tr className="border-b border-[var(--card-border)] text-dim">
              <th className="px-3 py-2 text-left font-medium w-16">Asset</th>
              <th className="px-3 py-2 text-left font-medium">Question</th>
              <th className="px-3 py-2 text-right font-medium">YES Bid</th>
              <th className="px-3 py-2 text-right font-medium">YES Ask</th>
              <th className="px-3 py-2 text-right font-medium">NO Bid</th>
              <th className="px-3 py-2 text-right font-medium">NO Ask</th>
              <th className="px-3 py-2 text-right font-medium">Expires</th>
            </tr>
          </thead>
          <tbody>
            {markets.map((m) => (
              <tr
                key={m.condition_id}
                className="border-b border-[var(--card-border)] hover:bg-white/[0.02] transition-colors"
              >
                <td className="px-3 py-2">
                  <span className="inline-block rounded px-1.5 py-0.5 text-[10px] font-bold bg-white/5 text-amber">
                    {m.asset || "BTC"}
                  </span>
                </td>
                <td className="px-3 py-2 text-foreground max-w-[300px] truncate">
                  {m.question}
                </td>
                <td className="px-3 py-2 text-right text-neon-green font-mono">
                  {m.yes_bid.toFixed(4)}
                </td>
                <td className="px-3 py-2 text-right text-neon-green font-mono">
                  {m.yes_ask.toFixed(4)}
                </td>
                <td className="px-3 py-2 text-right text-neon-red font-mono">
                  {m.no_bid.toFixed(4)}
                </td>
                <td className="px-3 py-2 text-right text-neon-red font-mono">
                  {m.no_ask.toFixed(4)}
                </td>
                <td className="px-3 py-2 text-right text-dim">
                  {m.end_date
                    ? new Date(m.end_date).toLocaleDateString()
                    : "—"}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}
