//! Poka-yoke: the CEP engine MUST stay pure — time is always an explicit input
//! (`push_at(_, ts)`, `tick(now)`), never read from a wall clock. Determinism is
//! the load-bearing property behind deterministic replay AND cross-tier parity
//! (the browser-WASM tier and the native-Rust tier must agree byte-for-byte). A
//! stray `Instant::now()` / `SystemTime` would silently break both, with no test
//! failure anywhere else. This build-time guard fails fast if one sneaks into
//! `src/`.
//!
//! `std::time::Duration` is fine (a value type, no clock) and is deliberately not
//! banned. The scan is over `src/` only, so this test file's own token list is
//! never matched.

use std::fs;
use std::path::Path;

/// Tokens that indicate reading a wall clock. Spelled as usage forms so prose
/// like "advance time to `now`" never trips the guard.
const FORBIDDEN: &[&str] = &[
    "Instant::now",
    "SystemTime",
    "Date::now",
    "chrono::",
    "std::time::Instant",
];

fn scan(dir: &Path, hits: &mut Vec<String>) {
    for entry in fs::read_dir(dir).expect("read src dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            scan(&path, hits);
        } else if path.extension().is_some_and(|e| e == "rs") {
            let src = fs::read_to_string(&path).expect("read rs file");
            for tok in FORBIDDEN {
                if src.contains(tok) {
                    hits.push(format!("{} contains `{}`", path.display(), tok));
                }
            }
        }
    }
}

#[test]
fn engine_src_reads_no_wall_clock() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut hits = Vec::new();
    scan(&src, &mut hits);
    assert!(
        hits.is_empty(),
        "tflo-cep must stay pure (time-as-input) for replay + cross-tier parity. \
         Wall-clock use found:\n  {}",
        hits.join("\n  ")
    );
}
