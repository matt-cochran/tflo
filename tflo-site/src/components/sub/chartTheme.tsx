import React from "react";

export const CHART_GRID_STROKE = "rgba(148, 163, 184, 0.14)";
export const CHART_AXIS_LINE_STROKE = "#475569";
export const CHART_AXIS_TICK = { fontSize: 11, fill: "#94a3b8" } as const;
export const CHART_AXIS_LINE = { stroke: CHART_AXIS_LINE_STROKE } as const;
export const CHART_MARGIN = {
  top: 5,
  right: 20,
  left: 10,
  bottom: 5,
} as const;

export const CHART_TOOLTIP_STYLE = {
  background: "#1e293b",
  border: "1px solid #334155",
  borderRadius: 4,
  fontSize: 12,
  color: "#e2e8f0",
} as const;
export const CHART_TOOLTIP_ITEM_STYLE = { color: "#e2e8f0" } as const;
export const CHART_TOOLTIP_LABEL_STYLE = { color: "#94a3b8" } as const;

export const CHART_COLORS = {
  price: "#0ea5e9",
  sma: "#fb923c",
  rsi: "#a78bfa",
  middle: "#22c55e",
  band: "rgba(34, 197, 94, 0.40)",
  bandFill: "rgba(34, 197, 94, 0.10)",
  cross: "#f87171",
  warning: "#f59e0b",
  zero: "#64748b",
  histPositive: "rgba(45, 212, 191, 0.65)",
  histNegative: "rgba(248, 113, 113, 0.65)",
  markerValid: "#34d399",
  markerInvalid: "#f87171",
  markerBg: "#0f172a",
} as const;

export const chartTooltipLabelFormatter = (label: string | number): string =>
  `t=${label}`;

interface EmptyChartProps {
  height: number;
  message?: string;
}

export function EmptyChart({ height, message = "No data" }: EmptyChartProps) {
  return (
    <div
      className="flex items-center justify-center rounded-md border border-border bg-bg-card text-sm text-text-muted"
      style={{ height }}
    >
      {message}
    </div>
  );
}
