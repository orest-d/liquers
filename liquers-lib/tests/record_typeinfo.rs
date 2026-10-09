// liquers-lib/tests/record_typeinfo.rs
#![cfg(feature = "records")]

// `ValueExtension` is liquers-lib's trait (`liquers-lib/src/value/extended.rs`), not core's.
use liquers_lib::value::{ExtValue, ValueExtension};

#[test]
fn record_view_type_info_is_registered_with_a_bare_identifier() {
    let infos = <ExtValue as ValueExtension>::type_descriptions();
    let info = infos
        .iter()
        .find(|i| i.type_identifier == "RecordView")
        .expect("RecordView TypeInfo");
    assert!(!info.supported_data_formats.is_empty());
}

#[test]
fn record_view_type_info_advertises_csv() {
    let infos = <ExtValue as ValueExtension>::type_descriptions();
    let info = infos
        .iter()
        .find(|i| i.type_identifier == "RecordView")
        .expect("RecordView TypeInfo");
    assert!(info.supported_data_formats.iter().any(|f| f == "csv"));
}

#[test]
fn record_source_type_info_advertises_only_the_manifest_formats() {
    let infos = <ExtValue as ValueExtension>::type_descriptions();
    let info = infos
        .iter()
        .find(|i| i.type_identifier == "RecordSource")
        .expect("RecordSource TypeInfo");
    // A `ManifestSource`'s only byte form is its manifest (§"A source serializes only as its
    // manifest"); every other `RecordSource` is refused on write, so the registry must not
    // over-promise table formats for the source variant.
    assert!(info.supported_data_formats.iter().any(|f| f == "yaml"));
    assert!(!info.supported_data_formats.iter().any(|f| f == "csv"));
}

fn record_view_formats() -> Vec<String> {
    <ExtValue as ValueExtension>::type_descriptions()
        .into_iter()
        .find(|i| i.type_identifier == "RecordView")
        .map(|i| i.supported_data_formats.iter().map(|f| f.to_string()).collect())
        .unwrap_or_default()
}

/// Every alias `TableFormat::from_data_format` accepts for a format this build can write is
/// declared: the registry gates the write path, so an undeclared alias is a format the codec
/// supports and the asset layer refuses (a `data.jsonl` or `data.arrow` filename).
#[test]
fn record_view_type_info_declares_every_table_format_alias() {
    let declared = record_view_formats();
    #[allow(unused_mut)] // extended only with records-ipc / records-parquet
    let mut expected = vec![
        "csv", "csv:comma", "tsv", "csv:tab", "ndjson", "jsonl", "json", "md", "markdown", "html",
    ];
    #[cfg(feature = "records-ipc")]
    expected.extend(["ipc", "feather", "arrow_ipc", "arrow"]);
    #[cfg(feature = "records-parquet")]
    expected.push("parquet");
    for alias in expected {
        assert!(
            declared.iter().any(|f| f == alias),
            "RecordView must declare '{alias}'; declared: {declared:?}"
        );
    }
}

/// Every declared format is written and read back, except the ones recorded as write-only:
/// `html` (liquers-records never reads HTML) and, without polars, `parquet`.
#[test]
fn every_declared_record_view_format_round_trips_or_is_recorded_as_write_only(
) -> Result<(), Box<dyn std::error::Error>> {
    use liquers_core::value::DefaultValueSerializer;
    use liquers_lib::value::ExtValueInterface;
    use liquers_records::{Buffer, Column, FieldSchema, FieldType, RecordBatch, RecordSchema};
    use std::sync::Arc;

    let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new("n", FieldType::Int)])?);
    let column = Column::Int {
        validity: None,
        values: Buffer::from_slice(&[1i64, 2, 3]),
    };
    let batch = RecordBatch::new(schema, vec![column], None, None, vec![])?;
    let value = ExtValue::from_record_view(Arc::new(batch));

    let mut write_only = Vec::new();
    for format in record_view_formats() {
        let bytes = value.as_bytes(&format)?;
        match ExtValue::deserialize_from_bytes(&bytes, "RecordView", &format) {
            Ok(back) => {
                let view = back.as_record_view()?;
                assert_eq!(view.len(), 3, "round trip of RecordView as {format}");
            }
            Err(_) => write_only.push(format),
        }
    }
    #[allow(unused_mut)]
    let mut recorded = vec!["html".to_string()];
    #[cfg(all(feature = "records-parquet", not(feature = "polars")))]
    recorded.push("parquet".to_string());
    assert_eq!(write_only, recorded, "write-only RecordView formats changed");
    Ok(())
}

