//! Batch records by time intervals.

use std::time::Duration;

/// Batch records by time intervals.
///
/// Groups records into batches based on time boundaries. Each batch contains
/// all records within a time interval.
///
/// # Examples
///
/// ```rust
/// use tflo_core::combinators::batch_by_time;
/// use std::time::Duration;
///
/// let records = vec![
///     (1000_i64, "a"),
///     (1500_i64, "b"),
///     (2500_i64, "c"),
///     (3500_i64, "d"),
/// ];
///
/// let batches: Vec<Vec<(i64, &str)>> = batch_by_time(
///     records.into_iter(),
///     |r| r.0,
///     Duration::from_secs(1),
/// ).collect();
///
/// assert_eq!(batches.len(), 3);
/// assert_eq!(batches[0].len(), 2); // 1000, 1500
/// assert_eq!(batches[1].len(), 1); // 2500
/// assert_eq!(batches[2].len(), 1); // 3500
/// ```
pub const fn batch_by_time<I, T, F, K>(iter: I, key_fn: F, interval: Duration) -> BatchByTime<I, T, F>
where
    I: Iterator<Item = T>,
    F: Fn(&T) -> K,
    K: Into<i64>,
{
    #[allow(clippy::cast_possible_wrap)]
    let interval_ms = interval.as_millis() as i64;
    BatchByTime {
        iter,
        key_fn,
        interval_ms,
        current_batch: Vec::new(),
        current_boundary: None,
        finished: false,
    }
}

/// Iterator that batches by time.
pub struct BatchByTime<I, T, F>
where
    I: Iterator<Item = T>,
{
    iter: I,
    key_fn: F,
    interval_ms: i64,
    current_batch: Vec<T>,
    current_boundary: Option<i64>,
    finished: bool,
}

impl<I, T, F> std::fmt::Debug for BatchByTime<I, T, F>
where
    I: Iterator<Item = T>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BatchByTime")
            .field("interval_ms", &self.interval_ms)
            .field("current_batch_size", &self.current_batch.len())
            .field("current_boundary", &self.current_boundary)
            .finish()
    }
}

impl<I, T, F, K> Iterator for BatchByTime<I, T, F>
where
    I: Iterator<Item = T>,
    F: Fn(&T) -> K,
    K: Into<i64>,
{
    type Item = Vec<T>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }

        loop {
            match self.iter.next() {
                Some(item) => {
                    let ts: i64 = (self.key_fn)(&item).into();
                    let boundary = (ts / self.interval_ms) * self.interval_ms;

                    match self.current_boundary {
                        None => {
                            self.current_boundary = Some(boundary);
                            self.current_batch.push(item);
                        }
                        Some(b) if b == boundary => {
                            self.current_batch.push(item);
                        }
                        Some(_) => {
                            // New boundary - return current batch and start new one
                            let batch = std::mem::take(&mut self.current_batch);
                            self.current_boundary = Some(boundary);
                            self.current_batch.push(item);
                            return Some(batch);
                        }
                    }
                }
                None => {
                    self.finished = true;
                    if self.current_batch.is_empty() {
                        return None;
                    }
                    return Some(std::mem::take(&mut self.current_batch));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_batch_by_time() {
        let records: Vec<i64> = vec![100, 200, 300, 1100, 1200, 2100];

        let batches: Vec<Vec<i64>> =
            batch_by_time(records.into_iter(), |&x| x, Duration::from_secs(1)).collect();

        assert_eq!(batches.len(), 3);
        assert_eq!(batches[0], vec![100, 200, 300]);
        assert_eq!(batches[1], vec![1100, 1200]);
        assert_eq!(batches[2], vec![2100]);
    }

    #[test]
    fn test_empty_input() {
        let records: Vec<i64> = vec![];
        let batches: Vec<Vec<i64>> =
            batch_by_time(records.into_iter(), |&x| x, Duration::from_secs(1)).collect();

        assert!(batches.is_empty());
    }

    #[test]
    fn test_single_batch() {
        let records: Vec<i64> = vec![100, 200, 300];
        let batches: Vec<Vec<i64>> =
            batch_by_time(records.into_iter(), |&x| x, Duration::from_secs(10)).collect();

        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0], vec![100, 200, 300]);
    }
}
