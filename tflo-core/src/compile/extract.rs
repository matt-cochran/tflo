// SAFETY (file-level rationale for `#[allow(clippy::arithmetic_side_effects)]`
// on the tuple-arity sums below): every `+` here adds at most 6
// `output_id_count()` values (one per tuple slot) plus the corresponding
// `split_at` checks. Each `output_id_count()` returns the number of
// `NodeId`s a graph output occupies — a per-type compile-time constant
// that is always ≥ 1 and in practice 1–2. Summing six of those cannot
// overflow `usize` on any realizable target.
use crate::comp::NodeId;
use crate::compile::ValueStore;

/// Trait for extracting typed outputs from the value store.
pub trait ExtractOutput: Sized + Send + Sync + 'static {
    /// Extract the output value from the store using the given node IDs.
    fn extract(store: &ValueStore, ids: &[NodeId]) -> Option<Self>;

    /// The number of node IDs required to extract this type.
    fn output_id_count() -> usize {
        1
    }

    /// Best-effort view of this output value as an `f64`.
    ///
    /// Returns `Some` only for the `f64` output type; the default is `None`.
    /// [`validated()`](crate::iter_ext::TFlowIteratorExt::validated) uses this
    /// to apply the NaN / infinity / negative value checks, which are only
    /// meaningful for a scalar `f64` output.
    fn as_f64(&self) -> Option<f64> {
        None
    }
}

impl<A, B> ExtractOutput for (A, B)
where
    A: ExtractOutput,
    B: ExtractOutput,
{
    fn extract(store: &ValueStore, ids: &[NodeId]) -> Option<Self> {
        let a_count = A::output_id_count();
        if ids.len() < a_count {
            return None;
        }
        let (a_ids, b_ids) = ids.split_at(a_count);
        Some((A::extract(store, a_ids)?, B::extract(store, b_ids)?))
    }

    #[allow(clippy::arithmetic_side_effects)] // see file-level SAFETY
    fn output_id_count() -> usize {
        A::output_id_count() + B::output_id_count()
    }
}

impl<A, B, C> ExtractOutput for (A, B, C)
where
    A: ExtractOutput,
    B: ExtractOutput,
    C: ExtractOutput,
{
    fn extract(store: &ValueStore, ids: &[NodeId]) -> Option<Self> {
        let a_count = A::output_id_count();
        let b_count = B::output_id_count();
        #[allow(clippy::arithmetic_side_effects)] // see file-level SAFETY
        let needed = a_count + b_count;
        if ids.len() < needed {
            return None;
        }
        let (a_ids, rest) = ids.split_at(a_count);
        let (b_ids, c_ids) = rest.split_at(b_count);
        Some((
            A::extract(store, a_ids)?,
            B::extract(store, b_ids)?,
            C::extract(store, c_ids)?,
        ))
    }

    #[allow(clippy::arithmetic_side_effects)] // see file-level SAFETY
    fn output_id_count() -> usize {
        A::output_id_count() + B::output_id_count() + C::output_id_count()
    }
}

impl<A, B, C, D> ExtractOutput for (A, B, C, D)
where
    A: ExtractOutput,
    B: ExtractOutput,
    C: ExtractOutput,
    D: ExtractOutput,
{
    fn extract(store: &ValueStore, ids: &[NodeId]) -> Option<Self> {
        let a_count = A::output_id_count();
        let b_count = B::output_id_count();
        let c_count = C::output_id_count();
        #[allow(clippy::arithmetic_side_effects)] // see file-level SAFETY
        let needed = a_count + b_count + c_count;
        if ids.len() < needed {
            return None;
        }
        let (a_ids, rest) = ids.split_at(a_count);
        let (b_ids, rest) = rest.split_at(b_count);
        let (c_ids, d_ids) = rest.split_at(c_count);
        Some((
            A::extract(store, a_ids)?,
            B::extract(store, b_ids)?,
            C::extract(store, c_ids)?,
            D::extract(store, d_ids)?,
        ))
    }

    #[allow(clippy::arithmetic_side_effects)] // see file-level SAFETY
    fn output_id_count() -> usize {
        A::output_id_count() + B::output_id_count() + C::output_id_count() + D::output_id_count()
    }
}

impl<A, B, C, D, E> ExtractOutput for (A, B, C, D, E)
where
    A: ExtractOutput,
    B: ExtractOutput,
    C: ExtractOutput,
    D: ExtractOutput,
    E: ExtractOutput,
{
    fn extract(store: &ValueStore, ids: &[NodeId]) -> Option<Self> {
        let a_count = A::output_id_count();
        let b_count = B::output_id_count();
        let c_count = C::output_id_count();
        let d_count = D::output_id_count();
        #[allow(clippy::arithmetic_side_effects)] // see file-level SAFETY
        let needed = a_count + b_count + c_count + d_count;
        if ids.len() < needed {
            return None;
        }
        let (a_ids, rest) = ids.split_at(a_count);
        let (b_ids, rest) = rest.split_at(b_count);
        let (c_ids, rest) = rest.split_at(c_count);
        let (d_ids, e_ids) = rest.split_at(d_count);
        Some((
            A::extract(store, a_ids)?,
            B::extract(store, b_ids)?,
            C::extract(store, c_ids)?,
            D::extract(store, d_ids)?,
            E::extract(store, e_ids)?,
        ))
    }

    #[allow(clippy::arithmetic_side_effects)] // see file-level SAFETY
    fn output_id_count() -> usize {
        A::output_id_count()
            + B::output_id_count()
            + C::output_id_count()
            + D::output_id_count()
            + E::output_id_count()
    }
}

impl<A, B, C, D, E, F> ExtractOutput for (A, B, C, D, E, F)
where
    A: ExtractOutput,
    B: ExtractOutput,
    C: ExtractOutput,
    D: ExtractOutput,
    E: ExtractOutput,
    F: ExtractOutput,
{
    fn extract(store: &ValueStore, ids: &[NodeId]) -> Option<Self> {
        let a_count = A::output_id_count();
        let b_count = B::output_id_count();
        let c_count = C::output_id_count();
        let d_count = D::output_id_count();
        let e_count = E::output_id_count();
        #[allow(clippy::arithmetic_side_effects)] // see file-level SAFETY
        let needed = a_count + b_count + c_count + d_count + e_count;
        if ids.len() < needed {
            return None;
        }
        let (a_ids, rest) = ids.split_at(a_count);
        let (b_ids, rest) = rest.split_at(b_count);
        let (c_ids, rest) = rest.split_at(c_count);
        let (d_ids, rest) = rest.split_at(d_count);
        let (e_ids, f_ids) = rest.split_at(e_count);
        Some((
            A::extract(store, a_ids)?,
            B::extract(store, b_ids)?,
            C::extract(store, c_ids)?,
            D::extract(store, d_ids)?,
            E::extract(store, e_ids)?,
            F::extract(store, f_ids)?,
        ))
    }

    #[allow(clippy::arithmetic_side_effects)] // see file-level SAFETY
    fn output_id_count() -> usize {
        A::output_id_count()
            + B::output_id_count()
            + C::output_id_count()
            + D::output_id_count()
            + E::output_id_count()
            + F::output_id_count()
    }
}
