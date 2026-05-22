import React from "react";
import {
  LineChart,
  Line,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  ResponsiveContainer,
  ReferenceLine,
} from "recharts";
import type { Tick, Band } from "../lib/wasm";

interface LiveChartProps {
  data: Tick[];
  sma?: (number | null)[];
  bollinger?: (Band | null)[];
  crosses?: { value: number; direction: string }[];
  width?: number;
  height?: number;
}

const CHART_COLORS = {
  price: "#0066cc",
  sma: "#ff6b35",
  upper: "rgba(76, 175, 80, 0.3)",
  lower: "rgba(76, 175, 80, 0.3)",
  middle: "#4caf50",
  cross: "#e53935",
};

export default function LiveChart({
  data,
  sma,
  bollinger,
  crosses,
  width = 600,
  height = 300,
}: LiveChartProps) {
  // Merge indicator data into chart points
  const chartData = data.map((tick, i) => {
    const point: Record<string, number | null> = {
      ts: tick.ts,
      value: tick.value,
    };
    if (sma && i < sma.length) point.sma = sma[i];
    if (bollinger && i < bollinger.length) {
      const b = bollinger[i];
      if (b) {
        point.bollUpper = b.upper;
        point.bollMiddle = b.middle;
        point.bollLower = b.lower;
      }
    }
    return point;
  });

  if (data.length === 0) {
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
        Press <strong style={{ margin: "0 0.25rem" }}>Play</strong> to start the
        data feed.
      </div>
    );
  }

  return (
    <div style={{ width: "100%", height }}>
      <ResponsiveContainer width="100%" height="100%">
        <LineChart
          data={chartData}
          margin={{ top: 5, right: 20, left: 10, bottom: 5 }}
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

          {/* Bollinger bands area */}
          {bollinger && bollinger.some((b) => b !== null) && (
            <>
              <Line
                type="monotone"
                dataKey="bollUpper"
                stroke={CHART_COLORS.upper}
                strokeWidth={1}
                dot={false}
                name="Upper"
              />
              <Line
                type="monotone"
                dataKey="bollLower"
                stroke={CHART_COLORS.lower}
                strokeWidth={1}
                dot={false}
                name="Lower"
              />
              <Line
                type="monotone"
                dataKey="bollMiddle"
                stroke={CHART_COLORS.middle}
                strokeWidth={1}
                dot={false}
                strokeDasharray="3 3"
                name="Middle"
              />
            </>
          )}

          {/* SMA line */}
          {sma && sma.some((v) => v !== null) && (
            <Line
              type="monotone"
              dataKey="sma"
              stroke={CHART_COLORS.sma}
              strokeWidth={1.5}
              strokeDasharray="4 2"
              dot={false}
              name="SMA"
              connectNulls={false}
            />
          )}

          {/* Price line */}
          <Line
            type="monotone"
            dataKey="value"
            stroke={CHART_COLORS.price}
            strokeWidth={2}
            dot={false}
            name="Price"
          />

          {/* Cross reference lines */}
          {crosses && crosses.length > 0 && (
            <ReferenceLine
              y={crosses[0].value}
              stroke={CHART_COLORS.cross}
              strokeDasharray="6 3"
              label={`Cross ${crosses[0].direction}`}
            />
          )}
        </LineChart>
      </ResponsiveContainer>
    </div>
  );
}
