#!/usr/bin/env node
import { execSync } from "node:child_process";
import { copyFileSync, existsSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, "..");
const WASM_CRATE = resolve(ROOT, "..", "tflo-wasm");
const PKG_DIR = resolve(WASM_CRATE, "pkg");

try {
  // Step 1: Build
  console.log("🔨 Building tflo-wasm with wasm-pack...");
  execSync("wasm-pack build --target web", {
    cwd: WASM_CRATE,
    stdio: "inherit",
  });
  console.log("✅ Build complete");

  // Step 2: Copy README
  const readmeSrc = resolve(WASM_CRATE, "README.md");
  const readmeDest = resolve(PKG_DIR, "README.md");
  if (existsSync(readmeSrc)) {
    copyFileSync(readmeSrc, readmeDest);
    console.log("📄 README.md copied to pkg/");
  }

  // Step 3: Publish
  console.log("📦 Publishing to npm...");
  execSync("npm publish", { cwd: PKG_DIR, stdio: "inherit" });
  console.log("✅ Published successfully!");
} catch (err) {
  console.error("❌ Publish failed:", err.message);
  process.exit(1);
}
