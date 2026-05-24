"use client";

/**
 * LOST-NOT-DEAD: orphaned React component, never wired into any page.
 *
 * 327-line indicator-knobs control panel: 4 indicators (SMA, RSI,
 * Bollinger, Cross) with per-indicator enable + parameter controls,
 * a feed-source selector (sine / step / noisy / sawtooth), and a
 * play/pause state. Emits a `KnobParams` object on every change.
 * Written in the initial commit (single git history entry,
 * `b0b3516 init`) but no page or other component imports it.
 *
 * Probable original intent: the control surface for an interactive
 * playground page paired with `CelRulesEditor.tsx` (also orphan) —
 * the user would toggle indicators and tune parameters live while
 * the chart re-renders.
 *
 * Recovery: most natural fit is a new `/playground/knobs` page that
 * wires this to `PlaygroundChart.tsx` (which already exists and is
 * wired into `/playground`). The current playground passes static
 * config; this would let users tune it live.
 *
 * Discovered via StructureOS SOS025 on 2026-05-24 cleanup pass; left
 * in tree per "lost-not-dead" policy. See `tflo-core/src/semantics.rs`
 * for the parallel Rust case.
 */

import React, { useState, useCallback } from "react";

interface IndicatorSma {
  period: number;
  enabled: boolean;
}

interface IndicatorRsi {
  period: number;
  enabled: boolean;
}

interface IndicatorBollinger {
  period: number;
  multiplier: number;
  enabled: boolean;
}

interface IndicatorCross {
  threshold: number;
  enabled: boolean;
}

interface Indicators {
  sma?: IndicatorSma;
  rsi?: IndicatorRsi;
  bollinger?: IndicatorBollinger;
  cross?: IndicatorCross;
}

interface KnobParams {
  feed: "sine" | "step" | "noisy" | "sawtooth";
  speed: number;
  indicators: Indicators;
}

interface KnobPanelProps {
  onChange: (params: KnobParams) => void;
  initialParams?: Partial<KnobParams>;
}

const FEED_OPTIONS: { value: KnobParams["feed"]; label: string }[] = [
  { value: "sine", label: "Sine" },
  { value: "step", label: "Step" },
  { value: "noisy", label: "Noisy" },
  { value: "sawtooth", label: "Sawtooth" },
];

const DEFAULT_PARAMS: KnobParams = {
  feed: "sine",
  speed: 50,
  indicators: {
    sma: { period: 14, enabled: false },
    rsi: { period: 14, enabled: false },
    bollinger: { period: 20, multiplier: 2, enabled: false },
    cross: { threshold: 50, enabled: false },
  },
};

