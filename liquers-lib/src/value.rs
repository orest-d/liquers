use egui::RichText;
use serde::{Deserialize, Serialize};
use serde_json;

use liquers_core::{
    command_metadata::CommandMetadata,
    error::ErrorType,
    metadata::{AssetInfo, MetadataRecord},
    query::{Key, Query},
    recipes::Recipe,
    value::{DefaultValueSerializer, ValueInterface},
};

use liquers_core::error::Error;
use polars::prelude::DataFrame;
use std::{
    borrow::Cow,
    collections::BTreeMap,
    convert::TryFrom,
    result::Result,
    sync::Arc,
};

#[derive(Debug, Clone)]
pub enum Value {
    None {},
    Bool {
        value: bool,
    },
    I32 {
        value: i32,
    },
    I64 {
        value: i64,
    },
    F64 {
        value: f64,
    },
    Text {
        value: String,
    },
    Array {
        value: Vec<Value>,
    },
    Object {
        value: BTreeMap<String, Value>,
    },
    Bytes {
        value: Vec<u8>,
    },
    Metadata {
        value: MetadataRecord,
    },
    AssetInfo {
        value: AssetInfo,
    },
    Recipe {
        value: Recipe,
    },
    CommandMetadata {
        value: CommandMetadata,
    },
    Query {
        value: Query,
    },
    Key {
        value: Key,
    },
    Image {
        value: RasterImage,
    },
    PolarsDataFrame {
        value: Arc<DataFrame>,
    },
    UiCommand {
        value: UiCommand,
    },
    Widget {
        value: Arc<std::sync::Mutex<dyn WidgetValue>>,
    },
}
impl Default for Value {
    fn default() -> Self {
        Value::None {}
    }
}

impl Value {
    pub fn from_image(image: RasterImage) -> Self {
        Value::Image { value: image }
    }
    pub fn from_ui<F>(f: F) -> Self
    where
        F: FnMut(&mut egui::Ui) -> Result<(), Error> + Send + 'static,
    {
        Value::UiCommand {
            value: UiCommand::new(f),
        }
    }
    pub fn from_widget(widget: Arc<std::sync::Mutex<dyn WidgetValue>>) -> Self {
        Value::Widget { value: widget }
    }
    pub fn show(&self, ui: &mut egui::Ui) {
        match self {
            Value::UiCommand { value } => {
                value.execute(ui);
            }
            Value::Widget { value } => {
                let mut widget = value.lock().expect("Failed to lock widget mutex");
                widget.show(ui);
            }
            Value::None {} => {
                ui.label(RichText::new("None").italics());
            }
            Value::Bool { value } => {
                ui.label(RichText::new(format!("Bool: {}", value)).italics());
            }
            Value::I32 { value } => {
                ui.label(RichText::new(format!("I32: {}", value)).italics());
            }
            Value::I64 { value } => {
                ui.label(RichText::new(format!("I64: {}", value)).italics());
            }
            Value::F64 { value } => {
                ui.label(RichText::new(format!("F64: {}", value)).italics());
            }
            Value::Text { value } => {
                ui.label(value);
            }
            Value::Array { value } => {
                ui.label(RichText::new("Array").italics());
            }
            Value::Object { value } => {
                ui.label(RichText::new("Object").italics());
            }
            Value::Bytes { value } => {
                ui.label(RichText::new(format!("Bytes: {} bytes", value.len())).italics());
            }
            Value::Metadata { value } => todo!(),
            Value::AssetInfo { value } => todo!(),
            Value::Recipe { value } => todo!(),
            Value::CommandMetadata { value } => todo!(),
            Value::Query { value } => todo!(),
            Value::Key { value } => todo!(),
            Value::Image { value } => todo!(),
            Value::PolarsDataFrame { value } => todo!(),
        }
    }
}

type UiClosure = Box<dyn FnMut(&mut egui::Ui) -> Result<(), Error> + Send>;
#[derive(Clone)]
pub struct UiCommand {
    value: Arc<std::sync::Mutex<UiClosure>>,
}

