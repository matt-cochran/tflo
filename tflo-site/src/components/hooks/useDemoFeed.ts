import { useState, useEffect, useRef, useCallback } from "react";
import { initWasm } from "../../lib/wasm";
import { computeDemo, type DemoData } from "../../lib/demo-compute";
import { DEMO_CONFIG, type DemoKey } from "../../lib/demo-config";

export type Speed = 1 | 2 | 4;

export const SPEED_LABELS: { value: Speed; label: string }[] = [
  { value: 1, label: "1x" },
  { value: 2, label: "2x" },
  { value: 4, label: "4x" },
];

export const WINDOW_SIZE = 60;
export const BASE_INTERVAL_MS = 100;

export interface UseDemoFeedOptions {
  autoplay?: boolean;
  loop?: boolean;
}

export interface DemoFeedState {
  demo: DemoData | null;
  wasmError: string | null;
  visibleTicks: DemoData["ticks"];
  visibleResults: DemoData["results"];
  playing: boolean;
  speed: Speed;
  jitter: number;
  pointer: number;
  progress: number;
  togglePlay: () => void;
  setSpeed: (s: Speed) => void;
  setJitter: (j: number) => void;
}

export function useDemoFeed(
  dataKey: DemoKey,
  { autoplay = false, loop: loopProp = true }: UseDemoFeedOptions = {},
): DemoFeedState {
  const baseDescriptor = DEMO_CONFIG[dataKey];

  const [demo, setDemo] = useState<DemoData | null>(null);
  const [wasmError, setWasmError] = useState<string | null>(null);
  const [jitter, setJitter] = useState(baseDescriptor.jitter);
  const [playing, setPlaying] = useState(autoplay);
  const [speed, setSpeed] = useState<Speed>(1);
  const [pointer, setPointer] = useState(0);
  const [lap, setLap] = useState(0);

  const loopSeedRef = useRef(0);
  const wasmReadyRef = useRef(false);
  const baseDescriptorRef = useRef(baseDescriptor);
  baseDescriptorRef.current = baseDescriptor;
  const jitterRef = useRef(jitter);
  jitterRef.current = jitter;
  const pointerRef = useRef(pointer);
  pointerRef.current = pointer;

  // Wasm init — once per dataKey switch. After init, compute the first feed.
  useEffect(() => {
    let cancelled = false;
    wasmReadyRef.current = false;
    setDemo(null);
    setPointer(0);
    setLap(0);
    setJitter(baseDescriptor.jitter);

    initWasm()
      .then(() => {
        if (cancelled) return;
        wasmReadyRef.current = true;
        loopSeedRef.current = Math.floor(Math.random() * 1e9);
        setDemo(
          computeDemo(
            { ...baseDescriptorRef.current, jitter: baseDescriptorRef.current.jitter },
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
  }, [dataKey, baseDescriptor]);

  // Recompute on user-driven jitter change. Skip while wasm not ready (init
  // effect will produce the first demo); skip when jitter still matches the
  // descriptor default (avoids a duplicate compute right after a dataKey switch).
  useEffect(() => {
    if (!wasmReadyRef.current) return;
    if (jitter === baseDescriptorRef.current.jitter && demo === null) return;
    loopSeedRef.current += 1;
    setDemo(
      computeDemo(
        { ...baseDescriptorRef.current, jitter },
        loopSeedRef.current,
      ),
    );
    // demo is intentionally read via the freshness check above; including it
    // in deps would cause an infinite recompute loop.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [jitter]);

  // Recompute on lap bump (animation wrapped around).
  useEffect(() => {
    if (lap === 0 || !wasmReadyRef.current) return;
    loopSeedRef.current += 1;
    setDemo(
      computeDemo(
        { ...baseDescriptorRef.current, jitter: jitterRef.current },
        loopSeedRef.current,
      ),
    );
    setPointer(0);
  }, [lap]);

  // Animation loop. setPointer is called with a plain value (not an updater
  // function) so no React purity rules apply to the side-effect-bearing
  // branches below — setLap signals the wrap via a separate state update.
  useEffect(() => {
    if (!playing) return;
    const ticksLen = demo?.ticks.length ?? 0;
    if (ticksLen === 0) return;

    const intervalMs = BASE_INTERVAL_MS / speed;
    let localPointer = pointerRef.current;

    const id = setInterval(() => {
      const next = localPointer + 1;
      if (next >= ticksLen) {
        if (loopProp) {
          setLap((l) => l + 1);
        }
        return;
      }
      localPointer = next;
      setPointer(next);
    }, intervalMs);

    return () => clearInterval(id);
  }, [playing, speed, demo, loopProp]);

  // Global space-to-toggle.
  const togglePlay = useCallback(() => {
    setPlaying((p) => !p);
  }, []);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key !== " " && e.key !== "Space") return;
      const target = e.target as HTMLElement | null;
      if (
        target &&
        (target.tagName === "INPUT" ||
          target.tagName === "TEXTAREA" ||
          target.isContentEditable)
      ) {
        return;
      }
      e.preventDefault();
      togglePlay();
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [togglePlay]);

  const ticks = demo?.ticks ?? [];
  const results = demo?.results ?? [];
  const windowStart = Math.max(0, pointer - WINDOW_SIZE);
  const visibleTicks = ticks.slice(windowStart, pointer + 1);
  const visibleResults = results.slice(windowStart, pointer + 1);
  const progress =
    ticks.length > 0 ? Math.round(((pointer + 1) / ticks.length) * 100) : 0;

  return {
    demo,
    wasmError,
    visibleTicks,
    visibleResults,
    playing,
    speed,
    jitter,
    pointer,
    progress,
    togglePlay,
    setSpeed,
    setJitter,
  };
}
