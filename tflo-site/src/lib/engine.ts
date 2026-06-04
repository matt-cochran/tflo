/**
 * LiveFeedEngine — reactive state manager for the playground.
 *
 * Accepts a data generator function, emits ticks frame-by-frame,
 * calls the wasm bridge on each batch, and exposes reactive state
 * for the chart components.
 */

import type { Tick, Band, CrossEvent, IndicatorConfig } from "./wasm";
import { computeSma, computeRsi, computeBollinger, detectCross } from "./wasm";

/** Indicator results for the current batch. */
export interface IndicatorState {
  sma?: (number | null)[];
  rsi?: (number | null)[];
  bollinger?: (Band | null)[];
  crosses?: CrossEvent[];
}

/** Chart-friendly snapshot — derived from PlaygroundState for LiveChart props. */
export interface ChartSnapshot {
  ticks: Tick[];
  sma?: (number | null)[];
  bollinger?: (Band | null)[];
  crosses?: CrossEvent[];
}

/** Full reactive state exposed to the UI. */
export interface PlaygroundState {
  /** All ticks emitted so far. */
  ticks: Tick[];
  /** Latest computed indicators. */
  indicators: IndicatorState;
  /** Whether the engine is currently running. */
  isRunning: boolean;
  /** Current playback speed multiplier. */
  speed: number;
  /** Total ticks generated. */
  totalTicks: number;
}

export type FeedGenerator = (count: number) => Tick[];

/** Configuration for how the engine processes ticks. */
export interface EngineConfig {
  /** Number of ticks to emit per frame. */
  batchSize: number;
  /** Frame interval in ms. */
  frameIntervalMs: number;
  /** Initial speed multiplier. */
  initialSpeed: number;
  /** Active indicator configurations. */
  indicators: IndicatorConfig[];
}

const DEFAULT_CONFIG: EngineConfig = {
  batchSize: 10,
  frameIntervalMs: 100,
  initialSpeed: 1,
  indicators: [],
};

/**
 * The LiveFeedEngine manages tick generation, wasm-based indicator
 * computation, and reactive state for the playground UI.
 */
export class LiveFeedEngine {
  private config: EngineConfig;
  private generator: FeedGenerator;
  private state: PlaygroundState;
  private timerId: ReturnType<typeof setInterval> | null = null;
  private tickCursor = 0;
  private historySize: number;

  /** Callbacks for state changes. */
  private listeners: Set<(state: PlaygroundState) => void> = new Set();

  constructor(
    generator: FeedGenerator,
    config: Partial<EngineConfig> = {},
    historySize = 500,
  ) {
    this.generator = generator;
    this.config = { ...DEFAULT_CONFIG, ...config };
    this.historySize = historySize;
    this.state = {
      ticks: [],
      indicators: {},
      isRunning: false,
      speed: this.config.initialSpeed,
      totalTicks: 0,
    };
  }

  /** Get the current state (snapshot, not live reference). */
  getState(): PlaygroundState {
    return { ...this.state, ticks: [...this.state.ticks] };
  }

  /** Subscribe to state changes. Returns an unsubscribe function. */
  subscribe(cb: (state: PlaygroundState) => void): () => void {
    this.listeners.add(cb);
    return () => this.listeners.delete(cb);
  }

  /** Start emitting ticks. */
  start(): void {
    if (this.state.isRunning) return;
    this.state = { ...this.state, isRunning: true };
    this.notify();
    this.scheduleFrame();
  }

  /** Pause tick emission. */
  pause(): void {
    if (!this.state.isRunning) return;
    this.clearTimer();
    this.state = { ...this.state, isRunning: false };
    this.notify();
  }

  /** Toggle between play and pause. */
  toggle(): void {
    if (this.state.isRunning) {
      this.pause();
    } else {
      this.start();
    }
  }

  /** Reset all state. */
  reset(): void {
    this.clearTimer();
    this.tickCursor = 0;
    this.state = {
      ticks: [],
      indicators: {},
      isRunning: false,
      speed: this.config.initialSpeed,
      totalTicks: 0,
    };
    this.notify();
  }

