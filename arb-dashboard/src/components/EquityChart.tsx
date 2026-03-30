"use client";

// src/components/EquityChart.tsx — Live equity curve matching TUI chart
import { AreaChart, Area, XAxis, YAxis, Tooltip, ResponsiveContainer, ReferenceLine } from "recharts";
import type { EquityPoint } from "@/lib/types";
import { formatCurrency } from "@/lib/utils";

interface EquityChartProps {
  data: EquityPoint[];
}

interface ChartDatum {
  time: string;
  equity: number;
}

function CustomTooltip({ active, payload }: { active?: boolean; payload?: Array<{ value: number; payload: ChartDatum }> }) {
  if (!active || !payload?.length) return null;
  const d = payload[0];
  return (
    <div className="card-cyber px-3 py-2 text-xs">
      <div className="text-dim">{d.payload.time}</div>
      <div className="text-cyan font-bold">{formatCurrency(d.value)}</div>
    </div>
  );
}

export function EquityChart({ data }: EquityChartProps) {
  if (data.length === 0) {
    return (
      <div className="card-cyber flex h-full items-center justify-center">
        <span className="text-dim text-sm">Waiting for equity data...</span>
      </div>
    );
  }

  const chartData: ChartDatum[] = data.map((p) => ({
    time: new Date(p.ts).toLocaleTimeString(),
    equity: p.equity,
  }));

  const equities = chartData.map((d) => d.equity);
  const min = Math.min(...equities);
  const max = Math.max(...equities);
  const pad = Math.max((max - min) * 0.05, 1);
  const startEquity = chartData[0]?.equity ?? 0;

  return (
    <div className="card-cyber flex flex-col p-3 h-full">
      <h2 className="mb-2 text-sm font-bold text-neon-green glow-green">
        📈 Equity Curve
      </h2>
      <div className="flex-1 min-h-0">
        <ResponsiveContainer width="100%" height="100%">
          <AreaChart data={chartData} margin={{ top: 5, right: 10, bottom: 5, left: 5 }}>
            <defs>
              <linearGradient id="equityGradient" x1="0" y1="0" x2="0" y2="1">
                <stop offset="0%" stopColor="#00d9ff" stopOpacity={0.4} />
                <stop offset="100%" stopColor="#00d9ff" stopOpacity={0} />
              </linearGradient>
            </defs>
            <XAxis
              dataKey="time"
              tick={{ fontSize: 10, fill: "#64648a" }}
              tickLine={false}
              axisLine={{ stroke: "#1a1a2e" }}
              interval="preserveStartEnd"
            />
            <YAxis
              domain={[min - pad, max + pad]}
              tick={{ fontSize: 10, fill: "#64648a" }}
              tickLine={false}
              axisLine={{ stroke: "#1a1a2e" }}
              tickFormatter={(v: number) => `$${v.toFixed(0)}`}
              width={60}
            />
            <Tooltip content={<CustomTooltip />} />
            <ReferenceLine
              y={startEquity}
              stroke="#64648a"
              strokeDasharray="3 3"
              label={{ value: "Start", fill: "#64648a", fontSize: 10 }}
            />
            <Area
              type="monotone"
              dataKey="equity"
              stroke="#00d9ff"
              strokeWidth={2}
              fill="url(#equityGradient)"
              dot={false}
              isAnimationActive={false}
            />
          </AreaChart>
        </ResponsiveContainer>
      </div>
    </div>
  );
}
