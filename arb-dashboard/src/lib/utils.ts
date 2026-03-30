// src/lib/utils.ts
import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

export function formatCurrency(value: number): string {
  return new Intl.NumberFormat("en-US", {
    style: "currency",
    currency: "USD",
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  }).format(value);
}

export function formatPnl(value: number): string {
  const sign = value >= 0 ? "+" : "";
  return `${sign}${formatCurrency(value)}`;
}

export function formatPercent(value: number): string {
  return `${value.toFixed(1)}%`;
}

export function formatLatency(us: number): string {
  return `${(us / 1000).toFixed(1)}ms`;
}

export function pnlColor(value: number): string {
  return value >= 0 ? "text-neon-green" : "text-neon-red";
}

export function latencyColor(us: number): string {
  if (us < 50_000) return "text-neon-green";
  if (us < 100_000) return "text-amber";
  return "text-neon-red";
}
