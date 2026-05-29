import React, { useMemo } from "react";
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
} from "./sub/chartTheme";

interface LiveChartProps {
  data: Tick[];
  sma?: (number | null)[];
  bollinger?: (Band | null)[];
  crosses?: { value: number; direction: string }[];
  width?: number;
  height?: number;
}

const LIVE_CHART_COLORS = {
  upper: CHART_COLORS.band,
  lower: CHART_COLORS.band,
  middle: CHART_COLORS.middle,
} as const;

export default function LiveChart({
  data,
  sma,
  bollinger,
  crosses,
  height = 300,
}: LiveChartProps) {
  const chartData = useMemo(() => {
    return data.map((tick, i) => {
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
  }, [data, sma, bollinger]);

  if (data.length === 0) {
    return <EmptyChart height={height} message="Press Play to start the data feed." />;
  }

  const hasBollinger = bollinger && bollinger.some((b) => b !== null);
  const hasSma = sma && sma.some((v) => v !== null);

  return (
    <div style={{ width: "100%", height }}>
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

          {hasBollinger && (
            <>
              <Line
                type="monotone"
                dataKey="bollUpper"
                stroke={LIVE_CHART_COLORS.upper}
                strokeWidth={1}
                dot={false}
                name="Upper"
              />
              <Line
                type="monotone"
                dataKey="bollLower"
                stroke={LIVE_CHART_COLORS.lower}
                strokeWidth={1}
                dot={false}
                name="Lower"
              />
              <Line
                type="monotone"
                dataKey="bollMiddle"
                stroke={LIVE_CHART_COLORS.middle}
                strokeWidth={1}
                dot={false}
                strokeDasharray="3 3"
                name="Middle"
              />
            </>
          )}

          {hasSma && (
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

          <Line
            type="monotone"
            dataKey="value"
            stroke={CHART_COLORS.price}
            strokeWidth={2}
            dot={false}
            name="Price"
          />

          {crosses?.map((cross, idx) => (
            <ReferenceLine
              key={`cross-${idx}`}
              y={cross.value}
              stroke={CHART_COLORS.cross}
              strokeDasharray="6 3"
              label={`Cross ${cross.direction}`}
            />
          ))}
        </LineChart>
      </ResponsiveContainer>
    </div>
  );
}
