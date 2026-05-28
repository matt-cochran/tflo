//! Cumulative (expanding window) aggregations.
//!
//! These primitives track running totals since the start of the stream,
//! unlike sliding windows which only consider recent values.

/// Cumulative sum tracker.
///
/// Tracks the running sum of all values seen since creation.
///
/// # Examples
///
/// ```rust
/// use tflo_ops::primitives::CumulativeSum;
///
/// let mut cumsum = CumulativeSum::new();
///
/// assert_eq!(cumsum.push(10.0), 10.0);
/// assert_eq!(cumsum.push(20.0), 30.0);
/// assert_eq!(cumsum.push(5.0), 35.0);
/// ```
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct CumulativeSum {
    sum: f64,
}

impl CumulativeSum {
    /// Create a new cumulative sum tracker.
    #[must_use]
    pub const fn new() -> Self {
        Self { sum: 0.0 }
    }

    /// Add a value and return the new cumulative sum.
    pub fn push(&mut self, value: f64) -> f64 {
        self.sum += value;
        self.sum
    }

    /// Get the current cumulative sum without adding a value.
    #[must_use]
    pub const fn value(&self) -> f64 {
        self.sum
    }

    /// Reset the cumulative sum to zero.
    pub const fn reset(&mut self) {
        self.sum = 0.0;
    }
}

/// Cumulative product tracker.
///
/// Tracks the running product of all values seen since creation.
///
/// # Examples
///
/// ```rust
/// use tflo_ops::primitives::CumulativeProduct;
///
/// let mut cumprod = CumulativeProduct::new();
///
/// assert_eq!(cumprod.push(2.0), 2.0);
/// assert_eq!(cumprod.push(3.0), 6.0);
/// assert_eq!(cumprod.push(4.0), 24.0);
/// ```
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CumulativeProduct {
    product: f64,
}

impl Default for CumulativeProduct {
    fn default() -> Self {
        Self::new()
    }
}

impl CumulativeProduct {
    /// Create a new cumulative product tracker.
    #[must_use]
    pub const fn new() -> Self {
        Self { product: 1.0 }
    }

    /// Add a value and return the new cumulative product.
    pub fn push(&mut self, value: f64) -> f64 {
        self.product *= value;
        self.product
    }

    /// Get the current cumulative product without adding a value.
    #[must_use]
    pub const fn value(&self) -> f64 {
        self.product
    }

    /// Reset the cumulative product to 1.0.
    pub const fn reset(&mut self) {
        self.product = 1.0;
    }
}

/// Cumulative maximum tracker.
///
/// Tracks the running maximum of all values seen since creation.
/// Also known as "high-water mark".
///
/// # Examples
///
/// ```rust
/// use tflo_ops::primitives::CumulativeMax;
///
/// let mut cummax = CumulativeMax::new();
///
/// assert_eq!(cummax.push(10.0), 10.0);
/// assert_eq!(cummax.push(5.0), 10.0);   // Still 10
/// assert_eq!(cummax.push(15.0), 15.0);  // New max
/// assert_eq!(cummax.push(12.0), 15.0);  // Still 15
/// ```
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CumulativeMax {
    max: f64,
}

impl Default for CumulativeMax {
    fn default() -> Self {
        Self::new()
    }
}

impl CumulativeMax {
    /// Create a new cumulative max tracker.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            max: f64::NEG_INFINITY,
        }
    }

    /// Add a value and return the new cumulative maximum.
    pub fn push(&mut self, value: f64) -> f64 {
        if value > self.max {
            self.max = value;
        }
        self.max
    }

    /// Get the current cumulative maximum without adding a value.
    #[must_use]
    pub const fn value(&self) -> f64 {
        self.max
    }

    /// Reset to initial state.
    pub const fn reset(&mut self) {
        self.max = f64::NEG_INFINITY;
    }
}

/// Cumulative minimum tracker.
///
/// Tracks the running minimum of all values seen since creation.
///
/// # Examples
///
/// ```rust
/// use tflo_ops::primitives::CumulativeMin;
///
/// let mut cummin = CumulativeMin::new();
///
/// assert_eq!(cummin.push(10.0), 10.0);
/// assert_eq!(cummin.push(15.0), 10.0);  // Still 10
/// assert_eq!(cummin.push(5.0), 5.0);    // New min
/// assert_eq!(cummin.push(8.0), 5.0);    // Still 5
/// ```
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CumulativeMin {
    min: f64,
}

impl Default for CumulativeMin {
    fn default() -> Self {
        Self::new()
    }
}

impl CumulativeMin {
    /// Create a new cumulative min tracker.
    #[must_use]
    pub const fn new() -> Self {
        Self { min: f64::INFINITY }
    }

