//! Tests for the value type system (`specs/design/value-type-system/`).
//!
//! `vts10.2` — `CombinedValue` must delegate *every* default to whichever side holds the value.
//! Regression test for `COMBINED-VALUE-DEFAULT-EXTENSION-NOT-DELEGATED`: `default_extension`
//! returned the constant `"ext"` for every extended value while its four siblings delegated, so an
//! extended value reported `default_filename() == "image.png"` and `default_extension() == "ext"`.
//! Since `ValueInterface::default_data_format` derives from `default_extension`, the *default data
//! format* of every extended value was `"ext"` — a format no serializer implements.

use liquers_core::type_system::TypeRegistry;
use liquers_core::value::ValueInterface;
use liquers_lib::value::{CombinedValue, ExtValue, ExtValueInterface, SimpleValue, ValueExtension};
use std::sync::Arc;

// `CombinedValue` requires `BaseValue: Default`, which `liquers_core::value::Value` does not
// implement; `SimpleValue` does (`value/simple.rs:62`) and is the intended base for a combined
// value type.
type Combined = CombinedValue<SimpleValue, ExtValue>;

fn image_value() -> Combined {
    let image = image::DynamicImage::new_rgb8(1, 1);
    Combined::new_extended(ExtValue::from_image(Arc::new(image)))
}

/// `vts10.2` — an extended value's defaults are the extension's own, not a placeholder.
#[test]
fn combined_value_delegates_all_defaults() {
    let value = image_value();

    assert_eq!(value.default_extension(), "png");
    assert_eq!(value.default_filename(), "image.png");
    assert_eq!(value.default_media_type(), "image/png");
    assert_eq!(value.identifier(), "Image");
}

/// The defaults must agree with each other: the filename ends in the extension, and the data
/// format derives from it. This is the invariant `"ext"` violated.
#[test]
fn combined_value_defaults_are_mutually_consistent() {
    let value = image_value();
    let extension = value.default_extension().to_string();
    let filename = value.default_filename().to_string();

    assert!(
        filename.ends_with(&format!(".{extension}")),
        "default_filename {filename:?} must end with default_extension {extension:?}"
    );
    assert_eq!(
        value.default_data_format(),
        extension,
        "default_data_format derives from default_extension"
    );
}

/// A widget with no behaviour, so the `egui.Widget` variant can be sampled.
#[cfg(feature = "egui")]
#[derive(Debug)]
struct SampleWidget;

#[cfg(feature = "egui")]
impl liquers_lib::egui::widgets::WidgetValue for SampleWidget {
    fn show(&mut self, ui: &mut egui::Ui) -> egui::Response {
        ui.label("")
    }
}

/// Whether `value`'s identifier must be in `ExtValue::type_descriptions()`.
///
/// Every `ExtValue` variant is named here, with no default arm: adding a variant fails to
/// compile until this match, and so `samples()`, is extended.
fn statically_described(value: &ExtValue) -> bool {
    match value {
        ExtValue::Image { .. } => true,
        #[cfg(feature = "polars")]
        ExtValue::PolarsDataFrame { .. } => true,
        #[cfg(feature = "egui")]
        ExtValue::UiCommand { .. } | ExtValue::Widget { .. } => true,
        ExtValue::UIElement { .. } => true,
        // The identifier belongs to the integrating crate, which registers it itself with
        // `EnvironmentBuilder::with_type_registry`; it is deliberately absent from the static list.
        ExtValue::Foreign { .. } => false,
        #[cfg(feature = "records")]
        ExtValue::RecordView { .. } | ExtValue::RecordSource { .. } => true,
    }
}

