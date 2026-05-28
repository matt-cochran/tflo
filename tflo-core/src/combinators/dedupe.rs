//! Deduplicate records within a time window.

use std::collections::HashMap;
use std::hash::Hash;
use std::time::Duration;

/// Remove duplicate records within a time window.
///
/// Duplicates are identified by a key function. Only the first occurrence
/// of a key within the time window is kept.
///
/// # Examples
///
/// ```rust
/// use tflo_core::combinators::dedupe_by_key;
/// use std::time::Duration;
///
/// let records = vec![
///     (1000_i64, "AAPL", 150.0),
///     (1100_i64, "AAPL", 150.0),  // Duplicate within window
///     (1200_i64, "GOOG", 2800.0),
///     (3000_i64, "AAPL", 151.0),  // Outside window, not a duplicate
/// ];
///
/// let deduped: Vec<_> = dedupe_by_key(
///     records.into_iter(),
///     |r| r.1,  // Key by symbol
///     |r| r.0,  // Timestamp
///     Duration::from_secs(1),
/// ).collect();
///
/// assert_eq!(deduped.len(), 3);  // AAPL at 1100 is removed
/// ```
pub fn dedupe_by_key<I, T, K, KF, TF>(
    iter: I,
    key_fn: KF,
    ts_fn: TF,
    window: Duration,
) -> DedupeByKey<I, T, K, KF, TF>
where
    I: Iterator<Item = T>,
    KF: Fn(&T) -> K,
    TF: Fn(&T) -> i64,
    K: Hash + Eq,
{
    #[allow(clippy::cast_possible_wrap)]
    let window_ms = window.as_millis() as i64;
    DedupeByKey {
        iter,
        key_fn,
        ts_fn,
        window_ms,
        seen: HashMap::new(),
    }
}

/// Iterator that deduplicates by key within a time window.
pub struct DedupeByKey<I, T, K, KF, TF>
where
    I: Iterator<Item = T>,
    K: Hash + Eq,
{
    iter: I,
    key_fn: KF,
    ts_fn: TF,
    window_ms: i64,
    seen: HashMap<K, i64>,
}

impl<I, T, K, KF, TF> std::fmt::Debug for DedupeByKey<I, T, K, KF, TF>
where
    I: Iterator<Item = T>,
    K: Hash + Eq,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DedupeByKey")
            .field("window_ms", &self.window_ms)
            .field("seen_count", &self.seen.len())
            .finish()
    }
}

impl<I, T, K, KF, TF> Iterator for DedupeByKey<I, T, K, KF, TF>
where
    I: Iterator<Item = T>,
    KF: Fn(&T) -> K,
    TF: Fn(&T) -> i64,
    K: Hash + Eq + Clone,
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let item = self.iter.next()?;
            let key: K = (self.key_fn)(&item);
            let ts: i64 = (self.ts_fn)(&item);

            // Clean up old entries. Saturating: if `ts < window_ms`
            // (start of stream, or small epoch), the cutoff clamps at
            // `i64::MIN` and the retain becomes a no-op — correct.
            let cutoff = ts.saturating_sub(self.window_ms);
            self.seen.retain(|_, &mut last_ts| last_ts >= cutoff);

            // Check if we've seen this key recently. Saturating: out-of-
            // order records (ts < last_ts) clamp at 0, which compares as
            // < window_ms so we correctly treat as duplicate-within-window.
            if let Some(&last_ts) = self.seen.get(&key) {
                if ts.saturating_sub(last_ts) < self.window_ms {
                    // Duplicate - skip
                    continue;
                }
            }

            // Not a duplicate - record and return
            let _ = self.seen.insert(key, ts);
            return Some(item);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dedupe() {
        let records = vec![
            (1000_i64, "A"),
            (1100_i64, "A"), // Duplicate
            (1200_i64, "B"),
            (1300_i64, "A"), // Still duplicate
            (3000_i64, "A"), // Outside window
        ];

        let deduped: Vec<_> = dedupe_by_key(
            records.into_iter(),
            |r| r.1.to_string(),
            |r| r.0,
            Duration::from_secs(1),
        )
        .collect();

        assert_eq!(deduped.len(), 3);
        assert_eq!(deduped[0].1, "A");
        assert_eq!(deduped[1].1, "B");
        assert_eq!(deduped[2].1, "A"); // After window expired
    }

    #[test]
    fn test_no_duplicates() {
        let records = vec![(1000_i64, "A"), (2000_i64, "B"), (3000_i64, "C")];

        let deduped: Vec<_> = dedupe_by_key(
            records.into_iter(),
            |r| r.1.to_string(),
            |r| r.0,
            Duration::from_millis(500),
        )
        .collect();

        assert_eq!(deduped.len(), 3);
    }

    #[test]
    fn test_all_duplicates() {
        let records = vec![
            (1000_i64, "A"),
            (1010_i64, "A"),
            (1020_i64, "A"),
            (1030_i64, "A"),
        ];

        let deduped: Vec<_> = dedupe_by_key(
            records.into_iter(),
            |r| r.1.to_string(),
            |r| r.0,
            Duration::from_secs(1),
        )
        .collect();

        assert_eq!(deduped.len(), 1);
    }
}
