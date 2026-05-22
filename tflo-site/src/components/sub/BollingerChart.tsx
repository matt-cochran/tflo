"use client";

import React from "react";
import {
  ComposedChart,
  Line,
  Area,
  XAxis,
  YAxis,
  Tooltip,
  ResponsiveContainer,
  CartesianGrid,
} from "recharts";
import type { Tick, Band } from "../../lib/wasm";

interface BollingerChartProps {
  ticks: Tick[];
  results: (Band | null)[];
  height: number;
}

function BollingerChartInner({ ticks, results, height }: BollingerChartProps) {
  const chartData = ticks.map((tick, i) => {
    const point: Record<string, number | null> = {
      ts: tick.ts,
      value: tick.value,
    };
    if (i < results.length) {
      const b = results[i];
      if (b) {
        point.bollUpper = b.upper;
        point.bollMiddle = b.middle;
        point.bollLower = b.lower;
      }
    }
    return point;
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

  const hasBands = results.some((b) => b !== null);

  return (
    <div role="img" aria-label="Bollinger Bands chart with price and volatility bands" style={{ width: "100%", height }}>
      <ResponsiveContainer width="100%" height="100%">
        <ComposedChart data={chartData} margin={{ top: 5, right: 20, left: 10, bottom: 5 }}>
          <defs>
            <linearGradient id="bollBandFill" x1="0" y1="0" x2="0" y2="1">
              <stop offset="5%" stopColor="rgba(76, 175, 80, 0.08)" />
              <stop offset="95%" stopColor="rgba(76, 175, 80, 0.02)" />
            </linearGradient>
          </defs>
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

          {hasBands && (
            <>
              {/* Semi-transparent fill for the band region */}
              <Area
                type="monotone"
                dataKey="bollUpper"
                fill="url(#bollBandFill)"
                stroke="none"
                isAnimationActive={false}
              />
              {/* Upper band line */}
              <Line
                type="monotone"
                dataKey="bollUpper"
                stroke="rgba(76, 175, 80, 0.35)"
                strokeWidth={1}
                dot={false}
                isAnimationActive={false}
                name="Upper"
              />
              {/* Middle band line */}
              <Line
                type="monotone"
                dataKey="bollMiddle"
                stroke="#4caf50"
                strokeWidth={1}
                strokeDasharray="3 3"
                dot={false}
                isAnimationActive={false}
                name="Middle"
              />
              {/* Lower band line */}
              <Line
                type="monotone"
                dataKey="bollLower"
                stroke="rgba(76, 175, 80, 0.35)"
                strokeWidth={1}
                dot={false}
                isAnimationActive={false}
                name="Lower"
              />
            </>
          )}

          {/* Price line */}
          <Line
            type="monotone"
            dataKey="value"
            stroke="#0066cc"
            strokeWidth={2}
            dot={false}
            isAnimationActive={false}
            name="Price"
          />
        </ComposedChart>
      </ResponsiveContainer>
    </div>
  );
}

const BollingerChart = React.memo(BollingerChartInner);
export default BollingerChart;
