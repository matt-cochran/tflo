#![cfg(feature = "integration-tests")]
//! Adapter-level integration tests for `S3StateStore`.
//!
//! These tests start a localstack S3 endpoint via testcontainers and drive
//! the crate's `S3Client` trait through a real AWS SDK adapter
//! (`LocalstackS3Client`, defined in this file). They validate the
//! end-to-end contract — including the S3-001 contract that
//! `S3Client::delete_object` actually removes the object — against a
//! real S3-compatible server.
//!
//! Requires:
//! - feature `integration-tests` (off by default)
//! - feature `async`             (the `AsyncStateStore` impl)
//! - Docker on the host (testcontainers)
//!
//! Run with:
//! ```text
//! cargo test -p tflo-state-s3 --features integration-tests,async
//! ```

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

// The integration-tests feature is only meaningful in combination with the
// `async` feature, which provides the `AsyncStateStore` impl this test
// suite exercises. Gate behind both so a misconfigured feature combo
// becomes a no-op compile, not a wall of confusing errors.
#[cfg(feature = "async")]
mod localstack_tests {
    use std::time::Duration;

    use async_trait::async_trait;
    use aws_config::BehaviorVersion;
    use aws_credential_types::Credentials;
    use aws_sdk_s3::config::Region;
    use aws_sdk_s3::primitives::ByteStream;
    use aws_sdk_s3::Client as AwsS3;
    use testcontainers::runners::AsyncRunner;
    use testcontainers::ContainerAsync;
    use testcontainers_modules::localstack::LocalStack;
    use tflo_core::keyed::{SnapshotMetadata, StateSnapshot};
    use tflo_core::state::AsyncStateStore;
    use tflo_state_s3::{S3Client, S3StateStore};
    use uuid::Uuid;

    /// Adapter from the crate's `S3Client` trait to a real AWS SDK client
    /// pointed at localstack. Lives in the test crate (NOT in `src/`)
    /// because the production crate is intentionally client-agnostic.
    struct LocalstackS3Client {
        inner: AwsS3,
    }

    #[async_trait]
    impl S3Client for LocalstackS3Client {
        async fn put_object(
            &self,
            bucket: &str,
            key: &str,
            data: &[u8],
        ) -> Result<(), String> {
            self.inner
                .put_object()
                .bucket(bucket)
                .key(key)
                .body(ByteStream::from(data.to_vec()))
                .send()
                .await
                .map(|_| ())
                .map_err(|e| format!("put_object failed: {e}"))
        }

        async fn get_object(
            &self,
            bucket: &str,
            key: &str,
        ) -> Result<Option<Vec<u8>>, String> {
            match self.inner.get_object().bucket(bucket).key(key).send().await {
                Ok(out) => {
                    let bytes = out
                        .body
                        .collect()
                        .await
                        .map_err(|e| format!("read body failed: {e}"))?
                        .into_bytes()
                        .to_vec();
                    Ok(Some(bytes))
                }
                Err(err) => {
                    // Treat NoSuchKey as "absent", surface anything else as Err.
                    let service_err = err.as_service_error();
                    if service_err.is_some_and(aws_sdk_s3::operation::get_object::GetObjectError::is_no_such_key)
                    {
                        Ok(None)
                    } else {
                        Err(format!("get_object failed: {err}"))
                    }
                }
            }
        }

        async fn list_objects(
            &self,
            bucket: &str,
            prefix: &str,
        ) -> Result<Vec<String>, String> {
            let mut keys = Vec::new();
            let mut continuation: Option<String> = None;
            loop {
                let mut req = self
                    .inner
                    .list_objects_v2()
                    .bucket(bucket)
                    .prefix(prefix);
                if let Some(token) = continuation.as_deref() {
                    req = req.continuation_token(token);
                }
                let out = req
                    .send()
                    .await
                    .map_err(|e| format!("list_objects_v2 failed: {e}"))?;
                if let Some(contents) = out.contents {
                    for obj in contents {
                        if let Some(k) = obj.key {
                            keys.push(k);
                        }
                    }
                }
                if out.is_truncated.unwrap_or(false) {
                    continuation = out.next_continuation_token;
                    if continuation.is_none() {
                        break;
                    }
                } else {
                    break;
                }
            }
            Ok(keys)
        }

        async fn delete_object(&self, bucket: &str, key: &str) -> Result<(), String> {
            self.inner
                .delete_object()
                .bucket(bucket)
                .key(key)
                .send()
                .await
                .map(|_| ())
                .map_err(|e| format!("delete_object failed: {e}"))
        }
    }

    /// Holds a localstack container plus pre-built clients pointed at it.
    struct Harness {
        // Holding the container keeps it alive for the duration of the test.
        _container: ContainerAsync<LocalStack>,
        aws: AwsS3,
        bucket: String,
    }

    async fn start_harness() -> Harness {
        let container = LocalStack::default()
            .start()
            .await
            .expect("start localstack container");
        let host = container
            .get_host()
            .await
            .expect("localstack host")
            .to_string();
        let port = container
            .get_host_port_ipv4(4566)
            .await
            .expect("localstack port 4566");
        let endpoint = format!("http://{host}:{port}");

        let creds = Credentials::new(
            "AKIAIOSFODNN7EXAMPLE",
            "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
            None,
            None,
            "localstack",
        );

        let shared = aws_config::defaults(BehaviorVersion::latest())
            .region(Region::new("us-east-1"))
            .endpoint_url(&endpoint)
            .credentials_provider(creds)
            .load()
            .await;
        // Localstack requires path-style addressing.
        let s3_conf = aws_sdk_s3::config::Builder::from(&shared)
            .force_path_style(true)
            .build();
        let aws = AwsS3::from_conf(s3_conf);

        let bucket = format!("test-{}", Uuid::new_v4());
        aws.create_bucket()
            .bucket(&bucket)
            .send()
            .await
            .expect("create bucket");

        Harness {
            _container: container,
            aws,
            bucket,
        }
    }

