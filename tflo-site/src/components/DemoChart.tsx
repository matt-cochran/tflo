"use client";

import React, { useCallback } from "react";
import SmaChart from "./sub/SmaChart";
import RsiChart from "./sub/RsiChart";
import BollingerChart from "./sub/BollingerChart";
import CrossChart from "./sub/CrossChart";
import MacdChart, { type MacdResult } from "./sub/MacdChart";
import HysteresisChart, {
  type HysteresisResult,
} from "./sub/HysteresisChart";
import GlitchChart, { type GlitchResult } from "./sub/GlitchChart";
import RuntChart, { type RuntResult } from "./sub/RuntChart";
import PulseWidthChart, {
  type PulseWidthResult,
} from "./sub/PulseWidthChart";
import WindowChart, { type WindowResult } from "./sub/WindowChart";
import type { Band, Tick } from "../lib/wasm";
import { DEMO_CONFIG, type DemoKey } from "../lib/demo-config";
import { useDemoFeed, SPEED_LABELS, type Speed } from "./hooks/useDemoFeed";

interface DemoChartProps {
  dataKey: DemoKey;
  loop?: boolean;
  autoplay?: boolean;
  height?: number;
}

const STATUS_PANEL_CLASS =
  "flex items-center justify-center rounded-md border border-border bg-bg-card text-sm text-text-muted px-4 text-center";

function StatusPanel({
  height,
  children,
  tone = "muted",
}: {
  height: number;
  children: React.ReactNode;
  tone?: "muted" | "error";
}) {
  const toneClass =
    tone === "error"
      ? "border-red-800 bg-red-900/40 text-red-300"
      : "border-border bg-bg-card text-text-muted";
  return (
    <div
      className={`${STATUS_PANEL_CLASS} ${toneClass}`}
      style={{ height }}
    >
      {children}
    </div>
  );
}

function renderIndicator(
  dataKey: DemoKey,
  visibleTicks: Tick[],
  visibleResults: unknown[],
  ticks: Tick[],
  results: unknown[],
  height: number,
) {
  switch (dataKey) {
    case "sma":
      return (
        <SmaChart
          ticks={visibleTicks}
          results={visibleResults as (number | null)[]}
          height={height}
        />
      );
    case "rsi":
      return (
        <RsiChart
          ticks={visibleTicks}
          results={visibleResults as (number | null)[]}
          height={height}
        />
      );
    case "bollinger":
      return (
        <BollingerChart
          ticks={visibleTicks}
          results={visibleResults as (Band | null)[]}
          height={height}
        />
      );
    case "cross": {
      const crossCompute = DEMO_CONFIG.cross.compute as {
        kind: "cross";
        threshold: number;
      };
      const crossEvents = (
        results as { value: number; cross: string | null }[]
      )
        .flatMap((r, i) =>
          r.cross !== null
            ? [
                {
                  ts: ticks[i]?.ts ?? 0,
                  value: r.value,
                  direction: r.cross as "above" | "below",
                },
              ]
            : [],
        )
        .filter(
          (ev) =>
            ev.ts >= (visibleTicks[0]?.ts ?? 0) &&
            ev.ts <= (visibleTicks[visibleTicks.length - 1]?.ts ?? 0),
        );
      return (
        <CrossChart
          ticks={visibleTicks}
          results={crossEvents}
          config={{ threshold: crossCompute.threshold }}
          height={height}
        />
      );
    }
    case "macd":
      return (
        <MacdChart
          ticks={visibleTicks}
          results={visibleResults as MacdResult[]}
          height={height}
        />
      );
    case "hysteresis": {
      const hysteresisCompute = DEMO_CONFIG.hysteresis.compute as {
        kind: "hysteresis";
        threshold: number;
        margin: number;
      };
      return (
        <HysteresisChart
          ticks={visibleTicks}
          results={visibleResults as HysteresisResult[]}
          config={{
            threshold: hysteresisCompute.threshold,
            margin: hysteresisCompute.margin,
          }}
          height={height}
        />
      );
    }
    case "glitch": {
      const glitchCompute = DEMO_CONFIG.glitch.compute as {
        kind: "glitch";
        threshold: number;
        minDuration: number;
      };
      return (
        <GlitchChart
          ticks={visibleTicks}
          results={visibleResults as GlitchResult[]}
          config={{
            threshold: glitchCompute.threshold,
            minDuration: glitchCompute.minDuration,
          }}
          height={height}
        />
      );
    }
    case "runt": {
      const runtCompute = DEMO_CONFIG.runt.compute as {
        kind: "runt";
        low: number;
        high: number;
      };
      return (
        <RuntChart
          ticks={visibleTicks}
          results={visibleResults as RuntResult[]}
          config={{ low: runtCompute.low, high: runtCompute.high }}
          height={height}
        />
      );
    }
    case "pulse-width": {
      const pulseWidthCompute = DEMO_CONFIG["pulse-width"].compute as {
        kind: "pulse-width";
        threshold: number;
        min: number;
        max: number;
      };
      return (
        <PulseWidthChart
          ticks={visibleTicks}
          results={visibleResults as PulseWidthResult[]}
          config={{
            threshold: pulseWidthCompute.threshold,
            min: pulseWidthCompute.min,
            max: pulseWidthCompute.max,
          }}
          height={height}
        />
      );
    }
    case "window": {
      const windowCompute = DEMO_CONFIG.window.compute as {
        kind: "window";
        low: number;
        high: number;
      };
      return (
        <WindowChart
          ticks={visibleTicks}
          results={visibleResults as WindowResult[]}
          config={{ low: windowCompute.low, high: windowCompute.high }}
          height={height}
        />
      );
    }
    default:
      return null;
  }
}

