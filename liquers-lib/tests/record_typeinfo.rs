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
