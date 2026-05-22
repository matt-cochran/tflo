//! Rate limiting for stream output.

use std::time::Duration;

/// Rate limit the output stream.
///
/// Ensures at least the specified duration passes between outputs.
/// Earlier items within the rate limit window are dropped.
///
/// # Examples
///
/// ```rust
/// use tflo_core::combinators::rate_limit;
/// use std::time::Duration;
///
/// let events = vec![
///     (1000_i64, "event1"),
///     (1100_i64, "event2"),  // Too soon, dropped
///     (1200_i64, "event3"),  // Too soon, dropped
///     (3000_i64, "event4"),  // OK, 2 seconds after event1
/// ];
///
/// let limited: Vec<_> = rate_limit(
///     events.into_iter(),
///     |e| e.0,
///     Duration::from_secs(1),
/// ).collect();
///
/// assert_eq!(limited.len(), 2);
/// assert_eq!(limited[0].1, "event1");
/// assert_eq!(limited[1].1, "event4");
/// ```
pub fn rate_limit<I, T, F, K>(iter: I, ts_fn: F, min_interval: Duration) -> RateLimit<I, T, F>
where
    I: Iterator<Item = T>,
    F: Fn(&T) -> K,
    K: Into<i64>,
{
    #[allow(clippy::cast_possible_wrap)]
    let interval_ms = min_interval.as_millis() as i64;
    RateLimit {
        iter,
        ts_fn,
        interval_ms,
        last_output_ts: None,
    }
}

/// Iterator that rate limits output.
pub struct RateLimit<I, T, F>
where
    I: Iterator<Item = T>,
{
    iter: I,
    ts_fn: F,
    interval_ms: i64,
    last_output_ts: Option<i64>,
}

impl<I, T, F> std::fmt::Debug for RateLimit<I, T, F>
where
    I: Iterator<Item = T>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RateLimit")
            .field("interval_ms", &self.interval_ms)
            .field("last_output_ts", &self.last_output_ts)
            .finish()
    }
}

impl<I, T, F, K> Iterator for RateLimit<I, T, F>
where
    I: Iterator<Item = T>,
    F: Fn(&T) -> K,
    K: Into<i64>,
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let item = self.iter.next()?;
            let ts: i64 = (self.ts_fn)(&item).into();

            match self.last_output_ts {
                None => {
                    self.last_output_ts = Some(ts);
                    return Some(item);
                }
                Some(last) if ts - last >= self.interval_ms => {
                    self.last_output_ts = Some(ts);
                    return Some(item);
                }
                Some(_) => {
                    // Rate limited - drop this item
                    continue;
                }
            }
        }
    }
}

/// Rate limiter that keeps the most recent item within each window.
#[allow(dead_code)]
pub fn rate_limit_keep_last<I, T, F, K>(
    iter: I,
    ts_fn: F,
    interval: Duration,
) -> RateLimitKeepLast<I, T, F>
where
    I: Iterator<Item = T>,
    F: Fn(&T) -> K,
    K: Into<i64>,
{
    #[allow(clippy::cast_possible_wrap)]
    let interval_ms = interval.as_millis() as i64;
    RateLimitKeepLast {
        iter,
        ts_fn,
        interval_ms,
        pending: None,
        last_output_ts: None,
    }
}

/// Iterator that rate limits but keeps the most recent item.
#[allow(dead_code)]
pub struct RateLimitKeepLast<I, T, F>
where
    I: Iterator<Item = T>,
{
    iter: I,
    ts_fn: F,
    interval_ms: i64,
    pending: Option<(i64, T)>,
    last_output_ts: Option<i64>,
}

impl<I, T, F> std::fmt::Debug for RateLimitKeepLast<I, T, F>
where
    I: Iterator<Item = T>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RateLimitKeepLast")
            .field("interval_ms", &self.interval_ms)
            .field("has_pending", &self.pending.is_some())
            .finish()
    }
}

impl<I, T, F, K> Iterator for RateLimitKeepLast<I, T, F>
where
    I: Iterator<Item = T>,
    F: Fn(&T) -> K,
    K: Into<i64>,
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.iter.next() {
                Some(item) => {
                    let ts: i64 = (self.ts_fn)(&item).into();

                    match self.last_output_ts {
                        None => {
                            self.last_output_ts = Some(ts);
                            return Some(item);
                        }
                        Some(last) if ts - last >= self.interval_ms => {
                            // Can output - first check if we have a pending item
                            if let Some((pending_ts, pending_item)) = self.pending.take() {
                                // Output pending, store current as new pending
                                self.last_output_ts = Some(pending_ts);
                                self.pending = Some((ts, item));
                                return Some(pending_item);
                            }
                            self.last_output_ts = Some(ts);
                            return Some(item);
                        }
                        Some(_) => {
                            // Rate limited - keep this as pending (overwrite previous)
                            self.pending = Some((ts, item));
                        }
                    }
                }
                None => {
                    // Input exhausted - return pending if any
                    return self.pending.take().map(|(_, item)| item);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limit() {
        let events: Vec<i64> = vec![1000, 1100, 1200, 2100, 2200, 3500];

        let limited: Vec<i64> =
            rate_limit(events.into_iter(), |&x| x, Duration::from_secs(1)).collect();

        assert_eq!(limited, vec![1000, 2100, 3500]);
    }

    #[test]
    fn test_rate_limit_empty() {
        let events: Vec<i64> = vec![];
        let limited: Vec<i64> =
            rate_limit(events.into_iter(), |&x| x, Duration::from_secs(1)).collect();

        assert!(limited.is_empty());
    }

    #[test]
    fn test_rate_limit_single() {
        let events: Vec<i64> = vec![1000];
        let limited: Vec<i64> =
            rate_limit(events.into_iter(), |&x| x, Duration::from_secs(1)).collect();

        assert_eq!(limited, vec![1000]);
    }

    #[test]
    fn test_rate_limit_keep_last() {
        let events: Vec<i64> = vec![1000, 1100, 1200, 1300];

        let limited: Vec<i64> =
            rate_limit_keep_last(events.into_iter(), |&x| x, Duration::from_secs(10)).collect();

        // First item output immediately, last item output at end
        assert_eq!(limited, vec![1000, 1300]);
    }
}
