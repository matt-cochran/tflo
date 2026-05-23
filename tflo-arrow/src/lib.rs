#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
//! Arrow / Parquet / Polars interop for tflo (Phase 4).
//!
//! # Features
//!
//! | Feature       | What it adds                                              |
//! |---------------|-----------------------------------------------------------|
//! | `arrow-impl`  | Default — `RecordBatch` ↔ tflo helpers, schema fingerprint|
//! | `parquet`     | Parquet read/write helpers (depends on `arrow-impl`)      |
//! | `polars`      | Polars `DataFrame` interop (depends on `arrow-impl`)      |
//!
//! The crate intentionally bundles all three so users get one
//! dependency for the entire columnar batch-replay story. Per Phase 1's
//! "one crate per concern" rule this *is* the concern — Polars sits on
//! Arrow and Parquet is the wire format; a single crate keeps schema
//! handling unified across the three.
//!
//! # Schema fingerprint
//!
//! [`schema_fingerprint`] produces a stable 32-byte hash over an Arrow
//! [`Schema`](arrow_schema::Schema). It is the columnar analog of
//! [`tflo_core::builder::TFlowBuilder::fingerprint`] and serves the same
//! poka-yoke purpose: detect a backfill against a structurally different
//! schema, refuse the load with a typed error, never silently produce
//! wrong output.

#![warn(missing_docs)]
#![warn(missing_debug_implementations)]
#![deny(unsafe_code)]

#[cfg(feature = "arrow-impl")]
use std::collections::hash_map::DefaultHasher;
#[cfg(feature = "arrow-impl")]
use std::hash::{Hash, Hasher};

// ── Schema fingerprint ─────────────────────────────────────────────────

/// Stable 32-byte topology hash of an Arrow [`Schema`](arrow_schema::Schema).
///
/// Hashes each field's `(name, data_type, nullable)` triple in column
/// order. Two schemas that produce the same fingerprint have the same
/// structural shape; a fingerprint mismatch must be treated as a hard
/// stop — the contract is identical to
/// [`tflo_core::builder::TFlowBuilder::fingerprint`].
#[cfg(feature = "arrow-impl")]
#[must_use]
pub fn schema_fingerprint(schema: &arrow_schema::Schema) -> [u8; 32] {
    let mut h = DefaultHasher::new();
    schema.fields().len().hash(&mut h);
    for field in schema.fields() {
        field.name().hash(&mut h);
        format!("{:?}", field.data_type()).hash(&mut h);
        field.is_nullable().hash(&mut h);
    }
    let h64 = h.finish();
    let mut out = [0u8; 32];
    out[..8].copy_from_slice(&h64.to_le_bytes());
    out[8..16].copy_from_slice(&h64.rotate_left(17).to_le_bytes());
    out[16..24].copy_from_slice(&h64.rotate_left(31).to_le_bytes());
    out[24..32].copy_from_slice(&h64.rotate_left(47).to_le_bytes());
    out
}

// ── Parquet helpers (feature `parquet`) ────────────────────────────────

/// Parquet I/O helpers, gated behind the `parquet` feature.
#[cfg(feature = "parquet")]
pub mod parquet_io {
    use arrow::record_batch::RecordBatch;
    use parquet::arrow::arrow_writer::ArrowWriter;
    use parquet::file::properties::WriterProperties;
    use std::fs::File;
    use std::path::Path;

    /// Write `batches` to `path` as a Parquet file.
    ///
    /// All batches must share the same schema (the first batch's schema
    /// is used).
    ///
    /// # Errors
    ///
    /// Returns an error string on I/O failure or schema mismatch.
    pub fn write_batches(path: &Path, batches: &[RecordBatch]) -> Result<(), String> {
        let first = batches
            .first()
            .ok_or_else(|| "write_batches: empty input".to_string())?;
        let file =
            File::create(path).map_err(|e| format!("create {path:?} failed: {e}"))?;
        let props = WriterProperties::builder().build();
        let mut writer = ArrowWriter::try_new(file, first.schema(), Some(props))
            .map_err(|e| format!("ArrowWriter::try_new failed: {e}"))?;
        for batch in batches {
            writer
                .write(batch)
                .map_err(|e| format!("write batch failed: {e}"))?;
        }
        writer
            .close()
            .map_err(|e| format!("writer close failed: {e}"))?;
        Ok(())
    }

