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
