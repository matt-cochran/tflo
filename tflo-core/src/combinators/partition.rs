//! Partition a stream based on a predicate.

/// Partition an iterator into two vectors based on a predicate.
///
/// Returns a tuple where the first vector contains items for which
/// the predicate returns true, and the second contains items for which
/// it returns false.
///
/// # Examples
///
/// ```rust
/// use tflo_core::combinators::partition;
///
/// let numbers = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
///
/// let (evens, odds) = partition(numbers, |x| x % 2 == 0);
///
/// assert_eq!(evens, vec![2, 4, 6, 8, 10]);
/// assert_eq!(odds, vec![1, 3, 5, 7, 9]);
/// ```
pub fn partition<I, T, F>(iter: I, predicate: F) -> (Vec<T>, Vec<T>)
where
    I: IntoIterator<Item = T>,
    F: Fn(&T) -> bool,
{
    let mut left = Vec::new();
    let mut right = Vec::new();

    for item in iter {
        if predicate(&item) {
            left.push(item);
        } else {
            right.push(item);
        }
    }

    (left, right)
}

/// Partition iterator that lazily yields items with their partition assignment.
///
/// Unlike [`partition`], this doesn't collect into vectors but yields
/// `(item, is_left)` tuples.
#[allow(dead_code)]
pub fn partition_lazy<I, T, F>(iter: I, predicate: F) -> PartitionLazy<I::IntoIter, T, F>
where
    I: IntoIterator<Item = T>,
    F: Fn(&T) -> bool,
{
    PartitionLazy {
        iter: iter.into_iter(),
        predicate,
    }
}

/// Iterator that yields items with their partition assignment.
#[allow(dead_code)]
pub struct PartitionLazy<I, T, F>
where
    I: Iterator<Item = T>,
{
    iter: I,
    predicate: F,
}

impl<I, T, F> std::fmt::Debug for PartitionLazy<I, T, F>
where
    I: Iterator<Item = T> + std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PartitionLazy")
            .field("iter", &self.iter)
            .finish()
    }
}

impl<I, T, F> Iterator for PartitionLazy<I, T, F>
where
    I: Iterator<Item = T>,
    F: Fn(&T) -> bool,
{
    type Item = (T, bool);

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|item| {
            let is_left = (self.predicate)(&item);
            (item, is_left)
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

/// Fork an iterator into multiple consumers.
///
/// This is useful when you need to process the same stream in multiple ways.
/// Note that this collects the iterator into a vector first.
#[allow(dead_code)]
pub fn fork<I, T>(iter: I, count: usize) -> Vec<Vec<T>>
where
    I: IntoIterator<Item = T>,
    T: Clone,
{
    let collected: Vec<T> = iter.into_iter().collect();
    (0..count).map(|_| collected.clone()).collect()
}

/// Extension trait for partition operations.
#[allow(dead_code)]
pub trait PartitionExt<T>: Iterator<Item = T> + Sized {
    /// Partition into two vectors based on a predicate.
    fn partition_by<F>(self, predicate: F) -> (Vec<T>, Vec<T>)
    where
        F: Fn(&T) -> bool,
    {
        partition(self, predicate)
    }

    /// Lazily partition, yielding `(item, is_match)` pairs.
    fn partition_lazy_by<F>(self, predicate: F) -> PartitionLazy<Self, T, F>
    where
        F: Fn(&T) -> bool,
    {
        partition_lazy(self, predicate)
    }
}

impl<I, T> PartitionExt<T> for I where I: Iterator<Item = T> {}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_partition() {
        let numbers = vec![1, 2, 3, 4, 5];
        let (evens, odds) = partition(numbers, |x| x % 2 == 0);

        assert_eq!(evens, vec![2, 4]);
        assert_eq!(odds, vec![1, 3, 5]);
    }

    #[test]
    fn test_partition_empty() {
        let numbers: Vec<i32> = vec![];
        let (evens, odds) = partition(numbers, |x| x % 2 == 0);

        assert!(evens.is_empty());
        assert!(odds.is_empty());
    }

    #[test]
    fn test_partition_all_left() {
        let numbers = vec![2, 4, 6, 8];
        let (evens, odds) = partition(numbers, |x| x % 2 == 0);

        assert_eq!(evens, vec![2, 4, 6, 8]);
        assert!(odds.is_empty());
    }

    #[test]
    fn test_partition_lazy() {
        let numbers = vec![1, 2, 3, 4, 5];
        let partitioned: Vec<_> = partition_lazy(numbers, |x| x % 2 == 0).collect();

        assert_eq!(partitioned.len(), 5);
        assert_eq!(partitioned[0], (1, false));
        assert_eq!(partitioned[1], (2, true));
    }

    #[test]
    fn test_fork() {
        let numbers = vec![1, 2, 3];
        let forked = fork(numbers, 3);

        assert_eq!(forked.len(), 3);
        assert!(forked.iter().all(|v| v == &vec![1, 2, 3]));
    }

    #[test]
    fn test_partition_ext() {
        let numbers = vec![1, 2, 3, 4, 5];
        let (evens, odds) = numbers.into_iter().partition_by(|x| x % 2 == 0);

        assert_eq!(evens, vec![2, 4]);
        assert_eq!(odds, vec![1, 3, 5]);
    }
}
