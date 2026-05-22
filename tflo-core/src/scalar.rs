//! Scalar trait for generic numeric types.
//!
//! This module provides the [`Scalar`] trait which abstracts over different
//! numeric types, allowing the library to work with various numeric representations.

use std::ops::{Add, Div, Mul, Sub};

/// Trait for scalar numeric types used in computations.
///
/// This trait allows the library to work with different numeric types
/// (f32, f64, integers, etc.) in a generic way.
pub trait Scalar:
    Copy
    + Clone
    + PartialOrd
    + PartialEq
    + Send
    + Sync
    + Add<Output = Self>
    + Sub<Output = Self>
    + Mul<Output = Self>
    + Div<Output = Self>
    + Default
{
    /// Zero value for this scalar type.
    fn zero() -> Self;

    /// One value for this scalar type.
    fn one() -> Self;

    /// Check if the value is finite (not NaN or infinite).
    fn is_finite(self) -> bool;

    /// Check if the value is NaN.
    fn is_nan(self) -> bool;

    /// Check if the value is infinite.
    fn is_infinite(self) -> bool;

    /// Convert from f64 (may lose precision).
    fn from_f64(value: f64) -> Self;

    /// Convert to f64 (may lose precision).
    fn to_f64(self) -> f64;

    /// Absolute value.
    fn abs(self) -> Self;

    /// Square root.
    fn sqrt(self) -> Self;

    /// Natural logarithm.
    fn ln(self) -> Self;

    /// Exponential function.
    fn exp(self) -> Self;

    /// Power function: self^exponent.
    fn powf(self, exponent: Self) -> Self;

    /// Maximum of two values.
    fn max(self, other: Self) -> Self;

    /// Minimum of two values.
    fn min(self, other: Self) -> Self;
}

impl Scalar for f64 {
    fn zero() -> Self {
        0.0
    }

    fn one() -> Self {
        1.0
    }

    fn is_finite(self) -> bool {
        self.is_finite()
    }

    fn is_nan(self) -> bool {
        self.is_nan()
    }

    fn is_infinite(self) -> bool {
        self.is_infinite()
    }

    fn from_f64(value: f64) -> Self {
        value
    }

    fn to_f64(self) -> f64 {
        self
    }

    fn abs(self) -> Self {
        self.abs()
    }

    fn sqrt(self) -> Self {
        self.sqrt()
    }

    fn ln(self) -> Self {
        self.ln()
    }

    fn exp(self) -> Self {
        self.exp()
    }

    fn powf(self, exponent: Self) -> Self {
        self.powf(exponent)
    }

    fn max(self, other: Self) -> Self {
        self.max(other)
    }

    fn min(self, other: Self) -> Self {
        self.min(other)
    }
}

impl Scalar for f32 {
    #[inline]
    fn zero() -> Self {
        0.0
    }

    #[inline]
    fn one() -> Self {
        1.0
    }

    fn is_finite(self) -> bool {
        self.is_finite()
    }

    fn is_nan(self) -> bool {
        self.is_nan()
    }

    fn is_infinite(self) -> bool {
        self.is_infinite()
    }

    fn from_f64(value: f64) -> Self {
        value as f32
    }

    fn to_f64(self) -> f64 {
        self as f64
    }

    fn abs(self) -> Self {
        self.abs()
    }

    fn sqrt(self) -> Self {
        self.sqrt()
    }

    fn ln(self) -> Self {
        self.ln()
    }

    fn exp(self) -> Self {
        self.exp()
    }

    fn powf(self, exponent: Self) -> Self {
        self.powf(exponent)
    }

    fn max(self, other: Self) -> Self {
        self.max(other)
    }

    fn min(self, other: Self) -> Self {
        self.min(other)
    }
}

