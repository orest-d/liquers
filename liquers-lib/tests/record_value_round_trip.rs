// liquers-lib/tests/record_value_round_trip.rs
#![cfg(feature = "records")]

use std::sync::Arc;

use liquers_lib::value::{ExtValueInterface, Value};
use liquers_records::{
    FieldSchema, ManifestSource, ManifestSpec, RecordBatch, RecordSchema, RecordSource, RecordView,
};

#[test]
fn record_value_view_round_trip_preserves_the_arc() -> Result<(), Box<dyn std::error::Error>> {
    // `RecordBatch::new` requires exactly one column per schema field
    // (`liquers-records/src/batch.rs`); this test only needs *some* view to prove the `Arc` round
    // trips through `Value`, so the schema declares no fields and the columns vector matches it —
    // an empty batch, not a batch with columns dropped to make an unrelated point.
    let schema = Arc::new(RecordSchema::new(Vec::<FieldSchema>::new())?);
    let batch = RecordBatch::new(schema, vec![], None, None, vec![])?;
    let view: Arc<dyn RecordView> = Arc::new(batch);

    let value = Value::from_record_view(view.clone());
    let extracted = value.as_record_view().expect("as_record_view");
    assert!(Arc::ptr_eq(&view, &extracted));
    Ok(())
}

#[test]
fn record_value_source_round_trip_preserves_the_arc() -> Result<(), Box<dyn std::error::Error>> {
    let spec = ManifestSpec {
        chunks: vec![],
        template: None,
        extension: None,
        stored: true,
        cached: true,
        uniform_schema: None,
        ..ManifestSpec::default()
    };
    let source: Arc<dyn RecordSource> = Arc::new(ManifestSource::new(spec, None)?);

    let value = Value::from_record_source(source.clone());
    let extracted = value.as_record_source().expect("as_record_source");
    assert!(Arc::ptr_eq(&source, &extracted));
    Ok(())
}
