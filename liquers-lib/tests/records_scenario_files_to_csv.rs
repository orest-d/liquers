// liquers-lib/tests/records_scenario_files_to_csv.rs
//
// Phase 3 §1.1 ("Scenario 1 — Files to CSV") — library-level tests exercising the primitives
// `ns-rec/file_records` and its neighbouring commands are built on: schema construction,
// `RecordBatchMut`, filtering through a comparison mask, `select_columns`, and CSV/Markdown/HTML
// serialization. See `specs/design/record-streams/phase3-tests.md` §1.1.
#![cfg(feature = "records")]

use std::sync::Arc;

use liquers_records::{
    CompareOp, FieldSchema, FieldType, FieldValue, KeyRole, RecordBatchMut, RecordSchema,
    RecordView, RecordViewMut,
};

fn schema_with_id_and_size() -> Arc<RecordSchema> {
    Arc::new(
        RecordSchema::new(vec![
            FieldSchema::new("file_id", FieldType::Text).with_key(KeyRole::Id),
            FieldSchema::new("size_bytes", FieldType::Int),
        ])
        .expect("schema valid"),
    )
}

#[test]
fn schema_with_id_and_roles_roundtrips() {
    let schema = schema_with_id_and_size();
    assert_eq!(schema.id_field(), Some(0));
    assert_eq!(schema.payload_fields(), vec![1]);
    assert_eq!(schema.index_of("file_id"), Some(0));
    assert_eq!(schema.index_of("size_bytes"), Some(1));
}

#[test]
fn record_batch_mut_builds_and_freezes() -> Result<(), Box<dyn std::error::Error>> {
    let schema = schema_with_id_and_size();
    let mut batch = RecordBatchMut::with_capacity(schema, 2);
    batch.append_row(&[FieldValue::Text(Arc::from("file1")), FieldValue::Int(42)])?;
    batch.append_row(&[FieldValue::Text(Arc::from("file2")), FieldValue::Int(100)])?;
    let frozen = batch.freeze()?;
    assert_eq!(frozen.len, 2);
    assert_eq!(frozen.schema.fields.len(), 2);
    Ok(())
}

#[test]
fn column_compare_builds_mask_matching_selected_rows() -> Result<(), Box<dyn std::error::Error>> {
    let schema = schema_with_id_and_size();
    let mut batch = RecordBatchMut::with_capacity(schema, 3);
    batch.append_row(&[FieldValue::Text(Arc::from("a")), FieldValue::Int(5)])?;
    batch.append_row(&[FieldValue::Text(Arc::from("b")), FieldValue::Int(15)])?;
    batch.append_row(&[FieldValue::Text(Arc::from("c")), FieldValue::Int(25)])?;
    let batch = batch.freeze()?;

    let size_col = batch.column(1)?;
    let mask = size_col.compare(CompareOp::Gt, &FieldValue::Int(10))?;

    assert_eq!(mask.count_ones(), 2); // rows 1 and 2 are > 10
    assert!(!mask.get(0));
    assert!(mask.get(1));
    assert!(mask.get(2));
    Ok(())
}

#[test]
fn select_columns_keeps_id_field_even_when_unnamed() -> Result<(), Box<dyn std::error::Error>> {
    let schema = Arc::new(RecordSchema::new(vec![
        FieldSchema::new("id", FieldType::Text).with_key(KeyRole::Id),
        FieldSchema::new("name", FieldType::Text),
        FieldSchema::new("size", FieldType::Int),
    ])?);
    let mut batch = RecordBatchMut::with_capacity(schema, 1);
    batch.append_row(&[
        FieldValue::Text(Arc::from("f1")),
        FieldValue::Text(Arc::from("report.csv")),
        FieldValue::Int(1024),
    ])?;
    let batch: Arc<dyn RecordView> = Arc::new(batch.freeze()?);

    let projected = batch.select_columns(&["name", "size"])?;
    assert_eq!(projected.schema().fields.len(), 3); // id kept
    // CORRECTED from Phase 3 §1.1: `select_columns` (`liquers-records/src/views.rs`, `impl dyn
    // RecordView`) places named columns first, in the order given, then appends any of Id/Source
    // not already among them — see its own doc comment and
    // `views.rs`'s `select_columns_keeps_id_field_even_when_not_named` test, which already pins
    // this order (`id_field() == Some(1)` there, one field after the one named column). The
    // Phase 3 draft assumed Id stayed first; for two named columns it lands last instead.
    assert_eq!(projected.schema().fields[0].name, "name");
    assert_eq!(projected.schema().fields[2].name, "id");
    assert_eq!(projected.schema().id_field(), Some(2));
    Ok(())
}

