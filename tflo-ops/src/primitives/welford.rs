//! Welford's algorithm for numerically stable online variance calculation.
//!
//! [`WelfordAccumulator`] implements Welford's online algorithm which provides
//! stable computation of variance and standard deviation, avoiding the
//! numerical instability of the naive sum-of-squares approach.

/// Numerically stable online variance calculator using Welford's algorithm.
///
/// This implementation avoids the numerical instability that can occur with
/// the naive approach of computing `E[X²] - E[X]²`, especially for:
/// - Large values
/// - Values with small variance relative to their magnitude
/// - Long-running streams with many observations
///
/// # Algorithm
///
/// For each new value x:
/// 1. Update count: n = n + 1
/// 2. Update mean: mean = mean + (x - mean) / n
/// 3. Update M2: M2 = M2 + (x - `mean_old`) * (x - `mean_new`)
///
/// Variance = M2 / n (population) or M2 / (n-1) (sample)
///
/// # Examples
///
/// ```rust
/// use tflo_ops::primitives::WelfordAccumulator;
///
/// let mut acc = WelfordAccumulator::new();
///
/// acc.push(2.0);
/// acc.push(4.0);
/// acc.push(4.0);
/// acc.push(4.0);
/// acc.push(5.0);
/// acc.push(5.0);
/// acc.push(7.0);
/// acc.push(9.0);
///
/// assert_eq!(acc.mean(), 5.0);
/// assert!((acc.population_variance() - 4.0).abs() < 1e-10);
/// assert!((acc.population_std() - 2.0).abs() < 1e-10);
/// ```
#[derive(Debug, Clone)]
pub struct WelfordAccumulator {
    count: u64,
    mean: f64,
    m2: f64, // Sum of squared differences from the mean
}

impl Default for WelfordAccumulator {
    fn default() -> Self {
        Self::new()
    }
}

