use liquers_core::value::ValueInterface;
use liquers_core::{error::ErrorType, value::DefaultValueSerializer};

use liquers_core::error::Error;
use std::{borrow::Cow, result::Result, sync::Arc};

use crate::image::serde::{deserialize_image_from_bytes, serialize_image_to_bytes};
#[cfg(feature = "polars")]
use crate::polars::serde::{deserialize_dataframe_from_reader, serialize_dataframe_to_writer};
// Re-exported, not merely imported. A crate defining its own value type needs these three, and
// `liquers_lib::value::{CombinedValue, SimpleValue, ValueExtension}` is where it will look for
// them — reaching into `value::extended` and `value::simple` is an avoidable papercut on the
// documented extension path. Explicit rather than a glob, so the public surface of this module is
// visible here.
pub use crate::value::extended::{CombinedValue, ValueExtension};
pub use crate::value::simple::SimpleValue;
use std::io::Cursor;

pub mod extended;
pub mod foreign;
pub mod simple;

#[derive(Debug, Clone)]
pub enum ExtValue {
    Image {
        value: Arc<image::DynamicImage>,
    },
    #[cfg(feature = "polars")]
    PolarsDataFrame {
        value: Arc<polars::frame::DataFrame>,
    },
    #[cfg(feature = "egui")]
    UiCommand {
        value: crate::egui::UiCommand,
    },
    #[cfg(feature = "egui")]
    Widget {
        value: Arc<std::sync::Mutex<dyn crate::egui::widgets::WidgetValue>>,
    },
    UIElement {
        value: Arc<dyn crate::ui::element::UIElement>,
    },
    /// An opaque value belonging to an integrated language runtime (JavaScript, Starlark,
    /// Python). Deliberately one variant for all languages — see [`foreign::ForeignValue`].
    Foreign {
        value: Arc<dyn crate::value::foreign::ForeignValue>,
    },
}

pub trait ExtValueInterface {
    fn from_image(image: Arc<image::DynamicImage>) -> Self;
    fn as_image(&self) -> Result<Arc<image::DynamicImage>, Error>;
    #[cfg(feature = "polars")]
    fn from_polars_dataframe(df: polars::frame::DataFrame) -> Self;
    #[cfg(feature = "polars")]
    fn as_polars_dataframe(&self) -> Result<Arc<polars::frame::DataFrame>, Error>;
    fn from_ui_element(element: Arc<dyn crate::ui::element::UIElement>) -> Self;
    fn as_ui_element(&self) -> Result<Arc<dyn crate::ui::element::UIElement>, Error>;
}

impl ExtValueInterface for ExtValue {
    fn from_image(image: Arc<image::DynamicImage>) -> Self {
        ExtValue::Image { value: image }
    }
    fn as_image(&self) -> Result<Arc<image::DynamicImage>, Error> {
        match self {
            ExtValue::Image { value } => Ok(value.clone()),
            ExtValue::UIElement { .. } | ExtValue::Foreign { .. } => {
                Err(Error::conversion_error(self.identifier().as_ref(), "Image"))
            }
            #[cfg(feature = "polars")]
            ExtValue::PolarsDataFrame { .. } => {
                Err(Error::conversion_error(self.identifier().as_ref(), "Image"))
            }
            #[cfg(feature = "egui")]
            ExtValue::UiCommand { .. } | ExtValue::Widget { .. } => {
                Err(Error::conversion_error(self.identifier().as_ref(), "Image"))
            }
        }
    }
    #[cfg(feature = "polars")]
    fn from_polars_dataframe(df: polars::frame::DataFrame) -> Self {
        ExtValue::PolarsDataFrame {
            value: Arc::new(df),
        }
    }
    #[cfg(feature = "polars")]
    fn as_polars_dataframe(&self) -> Result<Arc<polars::frame::DataFrame>, Error> {
        match self {
            ExtValue::PolarsDataFrame { value } => Ok(value.clone()),
            ExtValue::Image { .. } | ExtValue::UIElement { .. } | ExtValue::Foreign { .. } => Err(
                Error::conversion_error(self.identifier().as_ref(), "Polars dataframe"),
            ),
            #[cfg(feature = "egui")]
            ExtValue::UiCommand { .. } | ExtValue::Widget { .. } => Err(Error::conversion_error(
                self.identifier().as_ref(),
                "Polars dataframe",
            )),
        }
    }
    fn from_ui_element(element: Arc<dyn crate::ui::element::UIElement>) -> Self {
        ExtValue::UIElement { value: element }
    }
    fn as_ui_element(&self) -> Result<Arc<dyn crate::ui::element::UIElement>, Error> {
        match self {
            ExtValue::UIElement { value } => Ok(value.clone()),
            ExtValue::Image { .. } | ExtValue::Foreign { .. } => Err(Error::conversion_error(
                self.identifier().as_ref(),
                "UIElement",
            )),
            #[cfg(feature = "polars")]
            ExtValue::PolarsDataFrame { .. } => Err(Error::conversion_error(
                self.identifier().as_ref(),
                "UIElement",
            )),
            #[cfg(feature = "egui")]
            ExtValue::UiCommand { .. } | ExtValue::Widget { .. } => Err(Error::conversion_error(
                self.identifier().as_ref(),
                "UIElement",
            )),
        }
    }
}

