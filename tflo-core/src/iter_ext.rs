//! Iterator extension trait for temporal computations.
//!
//! This module provides the `.temporal()` and `.temporal_with()` methods
//! that can be called on any iterator to create temporal computations.

use crate::builder::{Compile, TFlowBuilder};
use crate::compile::{CompiledGraph, ExtractOutput};
use crate::error::{TFloError, TFloResult};
use std::collections::HashMap;
use std::sync::Arc;

/// Extension trait for adding temporal computations to iterators.
///
/// This trait is automatically implemented for all iterators, allowing
/// you to call `.temporal()` or `.temporal_with()` on any iterator.
///
/// # Examples
///
/// ```rust
/// use tflo_core::prelude::*;
///
/// #[derive(Clone)]
/// struct Tick {
///     ts: i64,
///     price: f64,
/// }
///
/// let ticks = vec![
///     Tick { ts: 1000, price: 100.0 },
///     Tick { ts: 2000, price: 101.0 },
///     Tick { ts: 3000, price: 102.0 },
/// ];
///
/// // Just the computed values (scale price by 2)
/// let doubled: Vec<f64> = ticks.iter().cloned()
///     .tflo(|t| {
///         t.timestamp(|x| x.ts);
///         let price = t.prop(|x| x.price);
///         price.map_f64(|x| x * 2.0)
///     })
///     .collect();
///
/// // Keep original record along with computed values
/// let enriched: Vec<(Tick, f64)> = ticks.into_iter()
///     .with(|t| {
///         t.timestamp(|x| x.ts);
///         let price = t.prop(|x| x.price);
///         price.map_f64(|x| x * 2.0)
///     })
///     .collect();
/// ```
pub trait TFlowIteratorExt<R>: Iterator<Item = R> + Sized {
    /// Apply temporal computations to the iterator, returning computed values.
    ///
    /// The closure receives a [`TemporalBuilder`] and should return one or more
    /// [`Comp`](crate::comp::Comp) values (or a tuple of them).
    ///
    /// # Panics
    ///
    /// Panics if `t.timestamp()` is not called for time-based windows.
    fn tflo<F, C>(self, f: F) -> TFloIter<Self, R, C::Output>
    where
        F: FnOnce(&mut TFlowBuilder<R>) -> C,
        C: Compile<R>,
        C::Output: ExtractOutput,
        R: 'static,
    {
        let mut builder = TFlowBuilder::new();
        let comps = f(&mut builder);

        let timestamp_fn = builder.timestamp_fn.clone().unwrap_or_else(|| {
            // Default to returning 0 if no timestamp function provided
            // This will work for count-based windows
            Arc::new(|_| 0)
        });

        let output_ids = comps.output_ids();
        let nodes = builder.into_nodes();
        let graph = CompiledGraph::compile(timestamp_fn, nodes, output_ids);

        TFloIter {
            iter: self,
            graph,
            _marker: std::marker::PhantomData,
        }
    }

    /// Apply temporal computations, keeping the original record.
    ///
    /// Similar to `temporal()`, but each output item is a tuple of
    /// `(original_record, computed_values)`.
    fn with<F, C>(self, f: F) -> TFloWithIter<Self, R, C::Output>
    where
        F: FnOnce(&mut TFlowBuilder<R>) -> C,
        C: Compile<R>,
        C::Output: ExtractOutput,
        R: Clone + 'static,
    {
        let mut builder = TFlowBuilder::new();
        let comps = f(&mut builder);

        let timestamp_fn = builder
            .timestamp_fn
            .clone()
            .unwrap_or_else(|| Arc::new(|_| 0));

        let output_ids = comps.output_ids();
        let nodes = builder.into_nodes();
        let graph = CompiledGraph::compile(timestamp_fn, nodes, output_ids);

        TFloWithIter {
            iter: self,
            graph,
            _marker: std::marker::PhantomData,
        }
    }

