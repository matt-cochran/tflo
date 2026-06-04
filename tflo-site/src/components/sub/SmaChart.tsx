"use client";

import React, { useMemo } from "react";
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

interface SmaChartProps {
  ticks: Tick[];
  results: (number | null)[];
  height: number;
}

function SmaChartInner({ ticks, results, height }: SmaChartProps) {
  const chartData = useMemo(
    () =>
      ticks.map((tick, i) => ({
        ts: tick.ts,
        value: tick.value,
        sma: i < results.length ? results[i] : null,
      })),
    [ticks, results],
  );

  if (chartData.length === 0) {
    return <EmptyChart height={height} />;
  }

  const hasSma = results.some((v) => v !== null);

  return (
    <div
      role="img"
      aria-label="SMA chart showing price and moving average"
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
          <Line
            type="monotone"
            dataKey="value"
            stroke={CHART_COLORS.price}
            strokeWidth={2}
            dot={false}
            isAnimationActive={false}
            name="Price"
          />
          {hasSma && (
            <Line
              type="monotone"
              dataKey="sma"
              stroke={CHART_COLORS.sma}
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