/// The description of a statically described `ExtValue` variant.
///
/// The same derivation `ValueExtension::type_info` provides by default. It lives here as a free
/// function because `ExtValue` *overrides* that method, and an override cannot call the default
/// body it replaced.
fn default_ext_type_info(value: &ExtValue) -> liquers_core::type_system::TypeInfo {
    let identifier = ValueExtension::identifier(value);
    <ExtValue as ValueExtension>::type_descriptions()
        .into_iter()
        .find(|info| info.type_identifier == identifier)
        .unwrap_or_else(|| {
            liquers_core::type_system::TypeInfo::new(identifier)
                .with_type_name(ValueExtension::type_name(value))
                .with_defaults(
                    ValueExtension::default_extension(value),
                    ValueExtension::default_extension(value),
                    ValueExtension::default_media_type(value),
                    ValueExtension::default_filename(value),
                )
        })
}

impl ValueExtension for ExtValue {
    fn type_descriptions() -> Vec<liquers_core::type_system::TypeInfo> {
        use liquers_core::type_system::TypeInfo;
        let mut descriptions = vec![
            // Every alias `parse_image_data_format` accepts (`image/serde.rs`). The registry gates
            // the write path, so an alias missing here is a format the codec supports and the
            // asset layer refuses.
            TypeInfo::new("Image")
                .with_type_name("image")
                .with_defaults("png", "png", "image/png", "image.png")
                .with_data_formats([
                    "png", "jpg", "jpeg", "jpe", "webp", "gif", "bmp", "tif", "tiff", "ico",
                    "dataurl",
                ]),
            TypeInfo::new("UIElement")
                .with_type_name("ui_element")
                .with_defaults("ui", "ui", "application/octet-stream", "element.ui"),
        ];
        #[cfg(feature = "polars")]
        descriptions.push(
            // Only what `serialize_dataframe_to_writer` actually implements. `json` and `ndjson`
            // parse into a `PolarsDataFormat` but both codecs return "not implemented yet"
            // (`polars/serde.rs`), so advertising them would be a false capability: `set_binary`
            // would accept bytes that cannot be materialized, and `set_state` would store metadata
            // with no data after serialization failed.
            TypeInfo::new("polars.DataFrame")
                .with_type_name("polars_dataframe")
                .with_defaults("csv", "csv", "text/csv", "data.csv")
                .with_data_formats(["csv", "csv:comma", "csv_comma", "parquet"]),
        );
        #[cfg(feature = "egui")]
        {
            descriptions.push(
                TypeInfo::new("egui.Command")
                    .with_type_name("ui_command")
                    .with_defaults("ui", "ui", "application/octet-stream", "data.ui"),
            );
            descriptions.push(
                TypeInfo::new("egui.Widget")
                    .with_type_name("widget")
                    .with_defaults(
                        "widget",
                        "widget",
                        "application/octet-stream",
                        "data.widget",
                    ),
            );
        }
        descriptions
    }