impl UiCommand {
    pub fn new<F>(f: F) -> Self
    where
        F: FnMut(&mut egui::Ui) -> Result<(), Error> + Send + 'static,
    {
        UiCommand {
            value: Arc::new(std::sync::Mutex::new(Box::new(f))),
        }
    }

    pub fn execute(&self, ui: &mut egui::Ui) {
        let mut closure = self.value.lock().unwrap();
        let res = (closure)(ui);
        if let Err(e) = res {
            let _ = display_error(ui, &e);
        }
    }
}

impl std::fmt::Debug for UiCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "UiCommand")
    }
}

impl ValueInterface for Value {
    fn none() -> Self {
        Value::None {}
    }
    fn is_none(&self) -> bool {
        if let Value::None {} = self {
            true
        } else {
            false
        }
    }

    fn new(txt: &str) -> Self {
        Value::Text {
            value: txt.to_owned(),
        }
    }

    fn try_into_string(&self) -> Result<String, Error> {
        match self {
            Value::None {} => Ok("None".to_owned()),
            Value::I32 { value: n } => Ok(format!("{n}")),
            Value::I64 { value: n } => Ok(format!("{n}")),
            Value::F64 { value: n } => Ok(format!("{n}")),
            Value::Text { value: t } => Ok(t.to_owned()),
            Value::Bytes { value: b } => Ok(String::from_utf8_lossy(b).to_string()),
            _ => Err(Error::conversion_error(self.identifier(), "string")),
        }
    }

    fn try_into_i32(&self) -> Result<i32, Error> {
        match self {
            Value::I32 { value: n } => Ok(*n),
            _ => Err(Error::conversion_error(self.identifier(), "i32")),
        }
    }

    fn try_into_json_value(&self) -> Result<serde_json::Value, Error> {
        match self {
            Value::None {} => Ok(serde_json::Value::Null),
            Value::Bool { value: b } => Ok(serde_json::Value::Bool(*b)),
            Value::I32 { value: n } => Ok(serde_json::Value::Number(serde_json::Number::from(*n))),
            Value::I64 { value: n } => Ok(serde_json::Value::Number(serde_json::Number::from(*n))),
            Value::F64 { value: n } => Ok(serde_json::Value::Number(
                serde_json::Number::from_f64(*n).unwrap(),
            )),
            Value::Text { value: t } => Ok(serde_json::Value::String(t.to_owned())),
            Value::Array { value: a } => {
                let mut v = Vec::new();
                for x in a {
                    v.push(x.try_into_json_value()?);
                }
                Ok(serde_json::Value::Array(v))
            }
            Value::Object { value: o } => {
                let mut m = serde_json::Map::new();
                for (k, v) in o {
                    m.insert(k.to_owned(), v.try_into_json_value()?);
                }
                Ok(serde_json::Value::Object(m))
            }
            _ => Err(Error::conversion_error(self.identifier(), "JSON value")),
        }
    }