    /// Apply temporal computations with validation options.
    ///
    /// Allows configuring validation like sorted timestamp checking.
    fn validated<F, C>(
        self,
        options: crate::validation::ValidationOptions,
        f: F,
    ) -> TFloValidatedIter<Self, R, C::Output>
    where
        F: FnOnce(&mut TFlowBuilder<R>) -> C,
        C: Compile<R>,
        C::Output: ExtractOutput,
        R: 'static,
    {
        let mut builder = TFlowBuilder::new();
        let comps = f(&mut builder);

        let timestamp_fn = builder
            .timestamp_fn
            .clone()
            .unwrap_or_else(|| Arc::new(|_| 0));

        let output_ids = comps.output_ids();
        let nodes = builder.into_nodes();
        let mut graph = CompiledGraph::compile(timestamp_fn.clone(), nodes, output_ids);
        // Honour the configured warmup: the graph suppresses output until it
        // has seen `min_warmup` records.
        graph.set_min_warmup(options.min_warmup);
        let validator = crate::validation::ValueValidator::new(options.clone());

        TFloValidatedIter {
            iter: self,
            graph,
            timestamp_fn,
            options,
            last_ts: None,
            validator,
            _marker: std::marker::PhantomData,
        }
    }

    /// Apply temporal computations, returning results that explicitly handle warmup and errors.
    ///
    /// Similar to `tflo()`, but returns `TFloResult` instead of silently filtering
    /// warmup periods and errors. Use this when you need explicit control over
    /// warmup handling and error propagation.
    fn tflo_try<F, C>(self, f: F) -> TFloTryIter<Self, R, C::Output>
    where
        F: FnOnce(&mut TFlowBuilder<R>) -> C,
        C: Compile<R>,
        C::Output: ExtractOutput,
        R: 'static,
    {
        let mut builder = TFlowBuilder::new();
        let comps = f(&mut builder);

        let timestamp_fn = builder
            .timestamp_fn
            .clone()
            .unwrap_or_else(|| Arc::new(|_| 0));

        let output_ids = comps.output_ids();
        let nodes = builder.into_nodes();
        let graph = CompiledGraph::compile(timestamp_fn, nodes, output_ids);

        TFloTryIter {
            iter: self,
            graph,
            _marker: std::marker::PhantomData,
        }
    }

    /// Apply keyed temporal computations, routing records to per-key graphs.
    ///
    /// This method partitions records by key and runs separate computation graphs
    /// for each key, ensuring state isolation while preserving key attribution
    /// in the pipeline context.
    ///
    /// # Example
    ///
    /// ```rust
    /// use tflo_core::prelude::*;
    ///
    /// #[derive(Clone)]
    /// struct Tick {
    ///     ts: i64,
    ///     symbol: String,
    ///     price: f64,
    /// }
    ///
    /// let ticks = vec![
    ///     Tick { ts: 1000, symbol: "AAPL".to_string(), price: 100.0 },
    ///     Tick { ts: 2000, symbol: "MSFT".to_string(), price: 200.0 },
    /// ];
    ///
    /// let results: Vec<_> = ticks.into_iter()
    ///     .tflo_keyed(
    ///         |r| r.symbol.clone(),
    ///         tflo_core::keyed::OutOfOrderPolicy::Error,
    ///         |t| {
    ///             t.timestamp(|x| x.ts);
    ///             let price = t.prop(|x| x.price);
    ///             price.map_f64(|x| x * 2.0)
    ///         }
    ///     )
    ///     .collect();
    /// ```
    fn tflo_keyed<KF, FF, C, K>(
        self,
        key_fn: KF,
        policy: crate::keyed::OutOfOrderPolicy,
        builder_fn: FF,
    ) -> crate::keyed::TFloKeyedIter<Self, R, C::Output, K, C>
    where
        KF: Fn(&R) -> K + Send + Sync + 'static,
        FF: Fn(&mut TFlowBuilder<R>) -> C + Send + Sync + 'static,
        C: Compile<R>,
        C::Output: ExtractOutput,
        K: std::hash::Hash + Eq + Clone + Send + Sync + Default + 'static,
        R: 'static,
    {
        let mut builder = TFlowBuilder::new();
        let _comps = builder_fn(&mut builder);

        let timestamp_fn = builder
            .get_timestamp_fn()
            .unwrap_or_else(|| Arc::new(|_| 0));

        crate::keyed::TFloKeyedIter {
            iter: self,
            graphs: HashMap::new(),
            timestamp_fn,
            key_fn: Arc::new(key_fn),
            builder_fn: Box::new(builder_fn),
            policy,
            ready_queue: std::collections::VecDeque::new(),
            flushed: false,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<I, R> TFlowIteratorExt<R> for I where I: Iterator<Item = R> {}

/// Iterator adapter that applies temporal computations.
pub struct TFloIter<I, R, O> {
    iter: I,
    graph: CompiledGraph<R, O>,
    _marker: std::marker::PhantomData<R>,
}

impl<I, R, O> std::fmt::Debug for TFloIter<I, R, O>
where
    I: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TemporalIter")
            .field("iter", &self.iter)
            .field("graph", &self.graph)
            .finish()
    }
}

impl<I, R, O> Iterator for TFloIter<I, R, O>
where
    I: Iterator<Item = R>,
    O: ExtractOutput,
{
    type Item = O;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let record = self.iter.next()?;
            if let Some(item) = self.graph.step(&record) {
                return Some(item.value);
            }
            // Continue if step returned None (warmup period)
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        // Lower bound could be 0 during warmup
        (0, self.iter.size_hint().1)
    }
}