    /// Add a value and return the new cumulative minimum.
    pub fn push(&mut self, value: f64) -> f64 {
        if value < self.min {
            self.min = value;
        }
        self.min
    }

    /// Get the current cumulative minimum without adding a value.
    #[must_use]
    pub const fn value(&self) -> f64 {
        self.min
    }

    /// Reset to initial state.
    pub const fn reset(&mut self) {
        self.min = f64::INFINITY;
    }
}

/// Cumulative mean tracker using Welford's algorithm.
///
/// Tracks the running mean of all values seen since creation
/// using numerically stable incremental computation.
///
/// # Examples
///
/// ```rust
/// use tflo_ops::primitives::CumulativeMean;
///
/// let mut cummean = CumulativeMean::new();
///
/// assert_eq!(cummean.push(10.0), 10.0);
/// assert_eq!(cummean.push(20.0), 15.0);
/// assert_eq!(cummean.push(30.0), 20.0);
/// ```
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct CumulativeMean {
    count: u64,
    mean: f64,
}

impl CumulativeMean {
    /// Create a new cumulative mean tracker.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            count: 0,
            mean: 0.0,
        }
    }

    /// Add a value and return the new cumulative mean.
    pub fn push(&mut self, value: f64) -> f64 {
        // SAFETY: `self.count` is a `u64` observation counter. Saturating at
        // `u64::MAX` is the only behavior that survives 1.8e19 pushes; under
        // saturation the running mean simply freezes — no panic.
        self.count = self.count.saturating_add(1);
        let delta = value - self.mean;
        self.mean += delta / self.count as f64;
        self.mean
    }

    /// Get the current cumulative mean without adding a value.
    ///
    /// Returns `f64::NAN` if no values have been added.
    #[must_use]
    pub const fn value(&self) -> f64 {
        if self.count == 0 { f64::NAN } else { self.mean }
    }

    /// Get the count of values.
    #[must_use]
    pub const fn count(&self) -> u64 {
        self.count
    }

    /// Reset to initial state.
    pub const fn reset(&mut self) {
        self.count = 0;
        self.mean = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cumsum() {
        let mut cumsum = CumulativeSum::new();

        assert_eq!(cumsum.push(10.0), 10.0);
        assert_eq!(cumsum.push(20.0), 30.0);
        assert_eq!(cumsum.push(-5.0), 25.0);
        assert_eq!(cumsum.value(), 25.0);

        cumsum.reset();
        assert_eq!(cumsum.value(), 0.0);
    }

    #[test]
    fn test_cumprod() {
        let mut cumprod = CumulativeProduct::new();

        assert_eq!(cumprod.push(2.0), 2.0);
        assert_eq!(cumprod.push(3.0), 6.0);
        assert_eq!(cumprod.push(0.5), 3.0);
        assert_eq!(cumprod.value(), 3.0);

        cumprod.reset();
        assert_eq!(cumprod.value(), 1.0);
    }

    #[test]
    fn test_cummax() {
        let mut cummax = CumulativeMax::new();

        assert_eq!(cummax.push(10.0), 10.0);
        assert_eq!(cummax.push(5.0), 10.0);
        assert_eq!(cummax.push(15.0), 15.0);
        assert_eq!(cummax.push(12.0), 15.0);
        assert_eq!(cummax.value(), 15.0);

        cummax.reset();
        assert_eq!(cummax.value(), f64::NEG_INFINITY);
    }

    #[test]
    fn test_cummin() {
        let mut cummin = CumulativeMin::new();

        assert_eq!(cummin.push(10.0), 10.0);
        assert_eq!(cummin.push(15.0), 10.0);
        assert_eq!(cummin.push(5.0), 5.0);
        assert_eq!(cummin.push(8.0), 5.0);
        assert_eq!(cummin.value(), 5.0);

        cummin.reset();
        assert_eq!(cummin.value(), f64::INFINITY);
    }

    #[test]
    fn test_cummean() {
        let mut cummean = CumulativeMean::new();

        assert!(cummean.value().is_nan());
        assert_eq!(cummean.push(10.0), 10.0);
        assert_eq!(cummean.push(20.0), 15.0);
        assert_eq!(cummean.push(30.0), 20.0);
        assert_eq!(cummean.count(), 3);
    }

    #[test]
    fn test_cummax_with_negative() {
        let mut cummax = CumulativeMax::new();

        assert_eq!(cummax.push(-10.0), -10.0);
        assert_eq!(cummax.push(-20.0), -10.0);
        assert_eq!(cummax.push(-5.0), -5.0);
    }

    #[test]
    fn test_cummin_with_negative() {
        let mut cummin = CumulativeMin::new();

        assert_eq!(cummin.push(-10.0), -10.0);
        assert_eq!(cummin.push(-5.0), -10.0);
        assert_eq!(cummin.push(-20.0), -20.0);
    }
}