    fn identifier(&self) -> Cow<'static, str> {
        match self {
            Value::None {} => "generic".into(),
            Value::Bool { value: _ } => "generic".into(),
            Value::I32 { value: _ } => "generic".into(),
            Value::I64 { value: _ } => "generic".into(),
            Value::F64 { value: _ } => "generic".into(),
            Value::Text { value: _ } => "text".into(),
            Value::Array { value: _ } => "generic".into(),
            Value::Object { value: _ } => "dictionary".into(),
            Value::Bytes { value: _ } => "bytes".into(),
            Value::Image { value: _ } => "image".into(),
            Value::Metadata { value: _ } => "metadata".into(),
            Value::AssetInfo { value: _ } => "asset_info".into(),
            Value::Recipe { value: _ } => "recipe".into(),
            Value::CommandMetadata { value: _ } => "command_metadata".into(),
            Value::Query { value: _ } => "query".into(),
            Value::Key { value: _ } => "key".into(),
            Value::PolarsDataFrame { value: _ } => "polars_dataframe".into(),
            Value::UiCommand { value: _ } => "ui_command".into(),
            Value::Widget { value: _ } => "widget".into(),
        }
    }

    fn type_name(&self) -> Cow<'static, str> {
        match self {
            Value::None {} => "none".into(),
            Value::Bool { value: _ } => "bool".into(),
            Value::I32 { value: _ } => "i32".into(),
            Value::I64 { value: _ } => "i64".into(),
            Value::F64 { value: _ } => "f64".into(),
            Value::Text { value: _ } => "text".into(),
            Value::Array { value: _ } => "array".into(),
            Value::Object { value: _ } => "object".into(),
            Value::Bytes { value: _ } => "bytes".into(),
            Value::Image { value: _ } => "image".into(),
            Value::Metadata { value: _ } => "metadata".into(),
            Value::AssetInfo { value: _ } => "asset_info".into(),
            Value::Recipe { value: _ } => "recipe".into(),
            Value::CommandMetadata { value: _ } => "command_metadata".into(),
            Value::Query { value: _ } => "query".into(),
            Value::Key { value: _ } => "key".into(),
            Value::PolarsDataFrame { value: _ } => "polars_dataframe".into(),
            Value::UiCommand { value: _ } => "ui_command".into(),
            Value::Widget { value: _ } => "widget".into(),
        }
    }

    fn default_extension(&self) -> Cow<'static, str> {
        match self {
            Value::None {} => "json".into(),
            Value::Bool { value: _ } => "json".into(),
            Value::I32 { value: _ } => "json".into(),
            Value::I64 { value: _ } => "json".into(),
            Value::F64 { value: _ } => "json".into(),
            Value::Text { value: _ } => "txt".into(),
            Value::Array { value: _ } => "json".into(),
            Value::Object { value: _ } => "json".into(),
            Value::Bytes { value: _ } => "b".into(),
            Value::Image { value: _ } => "png".into(),
            Value::Metadata { value: _ } => "json".into(),
            Value::AssetInfo { value: _ } => "json".into(),
            Value::Recipe { value: _ } => "json".into(),
            Value::CommandMetadata { value: _ } => "json".into(),
            Value::Query { value: _ } => "txt".into(),
            Value::Key { value: _ } => "txt".into(),
            Value::PolarsDataFrame { value: _ } => "csv".into(),
            Value::UiCommand { value: _ } => "ui".into(),
            Value::Widget { value: _ } => "widget".into(),
        }
    }

    fn default_filename(&self) -> Cow<'static, str> {
        match self {
            Value::None {} => "data.json".into(),
            Value::Bool { value: _ } => "data.json".into(),
            Value::I32 { value: _ } => "data.json".into(),
            Value::I64 { value: _ } => "data.json".into(),
            Value::F64 { value: _ } => "data.json".into(),
            Value::Text { value: _ } => "text.txt".into(),
            Value::Array { value: _ } => "data.json".into(),
            Value::Object { value: _ } => "data.json".into(),
            Value::Bytes { value: _ } => "binary.b".into(),
            Value::Image { value: _ } => "image.png".into(),
            Value::Metadata { value: _ } => "metadata.json".into(),
            Value::AssetInfo { value: _ } => "asset_info.json".into(),
            Value::Recipe { value: _ } => "recipe.json".into(),
            Value::CommandMetadata { value: _ } => "command_metadata.json".into(),
            Value::Query { value: _ } => "query.txt".into(),
            Value::Key { value: _ } => "key.txt".into(),
            Value::PolarsDataFrame { value: _ } => "data.csv".into(),
            Value::UiCommand { value: _ } => "data.ui".into(),
            Value::Widget { value: _ } => "data.widget".into(),
        }
    }

    fn default_media_type(&self) -> Cow<'static, str> {
        match self {
            Value::None {} => "application/json".into(),
            Value::Bool { value: _ } => "application/json".into(),
            Value::I32 { value: _ } => "application/json".into(),
            Value::I64 { value: _ } => "application/json".into(),
            Value::F64 { value: _ } => "application/json".into(),
            Value::Text { value: _ } => "text/plain".into(),
            Value::Array { value: _ } => "application/json".into(),
            Value::Object { value: _ } => "application/json".into(),
            Value::Bytes { value: _ } => "application/octet-stream".into(),
            Value::Image { value: _ } => "image/png".into(),
            Value::Metadata { value: _ } => "application/json".into(),
            Value::AssetInfo { value: _ } => "application/json".into(),
            Value::Recipe { value: _ } => "application/json".into(),
            Value::CommandMetadata { value: _ } => "application/json".into(),
            Value::Query { value: _ } => "text/plain".into(),
            Value::Key { value: _ } => "text/plain".into(),
            Value::PolarsDataFrame { value: _ } => "text/csv".into(),
            Value::UiCommand { value: _ } => "application/octet-stream".into(),
            Value::Widget { value: _ } => "application/octet-stream".into(),
        }
    }

    fn from_string(txt: String) -> Self {
        Value::Text { value: txt }
    }

    fn from_i32(n: i32) -> Self {
        Value::I32 { value: n }
    }

    fn from_i64(n: i64) -> Self {
        Value::I64 { value: n }
    }

    fn from_f64(n: f64) -> Self {
        Value::F64 { value: n }
    }

    fn from_bool(b: bool) -> Self {
        Value::Bool { value: b }
    }

    fn from_bytes(b: Vec<u8>) -> Self {
        Value::Bytes { value: b }
    }

    fn try_from_json_value(value: &serde_json::Value) -> Result<Self, Error> {
        match value {
            serde_json::Value::Null => Ok(Value::None {}),
            serde_json::Value::Bool(b) => Ok(Value::Bool { value: *b }),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Ok(Value::I64 { value: i })
                } else if let Some(f) = n.as_f64() {
                    Ok(Value::F64 { value: f })
                } else {
                    Err(Error::conversion_error_with_message(
                        value,
                        "i64 or f64",
                        "Invalid JSON number",
                    ))
                }
            }
            serde_json::Value::String(s) => Ok(Value::Text {
                value: s.to_owned(),
            }),
            serde_json::Value::Array(a) => {
                let mut v = Vec::new();
                for x in a {
                    v.push(Value::try_from_json_value(x)?);
                }
                Ok(Value::Array { value: v })
            }
            serde_json::Value::Object(o) => {
                let mut m = BTreeMap::new();
                for (k, v) in o {
                    m.insert(k.to_owned(), Value::try_from_json_value(v)?);
                }
                Ok(Value::Object { value: m })
            }
        }
    }

    fn try_into_i64(&self) -> Result<i64, Error> {
        match self {
            Value::I32 { value: n } => Ok(*n as i64),
            Value::I64 { value: n } => Ok(*n),
            _ => Err(Error::conversion_error(self.identifier(), "i64")),
        }
    }

    fn try_into_bool(&self) -> Result<bool, Error> {
        match self {
            Value::Bool { value: b } => Ok(*b),
            Value::I32 { value: n } => Ok(*n != 0),
            Value::I64 { value: n } => Ok(*n != 0),
            _ => Err(Error::conversion_error(self.identifier(), "bool")),
        }
    }

    fn try_into_f64(&self) -> Result<f64, Error> {
        match self {
            Value::I32 { value: n } => Ok(*n as f64),
            Value::I64 { value: n } => Ok(*n as f64),
            Value::F64 { value: n } => Ok(*n),
            _ => Err(Error::conversion_error(self.identifier(), "f64")),
        }
    }

    fn from_metadata(metadata: liquers_core::metadata::MetadataRecord) -> Self {
        Value::Metadata { value: metadata }
    }

    fn from_asset_info(asset_info: AssetInfo) -> Self {
        Value::AssetInfo { value: asset_info }
    }

    fn from_recipe(recipe: liquers_core::recipes::Recipe) -> Self {
        todo!("Implement from_recipe with correct type");
    }

    fn from_query(query: &liquers_core::query::Query) -> Self {
        Value::Query {
            value: query.clone(),
        }
    }

    fn from_key(key: &liquers_core::query::Key) -> Self {
        Value::Key { value: key.clone() }
    }
}

