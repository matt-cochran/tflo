//! S3-based checkpoint / restore — real API usage with an in-process mock client.
//!
//! `S3StateStore<C: S3Client>` is generic over any S3-compatible client.
//! In production you plug in `aws-sdk-s3`, MinIO, etc.  Here we use a
//! simple in-memory mock so the example compiles and runs without AWS
//! credentials or a live bucket.
//!
//! Scenario: a fraud-detection service consumes a stream of payment
//! transactions and periodically checkpoints its detector state to S3 so
//! it can recover after a restart without re-scanning the whole stream.
//!
//! Run: cargo run --example docs-s3-checkpoint

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tflo_core::keyed::{StateSnapshot, StateStore};
use tflo_state_s3::{S3Client, S3StateStore};

// ---- Domain type -------------------------------------------------------

/// A payment transaction observed by the fraud-detection stream:
/// a timestamp and the charged amount in dollars.
#[derive(Clone, Debug)]
struct Txn {
    ts: i64,
    amount: f64,
}

impl Txn {
    fn new(ts: i64, amount: f64) -> Self {
        Self { ts, amount }
    }
}

// ---- Minimal in-memory S3 mock -----------------------------------------

/// In-memory mock of an S3-compatible object store.
#[derive(Clone, Debug, Default)]
struct MemS3Client {
    objects: Arc<Mutex<HashMap<String, Vec<u8>>>>,
}

#[async_trait]
impl S3Client for MemS3Client {
    async fn put_object(&self, _bucket: &str, key: &str, data: &[u8]) -> Result<(), String> {
        self.objects
            .lock()
            .unwrap()
            .insert(key.to_string(), data.to_vec());
        Ok(())
    }

    async fn get_object(&self, _bucket: &str, key: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(self.objects.lock().unwrap().get(key).cloned())
    }

    async fn list_objects(&self, _bucket: &str, prefix: &str) -> Result<Vec<String>, String> {
        let store = self.objects.lock().unwrap();
        let keys: Vec<String> = store
            .keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect();
        Ok(keys)
    }
}

// ---- Main ---------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<(), String> {
    // A few sample transactions from the payment stream.
    let txns = vec![
        Txn::new(1_000_000, 42.50),
        Txn::new(1_000_001, 980.00),
        Txn::new(1_000_002, 12.99),
    ];
    let total: f64 = txns.iter().map(|t| t.amount).sum();
    println!(
        "Observed {} transactions (${total:.2} total) before checkpoint",
        txns.len()
    );

    // Construct the store — same API surface as FileStateStore.
    let client = MemS3Client::default();
    let store = S3StateStore::new(client, "my-bucket".to_string(), "checkpoints/".to_string());

    // Build a snapshot (mirroring the file-checkpoint example).
    let snapshot = StateSnapshot {
        data: b"example-state-bytes".to_vec(),
        metadata: tflo_core::keyed::SnapshotMetadata {
            key: Some(b"fraud-detector".to_vec()),
            timestamp_ms: txns.last().map(|t| t.ts).unwrap_or(0),
            version: 1,
        },
    };

    // Persist via the S3-backed store.
    // NOTE: S3StateStore::save() bridges the async S3Client into the
    // synchronous StateStore trait by calling block_on internally
    // (requires the "async" feature on tflo-state-s3).
    // When that feature is absent the call returns a clear error message
    // rather than silently dropping the snapshot.
    match store.save(b"fraud-detector", &snapshot) {
        Ok(()) => println!("save: OK"),
        Err(e) => println!("save: {e}  (enable tflo-state-s3 'async' feature for full S3 round-trip)"),
    }

    // Load it back.
    match store.load(b"fraud-detector") {
        Ok(Some(loaded)) => {
            println!(
                "load: OK  ({} bytes, version={})",
                loaded.data.len(),
                loaded.metadata.version
            );
        }
        Ok(None) => println!("load: not found"),
        Err(e) => println!("load: {e}"),
    }

    // List keys stored under the prefix.
    match store.list_keys() {
        Ok(keys) => println!("list_keys: {} key(s)", keys.len()),
        Err(e) => println!("list_keys: {e}"),
    }

    println!("S3 checkpoint demo: done");
    Ok(())
}
