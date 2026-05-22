"use client";

import React from "react";
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

function MacdChartInner({ ticks, results, height }: MacdChartProps) {
  const chartData = ticks.map((tick, i) => {
    const r = results[i];
    return {
      ts: tick.ts,
      macd: r?.macd ?? null,
      signal: r?.signal ?? null,
      histogram: r?.histogram ?? null,
      cross: r?.cross ?? null,
    };
  });

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

  return (
    <div
      role="img"
      aria-label="MACD chart showing the MACD line crossing its signal line, with histogram"
      style={{ width: "100%", height }}
    >
      <ResponsiveContainer width="100%" height="100%">
        <ComposedChart
          data={chartData}
          margin={{ top: 5, right: 20, left: 10, bottom: 5 }}
        >
          <CartesianGrid strokeDasharray="3 3" stroke="#e0e0e0" />
          <XAxis dataKey="ts" tick={false} axisLine={{ stroke: "#ccc" }} />
          <YAxis
            domain={[
              (dataMin: number) => Math.floor(Math.min(dataMin, 0)),
              (dataMax: number) => Math.ceil(Math.max(dataMax, 0)),
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

          {/* Zero line — the MACD oscillates around it */}
          <ReferenceLine y={0} stroke="#999" strokeWidth={1} />

          {/* Histogram (MACD − signal): green when positive, red when negative */}
          <Bar dataKey="histogram" name="Histogram" isAnimationActive={false}>
            {chartData.map((d, i) => (
              <Cell
                key={`hist-${i}`}
                fill={
                  (d.histogram ?? 0) >= 0
                    ? "rgba(38, 166, 154, 0.55)"
                    : "rgba(239, 83, 80, 0.55)"
                }
              />
            ))}
          </Bar>

          {/* MACD line with cross markers */}
          <Line
            type="monotone"
            dataKey="macd"
            stroke="#0066cc"
            strokeWidth={2}
            isAnimationActive={false}
            connectNulls={false}
            name="MACD"
            // eslint-disable-next-line @typescript-eslint/no-explicit-any
            dot={(dotProps: any) => {
              const cx = dotProps.cx;
              const cy = dotProps.cy;
              const idx = dotProps.index;
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
                      fill={point.cross === "above" ? "#26a69a" : "#ef5350"}
                      stroke="#fff"
                      strokeWidth={1.5}
                    />
                  </svg>
                );
              }
              return null;
            }}
          />

          {/* Signal line */}
          <Line
            type="monotone"
            dataKey="signal"
            stroke="#ff6b35"
            strokeWidth={1.5}
            strokeDasharray="4 2"
            dot={false}
            isAnimationActive={false}
            connectNulls={false}
            name="Signal"
          />

          {/* Vertical dashed lines at crossing points */}
          {chartData.map((d, idx) =>
            d.cross !== null ? (
              <ReferenceLine
                key={`cross-line-${idx}`}
                x={d.ts}
                stroke="#e53935"
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
