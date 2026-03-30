"use client";

import { useBotData } from "@/hooks/useBotData";
import { StatsBar } from "@/components/StatsBar";
import { EquityChart } from "@/components/EquityChart";
import { ExecutionLog } from "@/components/ExecutionLog";
import { MarketsTable } from "@/components/MarketsTable";
import { ControlBar } from "@/components/ControlBar";
import { PriceFeeds } from "@/components/PriceFeeds";
import { TradeStats } from "@/components/TradeStats";

export default function Home() {
  const { status, logs, equity, markets, connected, togglePause, resetDaily } =
    useBotData();

  return (
    <div className="flex flex-col h-screen overflow-hidden p-2 gap-2">
      {/* Top stats bar */}
      <StatsBar status={status} connected={connected} />

      {/* Main body: three-column layout */}
      <div className="flex-1 min-h-0 grid grid-cols-12 gap-2">
        {/* Left sidebar — feeds + stats */}
        <div className="col-span-2 flex flex-col gap-2 overflow-y-auto">
          <PriceFeeds status={status} />
          <TradeStats status={status} />
        </div>

        {/* Center — equity chart */}
        <div className="col-span-6 flex flex-col min-h-0">
          <EquityChart data={equity} />
        </div>

        {/* Right — execution log */}
        <div className="col-span-4 flex flex-col min-h-0">
          <ExecutionLog logs={logs} />
        </div>
      </div>

      {/* Markets table */}
      <div className="max-h-[30vh] overflow-y-auto">
        <MarketsTable markets={markets} />
      </div>

      {/* Bottom control bar */}
      <ControlBar
        status={status}
        connected={connected}
        onTogglePause={togglePause}
        onResetDaily={resetDaily}
      />
    </div>
  );
}
