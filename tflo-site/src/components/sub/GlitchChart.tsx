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

/** One row of glitch-filter demo output, aligned 1:1 with a tick. */
export interface GlitchResult {
  value: number;
  event: "valid" | "glitch" | null;
}

interface GlitchChartProps {
  ticks: Tick[];
  results: GlitchResult[];
  config: { threshold: number; minDuration: number };
  height: number;
}

function GlitchChartInner({
  ticks,
  results,
  config,
  height,
}: GlitchChartProps) {
  const chartData = useMemo(
    () =>
      ticks.map((t, i) => ({
        ts: t.ts,
        value: t.value,
        event: results[i]?.event ?? null,
      })),
    [ticks, results],
  );

  const threshold = config.threshold;

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
      aria-label="Glitch filter chart"
      style={{ width: "100%", height }}
    >
      <ResponsiveContainer width="100%" height="100%">
        <LineChart data={chartData} margin={CHART_MARGIN}>
          <CartesianGrid strokeDasharray="3 3" stroke={CHART_GRID_STROKE} />
          <XAxis dataKey="ts" tick={false} axisLine={CHART_AXIS_LINE} />
          <YAxis
            domain={["auto", "auto"]}
            tick={CHART_AXIS_TICK}
            axisLine={CHART_AXIS_LINE}
          />
          <Tooltip
            contentStyle={CHART_TOOLTIP_STYLE}
            itemStyle={CHART_TOOLTIP_ITEM_STYLE}
            labelStyle={CHART_TOOLTIP_LABEL_STYLE}
            labelFormatter={chartTooltipLabelFormatter}
          />

          {results.map((r, i) => {
            if (r.event === null) return null;
            let startIdx = i;
            while (startIdx > 0 && chartData[startIdx - 1].value > threshold) {
              startIdx -= 1;
            }
            return (
              <ReferenceArea
                key={`pulse-${i}`}
                x1={chartData[startIdx].ts}
                x2={chartData[i].ts}
                fill={
                  r.event === "valid"
                    ? "rgba(52, 211, 153, 0.18)"
                    : "rgba(248, 113, 113, 0.18)"
                }
                fillOpacity={1}
              />
            );
          })}

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
        </LineChart>
      </ResponsiveContainer>
    </div>
  );
}

const GlitchChart = React.memo(GlitchChartInner);
export default GlitchChart;
