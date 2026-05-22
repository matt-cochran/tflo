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
  CartesianGrid,
} from "recharts";
import type { Tick, CrossEvent } from "../../lib/wasm";

interface CrossChartProps {
  ticks: Tick[];
  results: CrossEvent[];
  config: { threshold: number };
  height: number;
}

function CrossChartInner({ ticks, results, config, height }: CrossChartProps) {
  const chartData = ticks.map((tick) => ({
    ts: tick.ts,
    value: tick.value,
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

  const threshold = config.threshold;

  // Build a set of crossing timestamps for custom dot rendering
  const crossTsSet = new Set(results.map((ev) => ev.ts));

  return (
    <div
      role="img"
      aria-label="Cross detection chart showing price threshold crossings"
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
              (dataMin: number) => Math.floor(Math.min(dataMin, threshold) - 5),
              (dataMax: number) => Math.ceil(Math.max(dataMax, threshold) + 5),
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

          {/* Price line with cross markers */}
          <Line
            type="monotone"
            dataKey="value"
            stroke="#0066cc"
            strokeWidth={2}
            isAnimationActive={false}
            // eslint-disable-next-line @typescript-eslint/no-explicit-any
            dot={(dotProps: any) => {
              const cx = dotProps.cx;
              const cy = dotProps.cy;
              const idx = dotProps.index;
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
                      fill="#e53935"
                      stroke="#fff"
                      strokeWidth={1.5}
                    />
                  </svg>
                );
              }
              return null;
            }}
            name="Price"
          />

          {/* Vertical dashed lines at crossing points */}
          {results.map((ev, idx) => (
            <ReferenceLine
              key={`cross-${idx}`}
              x={ev.ts}
              stroke="#e53935"
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
