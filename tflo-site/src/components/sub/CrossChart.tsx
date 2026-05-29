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
import type { Tick, CrossEvent } from "../../lib/wasm";
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

interface CrossChartProps {
  ticks: Tick[];
  results: CrossEvent[];
  config: { threshold: number };
  height: number;
}

function CrossChartInner({ ticks, results, config, height }: CrossChartProps) {
  const chartData = useMemo(
    () => ticks.map((tick) => ({ ts: tick.ts, value: tick.value })),
    [ticks],
  );

  const crossTsSet = useMemo(
    () => new Set(results.map((ev) => ev.ts)),
    [results],
  );

  const threshold = config.threshold;

  const yDomain = useMemo<[(dataMin: number) => number, (dataMax: number) => number]>(
    () => [
      (dataMin: number) => Math.floor(Math.min(dataMin, threshold) - 5),
      (dataMax: number) => Math.ceil(Math.max(dataMax, threshold) + 5),
    ],
    [threshold],
  );

  const thresholdLabel = useMemo(
    () => ({
      value: `Threshold: ${threshold}`,
      position: "right" as const,
      fontSize: 10,
      fill: CHART_COLORS.cross,
    }),
    [threshold],
  );

  if (chartData.length === 0) {
    return <EmptyChart height={height} />;
  }

  return (
    <div
      role="img"
      aria-label="Cross detection chart showing price threshold crossings"
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
          />

          <ReferenceLine
            y={threshold}
            stroke={CHART_COLORS.cross}
            strokeDasharray="6 3"
            strokeWidth={1.5}
            label={thresholdLabel}
          />

          <Line
            type="monotone"
            dataKey="value"
            stroke={CHART_COLORS.price}
            strokeWidth={2}
            isAnimationActive={false}
            // eslint-disable-next-line @typescript-eslint/no-explicit-any
            dot={(dotProps: any) => {
              const { cx, cy, index: idx } = dotProps;
              if (cx == null || cy == null) return null;
              const tick = ticks[idx];
              if (tick && crossTsSet.has(tick.ts)) {
                return (
                  <svg
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
                      fill={CHART_COLORS.cross}
                      stroke={CHART_COLORS.markerBg}
                      strokeWidth={1.5}
                    />
                  </svg>
                );
              }
              return null;
            }}
            name="Price"
          />

          {results.map((ev, idx) => (
            <ReferenceLine
              key={`cross-${idx}`}
              x={ev.ts}
              stroke={CHART_COLORS.cross}
              strokeDasharray="3 3"
              strokeWidth={1}
              strokeOpacity={0.5}
            />
          ))}
        </LineChart>
      </ResponsiveContainer>
    </div>
  );
}

const CrossChart = React.memo(CrossChartInner);
export default CrossChart;