/// `md` is declared on both `Text` and `RecordView`. Through the combined `Value`, whose base
/// serializer is asked first, a `RecordView` written as markdown must still read back as a table,
/// not as the markdown text (`specs/design/text-value-markdown-format/`).
#[test]
fn record_view_markdown_reads_back_as_a_table_through_the_combined_value(
) -> Result<(), Box<dyn std::error::Error>> {
    use liquers_core::value::{DefaultValueSerializer, ValueInterface};
    use liquers_lib::value::{ExtValueInterface, Value};
    use liquers_records::{Buffer, Column, FieldSchema, FieldType, RecordBatch, RecordSchema};
    use std::sync::Arc;

    let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new("n", FieldType::Int)])?);
    let column = Column::Int {
        validity: None,
        values: Buffer::from_slice(&[1i64, 2, 3]),
    };
    let batch = RecordBatch::new(schema, vec![column], None, None, vec![])?;
    let value = Value::from_record_view(Arc::new(batch));
    for format in ["md", "markdown"] {
        let bytes = value.as_bytes(format)?;
        let back = Value::deserialize_from_bytes(&bytes, "RecordView", format)?;
        assert_eq!(back.identifier(), "RecordView", "RecordView as {format}");
        assert_eq!(back.as_record_view()?.len(), 3, "RecordView as {format}");
    }
    let text = Value::deserialize_from_bytes(b"# x", "Text", "md")?;
    assert_eq!(text.try_into_string()?, "# x");
    Ok(())
}

fn three_row_view() -> Result<liquers_lib::value::Value, Box<dyn std::error::Error>> {
    use liquers_lib::value::{ExtValueInterface, Value};
    use liquers_records::{Buffer, Column, FieldSchema, FieldType, RecordBatch, RecordSchema};
    use std::sync::Arc;

    let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new("n", FieldType::Int)])?);
    let column = Column::Int {
        validity: None,
        values: Buffer::from_slice(&[1i64, 2, 3]),
    };
    let batch = RecordBatch::new(schema, vec![column], None, None, vec![])?;
    Ok(Value::from_record_view(Arc::new(batch)))
}

/// AC-1 (`specs/design/combined-value-identifier-dispatch/`): the base value reads any `json` as
/// plain JSON, so a `RecordView` written as JSON used to read back as an `Array`.
#[test]
fn record_view_json_reads_back_as_a_record_view() -> Result<(), Box<dyn std::error::Error>> {
    use liquers_core::value::{DefaultValueSerializer, ValueInterface};
    use liquers_lib::value::{ExtValueInterface, Value};

    let bytes = three_row_view()?.as_bytes("json")?;
    let back = Value::deserialize_from_bytes(&bytes, "RecordView", "json")?;
    assert_eq!(back.identifier(), "RecordView");
    assert_eq!(back.as_record_view()?.len(), 3);
    Ok(())
}

/// AC-2: a manifest written as `yaml` or `json` used to read back as an `Object`.
#[test]
fn record_source_manifest_reads_back_as_a_record_source() -> Result<(), Box<dyn std::error::Error>>
{
    use liquers_core::value::{DefaultValueSerializer, ValueInterface};
    use liquers_lib::value::{ExtValueInterface, Value};
    use liquers_records::{ManifestSource, ManifestSpec, RecordSource};
    use std::sync::Arc;

    let source: Arc<dyn RecordSource> =
        Arc::new(ManifestSource::new(ManifestSpec::default(), None)?);
    let value = Value::from_record_source(source);
    for format in ["yaml", "json"] {
        let bytes = value.as_bytes(format)?;
        let back = Value::deserialize_from_bytes(&bytes, "RecordSource", format)?;
        assert_eq!(back.identifier(), "RecordSource", "RecordSource as {format}");
        back.as_record_source()?;
    }
    Ok(())
}

/// AC-3: `html` is write-only for `RecordView`. Reading it refuses instead of producing `Text`.
#[test]
fn record_view_html_refuses_instead_of_reading_as_text() -> Result<(), Box<dyn std::error::Error>> {
    use liquers_core::value::DefaultValueSerializer;
    use liquers_lib::value::Value;

    let bytes = three_row_view()?.as_bytes("html")?;
    assert!(
        Value::deserialize_from_bytes(&bytes, "RecordView", "html").is_err(),
        "a RecordView written as html must not read back as Text"
    );
    Ok(())
}
