//! CEL conversion traits (no internal `crate::` deps — keeps the module a leaf).

use cel_interpreter::Context;

/// Trait for types that can be converted to a CEL evaluation context.
///
/// Implement this trait for your domain types to enable CEL filtering
/// and rule evaluation.
///
/// # Examples
///
/// ```rust
/// use tflo_cel::traits::IntoCelContext;
/// use cel_interpreter::Context;
///
/// struct Detection {
///     ts: i64,
///     freq_hz: u64,
///     power_dbm: f64,
///     snr_db: f64,
///     is_signal: bool,
/// }
///
/// impl IntoCelContext for Detection {
///     fn into_cel_context(&self) -> Context<'static> {
///         let mut ctx = Context::default();
///         ctx.add_variable("ts", self.ts).unwrap();
///         ctx.add_variable("freq_hz", self.freq_hz as i64).unwrap();
///         ctx.add_variable("freq_mhz", self.freq_hz as f64 / 1e6).unwrap();
///         ctx.add_variable("power", self.power_dbm).unwrap();
///         ctx.add_variable("snr", self.snr_db).unwrap();
///         ctx.add_variable("is_signal", self.is_signal).unwrap();
///         ctx
///     }
/// }
/// ```
pub trait IntoCelContext {
    /// Convert this value into a CEL context for evaluation.
    fn into_cel_context(&self) -> Context<'static>;
}
