//! Concrete operator catalog built on the generic shapes.
//!
//! [`windows`] holds the windowed-aggregation operators (SMA, rolling std,
//! EMA, RSI, …) and the [`WindowOps`](windows::WindowOps) extension trait that
//! exposes every windowed method on `Comp`. [`stats`] holds the distribution
//! and bivariate reductions (median, quantile, correlation, …) that
//! `WindowOps` reuses. [`trackers`] holds the stateful single-state trackers
//! (prev, lag, delta, cumulative aggregates, returns, rate derivatives) and
//! the [`StatefulOps`](trackers::StatefulOps) extension trait. [`detectors`]
//! holds the event-detector operators (cross, glitch, runt, pulse-width,
//! window) and the [`CrossOps`](detectors::CrossOps) /
//! [`DetectorOps`](detectors::DetectorOps) extension traits.

pub mod detectors;
pub mod math;
pub mod stats;
pub mod trackers;
pub mod windows;
