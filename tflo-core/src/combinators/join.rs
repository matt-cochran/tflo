//! Window-based join of two streams.

use std::collections::VecDeque;
use std::time::Duration;

/// Join two streams within a time window.
///
/// For each item in the left stream, finds all items in the right stream
/// that are within the specified time window.
///
/// # Examples
///
/// ```rust
/// use tflo_core::combinators::window_join;
/// use std::time::Duration;
///
/// #[derive(Clone, Debug)]
/// struct Tick { ts: i64, symbol: String }
///
/// #[derive(Clone, Debug)]
/// struct News { ts: i64, headline: String }
///
/// let ticks = vec![
///     Tick { ts: 1000, symbol: "AAPL".into() },
///     Tick { ts: 5000, symbol: "AAPL".into() },
/// ];
///
/// let news = vec![
///     News { ts: 900, headline: "Apple announces...".into() },
///     News { ts: 1500, headline: "Market update".into() },
/// ];
///
/// let joined: Vec<(Tick, Vec<News>)> = window_join(
///     ticks.into_iter(),
///     news.into_iter(),
///     |t| t.ts,
///     |n| n.ts,
///     Duration::from_secs(2),
/// ).collect();
///
/// // First tick at ts=1000 should match news at ts=900 and ts=1500
/// assert_eq!(joined[0].1.len(), 2);
/// ```
pub const fn window_join<L, R, LI, RI, LK, RK, LT, RT>(
    left: LI,
    right: RI,
    left_key: LK,
    right_key: RK,
    window: Duration,
) -> WindowJoin<LI, RI, L, R, LK, RK>
where
    LI: Iterator<Item = L>,
    RI: Iterator<Item = R>,
    LK: Fn(&L) -> LT,
    RK: Fn(&R) -> RT,
    LT: Into<i64>,
    RT: Into<i64>,
    R: Clone,
{
    #[allow(clippy::cast_possible_wrap)]
    let window_ms = window.as_millis() as i64;
    WindowJoin {
        left,
        right,
        left_key,
        right_key,
        window_ms,
        right_buffer: VecDeque::new(),
        right_exhausted: false,
    }
}

/// Iterator that joins two streams within a time window.
pub struct WindowJoin<LI, RI, L, R, LK, RK>
where
    LI: Iterator<Item = L>,
    RI: Iterator<Item = R>,
{
    left: LI,
    right: RI,
    left_key: LK,
    right_key: RK,
    window_ms: i64,
    right_buffer: VecDeque<(i64, R)>,
    right_exhausted: bool,
}

impl<LI, RI, L, R, LK, RK> std::fmt::Debug for WindowJoin<LI, RI, L, R, LK, RK>
where
    LI: Iterator<Item = L>,
    RI: Iterator<Item = R>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WindowJoin")
            .field("buffer_size", &self.right_buffer.len())
            .field("right_exhausted", &self.right_exhausted)
            .finish()
    }
}

impl<LI, RI, L, R, LK, RK, LT, RT> Iterator for WindowJoin<LI, RI, L, R, LK, RK>
where
    LI: Iterator<Item = L>,
    RI: Iterator<Item = R>,
    LK: Fn(&L) -> LT,
    RK: Fn(&R) -> RT,
    LT: Into<i64>,
    RT: Into<i64>,
    R: Clone,
{
    type Item = (L, Vec<R>);

    fn next(&mut self) -> Option<Self::Item> {
        let left_item = self.left.next()?;
        let left_ts: i64 = (self.left_key)(&left_item).into();

        // Buffer right items until we have enough. Saturating: a
        // pathological `left_ts` near `i64::MAX` clamps the window end
        // at `i64::MAX` — semantically "match everything past it",
        // which is the only sensible behavior for end-of-time inputs.
        let window_end = left_ts.saturating_add(self.window_ms);
        while !self.right_exhausted {
            if let Some((last_ts, _)) = self.right_buffer.back() {
                if *last_ts > window_end {
                    break;
                }
            }
            match self.right.next() {
                Some(r) => {
                    let r_ts: i64 = (self.right_key)(&r).into();
                    self.right_buffer.push_back((r_ts, r));
                }
                None => {
                    self.right_exhausted = true;
                }
            }
        }

        // Evict old items from buffer. Saturating: `left_ts <
        // window_ms` (early in the stream, or small epoch) clamps the
        // window start at `i64::MIN` — semantically "no eviction yet",
        // which matches the intent.
        let window_start = left_ts.saturating_sub(self.window_ms);
        while let Some((ts, _)) = self.right_buffer.front() {
            if *ts < window_start {
                let _ = self.right_buffer.pop_front();
            } else {
                break;
            }
        }

        // Find matching items
        let matches: Vec<R> = self
            .right_buffer
            .iter()
            .filter(|(ts, _)| *ts >= window_start && *ts <= window_end)
            .map(|(_, r)| r.clone())
            .collect();

        Some((left_item, matches))
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_window_join() {
        let left = vec![(1000_i64, "A"), (3000_i64, "B"), (5000_i64, "C")];
        let right = vec![(500_i64, 1), (1500_i64, 2), (2500_i64, 3), (4500_i64, 4)];

        let joined: Vec<_> = window_join(
            left.into_iter(),
            right.into_iter(),
            |l| l.0,
            |r| r.0,
            Duration::from_secs(1),
        )
        .collect();

        assert_eq!(joined.len(), 3);

        // A at 1000 matches 500 and 1500
        assert_eq!(joined[0].1.len(), 2);

        // B at 3000 matches 2500
        assert_eq!(joined[1].1.len(), 1);

        // C at 5000 matches 4500
        assert_eq!(joined[2].1.len(), 1);
    }

    #[test]
    fn test_no_matches() {
        let left = vec![(1000_i64, "A")];
        let right = vec![(5000_i64, 1)];

        let joined: Vec<_> = window_join(
            left.into_iter(),
            right.into_iter(),
            |l| l.0,
            |r| r.0,
            Duration::from_millis(100),
        )
        .collect();

        assert_eq!(joined.len(), 1);
        assert!(joined[0].1.is_empty());
    }
}
