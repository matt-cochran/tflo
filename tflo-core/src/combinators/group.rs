//! Group records by key for per-key processing.

use std::collections::HashMap;
use std::hash::Hash;

/// Accumulates records by key for per-key processing.
///
/// This is useful when you need to process records for each unique key
/// separately, such as computing per-symbol statistics.
///
/// # Examples
///
/// ```rust
/// use tflo_core::combinators::GroupByKey;
///
/// #[derive(Clone)]
/// struct Tick {
///     symbol: String,
///     price: f64,
/// }
///
/// let ticks = vec![
///     Tick { symbol: "AAPL".into(), price: 150.0 },
///     Tick { symbol: "GOOG".into(), price: 2800.0 },
///     Tick { symbol: "AAPL".into(), price: 151.0 },
/// ];
///
/// let mut groups = GroupByKey::new(|t: &Tick| t.symbol.clone());
///
/// for tick in ticks {
///     groups.push(tick);
/// }
///
/// assert_eq!(groups.key_count(), 2);
/// assert_eq!(groups.get(&"AAPL".to_string()).map(|v| v.len()), Some(2));
/// ```
#[derive(Debug)]
pub struct GroupByKey<K, V, F>
where
    K: Hash + Eq,
    F: Fn(&V) -> K,
{
    key_fn: F,
    groups: HashMap<K, Vec<V>>,
}

impl<K, V, F> GroupByKey<K, V, F>
where
    K: Hash + Eq,
    F: Fn(&V) -> K,
{
    /// Create a new group-by accumulator with the given key function.
    pub fn new(key_fn: F) -> Self {
        Self {
            key_fn,
            groups: HashMap::new(),
        }
    }

    /// Add a value to its group.
    pub fn push(&mut self, value: V) {
        let key = (self.key_fn)(&value);
        self.groups.entry(key).or_default().push(value);
    }

    /// Get the values for a specific key.
    #[must_use]
    pub fn get(&self, key: &K) -> Option<&Vec<V>> {
        self.groups.get(key)
    }

    /// Get a mutable reference to the values for a specific key.
    pub fn get_mut(&mut self, key: &K) -> Option<&mut Vec<V>> {
        self.groups.get_mut(key)
    }

    /// Get the number of unique keys.
    #[must_use]
    pub fn key_count(&self) -> usize {
        self.groups.len()
    }

    /// Check if there are any groups.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    /// Get an iterator over all groups.
    pub fn groups(&self) -> impl Iterator<Item = (&K, &Vec<V>)> {
        self.groups.iter()
    }

    /// Get a mutable iterator over all groups.
    pub fn groups_mut(&mut self) -> impl Iterator<Item = (&K, &mut Vec<V>)> {
        self.groups.iter_mut()
    }

    /// Consume the accumulator and return the groups.
    #[must_use]
    pub fn into_groups(self) -> HashMap<K, Vec<V>> {
        self.groups
    }

    /// Get all keys.
    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.groups.keys()
    }

    /// Clear all groups.
    pub fn clear(&mut self) {
        self.groups.clear();
    }

    /// Remove a group by key.
    pub fn remove(&mut self, key: &K) -> Option<Vec<V>> {
        self.groups.remove(key)
    }
}

impl<K, V, F> Extend<V> for GroupByKey<K, V, F>
where
    K: Hash + Eq,
    F: Fn(&V) -> K,
{
    fn extend<I: IntoIterator<Item = V>>(&mut self, iter: I) {
        for value in iter {
            self.push(value);
        }
    }
}

/// Iterator extension for grouping.
#[allow(dead_code)]
pub trait GroupByExt<V>: Iterator<Item = V> + Sized {
    /// Collect into groups by key.
    fn group_by_key<K, F>(self, key_fn: F) -> GroupByKey<K, V, F>
    where
        K: Hash + Eq,
        F: Fn(&V) -> K,
    {
        let mut groups = GroupByKey::new(key_fn);
        for value in self {
            groups.push(value);
        }
        groups
    }
}

impl<I, V> GroupByExt<V> for I where I: Iterator<Item = V> {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_group_by_key() {
        let values = vec![1, 2, 3, 4, 5, 6];
        let mut groups = GroupByKey::new(|&x: &i32| x % 2);

        for v in values {
            groups.push(v);
        }

        assert_eq!(groups.key_count(), 2);
        assert_eq!(groups.get(&0), Some(&vec![2, 4, 6]));
        assert_eq!(groups.get(&1), Some(&vec![1, 3, 5]));
    }

    #[test]
    fn test_group_by_ext() {
        let values = vec!["apple", "banana", "apricot", "blueberry"];
        let groups = values.into_iter().group_by_key(|s| s.chars().next());

        assert_eq!(groups.key_count(), 2);
        assert_eq!(groups.get(&Some('a')).map(|v| v.len()), Some(2));
        assert_eq!(groups.get(&Some('b')).map(|v| v.len()), Some(2));
    }

    #[test]
    fn test_into_groups() {
        let values = vec![1, 2, 3, 4];
        let mut groups = GroupByKey::new(|&x: &i32| x % 2);
        groups.extend(values);

        let map = groups.into_groups();
        assert!(map.contains_key(&0));
        assert!(map.contains_key(&1));
    }
}