impl TryFrom<&Value> for i32 {
    type Error = Error;
    fn try_from(value: &Value) -> Result<Self, Self::Error> {
        match value {
            Value::I32 { value: x } => Ok(*x),
            Value::I64 { value: x } => i32::try_from(*x)
                .map_err(|e| Error::conversion_error_with_message("I64", "i32", &e.to_string())),
            _ => Err(Error::conversion_error(value.type_name(), "i32")),
        }
    }
}

impl TryFrom<Value> for i32 {
    type Error = Error;
    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::I32 { value: x } => Ok(x),
            Value::I64 { value: x } => i32::try_from(x)
                .map_err(|e| Error::conversion_error_with_message("I64", "i32", &e.to_string())),
            _ => Err(Error::conversion_error(value.type_name(), "i32")),
        }
    }
}

impl From<i32> for Value {
    fn from(value: i32) -> Value {
        Value::I32 { value }
    }
}

impl From<()> for Value {
    fn from(_value: ()) -> Value {
        Value::none()
    }
}

impl TryFrom<Value> for i64 {
    type Error = Error;
    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::I32 { value: x } => Ok(x as i64),
            Value::I64 { value: x } => Ok(x),
            _ => Err(Error::conversion_error(value.type_name(), "i64")),
        }
    }
}
impl From<i64> for Value {
    fn from(value: i64) -> Value {
        Value::I64 { value }
    }
}