    /// Read all batches from a Parquet file at `path`.
    ///
    /// # Errors
    ///
    /// Returns an error string on I/O or schema failure.
    pub fn read_batches(path: &Path) -> Result<Vec<RecordBatch>, String> {
        use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
        let file = File::open(path).map_err(|e| format!("open {path:?} failed: {e}"))?;
        let builder = ParquetRecordBatchReaderBuilder::try_new(file)
            .map_err(|e| format!("reader builder failed: {e}"))?;
        let reader = builder
            .build()
            .map_err(|e| format!("reader build failed: {e}"))?;
        let mut out = Vec::new();
        for batch in reader {
            out.push(batch.map_err(|e| format!("batch read failed: {e}"))?);
        }
        Ok(out)
    }
}

// ── Polars helpers (feature `polars`) ──────────────────────────────────

/// Polars interop, gated behind the `polars` feature.
#[cfg(feature = "polars")]
pub mod polars_interop {
    use polars::prelude::*;

    /// Iterate a [`DataFrame`] as `(column_name, AnyValue)` rows.
    ///
    /// Provided because the obvious "iterate as rows" call is awkward
    /// in raw Polars — this returns owned values so the iterator can
    /// outlive the frame.
    ///
    /// # Errors
    ///
    /// Returns an error string when a column cannot be accessed.
    pub fn rows_as_named_values(df: &DataFrame) -> Result<Vec<Vec<(String, String)>>, String> {
        let cols = df.get_columns();
        let mut rows = Vec::with_capacity(df.height());
        for row_idx in 0..df.height() {
            let mut row = Vec::with_capacity(cols.len());
            for col in cols {
                let value = col.get(row_idx).map_err(|e| format!("get: {e}"))?;
                row.push((col.name().to_string(), format!("{value}")));
            }
            rows.push(row);
        }
        Ok(rows)
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    #[cfg(feature = "arrow-impl")]
    mod arrow_tests {
        use crate::schema_fingerprint;
        use arrow_schema::{DataType, Field, Schema};
        use std::sync::Arc;

        fn schema_a() -> Schema {
            Schema::new(vec![
                Field::new("ts", DataType::Int64, false),
                Field::new("value", DataType::Float64, true),
            ])
        }

        fn schema_b_renamed() -> Schema {
            Schema::new(vec![
                Field::new("timestamp", DataType::Int64, false),
                Field::new("value", DataType::Float64, true),
            ])
        }

        fn schema_c_extra() -> Schema {
            Schema::new(vec![
                Field::new("ts", DataType::Int64, false),
                Field::new("value", DataType::Float64, true),
                Field::new("device_id", DataType::Utf8, false),
            ])
        }

        #[test]
        fn fingerprint_is_stable_across_calls() {
            let s = schema_a();
            assert_eq!(schema_fingerprint(&s), schema_fingerprint(&s));
        }

        #[test]
        fn fingerprint_differs_when_field_renamed() {
            assert_ne!(
                schema_fingerprint(&schema_a()),
                schema_fingerprint(&schema_b_renamed())
            );
        }

        #[test]
        fn fingerprint_differs_when_field_added() {
            assert_ne!(
                schema_fingerprint(&schema_a()),
                schema_fingerprint(&schema_c_extra())
            );
        }

        #[test]
        fn fingerprint_is_32_bytes() {
            let s = schema_a();
            assert_eq!(schema_fingerprint(&s).len(), 32);
        }

        #[test]
        fn arc_wrapped_schema_compares_equal() {
            let a = Arc::new(schema_a());
            let b = Arc::new(schema_a());
            assert_eq!(schema_fingerprint(&a), schema_fingerprint(&b));
        }
    }

    #[cfg(feature = "parquet")]
    mod parquet_tests {
        use crate::parquet_io::{read_batches, write_batches};
        use arrow::array::{Float64Array, Int64Array};
        use arrow::record_batch::RecordBatch;
        use arrow_schema::{DataType, Field, Schema};
        use std::sync::Arc;

        fn make_batch() -> RecordBatch {
            let schema = Arc::new(Schema::new(vec![
                Field::new("ts", DataType::Int64, false),
                Field::new("value", DataType::Float64, false),
            ]));
            RecordBatch::try_new(
                schema,
                vec![
                    Arc::new(Int64Array::from(vec![1, 2, 3])),
                    Arc::new(Float64Array::from(vec![10.0, 20.0, 30.0])),
                ],
            )
            .expect("batch")
        }

        #[test]
        fn write_then_read_round_trip() {
            let dir = std::env::temp_dir().join(format!(
                "tflo-arrow-test-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0)
            ));
            std::fs::create_dir_all(&dir).expect("mkdir");
            let path = dir.join("test.parquet");
            let batch = make_batch();
            write_batches(&path, &[batch.clone()]).expect("write");
            let read_back = read_batches(&path).expect("read");
            assert_eq!(read_back.len(), 1);
            assert_eq!(read_back[0].num_rows(), 3);
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}
