"use client";

import React from "react";
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

function HysteresisChartInner({
  ticks,
  results,
  config,
  height,
}: HysteresisChartProps) {
  const chartData = ticks.map((t, i) => ({
    ts: t.ts,
    value: t.value,
    event: results[i]?.event ?? null,
  }));

  if (chartData.length === 0) {
    return (
      <div
        style={{
          height,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          color: "#666",
          background: "#f5f5f5",
          borderRadius: 8,
          fontSize: "0.9rem",
        }}
      >
        No data
      </div>
    );
  }

  const { threshold, margin } = config;

  return (
    <div
      role="img"
      aria-label="Hysteresis cross detection chart"
      style={{ width: "100%", height }}
    >
      <ResponsiveContainer width="100%" height="100%">
        <LineChart
          data={chartData}
          margin={{ top: 5, right: 20, left: 10, bottom: 5 }}
        >
          <CartesianGrid strokeDasharray="3 3" stroke="#e0e0e0" />
          <XAxis dataKey="ts" tick={false} axisLine={{ stroke: "#ccc" }} />
          <YAxis
            domain={[
              (min: number) =>
                Math.floor(Math.min(min, threshold - margin) - 5),
              (max: number) =>
                Math.ceil(Math.max(max, threshold + margin) + 5),
            ]}
            tick={{ fontSize: 11 }}
            axisLine={{ stroke: "#ccc" }}
          />
          <Tooltip
            contentStyle={{
              background: "#fff",
              border: "1px solid #ddd",
              borderRadius: 4,
              fontSize: 12,
            }}
            labelFormatter={(label) => `t=${label}`}
            formatter={(value: unknown, name: unknown) => {
              const v = value as number | null;
              return [v == null ? "—" : v.toFixed(2), name as string];
            }}
          />

          {/* Shaded dead-band around the threshold */}
          <ReferenceArea
            y1={threshold - margin}
            y2={threshold + margin}
            fill="rgba(229,57,53,0.08)"
          />

          {/* Faint dashed band edges */}
          <ReferenceLine
            y={threshold + margin}
            stroke="#e53935"
            strokeOpacity={0.4}
            strokeDasharray="3 3"
            strokeWidth={1}
          />
          <ReferenceLine
            y={threshold - margin}
            stroke="#e53935"
            strokeOpacity={0.4}
            strokeDasharray="3 3"
            strokeWidth={1}
          />

          {/* Threshold reference line */}
          <ReferenceLine
            y={threshold}
            stroke="#e53935"
            strokeDasharray="6 3"
            strokeWidth={1.5}
            label={{
              value: `Threshold: ${threshold}`,
              position: "right",
              fontSize: 10,
              fill: "#e53935",
            }}
          />

          {/* Signal line with hysteresis-cross markers */}
          <Line
            type="monotone"
            dataKey="value"
            stroke="#0066cc"
            strokeWidth={2}
            isAnimationActive={false}
            name="Signal"
            // eslint-disable-next-line @typescript-eslint/no-explicit-any
            dot={(dotProps: any) => {
              const cx = dotProps.cx;
              const cy = dotProps.cy;
              const idx = dotProps.index;
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
                      fill={point.event === "rising" ? "#26a69a" : "#ef5350"}
                      stroke="#fff"
                      strokeWidth={1.5}
                    />
                  </svg>
                );
              }
              return null;
            }}
          />

          {/* Vertical dashed lines at event ticks */}
          {chartData.map((d, idx) =>
            d.event !== null ? (
              <ReferenceLine
                key={`event-line-${idx}`}
                x={d.ts}
                stroke="#e53935"
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
