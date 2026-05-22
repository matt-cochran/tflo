//! Concrete operator catalog built on the generic shapes.
//!
//! [`windows`] holds the windowed-aggregation operators (SMA, rolling std,
//! EMA, RSI, …) and the [`WindowOps`](windows::WindowOps) extension trait that
//! exposes every windowed method on `Comp`. [`stats`] holds the distribution
//! and bivariate reductions (median, quantile, correlation, …) that
//! `WindowOps` reuses.

pub mod stats;
pub mod windows;
