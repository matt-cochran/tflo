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
import type { Tick } from "../../lib/wasm";

interface RsiChartProps {
  ticks: Tick[];
  results: (number | null)[];
  height: number;
}

function RsiChartInner({ ticks, results, height }: RsiChartProps) {
  const priceData = ticks.map((tick) => ({
    ts: tick.ts,
    value: tick.value,
  }));

  const rsiData = ticks.map((tick, i) => ({
    ts: tick.ts,
    rsi: i < results.length ? results[i] : null,
  }));

  const halfHeight = Math.max(Math.floor(height / 2), 80);

  if (priceData.length === 0) {
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
      aria-label="RSI chart with price panel and RSI indicator panel"
      style={{ width: "100%", height }}
    >
      {/* Upper panel: Price */}
      <div style={{ width: "100%", height: halfHeight }}>
        <ResponsiveContainer width="100%" height="100%">
          <LineChart
            data={priceData}
            margin={{ top: 5, right: 20, left: 10, bottom: 0 }}
          >
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
          </LineChart>
        </ResponsiveContainer>
      </div>

      {/* Lower panel: RSI */}
      <div
        style={{
          width: "100%",
          height: halfHeight,
          borderTop: "1px solid #e0e0e0",
        }}
      >
        <ResponsiveContainer width="100%" height="100%">
          <LineChart
            data={rsiData}
            margin={{ top: 0, right: 20, left: 10, bottom: 5 }}
          >
            <CartesianGrid strokeDasharray="3 3" stroke="#e0e0e0" />
            <XAxis dataKey="ts" tick={false} axisLine={{ stroke: "#ccc" }} />
            <YAxis
              domain={[0, 100]}
              ticks={[0, 30, 50, 70, 100]}
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
              formatter={(value: unknown) => {
                const v = value as number;
                return [v.toFixed(1), "RSI"];
              }}
            />
            {/* Overbought / oversold reference lines */}
            <ReferenceLine
              y={70}
              stroke="#e53935"
              strokeDasharray="4 3"
              strokeWidth={1}
              label={{
                value: "70",
                position: "right",
                fontSize: 10,
                fill: "#e53935",
              }}
            />
            <ReferenceLine
              y={30}
              stroke="#4caf50"
              strokeDasharray="4 3"
              strokeWidth={1}
              label={{
                value: "30",
                position: "right",
                fontSize: 10,
                fill: "#4caf50",
              }}
            />
            {/* Center line */}
            <ReferenceLine
              y={50}
              stroke="#999"
              strokeDasharray="2 2"
              strokeWidth={1}
            />
            {results.some((v) => v !== null) && (
              <Line
                type="monotone"
                dataKey="rsi"
                stroke="#7b1fa2"
                strokeWidth={1.5}
                dot={false}
                isAnimationActive={false}
                name="RSI"
                connectNulls={false}
              />
            )}
          </LineChart>
        </ResponsiveContainer>
      </div>
    </div>
  );
}

const RsiChart = React.memo(RsiChartInner);
export default RsiChart;
