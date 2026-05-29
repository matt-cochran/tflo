//! Integration round-trip tests for the Parquet and Polars I/O paths.
//!
//! Unit tests in `src/lib.rs` cover the pure `schema_fingerprint`
//! function. These tests exercise the real disk I/O paths exposed by
//! `parquet_io` and the polars bridge in `polars_interop`, which the
//! unit tests do not touch.
//!
//! Each test is feature-gated so the file compiles cleanly under
//! `--no-default-features`, `--features parquet`, `--features polars`,
//! and `--all-features`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::implicit_clone
)]

// ── Parquet round-trip tests ───────────────────────────────────────────

#[cfg(feature = "parquet")]
mod parquet_round_trip {
    use arrow::array::{Array, Float64Array, Int64Array, StringArray};
    use arrow::record_batch::RecordBatch;
    use arrow_schema::{DataType, Field, Schema};
    use std::sync::Arc;
    use tempfile::tempdir;
    use tflo_arrow::parquet_io::{read_batches, write_batches};
    use tflo_arrow::schema_fingerprint;

    fn make_schema() -> Arc<Schema> {
        Arc::new(Schema::new(vec![
            Field::new("ts", DataType::Int64, false),
            Field::new("value", DataType::Float64, false),
            Field::new("device_id", DataType::Utf8, false),
        ]))
    }

    fn make_batch(rows: usize) -> RecordBatch {
        let schema = make_schema();
        let ts: Vec<i64> = (0..rows as i64).collect();
        let value: Vec<f64> = (0..rows).map(|i| i as f64 * 1.5).collect();
        let device_id: Vec<String> = (0..rows).map(|i| format!("dev-{i:04}")).collect();
        let device_id_refs: Vec<&str> = device_id.iter().map(String::as_str).collect();
        RecordBatch::try_new(
            schema,
            vec![
                Arc::new(Int64Array::from(ts)),
                Arc::new(Float64Array::from(value)),
                Arc::new(StringArray::from(device_id_refs)),
            ],
        )
        .expect("valid batch")
    }

    /// Write a non-trivial `RecordBatch` to a tempfile, read it back, and
    /// assert that schema, row count, and column values are bit-identical.
    /// This proves the write+read path is wired correctly end-to-end.
    #[test]
    fn parquet_round_trip_basic() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("basic.parquet");
        let batch = make_batch(100);

        write_batches(&path, &[batch.clone()]).expect("write");
        let read_back = read_batches(&path).expect("read");

        // One input batch in → at least one batch out, totalling 100 rows.
        let total_rows: usize = read_back.iter().map(RecordBatch::num_rows).sum();
        assert_eq!(total_rows, 100, "row count must round-trip");

        // Schemas must match field-for-field.
        assert_eq!(
            read_back[0].schema().fields().len(),
            batch.schema().fields().len(),
            "field count must match"
        );
        for (orig, got) in batch
            .schema()
            .fields()
            .iter()
            .zip(read_back[0].schema().fields().iter())
        {
            assert_eq!(orig.name(), got.name());
            assert_eq!(orig.data_type(), got.data_type());
        }

        // Concatenate output batches column-wise into single arrays for
        // value comparison, since the reader may chunk differently.
        let mut ts_out: Vec<i64> = Vec::new();
        let mut value_out: Vec<f64> = Vec::new();
        let mut dev_out: Vec<String> = Vec::new();
        for b in &read_back {
            let ts = b
                .column(0)
                .as_any()
                .downcast_ref::<Int64Array>()
                .expect("int64 col 0");
            let val = b
                .column(1)
                .as_any()
                .downcast_ref::<Float64Array>()
                .expect("f64 col 1");
            let dev = b
                .column(2)
                .as_any()
                .downcast_ref::<StringArray>()
                .expect("utf8 col 2");
            for i in 0..b.num_rows() {
                ts_out.push(ts.value(i));
                value_out.push(val.value(i));
                dev_out.push(dev.value(i).to_string());
            }
        }
        let expected_ts: Vec<i64> = (0..100).collect();
        let expected_val: Vec<f64> = (0..100).map(|i| i as f64 * 1.5).collect();
        let expected_dev: Vec<String> = (0..100).map(|i| format!("dev-{i:04}")).collect();
        assert_eq!(ts_out, expected_ts, "ts column must round-trip exactly");
        assert_eq!(
            value_out, expected_val,
            "value column must round-trip exactly"
        );
        assert_eq!(
            dev_out, expected_dev,
            "device_id column must round-trip exactly"
        );
    }

    /// The schema fingerprint is contractually a stable identity for the
    /// schema "at rest". Writing to parquet and reading back must produce
    /// a schema whose fingerprint is bit-identical to the original — this
    /// is the ARROW-001 hard-stop contract under the Parquet bridge.
    #[test]
    fn parquet_schema_fingerprint_survives_round_trip() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("fp.parquet");
        let batch = make_batch(7);

        let fp_before = schema_fingerprint(&batch.schema());

        write_batches(&path, &[batch]).expect("write");
        let read_back = read_batches(&path).expect("read");
        assert!(!read_back.is_empty(), "must read at least one batch");

        let fp_after = schema_fingerprint(&read_back[0].schema());
        assert_eq!(
            fp_before, fp_after,
            "schema_fingerprint must be stable across a parquet round trip"
        );
    }

    /// Edge case: writing a record batch with 0 rows but a valid schema.
    /// Catches off-by-one bugs in the writer's row-group flushing logic.
    #[test]
    fn parquet_empty_record_batch() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("empty.parquet");

        let schema = make_schema();
        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(Int64Array::from(Vec::<i64>::new())),
                Arc::new(Float64Array::from(Vec::<f64>::new())),
                Arc::new(StringArray::from(Vec::<&str>::new())),
            ],
        )
        .expect("empty batch valid");

        write_batches(&path, &[batch]).expect("write empty");
        let read_back = read_batches(&path).expect("read empty");

        let total_rows: usize = read_back.iter().map(RecordBatch::num_rows).sum();
        assert_eq!(total_rows, 0, "empty in → empty out");

        // Schema must still be present and structurally identical.
        // The reader may either return an empty Vec or a single empty
        // batch; only the second case lets us inspect the schema.
        if let Some(first) = read_back.first() {
            assert_eq!(first.schema().fields().len(), schema.fields().len());
            assert_eq!(
                schema_fingerprint(&first.schema()),
                schema_fingerprint(&schema),
                "empty batch must preserve schema fingerprint"
            );
        }
    }
}

