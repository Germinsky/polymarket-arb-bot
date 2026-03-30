"use client";

// src/components/ExecutionLog.tsx — Scrolling log matching TUI right panel
import { useEffect, useRef } from "react";
import { cn } from "@/lib/utils";
import type { LogEntry } from "@/lib/types";
import { LEVEL_COLORS } from "@/lib/types";

interface ExecutionLogProps {
  logs: LogEntry[];
}

export function ExecutionLog({ logs }: ExecutionLogProps) {
  const bottomRef = useRef<HTMLDivElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    // Auto-scroll only if near bottom
    const isNearBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 80;
    if (isNearBottom) {
      bottomRef.current?.scrollIntoView({ behavior: "smooth" });
    }
  }, [logs.length]);

  return (
    <div className="card-cyber flex flex-col h-full">
      <h2 className="shrink-0 px-3 py-2 text-sm font-bold text-amber border-b border-[var(--card-border)]">
        📋 Execution Log
      </h2>
      <div
        ref={containerRef}
        className="flex-1 min-h-0 overflow-y-auto p-2 font-mono text-xs leading-relaxed"
      >
        {logs.length === 0 ? (
          <div className="flex h-full items-center justify-center text-dim">
            No log entries yet...
          </div>
        ) : (
          logs.map((entry, i) => {
            const time = new Date(entry.ts).toLocaleTimeString("en-US", {
              hour12: false,
              hour: "2-digit",
              minute: "2-digit",
              second: "2-digit",
              fractionalSecondDigits: 3,
            });
            const levelColor = LEVEL_COLORS[entry.level] || "text-foreground";

            return (
              <div key={i} className="flex gap-1.5 py-0.5 hover:bg-white/[0.02] rounded px-1">
                <span className="text-dim shrink-0">[{time}]</span>
                <span className={cn("shrink-0 font-bold", levelColor)}>
                  [{entry.level}]
                </span>
                <span className="text-foreground break-all">{entry.message}</span>
              </div>
            );
          })
        )}
        <div ref={bottomRef} />
      </div>
    </div>
  );
}