impl TryFrom<Value> for f64 {
    type Error = Error;
    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::I32 { value: x } => Ok(x as f64),
            Value::I64 { value: x } => Ok(x as f64),
            Value::F64 { value: x } => Ok(x),
            _ => Err(Error::conversion_error(value.type_name(), "f64")),
        }
    }
}
impl From<f64> for Value {
    fn from(value: f64) -> Value {
        Value::F64 { value }
    }
}

impl TryFrom<Value> for f32 {
    type Error = Error;
    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::I32 { value: x } => Ok(x as f32),
            Value::I64 { value: x } => Ok(x as f32),
            Value::F64 { value: x } => Ok(x as f32),
            _ => Err(Error::conversion_error(value.type_name(), "f32")),
        }
    }
}
impl From<f32> for Value {
    fn from(value: f32) -> Value {
        Value::F64 {
            value: value as f64,
        }
    }
}

impl TryFrom<Value> for bool {
    type Error = Error;
    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::Bool { value: x } => Ok(x),
            Value::I32 { value: x } => Ok(x != 0),
            Value::I64 { value: x } => Ok(x != 0),
            _ => Err(Error::conversion_error(value.type_name(), "bool")),
        }
    }
}
impl From<bool> for Value {
    fn from(value: bool) -> Value {
        Value::Bool { value }
    }
}

impl TryFrom<Value> for String {
    type Error = Error;
    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::Text { value: x } => Ok(x),
            Value::I32 { value: x } => Ok(format!("{}", x)),
            Value::I64 { value: x } => Ok(format!("{}", x)),
            Value::F64 { value: x } => Ok(format!("{}", x)),
            _ => Err(Error::conversion_error(value.type_name(), "string")),
        }
    }
}

impl From<String> for Value {
    fn from(value: String) -> Value {
        Value::Text { value }
    }
}
impl From<&str> for Value {
    fn from(value: &str) -> Value {
        Value::Text {
            value: value.to_owned(),
        }
    }
}

impl DefaultValueSerializer for Value {
    fn as_bytes(&self, format: &str) -> Result<Vec<u8>, Error> {
        match format {
            "txt" | "html" => match self {
                Value::None {} => Ok("none".as_bytes().to_vec()),
                Value::Bool { value: true } => Ok("true".as_bytes().to_vec()),
                Value::Bool { value: false } => Ok("false".as_bytes().to_vec()),
                Value::I32 { value: x } => Ok(format!("{x}").into_bytes()),
                Value::I64 { value: x } => Ok(format!("{x}").into_bytes()),
                Value::F64 { value: x } => Ok(format!("{x}").into_bytes()),
                Value::Text { value: x } => Ok(x.as_bytes().to_vec()),
                _ => Err(Error::new(
                    ErrorType::SerializationError,
                    format!(
                        "Serialization to {} not supported by {}",
                        format,
                        self.type_name()
                    ),
                )),
            },
            _ => Err(Error::new(
                ErrorType::SerializationError,
                format!("Unsupported format {}", format),
            )),
        }
    }
    fn deserialize_from_bytes(b: &[u8], type_identifier: &str, fmt: &str) -> Result<Self, Error> {
        match fmt {
            "txt" | "html" | "toml" => {
                let s = String::from_utf8_lossy(b).to_string();
                Ok(Value::Text { value: s })
            }
            _ => Err(Error::new(
                ErrorType::SerializationError,
                format!("Unsupported format in from_bytes:{}", fmt),
            )),
        }
    }
}
use eframe::egui;
use image::{self, ImageEncoder};
use resvg::usvg::{Options, Tree};
use tiny_skia::Pixmap;
use usvg::Transform;

