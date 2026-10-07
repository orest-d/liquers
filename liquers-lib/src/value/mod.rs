use liquers_core::value::ValueInterface;
use liquers_core::{error::ErrorType, value::DefaultValueSerializer};

use liquers_core::error::Error;
use std::{borrow::Cow, result::Result, sync::Arc};

use crate::image::serde::{deserialize_image_from_bytes, serialize_image_to_bytes};
#[cfg(feature = "polars")]
use crate::polars::serde::{deserialize_dataframe_from_reader, serialize_dataframe_to_writer};
#[cfg(feature = "records")]
use liquers_records::{
    read_table, write_table, FieldValue, ManifestSource, ManifestSpec, ReadOptions, ReadSchema,
    RecordSource, RecordView, TableFormat, WriteOptions,
};
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
    /// A finite table — a `RecordBatch` or any view over one. Shareable, cacheable; serialized
    /// by `write_table`, which takes `&dyn RecordView` directly (no materialization needed to
    /// serialize — only a source needs that). See
    /// `specs/design/record-streams/phase2-architecture.md` §"Value extension".
    #[cfg(feature = "records")]
    RecordView { value: Arc<dyn RecordView> },
    /// Something that can be asked, repeatedly, for a stream of views. Shareable, never
    /// consumed by use; serializable only as its manifest (`ManifestSource`'s byte form) — every
    /// other source has none and is refused on write. See phase2-architecture.md §"A source
    /// serializes only as its manifest".
    #[cfg(feature = "records")]
    RecordSource { value: Arc<dyn RecordSource> },
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
    #[cfg(feature = "records")]
    fn from_record_view(view: Arc<dyn RecordView>) -> Self;
    #[cfg(feature = "records")]
    fn as_record_view(&self) -> Result<Arc<dyn RecordView>, Error>;
    #[cfg(feature = "records")]
    fn from_record_source(source: Arc<dyn RecordSource>) -> Self;
    #[cfg(feature = "records")]
    fn as_record_source(&self) -> Result<Arc<dyn RecordSource>, Error>;
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
            #[cfg(feature = "records")]
            ExtValue::RecordView { .. } | ExtValue::RecordSource { .. } => {
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
            #[cfg(feature = "records")]
            ExtValue::RecordView { .. } | ExtValue::RecordSource { .. } => Err(
                Error::conversion_error(self.identifier().as_ref(), "Polars dataframe"),
            ),
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
            #[cfg(feature = "records")]
            ExtValue::RecordView { .. } | ExtValue::RecordSource { .. } => Err(
                Error::conversion_error(self.identifier().as_ref(), "UIElement"),
            ),
        }
    }
    #[cfg(feature = "records")]
    fn from_record_view(view: Arc<dyn RecordView>) -> Self {
        ExtValue::RecordView { value: view }
    }
    #[cfg(feature = "records")]
    fn as_record_view(&self) -> Result<Arc<dyn RecordView>, Error> {
        match self {
            ExtValue::RecordView { value } => Ok(value.clone()),
            ExtValue::Image { .. }
            | ExtValue::UIElement { .. }
            | ExtValue::Foreign { .. }
            | ExtValue::RecordSource { .. } => {
                Err(Error::conversion_error(self.identifier().as_ref(), "RecordView"))
            }
            #[cfg(feature = "polars")]
            ExtValue::PolarsDataFrame { .. } => {
                Err(Error::conversion_error(self.identifier().as_ref(), "RecordView"))
            }
            #[cfg(feature = "egui")]
            ExtValue::UiCommand { .. } | ExtValue::Widget { .. } => {
                Err(Error::conversion_error(self.identifier().as_ref(), "RecordView"))
            }
        }
    }
    #[cfg(feature = "records")]
    fn from_record_source(source: Arc<dyn RecordSource>) -> Self {
        ExtValue::RecordSource { value: source }
    }
    #[cfg(feature = "records")]
    fn as_record_source(&self) -> Result<Arc<dyn RecordSource>, Error> {
        match self {
            ExtValue::RecordSource { value } => Ok(value.clone()),
            ExtValue::Image { .. }
            | ExtValue::UIElement { .. }
            | ExtValue::Foreign { .. }
            | ExtValue::RecordView { .. } => {
                Err(Error::conversion_error(self.identifier().as_ref(), "RecordSource"))
            }
            #[cfg(feature = "polars")]
            ExtValue::PolarsDataFrame { .. } => {
                Err(Error::conversion_error(self.identifier().as_ref(), "RecordSource"))
            }
            #[cfg(feature = "egui")]
            ExtValue::UiCommand { .. } | ExtValue::Widget { .. } => {
                Err(Error::conversion_error(self.identifier().as_ref(), "RecordSource"))
            }
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

/// The refusal every scalar hook gave by default before `ExtValue` overrode them — kept explicit
/// here so a variant that does not support scalar reading still states so via its own match arm
/// rather than a catch-all (`CLAUDE.md`: explicit match arms, no `_ =>`).
fn ext_scalar_refusal(value: &ExtValue, target: &str) -> Error {
    Error::conversion_error(ValueExtension::identifier(value), target)
}

/// A human-readable name for a [`FieldValue`]'s shape, used only to name what a scalar
/// conversion refused — never to carry the value itself.
#[cfg(feature = "records")]
fn field_value_kind(cell: &FieldValue) -> &'static str {
    match cell {
        FieldValue::Null => "Null",
        FieldValue::Bool(_) => "Bool",
        FieldValue::Int(_) => "Int",
        FieldValue::UInt(_) => "UInt",
        FieldValue::Float(_) => "Float",
        FieldValue::Text(_) => "Text",
        FieldValue::Bytes(_) => "Bytes",
        FieldValue::Date(_) => "Date",
        FieldValue::Timestamp(_) => "Timestamp",
        FieldValue::Vector(_) => "Vector",
    }
}

/// A single-cell view's cell as the base value it reads as —
/// `specs/design/record-streams/phase2-architecture.md` §"A view as a value". The scalar
/// conversions are then the base value's own, so a scalar read from a table and one written in a
/// query cannot disagree (including the base value's lossy `i64 → f64`).
#[cfg(feature = "records")]
fn record_view_cell_as_base(cell: &FieldValue) -> Result<SimpleValue, Error> {
    Ok(match cell {
        FieldValue::Null => SimpleValue::None {},
        FieldValue::Bool(value) => SimpleValue::Bool { value: *value },
        FieldValue::Int(value) => SimpleValue::I64 { value: *value },
        FieldValue::UInt(value) => SimpleValue::I64 {
            value: i64::try_from(*value)
                .map_err(|_| Error::conversion_error(field_value_kind(cell), "i64"))?,
        },
        FieldValue::Float(value) => SimpleValue::F64 { value: *value },
        FieldValue::Text(value) => SimpleValue::Text { value: value.to_string() },
        FieldValue::Bytes(value) => SimpleValue::Bytes { value: value.to_vec() },
        // Core has no temporal variant: ISO-8601 text, as the table formats write it.
        FieldValue::Date(days) => {
            let date = chrono::NaiveDate::from_num_days_from_ce_opt(days.saturating_add(719_163))
                .ok_or_else(|| Error::conversion_error(field_value_kind(cell), "date"))?;
            SimpleValue::Text { value: date.format("%Y-%m-%d").to_string() }
        }
        FieldValue::Timestamp(micros) => {
            let at = chrono::DateTime::<chrono::Utc>::from_timestamp_micros(*micros)
                .ok_or_else(|| Error::conversion_error(field_value_kind(cell), "timestamp"))?;
            SimpleValue::Text {
                value: at.to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
            }
        }
        FieldValue::Vector(values) => SimpleValue::Array {
            value: values
                .iter()
                .map(|v| SimpleValue::F64 { value: f64::from(*v) })
                .collect(),
        },
    })
}

#[cfg(feature = "records")]
fn record_view_cell_i64(cell: &FieldValue) -> Result<i64, Error> {
    record_view_cell_as_base(cell)?.try_into_i64()
}

#[cfg(feature = "records")]
fn record_view_cell_i32(cell: &FieldValue) -> Result<i32, Error> {
    record_view_cell_as_base(cell)?.try_into_i32()
}

#[cfg(feature = "records")]
fn record_view_cell_f64(cell: &FieldValue) -> Result<f64, Error> {
    record_view_cell_as_base(cell)?.try_into_f64()
}

#[cfg(feature = "records")]
fn record_view_cell_bool(cell: &FieldValue) -> Result<bool, Error> {
    record_view_cell_as_base(cell)?.try_into_bool()
}

#[cfg(feature = "records")]
fn record_view_cell_string(cell: &FieldValue) -> Result<String, Error> {
    record_view_cell_as_base(cell)?.try_into_string()
}

/// Whether `view` has the shape `RecordView::single_cell` reads — one row, and exactly one
/// payload column or, with none, the `Id` column or a sole column. Mirrors `single_cell` exactly;
/// asked first so that `try_into_json_value` can tell "not a scalar shape" (an array of rows) from
/// a real failure.
#[cfg(feature = "records")]
fn record_view_is_single_cell(view: &dyn RecordView) -> bool {
    let schema = view.schema();
    let payload = schema.payload_fields().len();
    view.len() == 1
        && (payload == 1
            || (payload == 0 && (schema.id_field().is_some() || schema.fields.len() == 1)))
}

/// `_option` reading: a `Null` cell is the base `None`, which the base value answers as `None`.
#[cfg(feature = "records")]
fn record_view_cell_i64_option(cell: &FieldValue) -> Result<Option<i64>, Error> {
    record_view_cell_as_base(cell)?.try_into_i64_option()
}

#[cfg(feature = "records")]
fn record_view_cell_f64_option(cell: &FieldValue) -> Result<Option<f64>, Error> {
    record_view_cell_as_base(cell)?.try_into_f64_option()
}

#[cfg(feature = "records")]
fn record_view_cell_string_option(cell: &FieldValue) -> Result<Option<String>, Error> {
    record_view_cell_as_base(cell)?.try_into_string_option()
}

impl ValueExtension for ExtValue {
    /// Reads as its single cell would (`specs/design/record-streams/phase2-architecture.md`
    /// §"A view as a value"): the cell's base value's own string conversion.
    /// `RecordSource` refuses, like every other non-record variant.
    fn try_into_string(&self) -> Result<String, Error> {
        match self {
            ExtValue::Image { .. } | ExtValue::UIElement { .. } | ExtValue::Foreign { .. } => {
                Err(ext_scalar_refusal(self, "string"))
            }
            #[cfg(feature = "polars")]
            ExtValue::PolarsDataFrame { .. } => Err(ext_scalar_refusal(self, "string")),
            #[cfg(feature = "egui")]
            ExtValue::UiCommand { .. } | ExtValue::Widget { .. } => {
                Err(ext_scalar_refusal(self, "string"))
            }
            #[cfg(feature = "records")]
            ExtValue::RecordSource { .. } => Err(ext_scalar_refusal(self, "string")),
            #[cfg(feature = "records")]
            ExtValue::RecordView { value } => record_view_cell_string(&value.single_cell()?),
        }
    }

    /// A `Null` cell is `None`, as for [`Self::try_into_i64_option`]; any other cell reads as
    /// [`Self::try_into_string`] does. Every other variant has no "no value" reading, so it
    /// answers exactly as its `try_into_string` does, wrapped in `Some`.
    fn try_into_string_option(&self) -> Result<Option<String>, Error> {
        match self {
            ExtValue::Image { .. } | ExtValue::UIElement { .. } | ExtValue::Foreign { .. } => {
                self.try_into_string().map(Some)
            }
            #[cfg(feature = "polars")]
            ExtValue::PolarsDataFrame { .. } => self.try_into_string().map(Some),
            #[cfg(feature = "egui")]
            ExtValue::UiCommand { .. } | ExtValue::Widget { .. } => {
                self.try_into_string().map(Some)
            }
            #[cfg(feature = "records")]
            ExtValue::RecordSource { .. } => self.try_into_string().map(Some),
            #[cfg(feature = "records")]
            ExtValue::RecordView { value } => {
                record_view_cell_string_option(&value.single_cell()?)
            }
        }
    }

    fn try_into_i32(&self) -> Result<i32, Error> {
        match self {
            ExtValue::Image { .. } | ExtValue::UIElement { .. } | ExtValue::Foreign { .. } => {
                Err(ext_scalar_refusal(self, "i32"))
            }
            #[cfg(feature = "polars")]
            ExtValue::PolarsDataFrame { .. } => Err(ext_scalar_refusal(self, "i32")),
            #[cfg(feature = "egui")]
            ExtValue::UiCommand { .. } | ExtValue::Widget { .. } => {
                Err(ext_scalar_refusal(self, "i32"))
            }
            #[cfg(feature = "records")]
            ExtValue::RecordSource { .. } => Err(ext_scalar_refusal(self, "i32")),
            #[cfg(feature = "records")]
            ExtValue::RecordView { value } => record_view_cell_i32(&value.single_cell()?),
        }
    }

    fn try_into_i64(&self) -> Result<i64, Error> {
        match self {
            ExtValue::Image { .. } | ExtValue::UIElement { .. } | ExtValue::Foreign { .. } => {
                Err(ext_scalar_refusal(self, "i64"))
            }
            #[cfg(feature = "polars")]
            ExtValue::PolarsDataFrame { .. } => Err(ext_scalar_refusal(self, "i64")),
            #[cfg(feature = "egui")]
            ExtValue::UiCommand { .. } | ExtValue::Widget { .. } => {
                Err(ext_scalar_refusal(self, "i64"))
            }
            #[cfg(feature = "records")]
            ExtValue::RecordSource { .. } => Err(ext_scalar_refusal(self, "i64")),
            #[cfg(feature = "records")]
            ExtValue::RecordView { value } => record_view_cell_i64(&value.single_cell()?),
        }
    }

    fn try_into_i64_option(&self) -> Result<Option<i64>, Error> {
        match self {
            ExtValue::Image { .. } | ExtValue::UIElement { .. } | ExtValue::Foreign { .. } => {
                Err(ext_scalar_refusal(self, "i64"))
            }
            #[cfg(feature = "polars")]
            ExtValue::PolarsDataFrame { .. } => Err(ext_scalar_refusal(self, "i64")),
            #[cfg(feature = "egui")]
            ExtValue::UiCommand { .. } | ExtValue::Widget { .. } => {
                Err(ext_scalar_refusal(self, "i64"))
            }
            #[cfg(feature = "records")]
            ExtValue::RecordSource { .. } => Err(ext_scalar_refusal(self, "i64")),
            #[cfg(feature = "records")]
            ExtValue::RecordView { value } => {
                record_view_cell_i64_option(&value.single_cell()?)
            }
        }
    }

    fn try_into_f64(&self) -> Result<f64, Error> {
        match self {
            ExtValue::Image { .. } | ExtValue::UIElement { .. } | ExtValue::Foreign { .. } => {
                Err(ext_scalar_refusal(self, "f64"))
            }
            #[cfg(feature = "polars")]
            ExtValue::PolarsDataFrame { .. } => Err(ext_scalar_refusal(self, "f64")),
            #[cfg(feature = "egui")]
            ExtValue::UiCommand { .. } | ExtValue::Widget { .. } => {
                Err(ext_scalar_refusal(self, "f64"))
            }
            #[cfg(feature = "records")]
            ExtValue::RecordSource { .. } => Err(ext_scalar_refusal(self, "f64")),
            #[cfg(feature = "records")]
            ExtValue::RecordView { value } => record_view_cell_f64(&value.single_cell()?),
        }
    }

    fn try_into_f64_option(&self) -> Result<Option<f64>, Error> {
        match self {
            ExtValue::Image { .. } | ExtValue::UIElement { .. } | ExtValue::Foreign { .. } => {
                Err(ext_scalar_refusal(self, "f64"))
            }
            #[cfg(feature = "polars")]
            ExtValue::PolarsDataFrame { .. } => Err(ext_scalar_refusal(self, "f64")),
            #[cfg(feature = "egui")]
            ExtValue::UiCommand { .. } | ExtValue::Widget { .. } => {
                Err(ext_scalar_refusal(self, "f64"))
            }
            #[cfg(feature = "records")]
            ExtValue::RecordSource { .. } => Err(ext_scalar_refusal(self, "f64")),
            #[cfg(feature = "records")]
            ExtValue::RecordView { value } => {
                record_view_cell_f64_option(&value.single_cell()?)
            }
        }
    }

    fn try_into_bool(&self) -> Result<bool, Error> {
        match self {
            ExtValue::Image { .. } | ExtValue::UIElement { .. } | ExtValue::Foreign { .. } => {
                Err(ext_scalar_refusal(self, "bool"))
            }
            #[cfg(feature = "polars")]
            ExtValue::PolarsDataFrame { .. } => Err(ext_scalar_refusal(self, "bool")),
            #[cfg(feature = "egui")]
            ExtValue::UiCommand { .. } | ExtValue::Widget { .. } => {
                Err(ext_scalar_refusal(self, "bool"))
            }
            #[cfg(feature = "records")]
            ExtValue::RecordSource { .. } => Err(ext_scalar_refusal(self, "bool")),
            #[cfg(feature = "records")]
            ExtValue::RecordView { value } => record_view_cell_bool(&value.single_cell()?),
        }
    }

    /// phase2-architecture.md §"A view as a value": a single-cell view gives its cell's base
    /// value's JSON — the same scalar the other hooks read — and any other view the records-orient
    /// array of row objects, written by `liquers_records::to_json` rather than a second writer.
    /// This is what a link inside a `multiple` parameter binds through. `RecordSource` refuses:
    /// its rows need `materialize`.
    fn try_into_json_value(&self) -> Result<serde_json::Value, Error> {
        match self {
            ExtValue::Image { .. } | ExtValue::UIElement { .. } | ExtValue::Foreign { .. } => {
                Err(ext_scalar_refusal(self, "JSON"))
            }
            #[cfg(feature = "polars")]
            ExtValue::PolarsDataFrame { .. } => Err(ext_scalar_refusal(self, "JSON")),
            #[cfg(feature = "egui")]
            ExtValue::UiCommand { .. } | ExtValue::Widget { .. } => {
                Err(ext_scalar_refusal(self, "JSON"))
            }
            #[cfg(feature = "records")]
            ExtValue::RecordSource { .. } => Err(ext_scalar_refusal(self, "JSON")),
            #[cfg(feature = "records")]
            ExtValue::RecordView { value } => {
                if record_view_is_single_cell(value.as_ref()) {
                    record_view_cell_as_base(&value.single_cell()?)?.try_into_json_value()
                } else {
                    liquers_records::to_json(value.as_ref(), liquers_records::JsonOrient::Records)
                }
            }
        }
    }

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
        #[cfg(feature = "records")]
        {
            // Every format `write_table` can actually produce with the features this build
            // enables — `ipc`/`parquet` are `TableFormat` variants in every build
            // (phase2-architecture.md §"Table formats") but `write_table` refuses them without
            // their writers, so they are declared only when this crate's own `records-ipc` /
            // `records-parquet` feature has turned that writer on. See
            // `specs/design/record-streams/phase2-architecture.md` §"Feature-gating discipline".
            //
            // Every alias `TableFormat::from_data_format` accepts is declared too (`csv:comma`,
            // `csv:tab`, `jsonl`, `markdown`; `feather`, `arrow_ipc`, `arrow` with IPC): the
            // registry gates the write path, so an undeclared alias would be a format the codec
            // supports and the asset layer refuses.
            #[allow(unused_mut)] // only mutated when records-ipc / records-parquet is enabled
            let mut record_view_formats: Vec<&'static str> = vec![
                "csv", "csv:comma", "tsv", "csv:tab", "ndjson", "jsonl", "json", "md", "markdown",
                "html",
            ];
            #[cfg(feature = "records-ipc")]
            record_view_formats.extend(["ipc", "feather", "arrow_ipc", "arrow"]);
            #[cfg(feature = "records-parquet")]
            record_view_formats.push("parquet");
            descriptions.push(
                TypeInfo::new("RecordView")
                    .with_type_name("record_view")
                    .with_defaults("csv", "csv", "text/csv", "data.csv")
                    .with_data_formats(record_view_formats),
            );
            descriptions.push(
                // A `RecordSource`'s only byte form is its manifest (`ManifestSource`'s spec) —
                // every other source has none and `as_bytes` refuses it. See
                // phase2-architecture.md §"A source serializes only as its manifest".
                TypeInfo::new("RecordSource")
                    .with_type_name("record_source")
                    .with_defaults("yaml", "yaml", "application/yaml", "manifest.yaml")
                    .with_data_formats(["yaml", "json"]),
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
            #[cfg(feature = "records")]
            ExtValue::RecordView { .. } | ExtValue::RecordSource { .. } => {
                default_ext_type_info(self)
            }
        }
    }

    /// Type identifiers follow `specs/reference/VALUE_TYPE_SYSTEM.md`.
    ///
    /// `Image` and `UIElement` are **bare**: Liquers owns those concepts and commits to them as
    /// canonical, even though `Image`'s payload comes from the `image` crate — a bare name is
    /// about concept ownership, not code location. `polars.DataFrame` carries a provider because
    /// Liquers explicitly does *not* commit to a canonical dataframe: polars and pandas, eager and
    /// lazy, arrow. `egui.*` likewise names a backend rather than a Liquers concept. `RecordView`
    /// and `RecordSource` are bare too — Liquers owns both concepts (phase2-architecture.md
    /// §"Value extension": "the identifiers ... bare CamelCase — Liquers owns both concepts").
    fn identifier(&self) -> Cow<'static, str> {
        match self {
            #[cfg(feature = "polars")]
            ExtValue::PolarsDataFrame { .. } => "polars.DataFrame".into(),
            #[cfg(feature = "egui")]
            ExtValue::UiCommand { .. } => "egui.Command".into(),
            #[cfg(feature = "egui")]
            ExtValue::Widget { .. } => "egui.Widget".into(),
            #[cfg(feature = "records")]
            ExtValue::RecordView { .. } => "RecordView".into(),
            #[cfg(feature = "records")]
            ExtValue::RecordSource { .. } => "RecordSource".into(),
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
            #[cfg(feature = "records")]
            ExtValue::RecordView { .. } => "record_view".into(),
            #[cfg(feature = "records")]
            ExtValue::RecordSource { .. } => "record_source".into(),
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
            #[cfg(feature = "records")]
            ExtValue::RecordView { .. } => "csv".into(),
            #[cfg(feature = "records")]
            ExtValue::RecordSource { .. } => "yaml".into(),
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
            #[cfg(feature = "records")]
            ExtValue::RecordView { .. } => "data.csv".into(),
            #[cfg(feature = "records")]
            ExtValue::RecordSource { .. } => "manifest.yaml".into(),
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
            #[cfg(feature = "records")]
            ExtValue::RecordView { .. } => "text/csv".into(),
            #[cfg(feature = "records")]
            ExtValue::RecordSource { .. } => "application/yaml".into(),
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
            // `write_table` takes `&dyn RecordView` directly, so a batch and any other view
            // serialize the same way — no `materialize()` call needed here.
            #[cfg(feature = "records")]
            ExtValue::RecordView { value } => {
                let table_format = TableFormat::from_data_format(format)?;
                write_table(value.as_ref(), table_format, &WriteOptions::default())
            }
            // A source's only byte form is its manifest (phase2-architecture.md §"A source
            // serializes only as its manifest"). Every other source — in memory, filtering,
            // mapping — has none and is refused here, with a typed `SerializationError` pointing
            // at `materialize` rather than `Error::new`, as this arm's `UIElement` neighbour does.
            #[cfg(feature = "records")]
            ExtValue::RecordSource { value } => match value.manifest() {
                Some(manifest) => match format {
                    "yaml" => serde_yaml::to_string(manifest)
                        .map(String::into_bytes)
                        .map_err(|e| Error::from_error(ErrorType::SerializationError, e)),
                    "json" => serde_json::to_vec(manifest)
                        .map_err(|e| Error::from_error(ErrorType::SerializationError, e)),
                    other => Err(Error::from_error(
                        ErrorType::SerializationError,
                        format!(
                            "RecordSource: unsupported manifest format '{}'; use 'yaml' or 'json'",
                            other
                        ),
                    )),
                },
                None => Err(Error::from_error(
                    ErrorType::SerializationError,
                    "RecordSource has no manifest and cannot be serialized; materialize it \
                     (ns-rec/materialize) to get its rows as bytes"
                        .to_string(),
                )),
            },
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
            // A `RecordView` identifier always deserializes to a `RecordBatch` — the reference
            // implementation of the trait — regardless of which view wrote the bytes
            // (phase2-architecture.md §"Value extension": "Deserialization rebuilds the
            // reference implementation"). No schema is available here, so this is the
            // schema-less reader (`ReadSchema::Infer`). Parquet is the one format `read_table`
            // itself refuses (`liquers-records` never reads Parquet — Tier 3): it goes through
            // `crate::records::read_parquet_record_batch` instead, which is the polars bridge
            // when the `polars` feature is on and a typed refusal naming it otherwise. Matched
            // explicitly, not `if let ... else`, so `TableFormat` stays a Liquers-owned enum with
            // no default arm here either (CLAUDE.md's "Match Statements").
            #[cfg(feature = "records")]
            "RecordView" => {
                let table_format = TableFormat::from_data_format(fmt)?;
                let batch = match table_format {
                    TableFormat::Parquet => crate::records::read_parquet_record_batch(b, ReadSchema::Infer)?,
                    TableFormat::Csv { separator } => {
                        read_table(b, TableFormat::Csv { separator }, ReadSchema::Infer, &ReadOptions::default())?
                    }
                    TableFormat::NdJson => {
                        read_table(b, TableFormat::NdJson, ReadSchema::Infer, &ReadOptions::default())?
                    }
                    TableFormat::Json => {
                        read_table(b, TableFormat::Json, ReadSchema::Infer, &ReadOptions::default())?
                    }
                    TableFormat::Markdown => {
                        read_table(b, TableFormat::Markdown, ReadSchema::Infer, &ReadOptions::default())?
                    }
                    TableFormat::Html => {
                        read_table(b, TableFormat::Html, ReadSchema::Infer, &ReadOptions::default())?
                    }
                    TableFormat::Ipc => {
                        read_table(b, TableFormat::Ipc, ReadSchema::Infer, &ReadOptions::default())?
                    }
                };
                Ok(ExtValue::from_record_view(Arc::new(batch)))
            }
            // The only byte form a `RecordSource` has is a manifest, which always deserializes
            // to a `ManifestSource` — keyless: the key is supplied later, once the value is read
            // back through a store, by `to_record_source` (phase2-architecture.md §"Getting a
            // source from a manifest file").
            #[cfg(feature = "records")]
            "RecordSource" => {
                let spec: ManifestSpec = match fmt {
                    "yaml" => serde_yaml::from_slice(b)
                        .map_err(|e| Error::from_error(ErrorType::SerializationError, e))?,
                    "json" => serde_json::from_slice(b)
                        .map_err(|e| Error::from_error(ErrorType::SerializationError, e))?,
                    other => {
                        return Err(Error::from_error(
                            ErrorType::SerializationError,
                            format!(
                                "RecordSource: unsupported manifest format '{}'; use 'yaml' or \
                                 'json'",
                                other
                            ),
                        ))
                    }
                };
                let source = ManifestSource::new(spec, None)?;
                Ok(ExtValue::from_record_source(Arc::new(source)))
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
    #[cfg(feature = "records")]
    fn from_record_view(view: Arc<dyn RecordView>) -> Self {
        Value::Extended(ExtValue::from_record_view(view))
    }
    #[cfg(feature = "records")]
    fn as_record_view(&self) -> Result<Arc<dyn RecordView>, Error> {
        match self {
            Value::Extended(ext) => ext.as_record_view(),
            Value::Base(_) => Err(Error::conversion_error(
                self.identifier().as_ref(),
                "RecordView",
            )),
        }
    }
    #[cfg(feature = "records")]
    fn from_record_source(source: Arc<dyn RecordSource>) -> Self {
        Value::Extended(ExtValue::from_record_source(source))
    }
    #[cfg(feature = "records")]
    fn as_record_source(&self) -> Result<Arc<dyn RecordSource>, Error> {
        match self {
            Value::Extended(ext) => ext.as_record_source(),
            Value::Base(_) => Err(Error::conversion_error(
                self.identifier().as_ref(),
                "RecordSource",
            )),
        }
    }
}