/// Iterator adapter that keeps original records with computed values.
pub struct TFloWithIter<I, R, O> {
    iter: I,
    graph: CompiledGraph<R, O>,
    _marker: std::marker::PhantomData<R>,
}

impl<I, R, O> std::fmt::Debug for TFloWithIter<I, R, O>
where
    I: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TemporalWithIter")
            .field("iter", &self.iter)
            .field("graph", &self.graph)
            .finish()
    }
}

impl<I, R, O> Iterator for TFloWithIter<I, R, O>
where
    I: Iterator<Item = R>,
    R: Clone,
    O: ExtractOutput,
{
    type Item = (R, O);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let record = self.iter.next()?;
            if let Some(item) = self.graph.step(&record) {
                return Some((record, item.value));
            }
            // Continue if step returned None (warmup period)
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        // Lower bound could be 0 during warmup
        (0, self.iter.size_hint().1)
    }
}

/// Iterator adapter that returns results with explicit error handling.
pub struct TFloTryIter<I, R, O> {
    iter: I,
    graph: CompiledGraph<R, O>,
    _marker: std::marker::PhantomData<R>,
}

impl<I, R, O> std::fmt::Debug for TFloTryIter<I, R, O>
where
    I: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TFloTryIter")
            .field("iter", &self.iter)
            .field("graph", &self.graph)
            .finish()
    }
}

impl<I, R, O> Iterator for TFloTryIter<I, R, O>
where
    I: Iterator<Item = R>,
    O: ExtractOutput,
{
    type Item = TFloResult<crate::pipeline::PipelineItem<crate::pipeline::Timestamped, O>>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let record = self.iter.next()?;
            match self.graph.step_with_status(&record) {
                crate::compile::StepResult::Ready(item) => return Some(Ok(item)),
                crate::compile::StepResult::WarmingUp { .. } => continue,
                crate::compile::StepResult::Error(e) => return Some(Err(TFloError::Compute(e))),
            }
        }
    }
}

/// Iterator adapter with validation.
pub struct TFloValidatedIter<I, R, O> {
    iter: I,
    graph: CompiledGraph<R, O>,
    timestamp_fn: Arc<dyn Fn(&R) -> i64 + Send + Sync>,
    options: crate::validation::ValidationOptions,
    last_ts: Option<i64>,
    validator: crate::validation::ValueValidator,
    _marker: std::marker::PhantomData<R>,
}

