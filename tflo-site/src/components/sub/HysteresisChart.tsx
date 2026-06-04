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

/** One row of hysteresis-cross demo output, aligned 1:1 with a tick. */
export interface HysteresisResult {
  value: number;
  event: "rising" | "falling" | null;
}

interface HysteresisChartProps {
  ticks: Tick[];
  results: HysteresisResult[];
  config: { threshold: number; margin: number };
  height: number;
}

function tooltipFormatter(value: unknown, name: unknown): [string, string] {
  const v = value as number | null;
  return [v == null ? "—" : v.toFixed(2), name as string];
}

function HysteresisChartInner({
  ticks,
  results,
  config,
  height,
}: HysteresisChartProps) {
  const chartData = useMemo(
    () =>
      ticks.map((t, i) => ({
        ts: t.ts,
        value: t.value,
        event: results[i]?.event ?? null,
      })),
    [ticks, results],
  );

  const { threshold, margin } = config;

  const yDomain = useMemo<
    [(dataMin: number) => number, (dataMax: number) => number]
  >(
    () => [
      (min: number) => Math.floor(Math.min(min, threshold - margin) - 5),
      (max: number) => Math.ceil(Math.max(max, threshold + margin) + 5),
    ],
    [threshold, margin],
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
      aria-label="Hysteresis cross detection chart"
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

          <ReferenceArea
            y1={threshold - margin}
            y2={threshold + margin}
            fill="rgba(248,113,113,0.10)"
          />

          <ReferenceLine
            y={threshold + margin}
            stroke={CHART_COLORS.cross}
            strokeOpacity={0.4}
            strokeDasharray="3 3"
            strokeWidth={1}
          />
          <ReferenceLine
            y={threshold - margin}
            stroke={CHART_COLORS.cross}
            strokeOpacity={0.4}
            strokeDasharray="3 3"
            strokeWidth={1}
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
                        point.event === "rising"
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

const HysteresisChart = React.memo(HysteresisChartInner);
export default HysteresisChart;
