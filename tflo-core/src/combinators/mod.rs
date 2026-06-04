//! Stream combinators for merging, joining, and transforming streams.
//!
//! This module provides utilities for combining and transforming multiple
//! streams of data:
//!
//! - [`merge_by_timestamp`]: Merge multiple sorted streams by timestamp
//! - [`GroupByKey`]: Group records by a key for per-key processing
//! - [`window_join`]: Join two streams within a time window
//! - [`batch_by_time`]: Batch records by time intervals
//! - [`dedupe_by_key`]: Remove duplicate records within a time window
//! - [`rate_limit`]: Throttle output rate
//! - [`partition`]: Split a stream based on a predicate

mod batch;
mod dedupe;
mod group;
mod join;
mod merge;
mod partition;
mod rate_limit;

pub use batch::batch_by_time;
pub use dedupe::dedupe_by_key;
pub use group::{GroupByExt, GroupByKey};
pub use join::{keyed_window_join, window_join};
pub use merge::merge_by_timestamp;
pub use partition::{PartitionExt, PartitionLazy, fork, partition, partition_lazy};
pub use rate_limit::{RateLimitKeepLast, rate_limit, rate_limit_keep_last};
