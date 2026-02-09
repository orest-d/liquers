
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
    UIElement {
        value: Arc<dyn crate::ui::element::UIElement>,
    },
}


pub trait ExtValueInterface {
    fn from_image(image: Arc<image::DynamicImage>) -> Self;
    fn as_image(&self) -> Result<Arc<image::DynamicImage>, Error>;
    fn from_polars_dataframe(df: polars::frame::DataFrame) -> Self;
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
            ExtValue::PolarsDataFrame { .. }
            | ExtValue::UiCommand { .. }
            | ExtValue::Widget { .. }
            | ExtValue::UIElement { .. } => {
                Err(Error::conversion_error(self.identifier().as_ref(), "Image"))
            }
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
            ExtValue::Image { .. }
            | ExtValue::UiCommand { .. }
            | ExtValue::Widget { .. }
            | ExtValue::UIElement { .. } => {
                Err(Error::conversion_error(self.identifier().as_ref(), "Polars dataframe"))
            }
        }
    }
    fn from_ui_element(element: Arc<dyn crate::ui::element::UIElement>) -> Self {
        ExtValue::UIElement { value: element }
    }
    fn as_ui_element(&self) -> Result<Arc<dyn crate::ui::element::UIElement>, Error> {
        match self {
            ExtValue::UIElement { value } => Ok(value.clone()),
            ExtValue::Image { .. }
            | ExtValue::PolarsDataFrame { .. }
            | ExtValue::UiCommand { .. }
            | ExtValue::Widget { .. } => {
                Err(Error::conversion_error(self.identifier().as_ref(), "UIElement"))
            }
        }
    }
}


impl ValueExtension for ExtValue {
    fn identifier(&self) -> Cow<'static, str> {
        match self {
            ExtValue::PolarsDataFrame { .. } => "polars_dataframe".into(),
            ExtValue::UiCommand { .. } => "ui_command".into(),
            ExtValue::Widget { .. } => "widget".into(),
            ExtValue::Image { .. } => "image".into(),
            ExtValue::UIElement { .. } => "ui_element".into(),
        }
    }

    fn type_name(&self) -> Cow<'static, str> {
        match self {
            ExtValue::PolarsDataFrame { .. } => "polars_dataframe".into(),
            ExtValue::UiCommand { .. } => "ui_command".into(),
            ExtValue::Widget { .. } => "widget".into(),
            ExtValue::Image { .. } => "image".into(),
            ExtValue::UIElement { .. } => "ui_element".into(),
        }
    }

    fn default_extension(&self) -> Cow<'static, str> {
        match self {
            ExtValue::PolarsDataFrame { .. } => "csv".into(),
            ExtValue::UiCommand { .. } => "ui".into(),
            ExtValue::Widget { .. } => "widget".into(),
            ExtValue::Image { .. } => "png".into(),
            ExtValue::UIElement { .. } => "ui".into(),
        }
    }

    fn default_filename(&self) -> Cow<'static, str> {
        match self {
            ExtValue::PolarsDataFrame { .. } => "data.csv".into(),
            ExtValue::UiCommand { .. } => "data.ui".into(),
            ExtValue::Widget { .. } => "data.widget".into(),
            ExtValue::Image { .. } => "image.png".into(),
            ExtValue::UIElement { .. } => "element.ui".into(),
        }
    }

    fn default_media_type(&self) -> Cow<'static, str> {
        match self {
            ExtValue::PolarsDataFrame { .. } => "text/csv".into(),
            ExtValue::UiCommand { .. } => "application/octet-stream".into(),
            ExtValue::Widget { .. } => "application/octet-stream".into(),
            ExtValue::Image { .. } => "image/png".into(),
            ExtValue::UIElement { .. } => "application/octet-stream".into(),
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
            Value::Base(_) => Err(Error::conversion_error(self.identifier().as_ref(), "Image")),
        }
    }
    fn from_polars_dataframe(df: polars::frame::DataFrame) -> Self {
        Value::Extended(ExtValue::from_polars_dataframe(df))
    }
    fn as_polars_dataframe(&self) -> Result<Arc<polars::frame::DataFrame>, Error> {
        match self {
            Value::Extended(ext) => ext.as_polars_dataframe(),
            Value::Base(_) => Err(Error::conversion_error(self.identifier().as_ref(), "Polars dataframe")),
        }
    }
    fn from_ui_element(element: Arc<dyn crate::ui::element::UIElement>) -> Self {
        Value::Extended(ExtValue::from_ui_element(element))
    }
    fn as_ui_element(&self) -> Result<Arc<dyn crate::ui::element::UIElement>, Error> {
        match self {
            Value::Extended(ext) => ext.as_ui_element(),
            Value::Base(_) => Err(Error::conversion_error(self.identifier().as_ref(), "UIElement")),
        }
    }
}