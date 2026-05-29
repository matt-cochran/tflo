"use client";

import React, { useMemo } from "react";
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

interface BollingerChartProps {
  ticks: Tick[];
  results: (Band | null)[];
  height: number;
}

function BollingerChartInner({ ticks, results, height }: BollingerChartProps) {
  const chartData = useMemo(
    () =>
      ticks.map((tick, i) => {
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
      }),
    [ticks, results],
  );

  if (chartData.length === 0) {
    return <EmptyChart height={height} />;
  }

  const hasBands = results.some((b) => b !== null);

  return (
    <div
      role="img"
      aria-label="Bollinger Bands chart with price and volatility bands"
      style={{ width: "100%", height }}
    >
      <ResponsiveContainer width="100%" height="100%">
        <ComposedChart data={chartData} margin={CHART_MARGIN}>
          <defs>
            <linearGradient id="bollBandFill" x1="0" y1="0" x2="0" y2="1">
              <stop offset="5%" stopColor={CHART_COLORS.bandFill} />
              <stop offset="95%" stopColor="rgba(34, 197, 94, 0.02)" />
            </linearGradient>
          </defs>
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

          {hasBands && (
            <>
              <Area
                type="monotone"
                dataKey="bollUpper"
                fill="url(#bollBandFill)"
                stroke="none"
                isAnimationActive={false}
              />
              <Line
                type="monotone"
                dataKey="bollUpper"
                stroke={CHART_COLORS.band}
                strokeWidth={1}
                dot={false}
                isAnimationActive={false}
                name="Upper"
              />
              <Line
                type="monotone"
                dataKey="bollMiddle"
                stroke={CHART_COLORS.middle}
                strokeWidth={1}
                strokeDasharray="3 3"
                dot={false}
                isAnimationActive={false}
                name="Middle"
              />
              <Line
                type="monotone"
                dataKey="bollLower"
                stroke={CHART_COLORS.band}
                strokeWidth={1}
                dot={false}
                isAnimationActive={false}
                name="Lower"
              />
            </>
          )}

          <Line
            type="monotone"
            dataKey="value"
            stroke={CHART_COLORS.price}
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
