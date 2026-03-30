"use client";

// src/components/PriceFeeds.tsx — Live BTC price feeds panel
import { cn } from "@/lib/utils";
import { formatCurrency } from "@/lib/utils";
import type { BotStatus } from "@/lib/types";

interface PriceFeedsProps {
  status: BotStatus | null;
}

function FeedRow({ name, price, primary }: { name: string; price: number; primary?: boolean }) {
  const alive = price > 0;
  return (
    <div className="flex items-center justify-between py-1.5">
      <div className="flex items-center gap-2">
        <div className={cn("h-1.5 w-1.5 rounded-full", alive ? "bg-neon-green" : "bg-neon-red")} />
        <span className={cn("text-xs", primary ? "text-foreground font-semibold" : "text-dim")}>
          {name}
        </span>
      </div>
      <span className={cn("text-xs font-mono", alive ? (primary ? "text-amber font-bold" : "text-foreground") : "text-dim")}>
        {alive ? formatCurrency(price) : "offline"}
      </span>
    </div>
  );
}

export function PriceFeeds({ status }: PriceFeedsProps) {
  return (
    <div className="card-cyber p-3">
      <h2 className="mb-2 text-sm font-bold text-amber">⚡ Price Feeds</h2>
      <FeedRow name="BTC (Binance)" price={status?.btc_binance ?? 0} primary />
      <FeedRow name="BTC (Coinbase)" price={status?.btc_coinbase ?? 0} />
      <FeedRow name="BTC (CoinGecko)" price={status?.btc_coingecko ?? 0} />
      <div className="mt-2 border-t border-[var(--card-border)] pt-2">
        <div className="flex items-center justify-between">
          <span className="text-xs text-dim">Best BTC</span>
          <span className="text-sm font-bold text-cyan glow-cyan">
            {status?.btc_price ? formatCurrency(status.btc_price) : "—"}
          </span>
        </div>
      </div>
      <div className="mt-2 border-t border-[var(--card-border)] pt-2 space-y-1">
        <FeedRow name="ETH" price={status?.eth_price ?? 0} />
        <FeedRow name="Crude Oil (CL)" price={status?.oil_price ?? 0} />
        <FeedRow name="Gold (GC)" price={status?.gold_price ?? 0} />
        <FeedRow name="Fed Rate" price={status?.fed_rate ?? 0} />
      </div>
    </div>
  );
}