/// One value per statically described `ExtValue` variant compiled into this build.
fn samples() -> Vec<ExtValue> {
    #[allow(unused_mut)] // extended only with optional features
    let mut samples: Vec<ExtValue> = vec![
        ExtValue::from_image(Arc::new(image::DynamicImage::new_rgb8(1, 1))),
        ExtValue::from_ui_element(Arc::new(liquers_lib::ui::element::Placeholder::new())),
    ];
    #[cfg(feature = "polars")]
    samples.push(ExtValue::from_polars_dataframe(
        polars::frame::DataFrame::empty(),
    ));
    #[cfg(feature = "egui")]
    {
        samples.push(ExtValue::UiCommand {
            value: liquers_lib::egui::UiCommand::new(|_ui| Ok(())),
        });
        samples.push(ExtValue::Widget {
            value: Arc::new(std::sync::Mutex::new(SampleWidget)),
        });
    }
    #[cfg(feature = "records")]
    {
        use liquers_records::{
            Buffer, Column, FieldSchema, FieldType, InMemorySource, RecordBatch, RecordSchema,
            RecordView,
        };
        let schema = RecordSchema::new(vec![FieldSchema::new("n", FieldType::Int)])
            .expect("a one-field schema");
        let column = Column::Int {
            validity: None,
            values: Buffer::from_slice(&[] as &[i64]),
        };
        let batch = RecordBatch::new(Arc::new(schema), vec![column], None, None, vec![])
            .expect("an empty batch");
        let view: Arc<dyn RecordView> = Arc::new(batch);
        samples.push(ExtValue::from_record_view(view.clone()));
        samples.push(ExtValue::from_record_source(Arc::new(InMemorySource::new(
            vec![view],
        ))));
    }
    samples
}

fn described_identifiers() -> Vec<String> {
    <ExtValue as ValueExtension>::type_descriptions()
        .iter()
        .map(|info| info.type_identifier.to_string())
        .collect()
}

/// `vts10.1` — every `ExtValue` variant has a description, in every feature configuration.
///
/// This is the check for step 4 of the guide: a variant with no `TypeInfo` cannot be stored,
/// because the write path refuses an identifier the registry does not contain. Every variant is
/// sampled except `Foreign` (see `statically_described`).
#[test]
fn ext_value_type_descriptions_complete() {
    let described = described_identifiers();
    for value in &samples() {
        let identifier = ValueExtension::identifier(value).to_string();
        assert!(
            statically_described(value),
            "{identifier:?} is sampled but not expected to be described"
        );
        assert!(
            described.contains(&identifier),
            "variant {identifier:?} has no TypeInfo; it cannot be stored. Described: {described:?}"
        );
    }
}

/// Every description belongs to a sampled variant, so a `TypeInfo` left behind after its variant
/// is removed (or put under the wrong feature) fails here.
#[test]
fn ext_value_type_descriptions_have_no_stale_entries() {
    let sampled: Vec<String> = samples()
        .iter()
        .map(|value| ValueExtension::identifier(value).to_string())
        .collect();
    for identifier in described_identifiers() {
        assert!(
            sampled.contains(&identifier),
            "{identifier:?} is described but no sampled variant has it. Sampled: {sampled:?}"
        );
    }
}

/// The combined value type presents one flat identifier space: whether a variant lives in the base
/// value or the extension carries no type-system meaning.
#[test]
fn combined_registry_contains_both_sides() {
    let registry = TypeRegistry::from_value_type::<Combined>();
    assert!(registry.contains("Text"), "base value types are registered");
    assert!(registry.contains("Image"), "extension types are registered");
    assert!(
        !registry.contains("error"),
        "there is no error type: a failure is metadata, not something a value can be"
    );
}

/// AC-4 (`specs/design/combined-value-identifier-dispatch/`): an identifier the extension declares
/// never reads back as a base value, in any of its own formats or in the formats the base reads
/// whatever the identifier. The payload is a JSON array, which the base would accept as `txt`,
/// `json` and `yaml`. Also pins that the two identifier lists are disjoint, which the dispatch
/// relies on.
#[test]
fn extension_identifiers_never_read_as_base_values() {
    use liquers_core::value::DefaultValueSerializer;

    let base: Vec<String> = SimpleValue::type_descriptions()
        .iter()
        .map(|info| info.type_identifier.to_string())
        .collect();
    let payload = b"[{\"n\": 1}, {\"n\": 2}]";
    for info in <ExtValue as ValueExtension>::type_descriptions() {
        let identifier = info.type_identifier.to_string();
        assert!(
            !base.contains(&identifier),
            "{identifier} is declared by both the base and the extension"
        );
        let mut formats: Vec<String> = info
            .supported_data_formats
            .iter()
            .map(|format| format.to_string())
            .collect();
        formats.extend(["txt", "json", "yaml"].map(String::from));
        for format in formats {
            match Combined::deserialize_from_bytes(payload, &identifier, &format) {
                Ok(CombinedValue::Extended(ext)) => assert_eq!(
                    ValueExtension::identifier(&ext),
                    identifier,
                    "{identifier} as {format}"
                ),
                Ok(CombinedValue::Base(base_value)) => panic!(
                    "{identifier} as {format} read as the base value {}",
                    base_value.identifier()
                ),
                Err(_) => {}
            }
        }
    }
}
