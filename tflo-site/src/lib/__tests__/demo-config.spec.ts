import { describe, it, expect } from "vitest";
import { DEMO_CONFIG, DEMO_DURATION, type DemoKey } from "../demo-config";

const KEYS: DemoKey[] = [
  "sma",
  "rsi",
  "bollinger",
  "macd",
  "cross",
  "hysteresis",
  "glitch",
  "runt",
  "pulse-width",
  "window",
];

describe("DEMO_CONFIG", () => {
  it("has a descriptor for every demo key", () => {
    for (const k of KEYS) {
      expect(DEMO_CONFIG[k]).toBeDefined();
      expect(DEMO_CONFIG[k].sourceFeed).toBeTypeOf("string");
      expect(DEMO_CONFIG[k].jitter).toBeGreaterThanOrEqual(0);
    }
  });

  it("each compute kind matches its demo key (except sma/ema split)", () => {
    expect(DEMO_CONFIG.macd.compute.kind).toBe("macd");
    expect(DEMO_CONFIG.glitch.compute.kind).toBe("glitch");
    expect(DEMO_CONFIG.window.compute.kind).toBe("window");
  });

  it("DEMO_DURATION is positive", () => {
    expect(DEMO_DURATION).toBeGreaterThan(0);
  });
});
