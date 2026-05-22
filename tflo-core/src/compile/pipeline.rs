use crate::compile::{CompiledGraph, ExtractOutput};
use crate::error::ComputeError;
use crate::pipeline::{PipelineContext, PipelineItem};

/// Compiled computation graph ready for execution.
///
/// This struct holds all the state needed to execute a computation graph
/// on streaming data. It's created by compiling a `TemporalBuilder`.
///
/// # Type Parameters
///
/// - `R`: The input record type
/// - `O`: The output type (must implement `ExtractOutput`)
/// - `C`: The pipeline context type (default: `Timestamped`)
///
/// # Pipeline Contexts
///
/// The context type determines what metadata flows through the pipeline:
///
/// - [`Timestamped`]: Time-based operations (default)
/// - [`Sequenced`](crate::pipeline::Sequenced): Count-based operations
/// - [`Hybrid`](crate::pipeline::Hybrid): Both timestamp and sequence
/// - Custom: Implement [`PipelineContext`] for domain-specific metadata
///
/// # Composition
///
/// Graphs can be composed using functional operators:
///
/// ```ignore
/// let combined = graph1
///     .zip(graph2)                    // Graph<R, (A, B), C>
///     .map(|(a, b)| a + b)           // Graph<R, f64, C>
///     .filter(|x| x > 0.0)           // Graph<R, Option<f64>, C>
///     .pipe(next_stage);             // Graph<R, B, C>
/// ```
///
/// Result of executing one step of a computation graph.
///
/// This enum provides explicit handling of warmup periods and errors,
/// replacing the previous `Option<T>` pattern where warmup was silently
/// filtered out.
///
/// # Examples
///
/// ```rust
/// use tflo_core::compile::{Absent, StepResult};
///
/// let result: StepResult<i64, f64> =
///     StepResult::WarmingUp { remaining: 1, reason: Absent::WarmingUp };
/// match result {
///     StepResult::Ready(_item) => {}
///     StepResult::WarmingUp { remaining, .. } => assert_eq!(remaining, 1),
///     StepResult::Error(_e) => {}
/// }
/// ```
#[derive(Debug, Clone, PartialEq)]
pub enum StepResult<C: PipelineContext, O> {
    /// Computation succeeded with a value.
    Ready(PipelineItem<C, O>),
    /// Computation produced no value this step.
    ///
    /// Despite the name, this covers every reason a value is absent. When the
    /// graph has genuinely not seen enough records, `remaining > 0`; once
    /// `remaining == 0` the graph is warmed up and the `reason` field carries
    /// the specific cause (a filtered value, a divide-by-zero, …).
    WarmingUp {
        /// Number of records still needed before the graph is warmed up.
        remaining: usize,
        /// Why the value is absent.
        reason: crate::compile::Absent,
    },
    /// Computation failed with an error.
    Error(ComputeError),
}

/// A graph created by piping one graph's output to another.
///
/// This struct holds two graphs where the first's output becomes
/// the second's input, with context flowing through.
pub struct PipelinedGraph<R, O1, O2, C: PipelineContext> {
    pub(crate) first: CompiledGraph<R, O1, C>,
    pub(crate) second: CompiledGraph<PipelineItem<C, O1>, O2, C>,
}

impl<R, O, C> CompiledGraph<R, O, C>
where
    O: ExtractOutput + Clone + Send + Sync + 'static,
    C: PipelineContext,
{
    /// Pipe the output of this graph to another graph.
    ///
    /// This enables multi-stage pipelines where the output of one graph
    /// becomes the input to another. The context flows through the pipeline,
    /// allowing the next stage to perform time-based or sequence-based
    /// operations.
    ///
    /// # Type Parameters
    ///
    /// The next graph takes `PipelineItem<C, O>` as its record type,
    /// giving it access to both the value and the context (timestamp, etc.).
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Stage 1: Compute some values
    /// let stage1: CompiledGraph<Tick, f64, Timestamped> = ...;
    ///
    /// // Stage 2: Process the values (has access to timestamp from stage 1)
    /// let stage2: CompiledGraph<PipelineItem<Timestamped, f64>, ThresholdCrossEventMode, Timestamped> = ...;
    ///
    /// // Combine into a single pipeline
    /// let pipeline = stage1.pipe(stage2);
    /// ```
    #[must_use]
    pub fn pipe<O2>(
        self,
        next: CompiledGraph<PipelineItem<C, O>, O2, C>,
    ) -> PipelinedGraph<R, O, O2, C>
    where
        O2: ExtractOutput,
    {
        PipelinedGraph {
            first: self,
            second: next,
        }
    }
}

impl<R, O1, O2, C> PipelinedGraph<R, O1, O2, C>
where
    O1: ExtractOutput + Clone + Send + Sync + 'static,
    O2: ExtractOutput,
    C: PipelineContext,
{
    /// Execute one step of the pipeline.
    ///
    /// Runs the first graph, then passes its output (with context) to
    /// the second graph.
    pub fn step(&mut self, record: &R) -> Option<PipelineItem<C, O2>> {
        let first_output = self.first.step(record)?;
        self.second.step(&first_output)
    }

    /// Execute one step and return just the value.
    pub fn step_value(&mut self, record: &R) -> Option<O2> {
        self.step(record).map(|item| item.value)
    }
}
