
use liquers_core::value::ValueInterface;
use liquers_core::{
    error::ErrorType,
    value::DefaultValueSerializer,
};

use liquers_core::error::Error;
use std::{
    borrow::Cow,
    result::Result,
    sync::Arc,
};

use crate::value::extended::*;
use crate::value::simple::*;

pub mod simple;
pub mod extended;

#[derive(Debug, Clone)]
pub enum ExtValue {
    Image {
        value: Arc<image::DynamicImage>,
    },
    PolarsDataFrame {
        value: Arc<polars::frame::DataFrame>,
    },
    UiCommand {
        value: crate::egui::UiCommand,
    },
    Widget {
        value: Arc<std::sync::Mutex<dyn crate::egui::widgets::WidgetValue>>,
    },
}


pub trait ExtValueInterface {
    fn from_image(image: Arc<image::DynamicImage>) -> Self;
    fn as_image(&self) -> Result<Arc<image::DynamicImage>, Error>;
    fn from_polars_dataframe(df: polars::frame::DataFrame) -> Self;
    fn as_polars_dataframe(&self) -> Result<Arc<polars::frame::DataFrame>, Error>;
}

impl ExtValueInterface for ExtValue {
    fn from_image(image: Arc<image::DynamicImage>) -> Self {
        ExtValue::Image { value: image }
    }
    fn as_image(&self) -> Result<Arc<image::DynamicImage>, Error> {
        match self {
            ExtValue::Image { value } => Ok(value.clone()),
            _ => Err(Error::conversion_error(self.identifier().as_ref(), "Image")),
        }
    }
    fn from_polars_dataframe(df: polars::frame::DataFrame) -> Self {
        ExtValue::PolarsDataFrame {
            value: Arc::new(df),
        }
    }
    fn as_polars_dataframe(&self) -> Result<Arc<polars::frame::DataFrame>, Error> {
        match self {
            ExtValue::PolarsDataFrame { value } => Ok(value.clone()),
            _ => Err(Error::conversion_error(self.identifier().as_ref(), "Polars dataframe")
            ),
        }
    }
}


impl ValueExtension for ExtValue {
    fn identifier(&self) -> Cow<'static, str> {
        match self {
            ExtValue::PolarsDataFrame { value: _ } => "polars_dataframe".into(),
            ExtValue::UiCommand { value: _ } => "ui_command".into(),
            ExtValue::Widget { value: _ } => "widget".into(),
            ExtValue::Image { value: _} => "image".into(),
        }
    }

    fn type_name(&self) -> Cow<'static, str> {
        match self {
            ExtValue::PolarsDataFrame { value: _ } => "polars_dataframe".into(),
            ExtValue::UiCommand { value: _ } => "ui_command".into(),
            ExtValue::Widget { value: _ } => "widget".into(),
            ExtValue::Image { value: _} => "image".into(),
        }
    }

    fn default_extension(&self) -> Cow<'static, str> {
        match self {
            ExtValue::PolarsDataFrame { value: _ } => "csv".into(),
            ExtValue::UiCommand { value: _ } => "ui".into(),
            ExtValue::Widget { value: _ } => "widget".into(),
            ExtValue::Image { value: _ } => "png".into(),
        }
    }

    fn default_filename(&self) -> Cow<'static, str> {
        match self {
            ExtValue::PolarsDataFrame { value: _ } => "data.csv".into(),
            ExtValue::UiCommand { value: _ } => "data.ui".into(),
            ExtValue::Widget { value: _ } => "data.widget".into(),
            ExtValue::Image { value: _} => "image.png".into(),
        }
    }

    fn default_media_type(&self) -> Cow<'static, str> {
        match self {
            ExtValue::PolarsDataFrame { value: _ } => "text/csv".into(),
            ExtValue::UiCommand { value: _ } => "application/octet-stream".into(),
            ExtValue::Widget { value: _ } => "application/octet-stream".into(),
            ExtValue::Image { value: _ } => "image/png".into(),
        }
    }
}



impl DefaultValueSerializer for ExtValue {
    fn as_bytes(&self, format: &str) -> Result<Vec<u8>, Error> {
        match format {
            _ => Err(Error::new(
                ErrorType::SerializationError,
                format!("Unsupported format {}", format),
            )),
        }
    }
    fn deserialize_from_bytes(_b: &[u8], _type_identifier: &str, fmt: &str) -> Result<Self, Error> {
        match fmt {
            _ => Err(Error::new(
                ErrorType::SerializationError,
                format!("Unsupported format in from_bytes:{}", fmt),
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
            _ => Err(Error::conversion_error(self.identifier().as_ref(), "Image")),
        }
    }
    fn from_polars_dataframe(df: polars::frame::DataFrame) -> Self {
        Value::Extended(ExtValue::from_polars_dataframe(df))
    }
    fn as_polars_dataframe(&self) -> Result<Arc<polars::frame::DataFrame>, Error> {
        match self {
            Value::Extended(ext) => ext.as_polars_dataframe(),
            _ => Err(Error::conversion_error(self.identifier().as_ref(), "Polars dataframe")),
        }
    }
}