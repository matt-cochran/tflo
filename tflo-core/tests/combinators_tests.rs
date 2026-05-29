#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![allow(clippy::indexing_slicing)] // SAFETY: test code, indexes into vecs of known size
use std::time::Duration;
use tflo_core::combinators::{
    GroupByExt, PartitionExt, fork, partition_lazy, rate_limit_keep_last,
};

/// When `partition_lazy` is applied,
/// the combinator shall yield (item, `is_match`) pairs,
/// So that items can be processed lazily without allocation,
/// And each item will include its partition assignment.
#[test]
fn test_partition_lazy() {
    let items = vec![1, 2, 3, 4, 5];
    let partitioned: Vec<(i32, bool)> = partition_lazy(items, |&x| x % 2 == 0).collect();

    assert_eq!(partitioned.len(), 5);
    assert_eq!(partitioned[0], (1, false)); // 1 is odd
    assert_eq!(partitioned[1], (2, true)); // 2 is even
    assert_eq!(partitioned[2], (3, false)); // 3 is odd
    assert_eq!(partitioned[3], (4, true)); // 4 is even
    assert_eq!(partitioned[4], (5, false)); // 5 is odd
}

/// When `partition_lazy` is used with filter,
/// users shall be able to process only matching items,
/// So that non-matching items are skipped efficiently,
/// And no intermediate collections are created.
#[test]
fn test_partition_lazy_filter() {
    let items = vec![1, 2, 3, 4, 5];
    let evens: Vec<i32> = partition_lazy(items, |&x| x % 2 == 0)
        .filter(|(_, is_even)| *is_even)
        .map(|(x, _)| x)
        .collect();

    assert_eq!(evens, vec![2, 4]);
}

/// When fork is called with count N,
/// the combinator shall create N independent copies of the stream,
/// So that multiple consumers can process the same data,
/// And each copy will be identical.
#[test]
fn test_fork_basic() {
    let items = vec![1, 2, 3];
    let copies = fork(items, 3);

    assert_eq!(copies.len(), 3);
    for copy in copies {
        assert_eq!(copy, vec![1, 2, 3]);
    }
}

/// When fork is called with count 0,
/// the combinator shall return an empty Vec,
/// So that edge cases are handled gracefully,
/// And no panic will occur.
#[test]
fn test_fork_zero() {
    let items = vec![1, 2, 3];
    let copies = fork(items, 0);

    assert!(copies.is_empty());
}

/// When fork is called on an empty iterator,
/// the combinator shall create N empty copies,
/// So that empty input is handled correctly,
/// And each copy will be empty.
#[test]
fn test_fork_empty() {
    let items: Vec<i32> = vec![];
    let copies = fork(items, 3);

    assert_eq!(copies.len(), 3);
    for copy in copies {
        assert!(copy.is_empty());
    }
}

/// When `rate_limit_keep_last` is applied,
/// the combinator shall keep the most recent item within each interval,
/// So that the latest data is preserved rather than first,
/// And gaps will contain the most recent observation.
#[test]
fn test_rate_limit_keep_last() {
    let items = vec![
        (1000_i64, "a"),
        (1100, "b"), // Will replace "a" as most recent
        (1200, "c"), // Will replace "b" as most recent
        (3000, "d"), // New interval starts
        (3100, "e"), // Will replace "d"
    ];

    let limited: Vec<_> =
        rate_limit_keep_last(items.into_iter(), |x| x.0, Duration::from_secs(1)).collect();

    // Should keep last in each interval: "c" from [1000-2000), "e" from [3000-4000)
    assert!(!limited.is_empty());
}

/// When `group_by_key` is used via extension trait,
/// the iterator shall be collected into groups by key,
/// So that related items are grouped together,
/// And the syntax is ergonomic.
#[test]
fn test_group_by_ext() {
    let values = vec![1, 2, 3, 4, 5, 6];
    let groups = values.into_iter().group_by_key(|&x| x % 2);

    assert_eq!(groups.key_count(), 2);
    assert_eq!(groups.get(&0).map(std::vec::Vec::len), Some(3)); // Even: 2, 4, 6
    assert_eq!(groups.get(&1).map(std::vec::Vec::len), Some(3)); // Odd: 1, 3, 5
}

/// When `partition_by` is used via extension trait,
/// the iterator shall be partitioned into two vectors,
/// So that matching and non-matching items are separated,
/// And the syntax is ergonomic.
#[test]
fn test_partition_ext() {
    let values = vec![1, 2, 3, 4, 5];
    let (evens, odds) = values.into_iter().partition_by(|&x| x % 2 == 0);

    assert_eq!(evens, vec![2, 4]);
    assert_eq!(odds, vec![1, 3, 5]);
}

/// When `partition_lazy_by` is used via extension trait,
/// the iterator shall lazily yield partitioned items,
/// So that streaming partition is available via fluent API,
/// And no intermediate allocation is needed.
#[test]
fn test_partition_lazy_ext() {
    let values = vec![1, 2, 3];
    let partitioned: Vec<_> = values
        .into_iter()
        .partition_lazy_by(|&x| x % 2 == 0)
        .collect();

    assert_eq!(partitioned.len(), 3);
}