function speedButtonClass(active: boolean): string {
  return [
    "rounded border px-2.5 py-0.5 text-xs font-medium transition-colors",
    active
      ? "border-primary bg-primary text-white"
      : "border-border bg-bg-card text-text-muted hover:bg-bg-card-hover",
  ].join(" ");
}

function DemoChart({
  dataKey,
  loop: loopProp = true,
  autoplay = false,
  height = 300,
}: DemoChartProps) {
  const feed = useDemoFeed(dataKey, { autoplay, loop: loopProp });
  const {
    demo,
    wasmError,
    visibleTicks,
    visibleResults,
    playing,
    speed,
    jitter,
    progress,
    togglePlay,
    setSpeed,
    setJitter,
  } = feed;
  const sourceFeed = DEMO_CONFIG[dataKey].sourceFeed;

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === " " || e.key === "Space") {
        e.preventDefault();
        togglePlay();
      }
    },
    [togglePlay],
  );

  const handleSpeedChange = useCallback(
    (s: Speed) => setSpeed(s),
    [setSpeed],
  );

  let chartBody: React.ReactNode;
  if (wasmError) {
    chartBody = (
      <StatusPanel height={height} tone="error">
        Demo unavailable — wasm failed to load: {wasmError}
      </StatusPanel>
    );
  } else if (!demo) {
    chartBody = <StatusPanel height={height}>Loading demo…</StatusPanel>;
  } else if (visibleTicks.length === 0) {
    chartBody = (
      <StatusPanel height={height}>
        Press <strong className="mx-1 text-text">Play</strong> or{" "}
        <strong className="mx-1 text-text">Space</strong> to start the demo.
      </StatusPanel>
    );
  } else {
    chartBody = renderIndicator(
      dataKey,
      visibleTicks,
      visibleResults,
      demo.ticks,
      demo.results,
      height,
    );
  }

  return (
    <div className="flex flex-col gap-2">
      <div
        className="flex flex-wrap items-center gap-3 rounded-md border border-border bg-bg-card px-2 py-1.5 text-sm text-text"
        onKeyDown={handleKeyDown}
      >
        <button
          onClick={togglePlay}
          aria-label={playing ? "Pause animation" : "Play animation"}
          className="rounded border border-border bg-bg-card px-2.5 py-0.5 text-base leading-snug text-text hover:bg-bg-card-hover"
        >
          {playing ? "⏸" : "▶"}
        </button>

        <span className="text-xs text-text-muted">Speed:</span>
        {SPEED_LABELS.map(({ value, label }) => (
          <button
            key={value}
            onClick={() => handleSpeedChange(value)}
            aria-label={`Set speed to ${label}`}
            aria-pressed={speed === value}
            className={speedButtonClass(speed === value)}
          >
            {label}
          </button>
        ))}

        <span className="ml-2 text-xs text-text-muted">Jitter</span>
        <input
          type="range"
          min={0}
          max={10}
          step={0.5}
          value={jitter}
          onChange={(e) => setJitter(Number(e.target.value))}
          aria-label="Feed jitter amount"
          className="w-20 accent-accent"
        />
        <span className="min-w-[1.5rem] text-xs text-text-muted">{jitter}</span>

        <span className="ml-auto text-xs text-text-muted">
          {sourceFeed} &middot; {progress}%
        </span>
      </div>

      <div className="relative w-full" style={{ minHeight: height }}>
        {chartBody}
      </div>
    </div>
  );
}

export default DemoChart;
