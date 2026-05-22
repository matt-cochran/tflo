"use client";

import React from "react";
import {
  LineChart,
  Line,
  XAxis,
  YAxis,
  Tooltip,
  ResponsiveContainer,
  CartesianGrid,
} from "recharts";
import type { Tick } from "../../lib/wasm";

interface SmaChartProps {
  ticks: Tick[];
  results: (number | null)[];
  height: number;
}

function SmaChartInner({ ticks, results, height }: SmaChartProps) {
  const chartData = ticks.map((tick, i) => ({
    ts: tick.ts,
    value: tick.value,
    sma: i < results.length ? results[i] : null,
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

  return (
    <div role="img" aria-label="SMA chart showing price and moving average" style={{ width: "100%", height }}>
      <ResponsiveContainer width="100%" height="100%">
        <LineChart data={chartData} margin={{ top: 5, right: 20, left: 10, bottom: 5 }}>
          <CartesianGrid strokeDasharray="3 3" stroke="#e0e0e0" />
          <XAxis dataKey="ts" tick={false} axisLine={{ stroke: "#ccc" }} />
          <YAxis
            domain={["auto", "auto"]}
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
          <Line
            type="monotone"
            dataKey="value"
            stroke="#0066cc"
            strokeWidth={2}
            dot={false}
            isAnimationActive={false}
            name="Price"
          />
          {results.some((v) => v !== null) && (
            <Line
              type="monotone"
              dataKey="sma"
              stroke="#ff6b35"
              strokeWidth={1.5}
              strokeDasharray="4 2"
              dot={false}
              isAnimationActive={false}
              name="SMA"
              connectNulls={false}
            />
          )}
        </LineChart>
      </ResponsiveContainer>
    </div>
  );
}

const SmaChart = React.memo(SmaChartInner);
export default SmaChart;