export default function KnobPanel({ onChange, initialParams }: KnobPanelProps) {
  const [params, setParams] = useState<KnobParams>(() => {
    const base = { ...DEFAULT_PARAMS };
    if (!initialParams) return base;

    return {
      feed: initialParams.feed ?? base.feed,
      speed: initialParams.speed ?? base.speed,
      indicators: {
        sma: { ...base.indicators.sma!, ...initialParams.indicators?.sma },
        rsi: { ...base.indicators.rsi!, ...initialParams.indicators?.rsi },
        bollinger: {
          ...base.indicators.bollinger!,
          ...initialParams.indicators?.bollinger,
        },
        cross: {
          ...base.indicators.cross!,
          ...initialParams.indicators?.cross,
        },
      },
    };
  });

  const [playing, setPlaying] = useState(false);

  const commit = useCallback(
    (updated: KnobParams) => {
      setParams(updated);
      onChange(updated);
    },
    [onChange],
  );

  const handleFeedChange = useCallback(
    (e: React.ChangeEvent<HTMLSelectElement>) => {
      const feed = e.target.value as KnobParams["feed"];
      commit({ ...params, feed });
    },
    [params, commit],
  );

  const handleSpeedChange = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const speed = Number(e.target.value);
      commit({ ...params, speed });
    },
    [params, commit],
  );

  const handlePlayPause = useCallback(() => {
    setPlaying((p) => !p);
  }, []);

  const handleReset = useCallback(() => {
    setPlaying(false);
    setParams(DEFAULT_PARAMS);
    onChange(DEFAULT_PARAMS);
  }, [onChange]);

  const handleIndicatorToggle = useCallback(
    (key: keyof Indicators) => (e: React.ChangeEvent<HTMLInputElement>) => {
      const ind = params.indicators[key];
      if (!ind) return;
      const updated = { ...ind, enabled: e.target.checked };
      commit({
        ...params,
        indicators: { ...params.indicators, [key]: updated },
      });
    },
    [params, commit],
  );

  const handleIndicatorNumber = useCallback(
    (key: keyof Indicators, field: string) =>
      (e: React.ChangeEvent<HTMLInputElement>) => {
        const ind = params.indicators[key];
        if (!ind) return;
        const updated = { ...ind, [field]: Number(e.target.value) };
        commit({
          ...params,
          indicators: { ...params.indicators, [key]: updated },
        });
      },
    [params, commit],
  );

  const btnClass =
    "px-3 py-1.5 rounded text-sm font-medium transition-colors focus:outline-none focus:ring-2 focus:ring-sky-500 focus:ring-offset-2 focus:ring-offset-slate-900";

  return (
    <div className="flex flex-col gap-4 rounded-lg border border-slate-700 bg-slate-900 p-4 text-white">
      {/* Header */}
      <div className="flex items-center justify-between">
        <h2 className="text-sm font-semibold uppercase tracking-wider text-slate-400">
          Controls
        </h2>
      </div>

      {/* Feed selector */}
      <div className="flex flex-col gap-1.5">
        <label
          htmlFor="feed-select"
          className="text-xs font-medium text-slate-400"
        >
          Feed
        </label>
        <select
          id="feed-select"
          value={params.feed}
          onChange={handleFeedChange}
          aria-label="Select data feed type"
          className="rounded-md border border-slate-700 bg-slate-800 px-3 py-1.5 text-sm text-white focus:outline-none focus:ring-2 focus:ring-sky-500"
        >
          {FEED_OPTIONS.map((opt) => (
            <option key={opt.value} value={opt.value}>
              {opt.label}
            </option>
          ))}
        </select>
      </div>

      {/* Play / Pause / Reset */}
      <div className="flex items-center gap-2">
        <button
          onClick={handlePlayPause}
          aria-label={playing ? "Pause" : "Play"}
          className={`${btnClass} border border-slate-700 bg-slate-800 text-white hover:bg-slate-700`}
        >
          {playing ? "⏸ Pause" : "▶ Play"}
        </button>
        <button
          onClick={handleReset}
          aria-label="Reset controls"
          className={`${btnClass} border border-slate-700 bg-slate-800 text-white hover:bg-slate-700`}
        >
          ↺ Reset
        </button>
      </div>

      {/* Speed slider */}
      <div className="flex flex-col gap-1.5">
        <div className="flex items-center justify-between">
          <label
            htmlFor="speed-slider"
            className="text-xs font-medium text-slate-400"
          >
            Speed
          </label>
          <span className="text-xs text-slate-500">{params.speed}</span>
        </div>
        <input
          id="speed-slider"
          type="range"
          min={1}
          max={100}
          value={params.speed}
          onChange={handleSpeedChange}
          aria-label="Animation speed"
          className="h-2 w-full cursor-pointer appearance-none rounded-lg bg-slate-700 accent-amber-500"
        />
      </div>

      {/* Divider */}
      <hr className="border-slate-700" />

      {/* Indicators */}
      <div className="flex flex-col gap-3">
        <h3 className="text-xs font-medium uppercase tracking-wider text-slate-400">
          Indicators
        </h3>

        {/* SMA */}
        {renderIndicatorRow(
          params,
          "sma",
          ["period"],
          { period: "Period" },
          handleIndicatorToggle,
          handleIndicatorNumber,
        )}

        {/* RSI */}
        {renderIndicatorRow(
          params,
          "rsi",
          ["period"],
          { period: "Period" },
          handleIndicatorToggle,
          handleIndicatorNumber,
        )}

        {/* Bollinger */}
        {renderIndicatorRow(
          params,
          "bollinger",
          ["period", "multiplier"],
          { period: "Period", multiplier: "Mult." },
          handleIndicatorToggle,
          handleIndicatorNumber,
        )}

        {/* Cross */}
        {renderIndicatorRow(
          params,
          "cross",
          ["threshold"],
          { threshold: "Threshold" },
          handleIndicatorToggle,
          handleIndicatorNumber,
        )}
      </div>
    </div>
  );

  function renderIndicatorRow(
    p: KnobParams,
    key: keyof Indicators,
    fields: string[],
    fieldLabels: Record<string, string>,
    onToggle: (
      k: keyof Indicators,
    ) => (e: React.ChangeEvent<HTMLInputElement>) => void,
    onNumber: (
      k: keyof Indicators,
      field: string,
    ) => (e: React.ChangeEvent<HTMLInputElement>) => void,
  ) {
    const ind = p.indicators[key];
    if (!ind) return null;
    const label = key.charAt(0).toUpperCase() + key.slice(1);

    return (
      <div className="flex flex-wrap items-center gap-2">
        <label className="flex items-center gap-2 text-sm">
          <input
            type="checkbox"
            checked={ind.enabled}
            onChange={onToggle(key)}
            aria-label={`Enable ${label} indicator`}
            className="h-4 w-4 rounded border-slate-600 bg-slate-800 text-amber-500 focus:ring-amber-500 focus:ring-offset-slate-900"
          />
          <span className="text-white">{label}</span>
        </label>
        {fields.map((field) => (
          <div key={field} className="flex items-center gap-1">
            <span className="text-xs text-slate-500">{fieldLabels[field]}</span>
            <input
              type="number"
              value={
                (ind as unknown as Record<string, unknown>)[field] as number
              }
              onChange={onNumber(key, field)}
              disabled={!ind.enabled}
              aria-label={`${label} ${fieldLabels[field]}`}
              className={`w-16 rounded-md border px-2 py-1 text-xs focus:outline-none focus:ring-2 focus:ring-sky-500 ${
                ind.enabled
                  ? "border-slate-700 bg-slate-800 text-white"
                  : "border-slate-800 bg-slate-900 text-slate-600"
              }`}
            />
          </div>
        ))}
      </div>
    );
  }
}