    /// Delegates the `Foreign` arm to the value itself; every other variant is described
    /// statically, so the inherited lookup is the right answer for it.
    ///
    /// A foreign value's identifier is not in `type_descriptions()` and cannot be — that list is
    /// static and the identifier belongs to an integration crate — so without this arm the
    /// inherited default would fall back to a derivation declaring no supported formats. Correct
    /// today, because `JsOpaque` genuinely serializes nothing; wrong the moment a foreign value
    /// can produce bytes.
    fn type_info(&self) -> liquers_core::type_system::TypeInfo {
        match self {
            ExtValue::Foreign { value } => value.type_info(),
            ExtValue::Image { .. } | ExtValue::UIElement { .. } => default_ext_type_info(self),
            #[cfg(feature = "polars")]
            ExtValue::PolarsDataFrame { .. } => default_ext_type_info(self),
            #[cfg(feature = "egui")]
            ExtValue::UiCommand { .. } | ExtValue::Widget { .. } => default_ext_type_info(self),
        }
    }

    /// Type identifiers follow `specs/reference/VALUE_TYPE_SYSTEM.md`.
    ///
    /// `Image` and `UIElement` are **bare**: Liquers owns those concepts and commits to them as
    /// canonical, even though `Image`'s payload comes from the `image` crate — a bare name is
    /// about concept ownership, not code location. `polars.DataFrame` carries a provider because
    /// Liquers explicitly does *not* commit to a canonical dataframe: polars and pandas, eager and
    /// lazy, arrow. `egui.*` likewise names a backend rather than a Liquers concept.
    fn identifier(&self) -> Cow<'static, str> {
        match self {
            #[cfg(feature = "polars")]
            ExtValue::PolarsDataFrame { .. } => "polars.DataFrame".into(),
            #[cfg(feature = "egui")]
            ExtValue::UiCommand { .. } => "egui.Command".into(),
            #[cfg(feature = "egui")]
            ExtValue::Widget { .. } => "egui.Widget".into(),
            ExtValue::Image { .. } => "Image".into(),
            ExtValue::UIElement { .. } => "UIElement".into(),
            ExtValue::Foreign { value } => value.identifier(),
        }
    }

    fn type_name(&self) -> Cow<'static, str> {
        match self {
            #[cfg(feature = "polars")]
            ExtValue::PolarsDataFrame { .. } => "polars_dataframe".into(),
            #[cfg(feature = "egui")]
            ExtValue::UiCommand { .. } => "ui_command".into(),
            #[cfg(feature = "egui")]
            ExtValue::Widget { .. } => "widget".into(),
            ExtValue::Image { .. } => "image".into(),
            ExtValue::UIElement { .. } => "ui_element".into(),
            ExtValue::Foreign { value } => value.type_name(),
        }
    }

    fn default_extension(&self) -> Cow<'static, str> {
        match self {
            #[cfg(feature = "polars")]
            ExtValue::PolarsDataFrame { .. } => "csv".into(),
            #[cfg(feature = "egui")]
            ExtValue::UiCommand { .. } => "ui".into(),
            #[cfg(feature = "egui")]
            ExtValue::Widget { .. } => "widget".into(),
            ExtValue::Image { .. } => "png".into(),
            ExtValue::UIElement { .. } => "ui".into(),
            ExtValue::Foreign { value } => value.default_extension(),
        }
    }

    fn default_filename(&self) -> Cow<'static, str> {
        match self {
            #[cfg(feature = "polars")]
            ExtValue::PolarsDataFrame { .. } => "data.csv".into(),
            #[cfg(feature = "egui")]
            ExtValue::UiCommand { .. } => "data.ui".into(),
            #[cfg(feature = "egui")]
            ExtValue::Widget { .. } => "data.widget".into(),
            ExtValue::Image { .. } => "image.png".into(),
            ExtValue::UIElement { .. } => "element.ui".into(),
            ExtValue::Foreign { value } => value.default_filename(),
        }
    }

    fn default_media_type(&self) -> Cow<'static, str> {
        match self {
            #[cfg(feature = "polars")]
            ExtValue::PolarsDataFrame { .. } => "text/csv".into(),
            #[cfg(feature = "egui")]
            ExtValue::UiCommand { .. } => "application/octet-stream".into(),
            #[cfg(feature = "egui")]
            ExtValue::Widget { .. } => "application/octet-stream".into(),
            ExtValue::Image { .. } => "image/png".into(),
            ExtValue::UIElement { .. } => "application/octet-stream".into(),
            ExtValue::Foreign { value } => value.default_media_type(),
        }
    }
}

