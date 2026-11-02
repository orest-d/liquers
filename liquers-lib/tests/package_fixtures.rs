//! The committed record fixtures of liquers-py's wheel tests (`liquers-py/tests/package/data/`),
//! written by this crate's own writers so the Python side needs no pyarrow to have them
//! (`specs/design/liquers-py-package/phase4-implementation.md`, Step 6).
//!
//! `package_fixtures_are_current` runs by default and compares the committed bytes with what the
//! writers produce now. `write_package_fixtures` rewrites them, and only when
//! `LIQUERS_UPDATE_FIXTURES=1` is set:
//!
//! ```bash
//! LIQUERS_UPDATE_FIXTURES=1 cargo test -p liquers-lib --test package_fixtures
//! ```
#![cfg(all(feature = "records-ipc", feature = "records-parquet"))]

use std::path::PathBuf;
use std::sync::Arc;

use liquers_core::error::Error;
use liquers_lib::records::{
    write_table, FieldSchema, FieldType, FieldValue, RecordBatchMut, RecordSchema, RecordViewMut, TableFormat,
    WriteOptions,
};

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../liquers-py/tests/package/data")
}

/// Two rows, three fields: what the wheel test reads back through `ns-rec/to_record-<format>`.
fn small_batch() -> Result<liquers_lib::records::RecordBatch, Error> {
    let schema = Arc::new(RecordSchema::new(vec![
        FieldSchema::new("a", FieldType::Int),
        FieldSchema::new("b", FieldType::Float),
        FieldSchema::new("c", FieldType::Text),
    ])?);
    let mut batch = RecordBatchMut::with_capacity(schema, 2);
    batch.append_row(&[FieldValue::Int(1), FieldValue::Float(1.5), FieldValue::Text(Arc::from("x"))])?;
    batch.append_row(&[FieldValue::Int(2), FieldValue::Null, FieldValue::Text(Arc::from("y"))])?;
    batch.freeze()
}

fn fixtures() -> Result<Vec<(&'static str, Vec<u8>)>, Error> {
    let batch = small_batch()?;
    Ok(vec![
        ("small.feather", write_table(&batch, TableFormat::Ipc, &WriteOptions::default())?),
        ("small.parquet", write_table(&batch, TableFormat::Parquet, &WriteOptions::default())?),
    ])
}

#[test]
fn write_package_fixtures() -> Result<(), Error> {
    if std::env::var("LIQUERS_UPDATE_FIXTURES").as_deref() != Ok("1") {
        return Ok(());
    }
    std::fs::create_dir_all(fixture_dir()).map_err(|e| Error::general_error(e.to_string()))?;
    for (name, bytes) in fixtures()? {
        std::fs::write(fixture_dir().join(name), bytes).map_err(|e| Error::general_error(e.to_string()))?;
    }
    Ok(())
}

#[test]
fn package_fixtures_are_current() -> Result<(), Error> {
    if std::env::var("LIQUERS_UPDATE_FIXTURES").as_deref() == Ok("1") {
        return Ok(());
    }
    for (name, bytes) in fixtures()? {
        let committed = std::fs::read(fixture_dir().join(name))
            .map_err(|e| Error::general_error(format!("{name}: {e}; regenerate with LIQUERS_UPDATE_FIXTURES=1")))?;
        assert!(committed == bytes, "{name} is stale; regenerate with LIQUERS_UPDATE_FIXTURES=1");
    }
    Ok(())
}
