/**
 * Data feed generators for the tflo playground.
 *
 * Each generator produces a sequence of `Tick` objects that can be
 * consumed by the wasm indicator engine.
 */

import type { Tick } from "./wasm";

/**
 * Sine wave generator — smooth periodic signal.
 *
 * @param hz - Frequency in Hz (number of cycles per second of data).
 * @param amplitude - Peak amplitude of the wave.
 * @param duration - Number of ticks to generate.
 * @param phaseOffset - Optional phase offset in radians (default 0).
 */
export function sineWave(
  hz: number,
  amplitude: number,
  duration: number,
  phaseOffset = 0,
): Tick[] {
  const ticks: Tick[] = [];
  for (let i = 0; i < duration; i++) {
    ticks.push({
      ts: i,
      value: amplitude * Math.sin(2 * Math.PI * hz * (i / duration) + phaseOffset),
    });
  }
  return ticks;
}

/**
 * Step function — discontinuous signal with random jumps.
 *
 * @param jumps - Number of step changes.
 * @param duration - Total number of ticks.
 */
export function stepFunction(jumps: number, duration: number): Tick[] {
  const ticks: Tick[] = [];
  let value = 50;
  const jumpPoints = new Set<number>();
  for (let j = 0; j < jumps; j++) {
    jumpPoints.add(Math.floor(Math.random() * duration));
  }

  for (let i = 0; i < duration; i++) {
    if (jumpPoints.has(i)) {
      value += (Math.random() - 0.5) * 40;
      value = Math.max(0, Math.min(100, value));
    }
    ticks.push({ ts: i, value });
  }
  return ticks;
}

/**
 * Noisy trend — random walk with drift.
 *
 * @param drift - Average change per tick.
 * @param volatility - Standard deviation of random noise.
 * @param duration - Number of ticks.
 * @param startValue - Initial value (default 50).
 */
export function noisyTrend(
  drift: number,
  volatility: number,
  duration: number,
  startValue = 50,
): Tick[] {
  const ticks: Tick[] = [];
  let value = startValue;

  for (let i = 0; i < duration; i++) {
    value += drift + volatility * randn();
    value = Math.max(0, Math.min(100, value));
    ticks.push({ ts: i, value });
  }
  return ticks;
}

/**
 * Sawtooth wave — periodic ramp signal.
 *
 * @param freq - Frequency of the sawtooth.
 * @param amplitude - Peak amplitude.
 * @param duration - Number of ticks.
 */
export function sawtooth(freq: number, amplitude: number, duration: number): Tick[] {
  const ticks: Tick[] = [];
  for (let i = 0; i < duration; i++) {
    const phase = (freq * i) / duration;
    const value = amplitude * (phase - Math.floor(phase));
    ticks.push({ ts: i, value });
  }
  return ticks;
}

/**
 * Gap injector — introduces realistic data gaps into a feed.
 *
 * @param feed - Original tick data.
 * @param gapProb - Probability of a gap at each position (0–1).
 */
export function gapInjector(feed: Tick[], gapProb: number): Tick[] {
  return feed.filter(() => Math.random() > gapProb);
}

/**
 * Spike injector — adds transient noise spikes to a feed.
 *
 * @param feed - Original tick data.
 * @param spikeProb - Probability of a spike at each position (0–1).
 * @param magnitude - Multiplier for spike amplitude.
 */
export function spikeInjector(
  feed: Tick[],
  spikeProb: number,
  magnitude: number,
): Tick[] {
  return feed.map((tick) => {
    if (Math.random() < spikeProb) {
      const direction = Math.random() > 0.5 ? 1 : -1;
      return {
        ...tick,
        value: tick.value + direction * magnitude * Math.random() * 10,
      };
    }
    return tick;
  });
}

// ── Helpers ───────────────────────────────────────────────────────────

/** Box-Muller transform for standard normal random numbers. */
function randn(): number {
  let u = 0;
  let v = 0;
  while (u === 0) u = Math.random();
  while (v === 0) v = Math.random();
  return Math.sqrt(-2.0 * Math.log(u)) * Math.cos(2.0 * Math.PI * v);
}

// ── Seeded RNG & synthetic feeds ──────────────────────────────────────

/**
 * Deterministic seeded PRNG (mulberry32). Returns a function yielding
 * floats in [0, 1). Same seed → same sequence.
 */
export function createRng(seed: number): () => number {
  let s = seed | 0;
  return () => {
    s = (s + 0x6d2b79f5) | 0;
    let t = Math.imul(s ^ (s >>> 15), 1 | s);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** Standard-normal sample from a seeded RNG (Box-Muller). */
function randnFrom(rng: () => number): number {
  let u = 0;
  let v = 0;
  while (u === 0) u = rng();
  while (v === 0) v = rng();
  return Math.sqrt(-2.0 * Math.log(u)) * Math.cos(2.0 * Math.PI * v);
}

/** A single pulse spec for {@link pulseTrain}. */
export interface PulseSpec {
  start: number;
  width: number;
  amplitude: number;
}

/**
 * Rectangular pulse train — a flat baseline with pulses of a given
 * start tick, width, and amplitude.
 */
export function pulseTrain(
  duration: number,
  pulses: PulseSpec[],
  baseline: number,
): Tick[] {
  const ticks: Tick[] = [];
  for (let i = 0; i < duration; i++) {
    let value = baseline;
    for (const p of pulses) {
      if (i >= p.start && i < p.start + p.width) {
        value = baseline + p.amplitude;
        break;
      }
    }
    ticks.push({ ts: i, value });
  }
  return ticks;
}

/**
 * A signal that lingers around `level` — a slow swing plus seeded noise,
 * so it repeatedly pokes across a nearby threshold.
 */
export function hoveringSignal(
  duration: number,
  level: number,
  swingAmp: number,
  hz: number,
  noiseAmp: number,
  rng: () => number,
): Tick[] {
  const ticks: Tick[] = [];
  for (let i = 0; i < duration; i++) {
    const swing = swingAmp * Math.sin(2 * Math.PI * hz * (i / duration));
    ticks.push({ ts: i, value: level + swing + noiseAmp * randnFrom(rng) });
  }
  return ticks;
}

/**
 * Add bounded uniform jitter to a feed's values. `ts` is untouched.
 * `amount` is the maximum absolute perturbation; 0 returns the input.
 */
export function applyJitter(
  feed: Tick[],
  amount: number,
  rng: () => number,
): Tick[] {
  if (amount === 0) return feed;
  return feed.map((t) => ({
    ts: t.ts,
    value: t.value + (rng() * 2 - 1) * amount,
  }));
}
