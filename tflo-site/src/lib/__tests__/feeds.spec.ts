import { describe, it, expect } from "vitest";
import {
  sineWave,
  stepFunction,
  noisyTrend,
  sawtooth,
  gapInjector,
  spikeInjector,
  createRng,
  pulseTrain,
  hoveringSignal,
  applyJitter,
} from "../feeds";
import type { Tick } from "../wasm";

describe("sineWave", () => {
  it("generates the correct number of ticks", () => {
    const ticks = sineWave(5, 40, 100);
    expect(ticks).toHaveLength(100);
  });

  it("starts at approximately 0", () => {
    const ticks = sineWave(5, 40, 100);
    expect(ticks[0].value).toBeCloseTo(0, 1);
  });

  it("oscillates within amplitude range", () => {
    const ticks = sineWave(5, 40, 100);
    for (const t of ticks) {
      expect(t.value).toBeGreaterThanOrEqual(-40);
      expect(t.value).toBeLessThanOrEqual(40);
    }
  });

  it("has monotonically increasing timestamps", () => {
    const ticks = sineWave(5, 40, 100);
    for (let i = 1; i < ticks.length; i++) {
      expect(ticks[i].ts).toBeGreaterThan(ticks[i - 1].ts);
    }
  });

  it("respects phase offset", () => {
    const a = sineWave(5, 40, 100, 0);
    const b = sineWave(5, 40, 100, Math.PI);
    // At index 25 (quarter cycle), values should be opposite
    expect(a[25].value).toBeCloseTo(-b[25].value, 2);
  });
});

describe("stepFunction", () => {
  it("generates the correct number of ticks", () => {
    const ticks = stepFunction(5, 100);
    expect(ticks).toHaveLength(100);
  });

  it("has monotonically increasing timestamps", () => {
    const ticks = stepFunction(5, 100);
    for (let i = 1; i < ticks.length; i++) {
      expect(ticks[i].ts).toBeGreaterThan(ticks[i - 1].ts);
    }
  });

  it("stays within 0-100 range", () => {
    const ticks = stepFunction(5, 100);
    for (const t of ticks) {
      expect(t.value).toBeGreaterThanOrEqual(0);
      expect(t.value).toBeLessThanOrEqual(100);
    }
  });

  it("has consistent value between jumps", () => {
    const ticks = stepFunction(1, 50);
    const distinct = new Set(ticks.map((t) => t.value));
    expect(distinct.size).toBeLessThanOrEqual(2);
  });

  it("works with zero jumps", () => {
    const ticks = stepFunction(0, 10);
    expect(ticks).toHaveLength(10);
    const distinct = new Set(ticks.map((t) => t.value));
    expect(distinct.size).toBe(1);
  });
});

describe("noisyTrend", () => {
  it("generates the correct number of ticks", () => {
    const ticks = noisyTrend(0.05, 1.5, 100);
    expect(ticks).toHaveLength(100);
  });

  it("respects start value", () => {
    const ticks = noisyTrend(0, 0, 10, 75);
    expect(ticks[0].value).toBe(75);
  });

  it("stays within 0-100 range", () => {
    const ticks = noisyTrend(0.05, 2.0, 1000);
    for (const t of ticks) {
      expect(t.value).toBeGreaterThanOrEqual(0);
      expect(t.value).toBeLessThanOrEqual(100);
    }
  });

  it("trends upward with positive drift", () => {
    const ticks = noisyTrend(0.5, 0.1, 200, 50);
    const endAvg = ticks.slice(-20).reduce((s, t) => s + t.value, 0) / 20;
    const startAvg = ticks.slice(0, 20).reduce((s, t) => s + t.value, 0) / 20;
    expect(endAvg).toBeGreaterThan(startAvg);
  });
});

