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
        if ids.len() < a_count + b_count {
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
        if ids.len() < a_count + b_count + c_count {
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
        if ids.len() < a_count + b_count + c_count + d_count {
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
        if ids.len() < a_count + b_count + c_count + d_count + e_count {
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

    fn output_id_count() -> usize {
        A::output_id_count()
            + B::output_id_count()
            + C::output_id_count()
            + D::output_id_count()
            + E::output_id_count()
            + F::output_id_count()
    }
}
