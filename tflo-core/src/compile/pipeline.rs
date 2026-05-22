use crate::compile::{CompiledGraph, ExtractOutput};
use crate::error::ComputeError;
use crate::event::ThresholdCrossEventMode;
use crate::pipeline::{PipelineContext, PipelineItem};
use crate::primitives::{GlitchResult, PulseWidthResult, RuntResult, WindowEvent};

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
/// use tflo_core::compile::StepResult;
///
/// let result: StepResult<i64, f64> = StepResult::WarmingUp { remaining: 1 };
/// match result {
///     StepResult::Ready(_item) => {}
///     StepResult::WarmingUp { remaining } => assert_eq!(remaining, 1),
///     StepResult::Error(_e) => {}
/// }
/// ```
#[derive(Debug, Clone, PartialEq)]
pub enum StepResult<C: PipelineContext, O> {
    /// Computation succeeded with a value.
    Ready(PipelineItem<C, O>),
    /// Computation is still warming up (insufficient data).
    WarmingUp {
        /// Number of records still needed before valid output.
        remaining: usize,
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

/// Helper methods for graphs producing `GlitchResult`.
impl<R, C> CompiledGraph<R, GlitchResult, C>
where
    C: PipelineContext,
{
    /// Convert to f64 (1.0 if valid pulse, 0.0 otherwise).
    #[must_use]
    pub fn to_f64_valid(self) -> CompiledGraph<R, f64, C> {
        self.map(|g| if g.is_valid_pulse() { 1.0 } else { 0.0 })
    }

    /// Convert to f64 (1.0 if rejected, 0.0 otherwise).
    #[must_use]
    pub fn to_f64_rejected(self) -> CompiledGraph<R, f64, C> {
        self.map(|g| if g.is_rejected() { 1.0 } else { 0.0 })
    }

    /// Convert to threshold crossing mode (Rising if valid, Falling if rejected, None otherwise).
    #[must_use]
    pub fn to_threshold_cross(self) -> CompiledGraph<R, ThresholdCrossEventMode, C> {
        self.map(|g| g.to_threshold_cross())
    }

    /// Convert to edge mode (deprecated, use to_threshold_cross instead).
    #[deprecated(since = "2.0.0", note = "Use to_threshold_cross() instead")]
    #[must_use]
    pub fn to_edge_mode(self) -> CompiledGraph<R, ThresholdCrossEventMode, C> {
        self.to_threshold_cross()
    }

    /// Convert to signal (deprecated, use to_threshold_cross instead).
    #[deprecated(since = "2.0.0", note = "Use to_threshold_cross() instead")]
    #[must_use]
    pub fn to_signal(self) -> CompiledGraph<R, ThresholdCrossEventMode, C> {
        self.to_threshold_cross()
    }
}

/// Helper methods for graphs producing `Option<RuntResult>`.
impl<R, C> CompiledGraph<R, Option<RuntResult>, C>
where
    C: PipelineContext,
{
    /// Convert to f64 (1.0 if valid pulse, 0.0 otherwise).
    #[must_use]
    pub fn to_f64_valid(self) -> CompiledGraph<R, f64, C> {
        self.map(|r| {
            r.map(|rr| if rr.is_valid() { 1.0 } else { 0.0 })
                .unwrap_or(0.0)
        })
    }

    /// Convert to f64 (1.0 if runt detected, 0.0 otherwise).
    #[must_use]
    pub fn to_f64_runt(self) -> CompiledGraph<R, f64, C> {
        self.map(|r| {
            r.map(|rr| if rr.is_runt() { 1.0 } else { 0.0 })
                .unwrap_or(0.0)
        })
    }

    /// Convert to threshold crossing mode.
    #[must_use]
    pub fn to_threshold_cross(self) -> CompiledGraph<R, ThresholdCrossEventMode, C> {
        self.map(|r| {
            r.map(|rr| rr.to_threshold_cross())
                .unwrap_or(ThresholdCrossEventMode::None)
        })
    }

    /// Convert to edge mode (deprecated, use to_threshold_cross instead).
    #[deprecated(since = "2.0.0", note = "Use to_threshold_cross() instead")]
    #[must_use]
    pub fn to_edge_mode(self) -> CompiledGraph<R, ThresholdCrossEventMode, C> {
        self.to_threshold_cross()
    }

    /// Convert to signal (deprecated, use to_threshold_cross instead).
    #[deprecated(since = "2.0.0", note = "Use to_threshold_cross() instead")]
    #[must_use]
    pub fn to_signal(self) -> CompiledGraph<R, ThresholdCrossEventMode, C> {
        self.to_threshold_cross()
    }

    /// Filter to only valid pulses.
    #[must_use]
    pub fn only_valid(self) -> CompiledGraph<R, Option<RuntResult>, C> {
        self.map(|r| r.filter(|rr| rr.is_valid()))
    }
}

/// Helper methods for graphs producing `Option<PulseWidthResult>`.
impl<R, C> CompiledGraph<R, Option<PulseWidthResult>, C>
where
    C: PipelineContext,
{
    /// Convert to f64 (1.0 if valid pulse, 0.0 otherwise).
    #[must_use]
    pub fn to_f64_valid(self) -> CompiledGraph<R, f64, C> {
        self.map(|p| {
            p.map(|pw| if pw.is_valid() { 1.0 } else { 0.0 })
                .unwrap_or(0.0)
        })
    }

    /// Convert to f64 (1.0 if too short, 0.0 otherwise).
    #[must_use]
    pub fn to_f64_too_short(self) -> CompiledGraph<R, f64, C> {
        self.map(|p| {
            p.map(|pw| if pw.is_too_short() { 1.0 } else { 0.0 })
                .unwrap_or(0.0)
        })
    }

    /// Convert to f64 (1.0 if too long, 0.0 otherwise).
    #[must_use]
    pub fn to_f64_too_long(self) -> CompiledGraph<R, f64, C> {
        self.map(|p| {
            p.map(|pw| if pw.is_too_long() { 1.0 } else { 0.0 })
                .unwrap_or(0.0)
        })
    }

    /// Get pulse duration in milliseconds (if valid).
    #[must_use]
    pub fn duration_ms(self) -> CompiledGraph<R, Option<i64>, C> {
        self.map(|p| p.and_then(|pw| pw.duration_ms()))
    }

    /// Convert to threshold crossing mode.
    #[must_use]
    pub fn to_threshold_cross(self) -> CompiledGraph<R, ThresholdCrossEventMode, C> {
        self.map(|p| {
            p.map(|pw| pw.to_threshold_cross())
                .unwrap_or(ThresholdCrossEventMode::None)
        })
    }

    /// Convert to edge mode (deprecated, use to_threshold_cross instead).
    #[deprecated(since = "2.0.0", note = "Use to_threshold_cross() instead")]
    #[must_use]
    pub fn to_edge_mode(self) -> CompiledGraph<R, ThresholdCrossEventMode, C> {
        self.to_threshold_cross()
    }

    /// Convert to signal (deprecated, use to_threshold_cross instead).
    #[deprecated(since = "2.0.0", note = "Use to_threshold_cross() instead")]
    #[must_use]
    pub fn to_signal(self) -> CompiledGraph<R, ThresholdCrossEventMode, C> {
        self.to_threshold_cross()
    }

    /// Filter to only valid pulses.
    #[must_use]
    pub fn only_valid(self) -> CompiledGraph<R, Option<PulseWidthResult>, C> {
        self.map(|p| p.filter(|pw| pw.is_valid()))
    }
}

/// Helper methods for graphs producing `Option<WindowEvent>`.
impl<R, C> CompiledGraph<R, Option<WindowEvent>, C>
where
    C: PipelineContext,
{
    /// Convert to f64 (1.0 if entered window, 0.0 otherwise).
    #[must_use]
    pub fn to_f64_entered(self) -> CompiledGraph<R, f64, C> {
        self.map(|w| {
            w.map(|we| if we.is_entered() { 1.0 } else { 0.0 })
                .unwrap_or(0.0)
        })
    }

    /// Convert to f64 (1.0 if exited low, 0.0 otherwise).
    #[must_use]
    pub fn to_f64_exited_low(self) -> CompiledGraph<R, f64, C> {
        self.map(|w| {
            w.map(|we| if we.is_exited_low() { 1.0 } else { 0.0 })
                .unwrap_or(0.0)
        })
    }

    /// Convert to f64 (1.0 if exited high, 0.0 otherwise).
    #[must_use]
    pub fn to_f64_exited_high(self) -> CompiledGraph<R, f64, C> {
        self.map(|w| {
            w.map(|we| if we.is_exited_high() { 1.0 } else { 0.0 })
                .unwrap_or(0.0)
        })
    }

    /// Convert to threshold crossing mode (Rising if entered, Falling if exited, None otherwise).
    #[must_use]
    pub fn to_threshold_cross(self) -> CompiledGraph<R, ThresholdCrossEventMode, C> {
        self.map(|w| {
            w.map(|we| we.to_threshold_cross())
                .unwrap_or(ThresholdCrossEventMode::None)
        })
    }

    #[deprecated(since = "2.0.0", note = "Use to_threshold_cross() instead")]
    /// Convert to edge mode (deprecated, use to_threshold_cross instead).
    #[must_use]
    pub fn to_edge_mode(self) -> CompiledGraph<R, ThresholdCrossEventMode, C> {
        self.to_threshold_cross()
    }

    /// Convert to signal (deprecated, use to_threshold_cross instead).
    #[deprecated(since = "2.0.0", note = "Use to_threshold_cross() instead")]
    #[must_use]
    pub fn to_signal(self) -> CompiledGraph<R, ThresholdCrossEventMode, C> {
        self.to_threshold_cross()
    }

    /// Filter to only entry events.
    #[must_use]
    pub fn only_entries(self) -> CompiledGraph<R, Option<WindowEvent>, C> {
        self.map(|w| w.filter(|we| we.is_entered()))
    }

    /// Filter to only exit events.
    #[must_use]
    pub fn only_exits(self) -> CompiledGraph<R, Option<WindowEvent>, C> {
        self.map(|w| w.filter(|we| we.is_exited_low() || we.is_exited_high()))
    }
}