describe("sawtooth", () => {
  it("generates the correct number of ticks", () => {
    const ticks = sawtooth(3, 50, 100);
    expect(ticks).toHaveLength(100);
  });

  it("starts at 0", () => {
    const ticks = sawtooth(3, 50, 100);
    expect(ticks[0].value).toBe(0);
  });

  it("stays within amplitude", () => {
    const ticks = sawtooth(3, 50, 100);
    for (const t of ticks) {
      expect(t.value).toBeGreaterThanOrEqual(0);
      expect(t.value).toBeLessThan(50);
    }
  });
});

describe("gapInjector", () => {
  it("removes some ticks based on probability", () => {
    const feed: Tick[] = Array.from({ length: 1000 }, (_, i) => ({
      ts: i,
      value: Math.random() * 100,
    }));
    const result = gapInjector(feed, 0.5);
    expect(result.length).toBeLessThan(feed.length);
    expect(result.length).toBeGreaterThan(0);
  });

  it("passes through all ticks with zero probability", () => {
    const feed: Tick[] = Array.from({ length: 10 }, (_, i) => ({
      ts: i,
      value: i,
    }));
    const result = gapInjector(feed, 0);
    expect(result).toHaveLength(10);
  });

  it("removes all ticks with probability 1", () => {
    const feed: Tick[] = Array.from({ length: 10 }, (_, i) => ({
      ts: i,
      value: i,
    }));
    const result = gapInjector(feed, 1);
    expect(result).toHaveLength(0);
  });
});

describe("spikeInjector", () => {
  it("preserves tick count", () => {
    const feed: Tick[] = Array.from({ length: 100 }, (_, i) => ({
      ts: i,
      value: 50,
    }));
    const result = spikeInjector(feed, 0.5, 2);
    expect(result).toHaveLength(100);
  });

  it("modifies some values with non-zero probability", () => {
    const feed: Tick[] = Array.from({ length: 1000 }, (_, i) => ({
      ts: i,
      value: 50,
    }));
    const result = spikeInjector(feed, 1, 5);
    const hasSpike = result.some((t) => t.value !== 50);
    expect(hasSpike).toBe(true);
  });

  it("preserves values with zero probability", () => {
    const feed: Tick[] = Array.from({ length: 10 }, (_, i) => ({
      ts: i,
      value: 42,
    }));
    const result = spikeInjector(feed, 0, 5);
    for (const t of result) {
      expect(t.value).toBe(42);
    }
  });
});

describe("createRng", () => {
  it("is deterministic for a given seed", () => {
    const a = createRng(42);
    const b = createRng(42);
    expect([a(), a(), a()]).toEqual([b(), b(), b()]);
  });
  it("differs across seeds", () => {
    const a = createRng(1);
    const b = createRng(2);
    expect(a()).not.toEqual(b());
  });
});

describe("pulseTrain", () => {
  it("produces baseline outside pulses and baseline+amplitude inside", () => {
    const ticks = pulseTrain(50, [{ start: 10, width: 5, amplitude: 60 }], 20);
    expect(ticks).toHaveLength(50);
    expect(ticks[0].value).toBe(20);
    expect(ticks[12].value).toBe(80);
    expect(ticks[15].value).toBe(20);
  });
});

describe("hoveringSignal", () => {
  it("produces `duration` ticks and is deterministic for a seed", () => {
    const a = hoveringSignal(100, 50, 7, 3, 2, createRng(7));
    const b = hoveringSignal(100, 50, 7, 3, 2, createRng(7));
    expect(a).toHaveLength(100);
    expect(a.map((t) => t.value)).toEqual(b.map((t) => t.value));
  });
});

describe("applyJitter", () => {
  it("leaves ts untouched and perturbs value within ±amount bounds", () => {
    const base = [
      { ts: 0, value: 50 },
      { ts: 1, value: 50 },
    ];
    const out = applyJitter(base, 3, createRng(1));
    expect(out.map((t) => t.ts)).toEqual([0, 1]);
    for (const t of out) {
      expect(Math.abs(t.value - 50)).toBeLessThanOrEqual(3);
    }
  });
  it("is a no-op when amount is 0", () => {
    const base = [{ ts: 0, value: 50 }];
    expect(applyJitter(base, 0, createRng(1))).toEqual(base);
  });
});