impl DefaultValueSerializer for ExtValue {
    fn as_bytes(&self, format: &str) -> Result<Vec<u8>, Error> {
        match self {
            ExtValue::Image { value } => serialize_image_to_bytes(value, format),
            #[cfg(feature = "polars")]
            ExtValue::PolarsDataFrame { value } => {
                let mut bytes = Vec::new();
                serialize_dataframe_to_writer(value, format, &mut bytes)?;
                Ok(bytes)
            }
            ExtValue::Foreign { value } => value.as_bytes(format),
            // Enumerated rather than caught by `_ =>` so that adding a variant is a compile
            // error here too. The previous catch-all silently absorbed new variants, which made
            // this the one match on ExtValue the compiler could not police.
            ExtValue::UIElement { .. } => Err(Error::from_error(
                ErrorType::SerializationError,
                format!(
                    "Serialization to {} not supported by {}",
                    format,
                    self.type_name()
                ),
            )),
            #[cfg(feature = "egui")]
            ExtValue::UiCommand { .. } | ExtValue::Widget { .. } => Err(Error::from_error(
                ErrorType::SerializationError,
                format!(
                    "Serialization to {} not supported by {}",
                    format,
                    self.type_name()
                ),
            )),
        }
    }
    fn deserialize_from_bytes(b: &[u8], type_identifier: &str, fmt: &str) -> Result<Self, Error> {
        match type_identifier {
            "Image" => {
                let img = deserialize_image_from_bytes(b, fmt)?;
                Ok(ExtValue::from_image(Arc::new(img)))
            }
            #[cfg(feature = "polars")]
            "polars.DataFrame" => {
                let df = deserialize_dataframe_from_reader(Cursor::new(b), fmt)?;
                Ok(ExtValue::from_polars_dataframe(df))
            }
            _ => Err(Error::from_error(
                ErrorType::SerializationError,
                format!(
                    "Unsupported type identifier in from_bytes:{}",
                    type_identifier
                ),
            )),
        }
    }
}

pub type Value = CombinedValue<SimpleValue, ExtValue>;

impl From<SimpleValue> for Value {
    fn from(simple: SimpleValue) -> Self {
        Value::Base(simple)
    }
}

impl From<ExtValue> for Value {
    fn from(ext: ExtValue) -> Self {
        Value::Extended(ext)
    }
}

impl ExtValueInterface for Value {
    fn from_image(image: Arc<image::DynamicImage>) -> Self {
        Value::Extended(ExtValue::from_image(image))
    }
    fn as_image(&self) -> Result<Arc<image::DynamicImage>, Error> {
        match self {
            Value::Extended(ext) => ext.as_image(),
            Value::Base(_) => Err(Error::conversion_error(self.identifier().as_ref(), "Image")),
        }
    }
    #[cfg(feature = "polars")]
    fn from_polars_dataframe(df: polars::frame::DataFrame) -> Self {
        Value::Extended(ExtValue::from_polars_dataframe(df))
    }
    #[cfg(feature = "polars")]
    fn as_polars_dataframe(&self) -> Result<Arc<polars::frame::DataFrame>, Error> {
        match self {
            Value::Extended(ext) => ext.as_polars_dataframe(),
            Value::Base(_) => Err(Error::conversion_error(
                self.identifier().as_ref(),
                "Polars dataframe",
            )),
        }
    }
    fn from_ui_element(element: Arc<dyn crate::ui::element::UIElement>) -> Self {
        Value::Extended(ExtValue::from_ui_element(element))
    }
    fn as_ui_element(&self) -> Result<Arc<dyn crate::ui::element::UIElement>, Error> {
        match self {
            Value::Extended(ext) => ext.as_ui_element(),
            Value::Base(_) => Err(Error::conversion_error(
                self.identifier().as_ref(),
                "UIElement",
            )),
        }
    }
}
