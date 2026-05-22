import { describe, it, expect, beforeAll } from "vitest";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import init, {
  WasmCrossDetector,
  WasmHysteresisCrossDetector,
  WasmGlitchFilter,
  WasmRuntDetector,
  WasmPulseWidthDetector,
  WasmWindowDetector,
} from "../../../public/wasm/tflo.js";

/**
 * Exercises the wasm detector classes across the real JS↔wasm boundary.
 * The Rust-side `wasm-bindgen-test` suite calls these Rust-to-Rust and so
 * cannot catch JS marshalling bugs (e.g. an `i64` param surfacing as a
 * `BigInt` that JS `number`s can't satisfy). This test loads the built
 * wasm and calls every detector with plain JS numbers.
 */
beforeAll(async () => {
  const bytes = readFileSync(
    fileURLToPath(
      new URL("../../../public/wasm/tflo_bg.wasm", import.meta.url),
    ),
  );
  await init(bytes);
});

describe("wasm detector classes — JS↔wasm boundary", () => {
  it("GlitchFilter constructs and updates with plain JS numbers", () => {
    const d = new WasmGlitchFilter(50, 10);
    expect(d.update(100, 0)).toBe("none");
    expect(d.update(0, 5)).toBe("glitch");
    d.free();
  });

  it("PulseWidthDetector constructs and updates with plain JS numbers", () => {
    const d = new WasmPulseWidthDetector(50, 8, 22);
    expect(d.update(100, 0)).toBe("none");
    expect(d.update(0, 4)).toBe("short");
    d.free();
  });

  it("all six detectors construct and reset with plain JS numbers", () => {
    const makers: (() => { reset(): void; free(): void })[] = [
      () => new WasmCrossDetector(),
      () => new WasmHysteresisCrossDetector(5),
      () => new WasmGlitchFilter(50, 10),
      () => new WasmRuntDetector(40, 85),
      () => new WasmPulseWidthDetector(50, 8, 22),
      () => new WasmWindowDetector(38, 68),
    ];
    for (const make of makers) {
      const d = make();
      d.reset();
      d.free();
    }
  });
});
