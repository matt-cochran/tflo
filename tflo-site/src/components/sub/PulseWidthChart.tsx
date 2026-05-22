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

/** One row of pulse-width demo output, aligned 1:1 with a tick. */
export interface PulseWidthResult {
  value: number;
  /** Fires on a pulse's falling edge; classifies the completed pulse. */
  event: "short" | "valid" | "long" | null;
}

interface PulseWidthChartProps {
  ticks: Tick[];
  results: PulseWidthResult[];
  config: { threshold: number; min: number; max: number };
  height: number;
}

/** Map a pulse classification to its display color. */
function classColor(event: string): string {
  switch (event) {
    case "short":
      return "#ef5350";
    case "long":
      return "#f59e0b";
    default:
      return "#26a69a";
  }
}

function PulseWidthChartInner({
  ticks,
  results,
  config,
  height,
}: PulseWidthChartProps) {
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

  const threshold = config.threshold;

  return (
    <div
      role="img"
      aria-label="Pulse-width detector chart"
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

          {/* Pulse tinting — one ReferenceArea per completed pulse */}
          {results.map((r, i) => {
            if (r.event === null) return null;
            // Walk back from the falling-edge tick to the rising edge.
            let startIdx = i;
            while (startIdx > 0 && chartData[startIdx - 1].value > threshold) {
              startIdx -= 1;
            }
            const color = classColor(r.event);
            return (
              <ReferenceArea
                key={`pulse-${i}`}
                x1={chartData[startIdx].ts}
                x2={chartData[i].ts}
                fill={color}
                fillOpacity={0.14}
                stroke="none"
                label={{
                  value: `${i - startIdx}t`,
                  position: "insideTop",
                  fontSize: 10,
                  fill: color,
                }}
              />
            );
          })}

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

          {/* Signal line with classification markers */}
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
                    key={`marker-${idx}`}
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
                      fill={classColor(point.event)}
                      stroke="#fff"
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

const PulseWidthChart = React.memo(PulseWidthChartInner);
export default PulseWidthChart;
