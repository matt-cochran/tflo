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
  ReferenceArea,
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

/** One row of window-detector demo output, aligned 1:1 with a tick. */
export interface WindowResult {
  value: number;
  event: "entered" | "exitedLow" | "exitedHigh" | null;
}

interface WindowChartProps {
  ticks: Tick[];
  results: WindowResult[];
  config: { low: number; high: number };
  height: number;
}

const EVENT_COLOR: Record<NonNullable<WindowResult["event"]>, string> = {
  entered: CHART_COLORS.markerValid,
  exitedLow: CHART_COLORS.warning,
  exitedHigh: CHART_COLORS.markerInvalid,
};

function WindowChartInner({
  ticks,
  results,
  config,
  height,
}: WindowChartProps) {
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
      (dataMin: number) => Math.floor(Math.min(dataMin, low) - 5),
      (dataMax: number) => Math.ceil(Math.max(dataMax, high) + 5),
    ],
    [low, high],
  );

  const lowLabel = useMemo(
    () => ({
      value: `Low: ${low}`,
      position: "right" as const,
      fontSize: 10,
      fill: CHART_COLORS.markerValid,
    }),
    [low],
  );

  const highLabel = useMemo(
    () => ({
      value: `High: ${high}`,
      position: "right" as const,
      fontSize: 10,
      fill: CHART_COLORS.markerValid,
    }),
    [high],
  );

  if (chartData.length === 0) {
    return <EmptyChart height={height} />;
  }

  return (
    <div
      role="img"
      aria-label="Window detector chart"
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

          <ReferenceArea
            y1={low}
            y2={high}
            fill="rgba(52,211,153,0.12)"
            stroke="none"
          />

          <ReferenceLine
            y={low}
            stroke={CHART_COLORS.markerValid}
            strokeDasharray="4 3"
            strokeOpacity={0.6}
            label={lowLabel}
          />

          <ReferenceLine
            y={high}
            stroke={CHART_COLORS.markerValid}
            strokeDasharray="4 3"
            strokeOpacity={0.6}
            label={highLabel}
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
                      fill={EVENT_COLOR[point.event]}
                      stroke={CHART_COLORS.markerBg}
                      strokeWidth={1.5}
                    />
                  </svg>
                );
              }
              return null;
            }}
            name="Signal"
          />

          {chartData.map((d, idx) =>
            d.event !== null ? (
              <ReferenceLine
                key={`event-line-${idx}`}
                x={d.ts}
                stroke={CHART_COLORS.markerValid}
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

const WindowChart = React.memo(WindowChartInner);
export default WindowChart;
