"use client";

import React, { useMemo } from "react";
import {
  LineChart,
  Line,
  XAxis,
  YAxis,
  Tooltip,
  ResponsiveContainer,
  ReferenceLine,
  CartesianGrid,
} from "recharts";
import type { Tick } from "../../lib/wasm";
import {
  CHART_AXIS_LINE,
  CHART_AXIS_TICK,
  CHART_COLORS,
  CHART_GRID_STROKE,
  CHART_MARGIN,
  CHART_TOOLTIP_ITEM_STYLE,
  CHART_TOOLTIP_LABEL_STYLE,
  CHART_TOOLTIP_STYLE,
  EmptyChart,
  chartTooltipLabelFormatter,
} from "./chartTheme";

/** One row of runt-detector demo output, aligned 1:1 with a tick. */
export interface RuntResult {
  value: number;
  /** Fires when a pulse falls back below `low`. */
  event: "valid" | "runt" | null;
}

interface RuntChartProps {
  ticks: Tick[];
  results: RuntResult[];
  config: { low: number; high: number };
  height: number;
}

function tooltipFormatter(value: unknown, name: unknown): [string, string] {
  const v = value as number | null;
  return [v == null ? "—" : v.toFixed(2), name as string];
}

function RuntChartInner({ ticks, results, config, height }: RuntChartProps) {
  const chartData = useMemo(
    () =>
      ticks.map((t, i) => ({
        ts: t.ts,
        value: t.value,
        event: results[i]?.event ?? null,
      })),
    [ticks, results],
  );

  const { low, high } = config;

  const yDomain = useMemo<
    [(dataMin: number) => number, (dataMax: number) => number]
  >(
    () => [
      (min: number) => Math.floor(Math.min(min, low) - 5),
      (max: number) => Math.ceil(Math.max(max, high) + 5),
    ],
    [low, high],
  );

  const highLabel = useMemo(
    () => ({
      value: `High: ${high}`,
      position: "right" as const,
      fontSize: 10,
      fill: CHART_COLORS.cross,
    }),
    [high],
  );

  const lowLabel = useMemo(
    () => ({
      value: `Low: ${low}`,
      position: "right" as const,
      fontSize: 10,
      fill: CHART_COLORS.warning,
    }),
    [low],
  );

  if (chartData.length === 0) {
    return <EmptyChart height={height} />;
  }

  return (
    <div
      role="img"
      aria-label="Runt detector chart"
      style={{ width: "100%", height }}
    >
      <ResponsiveContainer width="100%" height="100%">
        <LineChart data={chartData} margin={CHART_MARGIN}>
          <CartesianGrid strokeDasharray="3 3" stroke={CHART_GRID_STROKE} />
          <XAxis dataKey="ts" tick={false} axisLine={CHART_AXIS_LINE} />
          <YAxis
            domain={yDomain}
            tick={CHART_AXIS_TICK}
            axisLine={CHART_AXIS_LINE}
          />
          <Tooltip
            contentStyle={CHART_TOOLTIP_STYLE}
            itemStyle={CHART_TOOLTIP_ITEM_STYLE}
            labelStyle={CHART_TOOLTIP_LABEL_STYLE}
            labelFormatter={chartTooltipLabelFormatter}
            formatter={tooltipFormatter}
          />

          <ReferenceLine
            y={high}
            stroke={CHART_COLORS.cross}
            strokeDasharray="6 3"
            strokeWidth={1.5}
            label={highLabel}
          />

          <ReferenceLine
            y={low}
            stroke={CHART_COLORS.warning}
            strokeDasharray="6 3"
            strokeWidth={1.5}
            label={lowLabel}
          />

          <Line
            type="monotone"
            dataKey="value"
            stroke={CHART_COLORS.price}
            strokeWidth={2}
            isAnimationActive={false}
            name="Signal"
            // eslint-disable-next-line @typescript-eslint/no-explicit-any
            dot={(dotProps: any) => {
              const { cx, cy, index: idx } = dotProps;
              if (cx == null || cy == null) return null;
              const point = chartData[idx];
              if (point && point.event !== null) {
                return (
                  <svg
                    key={`event-${idx}`}
                    x={cx - 5}
                    y={cy - 5}
                    width={10}
                    height={10}
                    viewBox="0 0 10 10"
                  >
                    <circle
                      cx={5}
                      cy={5}
                      r={4}
                      fill={
                        point.event === "valid"
                          ? CHART_COLORS.markerValid
                          : CHART_COLORS.markerInvalid
                      }
                      stroke={CHART_COLORS.markerBg}
                      strokeWidth={1.5}
                    />
                  </svg>
                );
              }
              return null;
            }}
          />

          {chartData.map((d, idx) =>
            d.event !== null ? (
              <ReferenceLine
                key={`event-line-${idx}`}
                x={d.ts}
                stroke={CHART_COLORS.cross}
                strokeDasharray="3 3"
                strokeWidth={1}
                strokeOpacity={0.5}
              />
            ) : null,
          )}
        </LineChart>
      </ResponsiveContainer>
    </div>
  );
}

const RuntChart = React.memo(RuntChartInner);
export default RuntChart;
