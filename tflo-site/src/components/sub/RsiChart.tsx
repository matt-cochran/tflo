"use client";

import React, { useMemo } from "react";
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

interface RsiChartProps {
  ticks: Tick[];
  results: (number | null)[];
  height: number;
}

const PRICE_MARGIN = { ...CHART_MARGIN, bottom: 0 } as const;
const RSI_MARGIN = { ...CHART_MARGIN, top: 0 } as const;
const RSI_TICKS = [0, 30, 50, 70, 100];

const RSI_LINE_OVERBOUGHT_LABEL = {
  value: "70",
  position: "right",
  fontSize: 10,
  fill: CHART_COLORS.cross,
} as const;
const RSI_LINE_OVERSOLD_LABEL = {
  value: "30",
  position: "right",
  fontSize: 10,
  fill: CHART_COLORS.middle,
} as const;

function rsiFormatter(value: unknown): [string, string] {
  const v = value as number;
  return [v.toFixed(1), "RSI"];
}

function RsiChartInner({ ticks, results, height }: RsiChartProps) {
  const priceData = useMemo(
    () => ticks.map((tick) => ({ ts: tick.ts, value: tick.value })),
    [ticks],
  );

  const rsiData = useMemo(
    () =>
      ticks.map((tick, i) => ({
        ts: tick.ts,
        rsi: i < results.length ? results[i] : null,
      })),
    [ticks, results],
  );

  const halfHeight = Math.max(Math.floor(height / 2), 80);

  if (priceData.length === 0) {
    return <EmptyChart height={height} />;
  }

  return (
    <div
      role="img"
      aria-label="RSI chart with price panel and RSI indicator panel"
      style={{ width: "100%", height }}
    >
      <div style={{ width: "100%", height: halfHeight }}>
        <ResponsiveContainer width="100%" height="100%">
          <LineChart data={priceData} margin={PRICE_MARGIN}>
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
          </LineChart>
        </ResponsiveContainer>
      </div>

      <div
        style={{
          width: "100%",
          height: halfHeight,
          borderTop: `1px solid ${CHART_GRID_STROKE}`,
        }}
      >
        <ResponsiveContainer width="100%" height="100%">
          <LineChart data={rsiData} margin={RSI_MARGIN}>
            <CartesianGrid strokeDasharray="3 3" stroke={CHART_GRID_STROKE} />
            <XAxis dataKey="ts" tick={false} axisLine={CHART_AXIS_LINE} />
            <YAxis
              domain={[0, 100]}
              ticks={RSI_TICKS}
              tick={CHART_AXIS_TICK}
              axisLine={CHART_AXIS_LINE}
            />
            <Tooltip
              contentStyle={CHART_TOOLTIP_STYLE}
              itemStyle={CHART_TOOLTIP_ITEM_STYLE}
              labelStyle={CHART_TOOLTIP_LABEL_STYLE}
              labelFormatter={chartTooltipLabelFormatter}
              formatter={rsiFormatter}
            />
            <ReferenceLine
              y={70}
              stroke={CHART_COLORS.cross}
              strokeDasharray="4 3"
              strokeWidth={1}
              label={RSI_LINE_OVERBOUGHT_LABEL}
            />
            <ReferenceLine
              y={30}
              stroke={CHART_COLORS.middle}
              strokeDasharray="4 3"
              strokeWidth={1}
              label={RSI_LINE_OVERSOLD_LABEL}
            />
            <ReferenceLine
              y={50}
              stroke={CHART_COLORS.zero}
              strokeDasharray="2 2"
              strokeWidth={1}
            />
            {results.some((v) => v !== null) && (
              <Line
                type="monotone"
                dataKey="rsi"
                stroke={CHART_COLORS.rsi}
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