  /** Set playback speed (1x–100x). */
  setSpeed(speed: number): void {
    const clamped = Math.max(1, Math.min(100, speed));
    this.config.frameIntervalMs = Math.round(100 / clamped);
    this.state = { ...this.state, speed: clamped };
    // Restart timer if running
    if (this.state.isRunning) {
      this.clearTimer();
      this.scheduleFrame();
    }
    this.notify();
  }

  /** Update active indicators. */
  setIndicators(indicators: IndicatorConfig[]): void {
    this.config = { ...this.config, indicators };
    // Recompute on existing data
    this.recomputeIndicators();
  }

  /** Advance one frame manually (useful for testing or stepping). */
  step(): void {
    const batch = this.generator(this.config.batchSize);
    this.tickCursor += batch.length;
    this.processBatch(batch);
  }

  // ── Private ─────────────────────────────────────────────────────────

  private scheduleFrame(): void {
    this.timerId = setInterval(() => {
      this.step();
    }, this.config.frameIntervalMs);
  }

  private clearTimer(): void {
    if (this.timerId !== null) {
      clearInterval(this.timerId);
      this.timerId = null;
    }
  }

  private processBatch(batch: Tick[]): void {
    // Accumulate with history limit
    const allTicks = [...this.state.ticks, ...batch];
    const trimmed = allTicks.slice(-this.historySize);

    this.state = {
      ...this.state,
      ticks: trimmed,
      totalTicks: this.state.totalTicks + batch.length,
    };

    this.recomputeIndicators();
    this.notify();
  }

  private recomputeIndicators(): void {
    if (this.state.ticks.length === 0) return;

    const indicators: IndicatorState = {};
    const ticks = this.state.ticks;

    for (const cfg of this.config.indicators) {
      switch (cfg.indicator) {
        case "sma":
          if (cfg.period) {
            indicators.sma = computeSma(ticks, { period: cfg.period });
          }
          break;
        case "rsi":
          if (cfg.period) {
            indicators.rsi = computeRsi(ticks, { period: cfg.period });
          }
          break;
        case "bollinger":
          if (cfg.period) {
            indicators.bollinger = computeBollinger(ticks, {
              period: cfg.period,
              multiplier: cfg.multiplier,
            });
          }
          break;
        case "cross":
          if (cfg.threshold !== undefined) {
            indicators.crosses = detectCross(ticks, {
              threshold: cfg.threshold,
              direction: cfg.direction,
            });
          }
          break;
      }
    }

    this.state = { ...this.state, indicators };
  }

  private notify(): void {
    const snapshot = this.getState();
    for (const cb of this.listeners) {
      try {
        cb(snapshot);
      } catch {
        // Silently ignore listener errors
      }
    }
  }
}

// ── Reactive store for PlaygroundChart ────────────────────────────────

/**
 * Module-level store bridging LiveFeedEngine state changes to React.
 * The vanilla JS script calls createEngine(), and React's PlaygroundChart
 * calls subscribeSnapshot(). Both reference the same module-level engine.
 */

let globalEngine: LiveFeedEngine | null = null;
type StoreListener = (snapshot: ChartSnapshot) => void;
const storeListeners = new Set<StoreListener>();

/** Create or recreate the engine. Called by the vanilla JS script. */
export function createEngine(
  generator: FeedGenerator,
  config: Partial<EngineConfig> = {},
  historySize = 500,
): LiveFeedEngine {
  if (globalEngine) {
    globalEngine.reset();
  }

  const engine = new LiveFeedEngine(generator, config, historySize);
  globalEngine = engine;

  engine.subscribe((state: PlaygroundState) => {
    const snapshot: ChartSnapshot = {
      ticks: state.ticks,
      sma: state.indicators.sma,
      bollinger: state.indicators.bollinger,
      crosses: state.indicators.crosses,
    };
    for (const cb of storeListeners) {
      try {
        cb(snapshot);
      } catch {
        /* ignore */
      }
    }
  });

  return engine;
}

/** Get the current global engine instance (for vanilla JS access). */
export function getEngine(): LiveFeedEngine | null {
  return globalEngine;
}

/** Subscribe to chart snapshots (used by React hook). */
export function subscribeSnapshot(cb: StoreListener): () => void {
  storeListeners.add(cb);
  return () => storeListeners.delete(cb);
}
