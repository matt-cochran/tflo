//! Async stream support for temporal computations.
//!
//! This module provides async versions of the temporal computation adapters,
//! allowing integration with async runtimes like Tokio.
//!
//! Enable with the `async` feature:
//!
//! ```toml
//! [dependencies]
//! tflow = { version = "0.1", features = ["async"] }
//! ```

use crate::builder::{Compile, TFlowBuilder};
use crate::compile::{CompiledGraph, ExtractOutput};
use crate::keyed::KeyedGraphState;
use crate::pipeline::KeyedTimestamped;
use futures::Stream;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

/// Extension trait for adding temporal computations to async streams.
///
/// This trait is automatically implemented for all types that implement
/// `Stream`, allowing you to call `.temporal()` on any async stream.
///
/// # Examples
///
/// ```ignore
/// use tflo::prelude::*;
/// use tokio_stream::StreamExt;
///
/// async fn process_stream(stream: impl Stream<Item = Tick>) {
///     let mut computed = stream.tflo(|t| {
///         t.timestamp(|x| x.ts);
///         let price = t.prop(|x| x.price);
///         price.map_f64(|x| x * 2.0)
///     });
///
///     while let Some(value) = computed.next().await {
///         println!("value: {}", value);
///     }
/// }
/// ```
pub trait TFloStreamExt<R>: Stream<Item = R> + Sized {
    /// Apply temporal computations to the stream.
    fn tflo<F, C>(self, f: F) -> TFloStream<Self, R, C::Output>
    where
        F: FnOnce(&mut TFlowBuilder<R>) -> C,
        C: Compile<R>,
        C::Output: ExtractOutput,
        R: 'static,
    {
        let mut builder = TFlowBuilder::new();
        let comps = f(&mut builder);

        let timestamp_fn = builder
            .get_timestamp_fn()
            .unwrap_or_else(|| Arc::new(|_| 0));

        let output_ids = comps.output_ids();
        let nodes = builder.into_nodes();
        let graph = CompiledGraph::compile(timestamp_fn, nodes, output_ids);

        TFloStream {
            stream: self,
            graph,
            _marker: std::marker::PhantomData,
        }
    }

    /// Apply temporal computations, keeping the original item.
    fn tflo_with<F, C>(self, f: F) -> TFlowWithStream<Self, R, C::Output>
    where
        F: FnOnce(&mut TFlowBuilder<R>) -> C,
        C: Compile<R>,
        C::Output: ExtractOutput,
        R: Clone + 'static,
    {
        let mut builder = TFlowBuilder::new();
        let comps = f(&mut builder);

        let timestamp_fn = builder
            .get_timestamp_fn()
            .unwrap_or_else(|| Arc::new(|_| 0));

        let output_ids = comps.output_ids();
        let nodes = builder.into_nodes();
        let graph = CompiledGraph::compile(timestamp_fn, nodes, output_ids);

        TFlowWithStream {
            stream: self,
            graph,
            _marker: std::marker::PhantomData,
        }
    }

    /// Apply keyed temporal computations, routing records to per-key graphs.
    ///
    /// This method partitions records by key and runs separate computation graphs
    /// for each key, ensuring state isolation while preserving key attribution
    /// in the pipeline context.
    fn tflo_keyed<KF, FF, C, K>(
        self,
        key_fn: KF,
        policy: crate::keyed::OutOfOrderPolicy,
        builder_fn: FF,
    ) -> TFloKeyedStream<Self, R, C::Output, K, C>
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

        TFloKeyedStream {
            stream: self,
            graphs: std::collections::HashMap::new(),
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

impl<S, R> TFloStreamExt<R> for S where S: Stream<Item = R> {}

/// Async stream that applies tflo computations.
#[pin_project::pin_project]
pub struct TFloStream<S, R, O> {
    #[pin]
    stream: S,
    graph: CompiledGraph<R, O>,
    _marker: std::marker::PhantomData<R>,
}

impl<S, R, O> std::fmt::Debug for TFloStream<S, R, O>
where
    S: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TemporalStream")
            .field("stream", &self.stream)
            .field("graph", &self.graph)
            .finish()
    }
}

