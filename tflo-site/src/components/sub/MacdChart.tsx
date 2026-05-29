"use client";

import React, { useMemo } from "react";
import {
  ComposedChart,
  Line,
  Bar,
  Cell,
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

/** One row of MACD demo output, aligned 1:1 with a tick. */
export interface MacdResult {
  macd: number | null;
  signal: number | null;
  histogram: number | null;
  cross: "above" | "below" | null;
}

interface MacdChartProps {
  ticks: Tick[];
  results: MacdResult[];
  height: number;
}

const MACD_Y_DOMAIN: [
  (dataMin: number) => number,
  (dataMax: number) => number,
] = [
  (dataMin: number) => Math.floor(Math.min(dataMin, 0)),
  (dataMax: number) => Math.ceil(Math.max(dataMax, 0)),
];

function macdFormatter(value: unknown, name: unknown): [string, string] {
  const v = value as number | null;
  return [v == null ? "—" : v.toFixed(2), name as string];
}

function MacdChartInner({ ticks, results, height }: MacdChartProps) {
  const chartData = useMemo(
    () =>
      ticks.map((tick, i) => {
        const r = results[i];
        return {
          ts: tick.ts,
          macd: r?.macd ?? null,
          signal: r?.signal ?? null,
          histogram: r?.histogram ?? null,
          cross: r?.cross ?? null,
        };
      }),
    [ticks, results],
  );

  if (chartData.length === 0) {
    return <EmptyChart height={height} />;
  }

  return (
    <div
      role="img"
      aria-label="MACD chart showing the MACD line crossing its signal line, with histogram"
      style={{ width: "100%", height }}
    >
      <ResponsiveContainer width="100%" height="100%">
        <ComposedChart data={chartData} margin={CHART_MARGIN}>
          <CartesianGrid strokeDasharray="3 3" stroke={CHART_GRID_STROKE} />
          <XAxis dataKey="ts" tick={false} axisLine={CHART_AXIS_LINE} />
          <YAxis
            domain={MACD_Y_DOMAIN}
            tick={CHART_AXIS_TICK}
            axisLine={CHART_AXIS_LINE}
          />
          <Tooltip
            contentStyle={CHART_TOOLTIP_STYLE}
            itemStyle={CHART_TOOLTIP_ITEM_STYLE}
            labelStyle={CHART_TOOLTIP_LABEL_STYLE}
            labelFormatter={chartTooltipLabelFormatter}
            formatter={macdFormatter}
          />

          <ReferenceLine y={0} stroke={CHART_COLORS.zero} strokeWidth={1} />

          <Bar dataKey="histogram" name="Histogram" isAnimationActive={false}>
            {chartData.map((d, i) => (
              <Cell
                key={`hist-${i}`}
                fill={
                  (d.histogram ?? 0) >= 0
                    ? CHART_COLORS.histPositive
                    : CHART_COLORS.histNegative
                }
              />
            ))}
          </Bar>

          <Line
            type="monotone"
            dataKey="macd"
            stroke={CHART_COLORS.price}
            strokeWidth={2}
            isAnimationActive={false}
            connectNulls={false}
            name="MACD"
            // eslint-disable-next-line @typescript-eslint/no-explicit-any
            dot={(dotProps: any) => {
              const { cx, cy, index: idx } = dotProps;
              if (cx == null || cy == null) return null;
              const point = chartData[idx];
              if (point && point.cross !== null) {
                return (
                  <svg
                    key={`cross-${idx}`}
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
                        point.cross === "above"
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

          <Line
            type="monotone"
            dataKey="signal"
            stroke={CHART_COLORS.sma}
            strokeWidth={1.5}
            strokeDasharray="4 2"
            dot={false}
            isAnimationActive={false}
            connectNulls={false}
            name="Signal"
          />

          {chartData.map((d, idx) =>
            d.cross !== null ? (
              <ReferenceLine
                key={`cross-line-${idx}`}
                x={d.ts}
                stroke={CHART_COLORS.cross}
                strokeDasharray="3 3"
                strokeWidth={1}
                strokeOpacity={0.5}
              />
            ) : null,
          )}
        </ComposedChart>
      </ResponsiveContainer>
    </div>
  );
}

const MacdChart = React.memo(MacdChartInner);
export default MacdChart;