#[test]
fn filter_through_mask_gathers_selected_rows() -> Result<(), Box<dyn std::error::Error>> {
    let schema = Arc::new(RecordSchema::new(vec![
        FieldSchema::new("name", FieldType::Text),
        FieldSchema::new("size", FieldType::Int),
    ])?);
    let mut batch = RecordBatchMut::with_capacity(schema, 3);
    batch.append_row(&[FieldValue::Text(Arc::from("a")), FieldValue::Int(5)])?;
    batch.append_row(&[FieldValue::Text(Arc::from("b")), FieldValue::Int(15)])?;
    batch.append_row(&[FieldValue::Text(Arc::from("c")), FieldValue::Int(5)])?;
    let batch: Arc<dyn RecordView> = Arc::new(batch.freeze()?);

    let size_col = batch.column(1)?;
    let mask = size_col.compare(CompareOp::Eq, &FieldValue::Int(5))?;
    let filtered = batch.filter(&mask)?;
    assert_eq!(filtered.len(), 2); // rows 0 and 2

    let mat = filtered.materialize()?;
    assert_eq!(mat.column(0)?.get(0)?, FieldValue::Text(Arc::from("a")));
    assert_eq!(mat.column(0)?.get(1)?, FieldValue::Text(Arc::from("c")));
    Ok(())
}

#[test]
fn csv_serialization_distinguishes_null_from_empty_string() -> Result<(), Box<dyn std::error::Error>> {
    use liquers_records::formats::{write_table, TableFormat, WriteOptions};

    let schema = Arc::new(RecordSchema::new(vec![
        FieldSchema::new("name", FieldType::Text),
        FieldSchema::new("note", FieldType::Text),
    ])?);
    let mut batch = RecordBatchMut::with_capacity(schema, 2);
    batch.append_row(&[FieldValue::Text(Arc::from("Alice")), FieldValue::Null])?;
    batch.append_row(&[FieldValue::Text(Arc::from("Bob")), FieldValue::Text(Arc::from(""))])?;
    let batch = batch.freeze()?;

    let csv = write_table(&batch, TableFormat::Csv { separator: b',' }, &WriteOptions::default())?;
    let csv_str = String::from_utf8(csv)?;

    assert!(csv_str.contains("Alice,\n") || csv_str.contains("Alice,\r\n")); // null: unquoted empty
    assert!(csv_str.contains("Bob,\"\"")); // empty string: quoted
    Ok(())
}

#[test]
fn markdown_uses_labels_not_names_in_header() -> Result<(), Box<dyn std::error::Error>> {
    use liquers_records::formats::{write_table, TableFormat, WriteOptions};

    let schema = Arc::new(RecordSchema::new(vec![
        FieldSchema::new("file_id", FieldType::Text).with_label("File ID"),
        FieldSchema::new("size_bytes", FieldType::Int).with_label("Size (bytes)"),
    ])?);
    let mut batch = RecordBatchMut::with_capacity(schema, 1);
    batch.append_row(&[FieldValue::Text(Arc::from("data.csv")), FieldValue::Int(2048)])?;
    let batch = batch.freeze()?;

    let md = write_table(&batch, TableFormat::Markdown, &WriteOptions::default())?;
    let md_str = String::from_utf8(md)?;
    assert!(md_str.contains("File ID"));
    assert!(md_str.contains("Size (bytes)"));
    assert!(!md_str.contains("file_id"));
    Ok(())
}

#[test]
fn html_escapes_cell_content() -> Result<(), Box<dyn std::error::Error>> {
    use liquers_records::formats::{write_table, TableFormat, WriteOptions};

    let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new("content", FieldType::Text)])?);
    let mut batch = RecordBatchMut::with_capacity(schema, 1);
    batch.append_row(&[FieldValue::Text(Arc::from("<script>alert('xss')</script>"))])?;
    let batch = batch.freeze()?;

    let html = write_table(&batch, TableFormat::Html, &WriteOptions::default())?;
    let html_str = String::from_utf8(html)?;
    assert!(html_str.contains("&lt;script&gt;"));
    assert!(!html_str.contains("<script>"));
    Ok(())
}