impl WelfordAccumulator {
    /// Create a new Welford accumulator.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            count: 0,
            mean: 0.0,
            m2: 0.0,
        }
    }

    /// Add a new value to the accumulator.
    pub fn push(&mut self, value: f64) {
        // SAFETY: `self.count` is a `u64` observation counter. Saturating at
        // `u64::MAX` is the only behavior that survives the absurd case of
        // 1.8e19 pushes; under saturation the mean / variance formulas degrade
        // gracefully to "stuck on the running estimate" rather than panic.
        self.count = self.count.saturating_add(1);
        let delta = value - self.mean;
        self.mean += delta / self.count as f64;
        let delta2 = value - self.mean;
        self.m2 += delta * delta2;
    }

    /// Remove a value from the accumulator (reverse of push).
    ///
    /// This is useful for sliding window implementations where old values
    /// need to be removed. The value must have been previously pushed.
    ///
    /// # Note
    ///
    /// Removing values can introduce numerical errors over many operations.
    /// For sliding windows, consider using [`WelfordWindow`] instead.
    pub fn remove(&mut self, value: f64) {
        if self.count == 0 {
            return;
        }

        if self.count == 1 {
            self.count = 0;
            self.mean = 0.0;
            self.m2 = 0.0;
            return;
        }

        let delta = value - self.mean;
        // SAFETY: the early-returns above ensure `self.count >= 2` here, so
        // `count - 1 >= 1` cannot underflow.
        #[allow(clippy::arithmetic_side_effects)]
        let new_count = self.count - 1;
        self.mean = (self.mean * self.count as f64 - value) / new_count as f64;
        let delta2 = value - self.mean;
        self.m2 -= delta * delta2;
        self.count = new_count;
    }

    /// Get the number of values.
    #[must_use]
    pub const fn count(&self) -> u64 {
        self.count
    }

    /// Check if the accumulator is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Get the mean of all values.
    ///
    /// Returns `f64::NAN` if empty.
    #[must_use]
    pub const fn mean(&self) -> f64 {
        if self.count == 0 { f64::NAN } else { self.mean }
    }

    /// Get the population variance.
    ///
    /// Returns `f64::NAN` if fewer than 2 values.
    #[must_use]
    pub fn population_variance(&self) -> f64 {
        if self.count < 2 {
            f64::NAN
        } else {
            (self.m2 / self.count as f64).max(0.0)
        }
    }

    /// Get the sample variance (using N-1 denominator).
    ///
    /// Returns `f64::NAN` if fewer than 2 values.
    #[must_use]
    pub fn sample_variance(&self) -> f64 {
        if self.count < 2 {
            f64::NAN
        } else {
            // SAFETY: the `count < 2` guard above ensures `count >= 2`, so
            // `count - 1 >= 1` cannot underflow.
            #[allow(clippy::arithmetic_side_effects)]
            let denom = (self.count - 1) as f64;
            (self.m2 / denom).max(0.0)
        }
    }

    /// Get the population standard deviation.
    ///
    /// Returns `f64::NAN` if fewer than 2 values.
    #[must_use]
    pub fn population_std(&self) -> f64 {
        self.population_variance().sqrt()
    }

    /// Get the sample standard deviation.
    ///
    /// Returns `f64::NAN` if fewer than 2 values.
    #[must_use]
    pub fn sample_std(&self) -> f64 {
        self.sample_variance().sqrt()
    }

    /// Reset the accumulator.
    pub const fn reset(&mut self) {
        self.count = 0;
        self.mean = 0.0;
        self.m2 = 0.0;
    }

    /// Merge another accumulator into this one.
    ///
    /// Uses Chan's parallel algorithm for combining partial results.
    pub fn merge(&mut self, other: &Self) {
        if other.count == 0 {
            return;
        }
        if self.count == 0 {
            *self = other.clone();
            return;
        }

        // SAFETY: saturating to keep numerical-stability properties under the
        // absurd merge case (two accumulators each near `u64::MAX`); the
        // saturated count then merely "freezes" the mean update — same
        // graceful degradation as `push`.
        let combined_count = self.count.saturating_add(other.count);
        let delta = other.mean - self.mean;
        let combined_mean = self.mean + delta * (other.count as f64 / combined_count as f64);

        let combined_m2 = self.m2
            + other.m2
            + delta * delta * (self.count as f64) * (other.count as f64) / (combined_count as f64);

        self.count = combined_count;
        self.mean = combined_mean;
        self.m2 = combined_m2;
    }
}

/// Time-based sliding window using Welford's algorithm.
///
/// Provides numerically stable variance calculation for a sliding time window.
/// This periodically recomputes statistics from scratch to avoid drift.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct WelfordWindow {
    window_ms: i64,
    buffer: std::collections::VecDeque<(i64, f64)>,
    accumulator: WelfordAccumulator,
    recompute_interval: usize,
    ops_since_recompute: usize,
}

impl WelfordWindow {
    /// Create a new Welford-based time window.
    #[must_use]
    pub const fn new(window: std::time::Duration) -> Self {
        Self::with_recompute_interval(window, 1000)
    }

    /// Create with a custom recompute interval.
    ///
    /// Recomputing periodically helps avoid numerical drift from many
    /// add/remove operations.
    #[must_use]
    pub const fn with_recompute_interval(window: std::time::Duration, interval: usize) -> Self {
        #[allow(clippy::cast_possible_wrap)]
        let window_ms = window.as_millis() as i64;
        Self {
            window_ms,
            buffer: std::collections::VecDeque::new(),
            accumulator: WelfordAccumulator::new(),
            recompute_interval: interval,
            ops_since_recompute: 0,
        }
    }