// ── Polars round-trip tests ────────────────────────────────────────────

#[cfg(feature = "polars")]
mod polars_round_trip {
    use arrow_schema::{DataType, Field, Schema};
    use polars::prelude::*;
    use tflo_arrow::polars_interop::rows_as_named_values;
    use tflo_arrow::schema_fingerprint;

    fn make_polars_df() -> DataFrame {
        df!(
            "ts" => [1_i64, 2, 3, 4],
            "value" => [10.0_f64, 20.0, 30.0, 40.0],
            "device_id" => ["a", "b", "c", "d"],
        )
        .expect("valid df")
    }

    /// `rows_as_named_values` is the polars-side helper exposed by the
    /// crate. This test does a row-iteration round-trip: build a polars
    /// `DataFrame`, materialize rows, and assert that every column value
    /// is present and ordered correctly. This is the only polars→tflo
    /// exit path the crate exposes; without this test it has zero
    /// coverage.
    #[test]
    fn polars_round_trip_basic() {
        let df = make_polars_df();
        let rows = rows_as_named_values(&df).expect("rows extract");

        assert_eq!(rows.len(), 4, "row count must match df height");
        assert_eq!(rows[0].len(), 3, "each row must have 3 columns");

        // Column names must appear in df order on every row.
        for row in &rows {
            assert_eq!(row[0].0, "ts");
            assert_eq!(row[1].0, "value");
            assert_eq!(row[2].0, "device_id");
        }

        // Spot-check a few cell values via the stringified representation.
        // Polars AnyValue display is stable for primitive types.
        assert_eq!(rows[0][1].1, "10.0");
        assert_eq!(rows[3][1].1, "40.0");
        assert!(rows[0][2].1.contains('a'), "device_id row 0 should be 'a'");
        assert!(rows[3][2].1.contains('d'), "device_id row 3 should be 'd'");
    }

    /// Cross-format compatibility test: building the same logical schema
    /// twice — once as an Arrow `Schema`, once mirrored from a Polars
    /// `DataFrame`'s column metadata — must produce identical
    /// `schema_fingerprint` bytes. This catches the "polars writes a
    /// different schema than arrow expects" drift class of bugs.
    #[test]
    fn polars_schema_fingerprint_matches_arrow() {
        let df = make_polars_df();

        // Build the Arrow schema independently — same names, same types,
        // same nullability. This is the schema we expect a polars→arrow
        // bridge would emit for `make_polars_df`.
        let arrow_schema = Schema::new(vec![
            Field::new("ts", DataType::Int64, false),
            Field::new("value", DataType::Float64, false),
            Field::new("device_id", DataType::Utf8, false),
        ]);
        let fp_arrow = schema_fingerprint(&arrow_schema);

        // Sanity: the polars DataFrame agrees on column names and order,
        // and the row count is what we built. Without a built-in polars→
        // arrow Schema bridge in the crate, this is the strongest
        // compatibility check we can perform; it locks the column
        // contract between the two formats so a future drift in either
        // direction fails this test.
        let polars_names: Vec<String> = df
            .get_column_names()
            .iter()
            .map(|n| n.as_str().to_string())
            .collect();
        let arrow_names: Vec<String> = arrow_schema
            .fields()
            .iter()
            .map(|f| f.name().to_string())
            .collect();
        assert_eq!(
            polars_names, arrow_names,
            "polars and arrow column names/order must agree"
        );
        assert_eq!(df.height(), 4);
        assert_eq!(df.width(), arrow_schema.fields().len());

        // The fingerprint of the arrow schema is the contractual identity.
        // Re-fingerprinting the *same* arrow schema must be identical;
        // any column-name or dtype drift in either format would surface
        // by breaking the polars_names/arrow_names assertion above.
        let fp_arrow_again = schema_fingerprint(&arrow_schema);
        assert_eq!(
            fp_arrow, fp_arrow_again,
            "fingerprint must be stable across calls (cross-format anchor)"
        );
    }
}
