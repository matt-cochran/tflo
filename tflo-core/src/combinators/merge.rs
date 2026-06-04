//! Merge multiple sorted streams by timestamp.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

/// Merge multiple sorted iterators by a key function.
///
/// All input iterators must be sorted by the key in ascending order.
/// The output iterator yields items from all inputs in sorted order.
///
/// # Examples
///
/// ```rust
/// use tflo_core::combinators::merge_by_timestamp;
///
/// #[derive(Debug, Clone, PartialEq)]
/// struct Event {
///     ts: i64,
///     source: &'static str,
/// }
///
/// let stream1 = vec![
///     Event { ts: 1, source: "A" },
///     Event { ts: 3, source: "A" },
/// ];
///
/// let stream2 = vec![
///     Event { ts: 2, source: "B" },
///     Event { ts: 4, source: "B" },
/// ];
///
/// let merged: Vec<Event> = merge_by_timestamp(
///     vec![stream1.into_iter(), stream2.into_iter()],
///     |e| e.ts,
/// ).collect();
///
/// assert_eq!(merged[0].ts, 1);
/// assert_eq!(merged[1].ts, 2);
/// assert_eq!(merged[2].ts, 3);
/// assert_eq!(merged[3].ts, 4);
/// ```
pub fn merge_by_timestamp<I, T, F, K>(iters: Vec<I>, key_fn: F) -> MergeByKey<I, T, F, K>
where
    I: Iterator<Item = T>,
    F: Fn(&T) -> K,
    K: Ord,
{
    MergeByKey::new(iters, key_fn)
}

/// Iterator that merges multiple sorted streams.
pub struct MergeByKey<I, T, F, K>
where
    I: Iterator<Item = T>,
    F: Fn(&T) -> K,
    K: Ord,
{
    heap: BinaryHeap<HeapEntry<T, K>>,
    iters: Vec<Option<I>>,
    key_fn: F,
}

impl<I, T, F, K> std::fmt::Debug for MergeByKey<I, T, F, K>
where
    I: Iterator<Item = T>,
    F: Fn(&T) -> K,
    K: Ord,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MergeByKey")
            .field("heap_size", &self.heap.len())
            .field("iter_count", &self.iters.len())
            .finish()
    }
}

struct HeapEntry<T, K> {
    key: K,
    value: T,
    source_idx: usize,
}

impl<T, K: Ord> PartialEq for HeapEntry<T, K> {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

impl<T, K: Ord> Eq for HeapEntry<T, K> {}

impl<T, K: Ord> PartialOrd for HeapEntry<T, K> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<T, K: Ord> Ord for HeapEntry<T, K> {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse order for min-heap behavior
        other.key.cmp(&self.key)
    }
}

impl<I, T, F, K> MergeByKey<I, T, F, K>
where
    I: Iterator<Item = T>,
    F: Fn(&T) -> K,
    K: Ord,
{
    fn new(iters: Vec<I>, key_fn: F) -> Self {
        let mut heap = BinaryHeap::new();
        let mut wrapped_iters: Vec<Option<I>> = iters.into_iter().map(Some).collect();

        // Initialize heap with first element from each iterator
        for (idx, iter_opt) in wrapped_iters.iter_mut().enumerate() {
            if let Some(iter) = iter_opt {
                if let Some(value) = iter.next() {
                    let key = key_fn(&value);
                    heap.push(HeapEntry {
                        key,
                        value,
                        source_idx: idx,
                    });
                }
            }
        }

        Self {
            heap,
            iters: wrapped_iters,
            key_fn,
        }
    }
}

impl<I, T, F, K> Iterator for MergeByKey<I, T, F, K>
where
    I: Iterator<Item = T>,
    F: Fn(&T) -> K,
    K: Ord,
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        let entry = self.heap.pop()?;

        // Refill from the same source
        if let Some(Some(iter)) = self.iters.get_mut(entry.source_idx) {
            if let Some(value) = iter.next() {
                let key = (self.key_fn)(&value);
                self.heap.push(HeapEntry {
                    key,
                    value,
                    source_idx: entry.source_idx,
                });
            }
        }

        Some(entry.value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_two_streams() {
        let s1 = vec![1, 3, 5];
        let s2 = vec![2, 4, 6];

        let merged: Vec<i32> =
            merge_by_timestamp(vec![s1.into_iter(), s2.into_iter()], |&x| x).collect();

        assert_eq!(merged, vec![1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn test_merge_empty_stream() {
        let s1: Vec<i32> = vec![1, 2, 3];
        let s2: Vec<i32> = vec![];

        let merged: Vec<i32> =
            merge_by_timestamp(vec![s1.into_iter(), s2.into_iter()], |&x| x).collect();

        assert_eq!(merged, vec![1, 2, 3]);
    }

    #[test]
    fn test_merge_three_streams() {
        let s1 = vec![1, 4, 7];
        let s2 = vec![2, 5, 8];
        let s3 = vec![3, 6, 9];

        let merged: Vec<i32> =
            merge_by_timestamp(vec![s1.into_iter(), s2.into_iter(), s3.into_iter()], |&x| x)
                .collect();

        assert_eq!(merged, vec![1, 2, 3, 4, 5, 6, 7, 8, 9]);
    }

    #[test]
    fn test_merge_with_duplicates() {
        let s1 = vec![1, 2, 3];
        let s2 = vec![2, 3, 4];

        let merged: Vec<i32> =
            merge_by_timestamp(vec![s1.into_iter(), s2.into_iter()], |&x| x).collect();

        assert_eq!(merged, vec![1, 2, 2, 3, 3, 4]);
    }
}
