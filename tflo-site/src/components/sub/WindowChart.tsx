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

/** Marker fill colour for each window event. */
const EVENT_COLOR: Record<NonNullable<WindowResult["event"]>, string> = {
  entered: "#26a69a",
  exitedLow: "#f59e0b",
  exitedHigh: "#ef5350",
};

function WindowChartInner({
  ticks,
  results,
  config,
  height,
}: WindowChartProps) {
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

  const { low, high } = config;

  return (
    <div
      role="img"
      aria-label="Window detector chart"
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
              (dataMin: number) => Math.floor(Math.min(dataMin, low) - 5),
              (dataMax: number) => Math.ceil(Math.max(dataMax, high) + 5),
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

          {/* Shaded window band */}
          <ReferenceArea
            y1={low}
            y2={high}
            fill="rgba(38,166,154,0.10)"
            stroke="none"
          />

          {/* Low boundary line */}
          <ReferenceLine
            y={low}
            stroke="#26a69a"
            strokeDasharray="4 3"
            strokeOpacity={0.6}
            label={{
              value: `Low: ${low}`,
              position: "right",
              fontSize: 10,
              fill: "#26a69a",
            }}
          />

          {/* High boundary line */}
          <ReferenceLine
            y={high}
            stroke="#26a69a"
            strokeDasharray="4 3"
            strokeOpacity={0.6}
            label={{
              value: `High: ${high}`,
              position: "right",
              fontSize: 10,
              fill: "#26a69a",
            }}
          />

          {/* Signal line with event markers */}
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
                      stroke="#fff"
                      strokeWidth={1.5}
                    />
                  </svg>
                );
              }
              return null;
            }}
            name="Signal"
          />

          {/* Vertical dashed lines at event ticks */}
          {chartData.map((d, idx) =>
            d.event !== null ? (
              <ReferenceLine
                key={`event-line-${idx}`}
                x={d.ts}
                stroke="#26a69a"
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