    /// Add a new value and evict old ones.
    pub fn push(&mut self, ts: i64, value: f64) {
        self.buffer.push_back((ts, value));
        self.accumulator.push(value);
        // SAFETY: bounded counter — `ops_since_recompute` is reset to 0 every
        // `recompute_interval` increments by the `recompute()` call below, so
        // it never exceeds `recompute_interval` (a `usize` configuration value
        // that is itself bounded by `usize::MAX`).
        self.ops_since_recompute = self.ops_since_recompute.saturating_add(1);

        // SAFETY: `ts - window_ms` is the standard time-cutoff pattern.
        // Underflow ("clamp to before time zero") is a meaningful semantic for
        // the eviction check below; `saturating_sub` makes that explicit.
        let cutoff = ts.saturating_sub(self.window_ms);
        while let Some(&(old_ts, old_val)) = self.buffer.front() {
            if old_ts < cutoff {
                let _ = self.buffer.pop_front();
                self.accumulator.remove(old_val);
                // SAFETY: same bounded counter as the increment above.
                self.ops_since_recompute = self.ops_since_recompute.saturating_add(1);
            } else {
                break;
            }
        }

        // Periodically recompute to avoid drift
        if self.ops_since_recompute >= self.recompute_interval {
            self.recompute();
        }
    }

    /// Force recomputation of statistics from buffer.
    pub fn recompute(&mut self) {
        self.accumulator.reset();
        for &(_, value) in &self.buffer {
            self.accumulator.push(value);
        }
        self.ops_since_recompute = 0;
    }

    /// Get the count of values in the window.
    #[must_use]
    pub fn count(&self) -> usize {
        self.buffer.len()
    }

    /// Get the mean.
    #[must_use]
    pub const fn mean(&self) -> f64 {
        self.accumulator.mean()
    }

    /// Get the population variance.
    #[must_use]
    pub fn variance(&self) -> f64 {
        self.accumulator.population_variance()
    }

    /// Get the population standard deviation.
    #[must_use]
    pub fn std(&self) -> f64 {
        self.accumulator.population_std()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_welford_basic() {
        let mut acc = WelfordAccumulator::new();

        // Standard test case: [2,4,4,4,5,5,7,9] mean=5, variance=4
        for v in [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0] {
            acc.push(v);
        }

        assert_eq!(acc.count(), 8);
        assert!((acc.mean() - 5.0).abs() < 1e-10);
        assert!((acc.population_variance() - 4.0).abs() < 1e-10);
        assert!((acc.population_std() - 2.0).abs() < 1e-10);
    }

    #[test]
    fn test_welford_numerical_stability() {
        let mut acc = WelfordAccumulator::new();

        // Large values with small variance - naive algorithm would fail
        let base = 1e9;
        for i in 0..1000 {
            acc.push(base + (i as f64) * 0.001);
        }

        // Mean should be approximately base + 0.4995
        let expected_mean = base + 0.4995;
        assert!((acc.mean() - expected_mean).abs() < 0.001);

        // Variance should be stable and non-negative
        assert!(acc.population_variance() >= 0.0);
        assert!(!acc.population_variance().is_nan());
    }

    #[test]
    fn test_welford_merge() {
        let mut acc1 = WelfordAccumulator::new();
        let mut acc2 = WelfordAccumulator::new();

        for v in [1.0, 2.0, 3.0, 4.0] {
            acc1.push(v);
        }
        for v in [5.0, 6.0, 7.0, 8.0] {
            acc2.push(v);
        }

        acc1.merge(&acc2);

        // Combined should equal pushing all values
        let mut combined = WelfordAccumulator::new();
        for v in [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0] {
            combined.push(v);
        }

        assert!((acc1.mean() - combined.mean()).abs() < 1e-10);
        assert!((acc1.population_variance() - combined.population_variance()).abs() < 1e-10);
    }

    #[test]
    fn test_welford_window() {
        let mut window = WelfordWindow::new(std::time::Duration::from_secs(5));

        window.push(1000, 10.0);
        window.push(2000, 20.0);
        window.push(3000, 30.0);

        assert_eq!(window.count(), 3);
        assert!((window.mean() - 20.0).abs() < 1e-10);

        // Push value that evicts first entry
        window.push(7000, 40.0);
        assert_eq!(window.count(), 3); // 2000, 3000, 7000
        assert!((window.mean() - 30.0).abs() < 1e-10);
    }
}
