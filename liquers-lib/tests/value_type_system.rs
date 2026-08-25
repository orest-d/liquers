//! Tests for the value type system (`specs/design/value-type-system/`).
//!
//! `vts10.2` — `CombinedValue` must delegate *every* default to whichever side holds the value.
//! Regression test for `COMBINED-VALUE-DEFAULT-EXTENSION-NOT-DELEGATED`: `default_extension`
//! returned the constant `"ext"` for every extended value while its four siblings delegated, so an
//! extended value reported `default_filename() == "image.png"` and `default_extension() == "ext"`.
//! Since `ValueInterface::default_data_format` derives from `default_extension`, the *default data
//! format* of every extended value was `"ext"` — a format no serializer implements.

use liquers_core::value::ValueInterface;
use liquers_lib::value::{CombinedValue, ExtValue, ExtValueInterface, SimpleValue};
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
