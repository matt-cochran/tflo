/**
 * Descriptors for every DemoChart demo: which synthetic feed to generate
 * and which wasm indicator/detector to run on it. The DemoChart component
 * and demo-compute.ts read this — there is exactly one place that knows
 * what each demo is.
 */

/** Every demo key. Must match DemoChart's `dataKey` prop. */
export type DemoKey =
  | "sma"
  | "rsi"
  | "bollinger"
  | "macd"
  | "cross"
  | "hysteresis"
  | "glitch"
  | "runt"
  | "pulse-width"
  | "window";

/** How the demo's feed is generated. */
export type FeedSpec =
  | { kind: "sine"; hz: number; amplitude: number; offset: number }
  | { kind: "noisyTrend"; drift: number; volatility: number; start: number }
  | {
      kind: "pulseTrain";
      baseline: number;
      pulses: { start: number; width: number; amplitude: number }[];
    }
  | {
      kind: "hovering";
      level: number;
      swingAmp: number;
      hz: number;
      noiseAmp: number;
    };

/** Which wasm computation runs on the feed. */
export type ComputeSpec =
  | { kind: "sma"; period: number }
  | { kind: "rsi"; period: number }
  | { kind: "bollinger"; period: number; multiplier: number }
  | { kind: "ema"; period: number }
  | { kind: "macd"; fast: number; slow: number; signal: number }
  | { kind: "cross"; threshold: number }
  | { kind: "hysteresis"; threshold: number; margin: number }
  | { kind: "glitch"; threshold: number; minDuration: number }
  | { kind: "runt"; low: number; high: number }
  | { kind: "pulse-width"; threshold: number; min: number; max: number }
  | { kind: "window"; low: number; high: number };

export interface DemoDescriptor {
  feed: FeedSpec;
  compute: ComputeSpec;
  /** Maximum jitter added to each looped feed regeneration. */
  jitter: number;
  /** Source-feed label shown in the DemoChart footer. */
  sourceFeed: string;
}

/** Number of ticks every demo feed contains. */
export const DEMO_DURATION = 200;

export const DEMO_CONFIG: Record<DemoKey, DemoDescriptor> = {
  sma: {
    feed: { kind: "sine", hz: 5, amplitude: 50, offset: 0 },
    compute: { kind: "sma", period: 20 },
    jitter: 2,
    sourceFeed: "sine",
  },
  rsi: {
    feed: { kind: "noisyTrend", drift: 0.05, volatility: 2.5, start: 50 },
    compute: { kind: "rsi", period: 14 },
    jitter: 0,
    sourceFeed: "noisy",
  },
  bollinger: {
    feed: { kind: "sine", hz: 5, amplitude: 50, offset: 0 },
    compute: { kind: "bollinger", period: 20, multiplier: 2 },
    jitter: 2,
    sourceFeed: "sine",
  },
  macd: {
    feed: { kind: "sine", hz: 5, amplitude: 50, offset: 0 },
    compute: { kind: "macd", fast: 12, slow: 26, signal: 9 },
    jitter: 1.5,
    sourceFeed: "sine",
  },
  cross: {
    feed: {
      kind: "pulseTrain",
      baseline: 50,
      pulses: [
        { start: 70, width: 19, amplitude: 7 },
        { start: 120, width: 20, amplitude: -10 },
        { start: 187, width: 13, amplitude: 7 },
      ],
    },
    compute: { kind: "cross", threshold: 55 },
    jitter: 1.5,
    sourceFeed: "step",
  },
  hysteresis: {
    feed: { kind: "hovering", level: 50, swingAmp: 7, hz: 3, noiseAmp: 2 },
    compute: { kind: "hysteresis", threshold: 50, margin: 4 },
    jitter: 0,
    sourceFeed: "hovering",
  },
  glitch: {
    feed: {
      kind: "pulseTrain",
      baseline: 20,
      pulses: [
        { start: 18, width: 6, amplitude: 60 },
        { start: 44, width: 18, amplitude: 60 },
        { start: 82, width: 4, amplitude: 60 },
        { start: 104, width: 26, amplitude: 60 },
        { start: 150, width: 7, amplitude: 60 },
        { start: 172, width: 16, amplitude: 60 },
      ],
    },
    compute: { kind: "glitch", threshold: 50, minDuration: 10 },
    jitter: 1.5,
    sourceFeed: "pulse",
  },
  runt: {
    feed: {
      kind: "pulseTrain",
      baseline: 20,
      pulses: [
        { start: 20, width: 14, amplitude: 50 },
        { start: 50, width: 16, amplitude: 85 },
        { start: 86, width: 12, amplitude: 45 },
        { start: 116, width: 18, amplitude: 90 },
        { start: 150, width: 14, amplitude: 48 },
        { start: 176, width: 15, amplitude: 88 },
      ],
    },
    compute: { kind: "runt", low: 40, high: 85 },
    jitter: 1.5,
    sourceFeed: "pulse",
  },
  "pulse-width": {
    feed: {
      kind: "pulseTrain",
      baseline: 20,
      pulses: [
        { start: 18, width: 5, amplitude: 55 },
        { start: 40, width: 15, amplitude: 55 },
        { start: 72, width: 32, amplitude: 55 },
        { start: 122, width: 12, amplitude: 55 },
        { start: 150, width: 6, amplitude: 55 },
        { start: 170, width: 20, amplitude: 55 },
      ],
    },
    compute: { kind: "pulse-width", threshold: 50, min: 8, max: 22 },
    jitter: 1.5,
    sourceFeed: "pulse",
  },
  window: {
    feed: { kind: "sine", hz: 2.5, amplitude: 32, offset: 50 },
    compute: { kind: "window", low: 38, high: 68 },
    jitter: 2,
    sourceFeed: "sine",
  },
};
