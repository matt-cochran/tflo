/**
 * Builds a demo's feed and runs the real wasm computation on it.
 *
 * Indicators (sma/rsi/bollinger/ema/macd) are computed in one batch wasm
 * call. Detectors (cross/hysteresis/glitch/runt/pulse-width/window) are fed
 * tick-by-tick into a streaming wasm detector struct. No indicator or
 * detector logic is implemented here — every number comes from wasm.
 */

import type { Tick, MacdPoint } from "./wasm";
import {
  computeSma,
  computeRsi,
  computeBollinger,
  computeEma,
  computeMacd,
  CrossDetector,
  HysteresisCrossDetector,
  GlitchFilter,
  RuntDetector,
  PulseWidthDetector,
  WindowDetector,
} from "./wasm";
import { createRng, pulseTrain, hoveringSignal, applyJitter } from "./feeds";
import {
  DEMO_DURATION,
  type DemoDescriptor,
  type FeedSpec,
} from "./demo-config";

/** A per-tick result row. Shape depends on the demo's compute kind. */
export type DemoResult =
  | number
  | null
  | MacdPoint
  | { middle: number; upper: number; lower: number }
  | { value: number; cross: string | null }
  | { value: number; event: string | null };

export interface DemoData {
  ticks: Tick[];
  results: DemoResult[];
}

/** Build a feed from a {@link FeedSpec}. */
function buildFeed(spec: FeedSpec, rng: () => number): Tick[] {
  switch (spec.kind) {
    case "sine": {
      const ticks: Tick[] = [];
      for (let i = 0; i < DEMO_DURATION; i++) {
        ticks.push({
          ts: i,
          value:
            spec.amplitude *
              Math.sin(2 * Math.PI * spec.hz * (i / DEMO_DURATION)) +
            spec.offset,
        });
      }
      return ticks;
    }
    case "noisyTrend": {
      const ticks: Tick[] = [];
      let value = spec.start;
      for (let i = 0; i < DEMO_DURATION; i++) {
        // standard-normal step from the seeded rng
        let u = 0;
        let v = 0;
        while (u === 0) u = rng();
        while (v === 0) v = rng();
        const g = Math.sqrt(-2 * Math.log(u)) * Math.cos(2 * Math.PI * v);
        value += spec.drift + spec.volatility * g;
        value = Math.max(0, Math.min(100, value));
        ticks.push({ ts: i, value });
      }
      return ticks;
    }
    case "pulseTrain":
      return pulseTrain(DEMO_DURATION, spec.pulses, spec.baseline);
    case "hovering":
      return hoveringSignal(
        DEMO_DURATION,
        spec.level,
        spec.swingAmp,
        spec.hz,
        spec.noiseAmp,
        rng,
      );
  }
}

/**
 * Generate a fresh jittered feed and compute a demo's results via wasm.
 * Takes a {@link DemoDescriptor} directly — inline demos pass the curated
 * descriptor from `DEMO_CONFIG`; the central demo passes one built from UI
 * controls. `seed` varies per loop so each iteration shows different live
 * data. Requires `initWasm()` to have resolved.
 */
export function computeDemo(
  descriptor: DemoDescriptor,
  seed: number,
): DemoData {
  const rng = createRng(seed);
  const ticks = applyJitter(buildFeed(descriptor.feed, rng), descriptor.jitter, rng);
  const c = descriptor.compute;

  switch (c.kind) {
    case "sma":
      return { ticks, results: computeSma(ticks, { period: c.period }) };
    case "ema":
      return { ticks, results: computeEma(ticks, { period: c.period }) };
    case "rsi":
      return { ticks, results: computeRsi(ticks, { period: c.period }) };
    case "bollinger":
      return {
        ticks,
        results: computeBollinger(ticks, {
          period: c.period,
          multiplier: c.multiplier,
        }),
      };
    case "macd":
      return {
        ticks,
        results: computeMacd(ticks, {
          fast: c.fast,
          slow: c.slow,
          signal: c.signal,
        }),
      };
    case "cross": {
      const det = new CrossDetector();
      const results = ticks.map((t) => {
        const ev = det.update(t.value, c.threshold);
        return {
          value: t.value,
          cross:
            ev === "rising" ? "above" : ev === "falling" ? "below" : null,
        };
      });
      det.free();
      return { ticks, results };
    }
    case "hysteresis": {
      const det = new HysteresisCrossDetector(c.margin);
      const results = ticks.map((t) => {
        const ev = det.update(t.value, c.threshold);
        return { value: t.value, event: ev === "none" ? null : ev };
      });
      det.free();
      return { ticks, results };
    }
    case "glitch": {
      const det = new GlitchFilter(c.threshold, c.minDuration);
      const results = ticks.map((t) => {
        const ev = det.update(t.value, t.ts);
        return { value: t.value, event: ev === "none" ? null : ev };
      });
      det.free();
      return { ticks, results };
    }
    case "runt": {
      const det = new RuntDetector(c.low, c.high);
      const results = ticks.map((t) => {
        const ev = det.update(t.value);
        return { value: t.value, event: ev === "none" ? null : ev };
      });
      det.free();
      return { ticks, results };
    }
    case "pulse-width": {
      const det = new PulseWidthDetector(c.threshold, c.min, c.max);
      const results = ticks.map((t) => {
        const ev = det.update(t.value, t.ts);
        return { value: t.value, event: ev === "none" ? null : ev };
      });
      det.free();
      return { ticks, results };
    }
    case "window": {
      const det = new WindowDetector(c.low, c.high);
      const results = ticks.map((t) => {
        const ev = det.update(t.value);
        return { value: t.value, event: ev === "none" ? null : ev };
      });
      det.free();
      return { ticks, results };
    }
  }
}
