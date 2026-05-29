/**
 * Optional wasm build script for tflo-site.
 *
 * Wraps wasm-pack so the dev server can start even when wasm-pack
 * is not installed. The playground will simply show a placeholder
 * until the wasm module is built.
 */

import { execSync } from "node:child_process";
import { rmSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const isDev = process.argv.includes("--dev");
const target = "web";
// Absolute path to tflo-site/public/wasm — the directory Astro serves.
// wasm-pack resolves --out-dir relative to the cwd, so a relative path here
// would land in the wrong place; an absolute path is unambiguous.
const outDir = resolve(scriptDir, "..", "public", "wasm");
const outName = "tflo";

try {
  const flags = isDev ? " --dev" : "";
  execSync(
    `wasm-pack build ../tflo-wasm --target ${target} --out-dir ${outDir} --out-name ${outName}${flags}`,
    { stdio: "inherit" },
  );
  // wasm-pack writes a `*`-everything .gitignore into --out-dir. We track
  // the artifact so Cloudflare Pages (no wasm-pack) can serve it, so strip
  // the inner ignore on every build.
  rmSync(resolve(outDir, ".gitignore"), { force: true });
  console.log("[wasm] ✅ tflo-wasm built successfully");
} catch (err) {
  if (err && typeof err === "object" && "status" in err && err.status === 127) {
    console.warn(
      "[wasm] ⚠️  wasm-pack not found. Install it with: cargo install wasm-pack",
    );
    console.warn(
      "[wasm]    The playground will fall back to a client-side placeholder.",
    );
    console.warn(
      "[wasm]    To build the wasm module later, run: npm run build:wasm",
    );
  } else {
    const message =
      err && typeof err === "object" && "message" in err
        ? err.message
        : String(err);
    console.error("[wasm] ❌ wasm build failed:", message);
    process.exit(1);
  }
}