    fn make_snap(payload: u8) -> StateSnapshot {
        StateSnapshot {
            data: vec![payload; 8],
            metadata: SnapshotMetadata {
                key: Some(b"k".to_vec()),
                timestamp_ms: i64::from(payload),
                version: 1,
                topology_fingerprint: Some([payload; 32]),
            },
        }
    }

    fn build_store(h: &Harness) -> S3StateStore<LocalstackS3Client> {
        S3StateStore::new(
            LocalstackS3Client {
                inner: h.aws.clone(),
            },
            h.bucket.clone(),
            "ckp/".into(),
        )
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn s3_save_load_round_trip() {
        tokio::time::timeout(Duration::from_secs(90), async {
            let h = start_harness().await;
            let store = build_store(&h);
            let snap = make_snap(42);
            store.save(b"k1", &snap).await.expect("save ok");
            let loaded = store.load(b"k1").await.expect("load ok").expect("present");
            assert_eq!(loaded.data, snap.data, "payload bytes must match");
            assert_eq!(
                loaded.metadata.topology_fingerprint,
                snap.metadata.topology_fingerprint,
            );
            assert_eq!(loaded.metadata.version, snap.metadata.version);
        })
        .await
        .expect("test exceeded 30s timeout");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn s3_delete_actually_removes_object() {
        tokio::time::timeout(Duration::from_secs(90), async {
            let h = start_harness().await;
            let store = build_store(&h);
            store.save(b"k1", &make_snap(7)).await.expect("save ok");

            // Sanity: object exists prior to delete.
            let pre = h
                .aws
                .list_objects_v2()
                .bucket(&h.bucket)
                .prefix("ckp/")
                .send()
                .await
                .expect("list pre")
                .contents
                .unwrap_or_default();
            assert_eq!(pre.len(), 1, "exactly one object before delete");

            // The S3-001 contract: delete must actually delete.
            store.delete(b"k1").await.expect("delete ok");

            // Verify via the raw AWS SDK, not via the store, so we know
            // the bytes are really gone — not just that the store hides them.
            let post = h
                .aws
                .list_objects_v2()
                .bucket(&h.bucket)
                .prefix("ckp/")
                .send()
                .await
                .expect("list post")
                .contents
                .unwrap_or_default();
            assert!(
                post.is_empty(),
                "delete must remove object from bucket, found {} objects",
                post.len()
            );
        })
        .await
        .expect("test exceeded 30s timeout");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn s3_list_keys_returns_only_matching_prefix() {
        tokio::time::timeout(Duration::from_secs(90), async {
            let h = start_harness().await;
            let store = build_store(&h);

            // Two keys the store will write under `ckp/`.
            store.save(b"prefix-a", &make_snap(1)).await.expect("save a");
            store.save(b"prefix-b", &make_snap(2)).await.expect("save b");

            // Plant a foreign object OUTSIDE the store's prefix. `list_keys`
            // must NOT return it, because parse_object_key strips the prefix
            // and would reject anything else.
            h.aws
                .put_object()
                .bucket(&h.bucket)
                .key("other-c/garbage.snapshot")
                .body(ByteStream::from(b"not-mine".to_vec()))
                .send()
                .await
                .expect("plant foreign object");

            let mut keys = store.list_keys().await.expect("list_keys ok");
            keys.sort();
            assert_eq!(
                keys,
                vec![b"prefix-a".to_vec(), b"prefix-b".to_vec()],
                "list_keys must return exactly the prefix-matched keys",
            );
        })
        .await
        .expect("test exceeded 30s timeout");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn s3_missing_key_returns_none_not_error() {
        tokio::time::timeout(Duration::from_secs(90), async {
            let h = start_harness().await;
            let store = build_store(&h);
            let res = store.load(b"never-written").await;
            match res {
                Ok(None) => {}
                Ok(Some(_)) => panic!("unexpected hit for missing key"),
                Err(e) => panic!("absent key must NOT surface as Err, got: {e}"),
            }
        })
        .await
        .expect("test exceeded 30s timeout");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn s3_concurrent_writes_dont_corrupt() {
        tokio::time::timeout(Duration::from_secs(90), async {
            let h = start_harness().await;
            // Each task gets its own store handle (cheap — just clones the
            // AWS client + bucket/prefix strings) so we exercise the real
            // concurrency path through put_object.
            let mut handles = Vec::with_capacity(10);
            for i in 0..10u8 {
                let store = build_store(&h);
                handles.push(tokio::spawn(async move {
                    let snap = make_snap(i);
                    store.save(b"shared", &snap).await.expect("save ok");
                }));
            }
            for h in handles {
                h.await.expect("task ok");
            }

            // The S3 contract is last-writer-wins: exactly one valid
            // snapshot remains, and it must be one of the values we wrote.
            let store = build_store(&h);
            let loaded = store
                .load(b"shared")
                .await
                .expect("load ok")
                .expect("present");
            // All snapshots are 8 bytes of `i` with metadata.timestamp_ms == i.
            assert_eq!(loaded.data.len(), 8, "no half-write: length intact");
            let payload = loaded.data[0];
            assert!(
                loaded.data.iter().all(|b| *b == payload),
                "no half-write: all bytes equal",
            );
            assert!(payload < 10, "payload must be one of the writer ids");
            assert_eq!(
                loaded.metadata.timestamp_ms,
                i64::from(payload),
                "metadata must correspond to the same writer as the data",
            );
        })
        .await
        .expect("test exceeded 30s timeout");
    }
}
