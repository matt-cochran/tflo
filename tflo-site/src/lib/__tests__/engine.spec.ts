import { describe, it, expect, vi, beforeEach } from "vitest";
import {
  LiveFeedEngine,
  createEngine,
  getEngine,
  subscribeSnapshot,
} from "../engine";
import type { PlaygroundState, FeedGenerator } from "../engine";

// ── Mock wasm module (can't load native wasm in Node.js test runner) ──

vi.mock("../wasm", () => ({
  computeSma: (ticks: { value: number }[], config: { period?: number }) =>
    ticks.map((_, i) => (i >= (config.period ?? 1) ? 50 : null)),
  computeRsi: () => [],
  computeBollinger: () => [],
  detectCross: () => [],
}));

// ── Helpers ───────────────────────────────────────────────────────────

function constantFeed(value: number, count?: number): FeedGenerator {
  return (n = 10) =>
    Array.from({ length: n }, (_, i) => ({ ts: (count ?? 0) + i, value }));
}

function bumpyFeed(): FeedGenerator {
  let cursor = 0;
  return (n = 10) => {
    const ticks = Array.from({ length: n }, (_, i) => ({
      ts: cursor + i,
      value: 50 + Math.sin(cursor + i) * 10,
    }));
    cursor += n;
    return ticks;
  };
}

describe("LiveFeedEngine", () => {
  let engine: LiveFeedEngine;

  beforeEach(() => {
    engine = new LiveFeedEngine(constantFeed(50), {
      batchSize: 5,
      frameIntervalMs: 50,
      initialSpeed: 1,
      indicators: [],
    });
  });

  it("starts with empty state", () => {
    const state = engine.getState();
    expect(state.ticks).toHaveLength(0);
    expect(state.isRunning).toBe(false);
    expect(state.speed).toBe(1);
    expect(state.totalTicks).toBe(0);
  });

  it("step() advances state by batchSize ticks", () => {
    engine.step();
    const state = engine.getState();
    expect(state.ticks).toHaveLength(5);
    expect(state.totalTicks).toBe(5);
  });

  it("multiple steps accumulate ticks up to history size", () => {
    engine = new LiveFeedEngine(constantFeed(50), { batchSize: 10 }, 30);
    for (let i = 0; i < 5; i++) engine.step();
    const state = engine.getState();
    expect(state.ticks.length).toBeLessThanOrEqual(30);
    expect(state.totalTicks).toBe(50);
  });

  it("notifies subscribers on state change", () => {
    const listener = vi.fn();
    engine.subscribe(listener);
    engine.step();
    expect(listener).toHaveBeenCalledTimes(1);
    expect(listener).toHaveBeenCalledWith(
      expect.objectContaining({ totalTicks: 5 }),
    );
  });

  it("toggle() starts and stops the engine", () => {
    engine.toggle();
    expect(engine.getState().isRunning).toBe(true);
    engine.toggle();
    expect(engine.getState().isRunning).toBe(false);
  });

  it("reset() clears all state", () => {
    engine.step();
    engine.step();
    expect(engine.getState().totalTicks).toBeGreaterThan(0);
    engine.reset();
    const state = engine.getState();
    expect(state.ticks).toHaveLength(0);
    expect(state.totalTicks).toBe(0);
    expect(state.isRunning).toBe(false);
  });

  it("setSpeed() adjusts frame interval", () => {
    engine.setSpeed(10);
    expect(engine.getState().speed).toBe(10);
  });

  it("setSpeed() clamps to 1–100", () => {
    engine.setSpeed(0);
    expect(engine.getState().speed).toBe(1);
    engine.setSpeed(200);
    expect(engine.getState().speed).toBe(100);
  });

  it("setIndicators() recomputes on existing data", () => {
    engine.step();
    engine.setIndicators([{ indicator: "sma", period: 3 }]);
    const state = engine.getState();
    expect(state.indicators.sma).toBeDefined();
  });

  it("unsubscribe() removes a listener", () => {
    const listener = vi.fn();
    const unsub = engine.subscribe(listener);
    unsub();
    engine.step();
    expect(listener).not.toHaveBeenCalled();
  });
});

describe("createEngine / getEngine / subscribeSnapshot", () => {
  beforeEach(() => {
    // Reset by creating a new engine
    createEngine(constantFeed(50), { batchSize: 3, frameIntervalMs: 10 });
  });

  it("createEngine returns a LiveFeedEngine", () => {
    const e = createEngine(constantFeed(50), { batchSize: 3 });
    expect(e).toBeInstanceOf(LiveFeedEngine);
  });

  it("getEngine() returns the last created engine", () => {
    const e = getEngine();
    expect(e).not.toBeNull();
    expect(e).toBeInstanceOf(LiveFeedEngine);
  });

  it("subscribeSnapshot receives chart snapshots when engine steps", () => {
    const snapshotListener = vi.fn();
    const unsub = subscribeSnapshot(snapshotListener);

    const engine = getEngine()!;
    engine.step();

    expect(snapshotListener).toHaveBeenCalledTimes(1);
    expect(snapshotListener).toHaveBeenCalledWith(
      expect.objectContaining({
        ticks: expect.any(Array),
      }),
    );
    const snapshot = snapshotListener.mock.calls[0][0];
    expect(snapshot.ticks.length).toBeGreaterThan(0);

    unsub();
  });

  it("subscribeSnapshot unsub stops receiving updates", () => {
    const snapshotListener = vi.fn();
    const unsub = subscribeSnapshot(snapshotListener);
    unsub();

    const engine = getEngine()!;
    engine.step();

    expect(snapshotListener).not.toHaveBeenCalled();
  });

  it("multiple subscribers all receive the same snapshot", () => {
    const a = vi.fn();
    const b = vi.fn();
    const unsubA = subscribeSnapshot(a);
    const unsubB = subscribeSnapshot(b);

    getEngine()!.step();

    expect(a).toHaveBeenCalledTimes(1);
    expect(b).toHaveBeenCalledTimes(1);
    // Both received the same ticks
    expect(a.mock.calls[0][0].ticks).toEqual(b.mock.calls[0][0].ticks);

    unsubA();
    unsubB();
  });

  it("works with bumpy feed (non-constant values)", () => {
    const engine = createEngine(bumpyFeed(), { batchSize: 10 });
    const listener = vi.fn();
    const unsub = subscribeSnapshot(listener);

    engine.step();
    engine.step();

    expect(listener).toHaveBeenCalledTimes(2);
    const snapshots = listener.mock.calls.map((c) => c[0]);
    expect(snapshots[0].ticks.length).toBe(10);
    expect(snapshots[1].ticks.length).toBe(20);
    // Values should differ
    expect(snapshots[0].ticks[0].value).not.toBe(snapshots[0].ticks[9].value);

    unsub();
  });
});