impl<I, R, O> std::fmt::Debug for TFloValidatedIter<I, R, O>
where
    I: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TemporalValidatedIter")
            .field("iter", &self.iter)
            .field("options", &self.options)
            .field("last_ts", &self.last_ts)
            .finish()
    }
}

impl<I, R, O> Iterator for TFloValidatedIter<I, R, O>
where
    I: Iterator<Item = R>,
    O: ExtractOutput,
{
    type Item = TFloResult<O>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let record = self.iter.next()?;
            let ts = (self.timestamp_fn)(&record);

            // Check sorted order if enabled.
            if self.options.assert_sorted {
                if let Some(last) = self.last_ts {
                    if ts < last {
                        return Some(Err(TFloError::OutOfOrderTimestamp {
                            previous: last,
                            current: ts,
                        }));
                    }
                }
            }
            // Check the maximum inter-record gap if configured.
            if let Some(max_gap) = self.options.max_gap_ms {
                if let Some(last) = self.last_ts {
                    if ts.saturating_sub(last) > max_gap {
                        return Some(Err(TFloError::TimestampGapExceeded {
                            previous: last,
                            current: ts,
                            max_gap,
                        }));
                    }
                }
            }
            self.last_ts = Some(ts);

            if let Some(item) = self.graph.step(&record) {
                // Apply the NaN / infinity / negative value checks. They are
                // only meaningful for scalar `f64` outputs (`as_f64` is `None`
                // for everything else).
                if let Some(value) = item.value.as_f64() {
                    match self.validator.check_strict(value) {
                        Ok(true) => return Some(Ok(item.value)),
                        // A `reject_*` option matched — filter this value out.
                        Ok(false) => continue,
                        Err(e) => return Some(Err(e)),
                    }
                }
                return Some(Ok(item.value));
            }
            // Continue if step returned None (warmup period).
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug)]
    struct TestRecord {
        ts: i64,
        value: f64,
    }

    #[test]
    fn test_temporal_map() {
        let records = vec![
            TestRecord {
                ts: 1000,
                value: 10.0,
            },
            TestRecord {
                ts: 2000,
                value: 20.0,
            },
            TestRecord {
                ts: 3000,
                value: 30.0,
            },
        ];

        let results: Vec<f64> = records
            .into_iter()
            .tflo(|t| {
                let _ = t.timestamp(|x| x.ts);
                let value = t.prop(|x| x.value);
                value.map_f64(|x| x * 2.0)
            })
            .collect();

        assert_eq!(results.len(), 3);
        assert!((results[0] - 20.0).abs() < 0.001);
        assert!((results[1] - 40.0).abs() < 0.001);
        assert!((results[2] - 60.0).abs() < 0.001);
    }

    #[test]
    fn test_temporal_with() {
        let records = vec![
            TestRecord {
                ts: 1000,
                value: 10.0,
            },
            TestRecord {
                ts: 2000,
                value: 20.0,
            },
        ];

        let results: Vec<(TestRecord, f64)> = records
            .into_iter()
            .with(|t| {
                let _ = t.timestamp(|x| x.ts);
                let value = t.prop(|x| x.value);
                value.map_f64(|x| x + 1.0)
            })
            .collect();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0.value, 10.0);
        assert!((results[0].1 - 11.0).abs() < 0.001);
    }

    #[test]
    fn test_temporal_tuple_output() {
        let records = vec![
            TestRecord {
                ts: 1000,
                value: 10.0,
            },
            TestRecord {
                ts: 2000,
                value: 20.0,
            },
        ];

        let results: Vec<(f64, f64)> = records
            .into_iter()
            .tflo(|t| {
                let _ = t.timestamp(|x| x.ts);
                let value = t.prop(|x| x.value);
                let doubled = value.map_f64(|x| x * 2.0);
                let tripled = value.map_f64(|x| x * 3.0);
                (doubled, tripled)
            })
            .collect();

        assert_eq!(results.len(), 2);
        assert!((results[1].0 - 40.0).abs() < 0.001); // doubled
        assert!((results[1].1 - 60.0).abs() < 0.001); // tripled
    }
}