use crate::egui::widgets::{WidgetValue, display_error};

/// A simple RGBA raster image with f32 channels per pixel.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct RasterImage {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<(f32, f32, f32, f32)>, // (r, g, b, a) for each pixel
}

impl RasterImage {
    /// Load a RasterImage from a PNG file.
    pub fn from_png(path: &str) -> image::ImageResult<Self> {
        let img = image::open(path)?.to_rgba8();
        let (width, height) = img.dimensions();
        let pixels = img
            .pixels()
            .map(|p| {
                let [r, g, b, a] = p.0;
                (
                    r as f32 / 255.0,
                    g as f32 / 255.0,
                    b as f32 / 255.0,
                    a as f32 / 255.0,
                )
            })
            .collect();
        Ok(Self {
            width: width as usize,
            height: height as usize,
            pixels,
        })
    }

    /// Load a RasterImage from PNG bytes.
    pub fn from_png_bytes(bytes: &[u8]) -> image::ImageResult<Self> {
        let img = image::load_from_memory(bytes)?.to_rgba8();
        let (width, height) = img.dimensions();
        let pixels = img
            .pixels()
            .map(|p| {
                let [r, g, b, a] = p.0;
                (
                    r as f32 / 255.0,
                    g as f32 / 255.0,
                    b as f32 / 255.0,
                    a as f32 / 255.0,
                )
            })
            .collect();
        Ok(Self {
            width: width as usize,
            height: height as usize,
            pixels,
        })
    }

    /// Load a RasterImage from an SVG file, rendered at the given size.
    pub fn from_svg(path: &str, width: u32, height: u32) -> Result<Self, String> {
        // Read SVG data
        let svg_data = std::fs::read(path).map_err(|e| e.to_string())?;
        let opt = Options::default();
        let rtree = Tree::from_data(&svg_data, &opt).map_err(|e| format!("{:?}", e))?;

        // Render SVG to pixmap
        let mut pixmap = Pixmap::new(width, height).ok_or("Failed to create pixmap")?;
        resvg::render(&rtree, Transform::default(), &mut pixmap.as_mut());

        // Convert pixmap to RasterImage
        let mut pixels = Vec::with_capacity((width * height) as usize);
        for p in pixmap.pixels() {
            let r = p.red();
            let g = p.green();
            let b = p.blue();
            let a = p.alpha();
            pixels.push((
                r as f32 / 255.0,
                g as f32 / 255.0,
                b as f32 / 255.0,
                a as f32 / 255.0,
            ));
        }
        Ok(Self {
            width: width as usize,
            height: height as usize,
            pixels,
        })
    }

    /// Render the image in egui at the given zoom factor.
    pub fn show(&self, ui: &mut egui::Ui, id: egui::Id, zoom: f32) {
        // Convert to egui::ColorImage
        let mut rgba_u8: Vec<u8> = Vec::with_capacity(self.width * self.height * 4);
        for &(r, g, b, a) in &self.pixels {
            rgba_u8.push((r.clamp(0.0, 1.0) * 255.0) as u8);
            rgba_u8.push((g.clamp(0.0, 1.0) * 255.0) as u8);
            rgba_u8.push((b.clamp(0.0, 1.0) * 255.0) as u8);
            rgba_u8.push((a.clamp(0.0, 1.0) * 255.0) as u8);
        }
        let color_image =
            egui::ColorImage::from_rgba_unmultiplied([self.width, self.height], &rgba_u8);

        let texture = ui.ctx().load_texture(
            format!("raster_image_{:?}", id),
            color_image,
            Default::default(),
        );

        let size = egui::Vec2::new(self.width as f32 * zoom, self.height as f32 * zoom);
        let im = egui::Image::from_texture(&texture).fit_to_exact_size(size);
        ui.add(im);
    }