impl<S, R, O> Stream for TFloStream<S, R, O>
where
    S: Stream<Item = R>,
    O: ExtractOutput,
{
    type Item = O;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        loop {
            match this.stream.as_mut().poll_next(cx) {
                Poll::Ready(Some(record)) => {
                    if let Some(item) = this.graph.step(&record) {
                        return Poll::Ready(Some(item.value));
                    }
                    // Continue polling if step returns None (warmup)
                }
                Poll::Ready(None) => return Poll::Ready(None),
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

/// Async stream that keeps original items with computed values.
#[pin_project::pin_project]
pub struct TFlowWithStream<S, R, O> {
    #[pin]
    stream: S,
    graph: CompiledGraph<R, O>,
    _marker: std::marker::PhantomData<R>,
}

impl<S, R, O> std::fmt::Debug for TFlowWithStream<S, R, O>
where
    S: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TemporalWithStream")
            .field("stream", &self.stream)
            .field("graph", &self.graph)
            .finish()
    }
}

impl<S, R, O> Stream for TFlowWithStream<S, R, O>
where
    S: Stream<Item = R>,
    R: Clone,
    O: ExtractOutput,
{
    type Item = (R, O);

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        loop {
            match this.stream.as_mut().poll_next(cx) {
                Poll::Ready(Some(record)) => {
                    if let Some(item) = this.graph.step(&record) {
                        return Poll::Ready(Some((record, item.value)));
                    }
                    // Continue polling if step returns None (warmup)
                }
                Poll::Ready(None) => return Poll::Ready(None),
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

#[allow(missing_debug_implementations)]
/// Async stream that applies keyed tflo computations.
#[pin_project::pin_project]
pub struct TFloKeyedStream<S, R, O, K, C>
where
    K: Clone + Send + Sync + Default + std::hash::Hash + Eq + 'static,
    O: ExtractOutput,
{
    #[pin]
    stream: S,
    graphs: std::collections::HashMap<K, crate::keyed::KeyedGraphState<R, O, K>>,
    timestamp_fn: Arc<dyn Fn(&R) -> i64 + Send + Sync>,
    key_fn: Arc<dyn Fn(&R) -> K + Send + Sync>,
    builder_fn: Box<dyn Fn(&mut TFlowBuilder<R>) -> C + Send + Sync>,
    policy: crate::keyed::OutOfOrderPolicy,
    /// Records released but not yet yielded — a `Buffer` step can release
    /// several at once.
    ready_queue: std::collections::VecDeque<
        crate::error::TFloResult<crate::pipeline::PipelineItem<KeyedTimestamped<K>, O>>,
    >,
    /// Set once the input stream is exhausted and every key has been flushed.
    flushed: bool,
    _marker: std::marker::PhantomData<(R, O)>,
}

impl<S, R, O, K, C> Stream for TFloKeyedStream<S, R, O, K, C>
where
    S: Stream<Item = R>,
    K: std::hash::Hash + Eq + Clone + Send + Sync + Default + 'static,
    O: ExtractOutput,
    C: Compile<R>,
    C::Output: ExtractOutput,
    R: 'static,
{
    type Item = crate::error::TFloResult<
        crate::pipeline::PipelineItem<crate::pipeline::KeyedTimestamped<K>, O>,
    >;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        loop {
            // Serve anything released by an earlier step or by flushing.
            if let Some(result) = this.ready_queue.pop_front() {
                return Poll::Ready(Some(result));
            }

            match this.stream.as_mut().poll_next(cx) {
                Poll::Ready(Some(record)) => {
                    let ts = (this.timestamp_fn)(&record);
                    let key = (this.key_fn)(&record);

                    // Get or create the graph for this key. `entry` does the
                    // get-or-insert in one lookup, so no fallible `get_mut`
                    // follows the insert.
                    let graph_state = this.graphs.entry(key.clone()).or_insert_with(|| {
                        let timestamp_fn_clone = this.timestamp_fn.clone();
                        let mut builder = TFlowBuilder::new();
                        builder.timestamp(move |r| timestamp_fn_clone(r));
                        let comps = (this.builder_fn)(&mut builder);
                        let output_ids = comps.output_ids();
                        let timestamp_fn = builder
                            .get_timestamp_fn()
                            .unwrap_or_else(|| this.timestamp_fn.clone());
                        let nodes = builder.into_nodes();
                        let graph: CompiledGraph<R, O, KeyedTimestamped<K>> =
                            CompiledGraph::compile(timestamp_fn, nodes, output_ids);
                        KeyedGraphState::new(graph, *this.policy)
                    });

                    match graph_state.step(record, ts, key) {
                        Ok(items) => {
                            this.ready_queue.extend(items.into_iter().map(Ok));
                        }
                        Err(e) => {
                            this.ready_queue
                                .push_back(Err(crate::error::TFloError::Compute(e)));
                        }
                    }
                }
                Poll::Ready(None) => {
                    // Input exhausted — flush every key's buffered records once
                    // so `Buffer`-policy records inside the lateness window are
                    // not silently lost.
                    if *this.flushed {
                        return Poll::Ready(None);
                    }
                    *this.flushed = true;
                    for (key, graph_state) in this.graphs.iter_mut() {
                        match graph_state.flush(key.clone()) {
                            Ok(items) => {
                                this.ready_queue.extend(items.into_iter().map(Ok));
                            }
                            Err(e) => {
                                this.ready_queue
                                    .push_back(Err(crate::error::TFloError::Compute(e)));
                            }
                        }
                    }
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

/// Create a stream from an iterator.
///
/// This is useful for testing or when you have data in memory.
pub fn from_iter<I, T>(iter: I) -> impl Stream<Item = T>
where
    I: IntoIterator<Item = T>,
{
    tokio_stream::iter(iter)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)] // SAFETY: test code, indexes into vecs of known size

    use super::*;
    use tokio_stream::StreamExt;

    #[derive(Clone, Debug)]
    struct TestRecord {
        ts: i64,
        value: f64,
    }

    #[tokio::test]
    async fn test_temporal_stream() {
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

        let stream = from_iter(records);

        // Use scan_f64 to compute a running mean (replaces the old sma() builder)
        let results: Vec<f64> = stream
            .tflo(|t| {
                let _ = t.timestamp(|x| x.ts);
                let value = t.prop(|x| x.value);
                value.scan_f64(
                    || (0.0_f64, 0_usize),
                    |s, x| {
                        s.0 += x;
                        s.1 += 1;
                        s.0 / s.1 as f64
                    },
                )
            })
            .collect()
            .await;

        assert_eq!(results.len(), 3);
        assert!((results[0] - 10.0).abs() < 0.001);
        assert!((results[1] - 15.0).abs() < 0.001);
        assert!((results[2] - 20.0).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_temporal_with_stream() {
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

        let stream = from_iter(records);

        // Use map_f64 identity to verify tflo_with streaming (replaces old sma() builder)
        let results: Vec<(TestRecord, f64)> = stream
            .tflo_with(|t| {
                let _ = t.timestamp(|x| x.ts);
                let value = t.prop(|x| x.value);
                value.map_f64(|x| x)
            })
            .collect()
            .await;

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0.value, 10.0);
        assert!((results[0].1 - 10.0).abs() < 0.001);
    }
}
