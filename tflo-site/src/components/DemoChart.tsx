"use client";

import React, { useState, useEffect, useRef, useCallback } from "react";
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
import { initWasm, type Band } from "../lib/wasm";
import { computeDemo, type DemoData } from "../lib/demo-compute";
import { DEMO_CONFIG, type DemoKey } from "../lib/demo-config";

type DataKey = DemoKey;

interface DemoChartProps {
  dataKey: DataKey;
  loop?: boolean;
  autoplay?: boolean;
  height?: number;
}

const WINDOW_SIZE = 60;
const BASE_INTERVAL_MS = 100;

type Speed = 1 | 2 | 4;

const SPEED_LABELS: { value: Speed; label: string }[] = [
  { value: 1, label: "1x" },
  { value: 2, label: "2x" },
  { value: 4, label: "4x" },
];

function DemoChart({
  dataKey,
  loop: loopProp = true,
  autoplay = false,
  height = 300,
}: DemoChartProps) {
  const baseDescriptor = DEMO_CONFIG[dataKey];
  const sourceFeed = baseDescriptor.sourceFeed;

  // User-controllable jitter, initialised to the demo's curated default.
  const [jitter, setJitter] = useState(baseDescriptor.jitter);

  // Live wasm-computed demo data; null until wasm has loaded and the
  // first feed has been computed.
  const [demo, setDemo] = useState<DemoData | null>(null);
  const [wasmError, setWasmError] = useState<string | null>(null);
  const loopSeedRef = useRef(0);
  // Latest jitter — read inside the animation interval without re-subscribing.
  const jitterRef = useRef(jitter);
  jitterRef.current = jitter;

  // Load wasm once, then compute the first feed.
  useEffect(() => {
    let cancelled = false;
    initWasm()
      .then(() => {
        if (cancelled) return;
        loopSeedRef.current = Math.floor(Math.random() * 1e9);
        setDemo(
          computeDemo(
            { ...baseDescriptor, jitter: jitterRef.current },
            loopSeedRef.current,
          ),
        );
      })
      .catch((err: unknown) => {
        if (!cancelled) {
          setWasmError(err instanceof Error ? err.message : String(err));
        }
      });
    return () => {
      cancelled = true;
    };
  }, [dataKey]);

  // Recompute immediately when the user changes jitter.
  useEffect(() => {
    if (!demo) return;
    loopSeedRef.current += 1;
    setDemo(computeDemo({ ...baseDescriptor, jitter }, loopSeedRef.current));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [jitter]);

  const ticks = demo?.ticks ?? [];
  const results = demo?.results ?? [];

  const [playing, setPlaying] = useState(autoplay);
  const [speed, setSpeed] = useState<Speed>(1);
  const [pointer, setPointer] = useState(0);
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // ---- Animation loop ----
  useEffect(() => {
    if (!playing || ticks.length === 0) {
      if (intervalRef.current) {
        clearInterval(intervalRef.current);
        intervalRef.current = null;
      }
      return;
    }

    const intervalMs = BASE_INTERVAL_MS / speed;

    intervalRef.current = setInterval(() => {
      setPointer((prev) => {
        const next = prev + 1;
        if (next >= ticks.length) {
          if (loopProp) {
            // Fresh jittered feed each loop — live, never the same twice.
            loopSeedRef.current += 1;
            setDemo(
              computeDemo(
                { ...baseDescriptor, jitter: jitterRef.current },
                loopSeedRef.current,
              ),
            );
            return 0;
          }
          return Math.max(0, ticks.length - 1);
        }
        return next;
      });
    }, intervalMs);

    return () => {
      if (intervalRef.current) {
        clearInterval(intervalRef.current);
        intervalRef.current = null;
      }
    };
  }, [playing, speed, ticks.length, loopProp]);

  // ---- Derived data ----
  const windowStart = Math.max(0, pointer - WINDOW_SIZE);
  const visibleTicks = ticks.slice(windowStart, pointer + 1);
  const visibleResults = results.slice(windowStart, pointer + 1);

  // ---- Controls ----
  const togglePlay = useCallback(() => {
    setPlaying((p) => !p);
  }, []);

  const handleSpeedChange = useCallback((s: Speed) => {
    setSpeed(s);
  }, []);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === " " || e.key === "Space") {
        e.preventDefault();
        togglePlay();
      }
    },
    [togglePlay],
  );

  // ---- Keyboard shortcut (global) ----
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === " " || e.key === "Space") {
        const target = e.target as HTMLElement;
        if (
          target.tagName === "INPUT" ||
          target.tagName === "TEXTAREA" ||
          target.isContentEditable
        ) {
          return;
        }
        e.preventDefault();
        togglePlay();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [togglePlay]);

  // ---- Sub-chart rendering ----
  const renderChart = () => {
    if (wasmError) {
      return (
        <div
          style={{
            height,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            color: "#c0392b",
            background: "#fdf0ef",
            borderRadius: 8,
            fontSize: "0.85rem",
            padding: "0 1rem",
            textAlign: "center",
          }}
        >
          Demo unavailable — wasm failed to load: {wasmError}
        </div>
      );
    }
    if (!demo) {
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
          Loading demo…
        </div>
      );
    }

    if (visibleTicks.length === 0) {
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
          Press <strong style={{ margin: "0 0.25rem" }}>Play</strong> or{" "}
          <strong style={{ margin: "0 0.25rem" }}>Space</strong> to start the
          demo.
        </div>
      );
    }

    const indicator = dataKey;

    switch (indicator) {
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
            config={{
              low: runtCompute.low,
              high: runtCompute.high,
            }}
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
            config={{
              low: windowCompute.low,
              high: windowCompute.high,
            }}
            height={height}
          />
        );
      }
      default:
        return null;
    }
  };

  // ---- Progress info ----
  const progress =
    ticks.length > 0 ? Math.round(((pointer + 1) / ticks.length) * 100) : 0;

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
      {/* Controls bar */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 12,
          padding: "6px 8px",
          background: "#fafafa",
          border: "1px solid #e0e0e0",
          borderRadius: 6,
          fontSize: 13,
        }}
        onKeyDown={handleKeyDown}
      >
        {/* Play/Pause */}
        <button
          onClick={togglePlay}
          aria-label={playing ? "Pause animation" : "Play animation"}
          style={{
            background: "none",
            border: "1px solid #ccc",
            borderRadius: 4,
            cursor: "pointer",
            padding: "2px 10px",
            fontSize: 16,
            lineHeight: 1.4,
            color: "#333",
          }}
        >
          {playing ? "⏸" : "▶"}
        </button>

        {/* Speed selector */}
        <span style={{ color: "#666", fontSize: 12 }}>Speed:</span>
        {SPEED_LABELS.map(({ value, label }) => (
          <button
            key={value}
            onClick={() => handleSpeedChange(value)}
            aria-label={`Set speed to ${label}`}
            aria-pressed={speed === value}
            style={{
              background: speed === value ? "#0066cc" : "transparent",
              color: speed === value ? "#fff" : "#333",
              border: `1px solid ${speed === value ? "#0066cc" : "#ccc"}`,
              borderRadius: 4,
              cursor: "pointer",
              padding: "2px 10px",
              fontSize: 12,
              lineHeight: 1.4,
            }}
          >
            {label}
          </button>
        ))}

        {/* Jitter slider */}
        <span style={{ color: "#666", fontSize: 12, marginLeft: 8 }}>
          Jitter
        </span>
        <input
          type="range"
          min={0}
          max={10}
          step={0.5}
          value={jitter}
          onChange={(e) => setJitter(Number(e.target.value))}
          aria-label="Feed jitter amount"
          style={{ width: 70 }}
        />
        <span style={{ color: "#999", fontSize: 11, minWidth: 22 }}>
          {jitter}
        </span>

        {/* Feed label */}
        <span style={{ marginLeft: "auto", color: "#999", fontSize: 11 }}>
          {sourceFeed} &middot; {progress}%
        </span>
      </div>

      {/* Chart area */}
      <div
        style={{
          position: "relative",
          width: "100%",
          minHeight: height,
        }}
      >
        {renderChart()}
      </div>
    </div>
  );
}

export default DemoChart;