    /// Encode the RasterImage as PNG and return the bytes.
    pub fn to_png_bytes(&self) -> Result<Vec<u8>, image::ImageError> {
        use image::{codecs::png::PngEncoder, ColorType, Rgba, RgbaImage};
        let mut img = RgbaImage::new(self.width as u32, self.height as u32);
        for (i, &(r, g, b, a)) in self.pixels.iter().enumerate() {
            let x = (i % self.width) as u32;
            let y = (i / self.width) as u32;
            img.put_pixel(
                x,
                y,
                Rgba([
                    (r.clamp(0.0, 1.0) * 255.0) as u8,
                    (g.clamp(0.0, 1.0) * 255.0) as u8,
                    (b.clamp(0.0, 1.0) * 255.0) as u8,
                    (a.clamp(0.0, 1.0) * 255.0) as u8,
                ]),
            );
        }
        let mut buf = Vec::new();
        let encoder = PngEncoder::new(&mut buf);
        encoder.write_image(
            &img,
            self.width as u32,
            self.height as u32,
            image::ExtendedColorType::Rgba8,
        )?;
        Ok(buf)
    }

    /// Create a new empty RasterImage with the specified dimensions and fill color.
    /// The color can be any type that implements Into<(f32, f32, f32, f32)>.
    pub fn new_filled<T: Into<(f32, f32, f32, f32)>>(
        width: usize,
        height: usize,
        color: T,
    ) -> Self {
        let color = color.into();
        let pixels = vec![color; width * height];
        Self {
            width,
            height,
            pixels,
        }
    }
}

/// Parse a color string (name or RRGGBB[AA] hex value, without #) into (r, g, b, a) as f32 tuple.
/// Supports common color names and hex values like "ff0000" or "ff000080".
pub fn parse_color(s: &str) -> Option<(f32, f32, f32, f32)> {
    let s = s.trim().to_lowercase();
    // Named colors
    let named = match s.as_str() {
        "black" => (0.0, 0.0, 0.0, 1.0),
        "white" => (1.0, 1.0, 1.0, 1.0),
        "red" => (1.0, 0.0, 0.0, 1.0),
        "green" => (0.0, 1.0, 0.0, 1.0),
        "blue" => (0.0, 0.0, 1.0, 1.0),
        "yellow" => (1.0, 1.0, 0.0, 1.0),
        "cyan" => (0.0, 1.0, 1.0, 1.0),
        "magenta" => (1.0, 0.0, 1.0, 1.0),
        "gray" | "grey" => (0.5, 0.5, 0.5, 1.0),
        "orange" => (1.0, 0.65, 0.0, 1.0),
        "purple" => (0.5, 0.0, 0.5, 1.0),
        "brown" => (0.6, 0.4, 0.2, 1.0),
        "pink" => (1.0, 0.75, 0.8, 1.0),
        "lime" => (0.0, 1.0, 0.0, 1.0),
        "navy" => (0.0, 0.0, 0.5, 1.0),
        "teal" => (0.0, 0.5, 0.5, 1.0),
        "olive" => (0.5, 0.5, 0.0, 1.0),
        "maroon" => (0.5, 0.0, 0.0, 1.0),
        "silver" => (0.75, 0.75, 0.75, 1.0),
        _ => {
            // Try hex without #
            match s.len() {
                6 => {
                    // RRGGBB
                    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
                    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
                    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
                    return Some((r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0));
                }
                8 => {
                    // RRGGBBAA
                    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
                    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
                    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
                    let a = u8::from_str_radix(&s[6..8], 16).ok()?;
                    return Some((
                        r as f32 / 255.0,
                        g as f32 / 255.0,
                        b as f32 / 255.0,
                        a as f32 / 255.0,
                    ));
                }
                _ => return None,
            }
        }
    };
    Some(named)
}
