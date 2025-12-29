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
use std::{
    borrow::Cow,
    collections::BTreeMap,
    convert::TryFrom,
    result::Result,
    sync::Arc,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SimpleValue {
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
        value: Vec<SimpleValue>,
    },
    Object {
        value: BTreeMap<String, SimpleValue>,
    },
    Bytes {
        value: Vec<u8>,
    },
    Metadata {
        value: MetadataRecord,
    },
    AssetInfo {
        value: Vec<AssetInfo>,
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
    }
}
impl Default for SimpleValue {
    fn default() -> Self {
        SimpleValue::None {}
    }
}


impl ValueInterface for SimpleValue {
    fn none() -> Self {
        SimpleValue::None {}
    }
    fn is_none(&self) -> bool {
        if let SimpleValue::None {} = self {
            true
        } else {
            false
        }
    }

    fn new(txt: &str) -> Self {
        SimpleValue::Text {
            value: txt.to_owned(),
        }
    }

    fn try_into_string(&self) -> Result<String, Error> {
        match self {
            SimpleValue::None {} => Ok("None".to_owned()),
            SimpleValue::I32 { value: n } => Ok(format!("{n}")),
            SimpleValue::I64 { value: n } => Ok(format!("{n}")),
            SimpleValue::F64 { value: n } => Ok(format!("{n}")),
            SimpleValue::Text { value: t } => Ok(t.to_owned()),
            SimpleValue::Bytes { value: b } => Ok(String::from_utf8_lossy(b).to_string()),
            _ => Err(Error::conversion_error(self.identifier(), "string")),
        }
    }

    fn try_into_i32(&self) -> Result<i32, Error> {
        match self {
            SimpleValue::I32 { value: n } => Ok(*n),
            _ => Err(Error::conversion_error(self.identifier(), "i32")),
        }
    }

    fn try_into_json_value(&self) -> Result<serde_json::Value, Error> {
        match self {
            SimpleValue::None {} => Ok(serde_json::Value::Null),
            SimpleValue::Bool { value: b } => Ok(serde_json::Value::Bool(*b)),
            SimpleValue::I32 { value: n } => Ok(serde_json::Value::Number(serde_json::Number::from(*n))),
            SimpleValue::I64 { value: n } => Ok(serde_json::Value::Number(serde_json::Number::from(*n))),
            SimpleValue::F64 { value: n } => Ok(serde_json::Value::Number(
                serde_json::Number::from_f64(*n).unwrap(),
            )),
            SimpleValue::Text { value: t } => Ok(serde_json::Value::String(t.to_owned())),
            SimpleValue::Array { value: a } => {
                let mut v = Vec::new();
                for x in a {
                    v.push(x.try_into_json_value()?);
                }
                Ok(serde_json::Value::Array(v))
            }
            SimpleValue::Object { value: o } => {
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
            SimpleValue::None {} => "generic".into(),
            SimpleValue::Bool { value: _ } => "generic".into(),
            SimpleValue::I32 { value: _ } => "generic".into(),
            SimpleValue::I64 { value: _ } => "generic".into(),
            SimpleValue::F64 { value: _ } => "generic".into(),
            SimpleValue::Text { value: _ } => "text".into(),
            SimpleValue::Array { value: _ } => "generic".into(),
            SimpleValue::Object { value: _ } => "dictionary".into(),
            SimpleValue::Bytes { value: _ } => "bytes".into(),
            SimpleValue::Metadata { value: _ } => "metadata".into(),
            SimpleValue::AssetInfo { value: _ } => "asset_info".into(),
            SimpleValue::Recipe { value: _ } => "recipe".into(),
            SimpleValue::CommandMetadata { value: _ } => "command_metadata".into(),
            SimpleValue::Query { value: _ } => "query".into(),
            SimpleValue::Key { value: _ } => "key".into(),
        }
    }

    fn type_name(&self) -> Cow<'static, str> {
        match self {
            SimpleValue::None {} => "none".into(),
            SimpleValue::Bool { value: _ } => "bool".into(),
            SimpleValue::I32 { value: _ } => "i32".into(),
            SimpleValue::I64 { value: _ } => "i64".into(),
            SimpleValue::F64 { value: _ } => "f64".into(),
            SimpleValue::Text { value: _ } => "text".into(),
            SimpleValue::Array { value: _ } => "array".into(),
            SimpleValue::Object { value: _ } => "object".into(),
            SimpleValue::Bytes { value: _ } => "bytes".into(),
            SimpleValue::Metadata { value: _ } => "metadata".into(),
            SimpleValue::AssetInfo { value: _ } => "asset_info".into(),
            SimpleValue::Recipe { value: _ } => "recipe".into(),
            SimpleValue::CommandMetadata { value: _ } => "command_metadata".into(),
            SimpleValue::Query { value: _ } => "query".into(),
            SimpleValue::Key { value: _ } => "key".into(),
        }
    }

    fn default_extension(&self) -> Cow<'static, str> {
        match self {
            SimpleValue::None {} => "json".into(),
            SimpleValue::Bool { value: _ } => "json".into(),
            SimpleValue::I32 { value: _ } => "json".into(),
            SimpleValue::I64 { value: _ } => "json".into(),
            SimpleValue::F64 { value: _ } => "json".into(),
            SimpleValue::Text { value: _ } => "txt".into(),
            SimpleValue::Array { value: _ } => "json".into(),
            SimpleValue::Object { value: _ } => "json".into(),
            SimpleValue::Bytes { value: _ } => "b".into(),
            SimpleValue::Metadata { value: _ } => "json".into(),
            SimpleValue::AssetInfo { value: _ } => "json".into(),
            SimpleValue::Recipe { value: _ } => "json".into(),
            SimpleValue::CommandMetadata { value: _ } => "json".into(),
            SimpleValue::Query { value: _ } => "txt".into(),
            SimpleValue::Key { value: _ } => "txt".into(),
        }
    }

    fn default_filename(&self) -> Cow<'static, str> {
        match self {
            SimpleValue::None {} => "data.json".into(),
            SimpleValue::Bool { value: _ } => "data.json".into(),
            SimpleValue::I32 { value: _ } => "data.json".into(),
            SimpleValue::I64 { value: _ } => "data.json".into(),
            SimpleValue::F64 { value: _ } => "data.json".into(),
            SimpleValue::Text { value: _ } => "text.txt".into(),
            SimpleValue::Array { value: _ } => "data.json".into(),
            SimpleValue::Object { value: _ } => "data.json".into(),
            SimpleValue::Bytes { value: _ } => "binary.b".into(),
            SimpleValue::Metadata { value: _ } => "metadata.json".into(),
            SimpleValue::AssetInfo { value: _ } => "asset_info.json".into(),
            SimpleValue::Recipe { value: _ } => "recipe.json".into(),
            SimpleValue::CommandMetadata { value: _ } => "command_metadata.json".into(),
            SimpleValue::Query { value: _ } => "query.txt".into(),
            SimpleValue::Key { value: _ } => "key.txt".into(),
        }
    }

    fn default_media_type(&self) -> Cow<'static, str> {
        match self {
            SimpleValue::None {} => "application/json".into(),
            SimpleValue::Bool { value: _ } => "application/json".into(),
            SimpleValue::I32 { value: _ } => "application/json".into(),
            SimpleValue::I64 { value: _ } => "application/json".into(),
            SimpleValue::F64 { value: _ } => "application/json".into(),
            SimpleValue::Text { value: _ } => "text/plain".into(),
            SimpleValue::Array { value: _ } => "application/json".into(),
            SimpleValue::Object { value: _ } => "application/json".into(),
            SimpleValue::Bytes { value: _ } => "application/octet-stream".into(),
            SimpleValue::Metadata { value: _ } => "application/json".into(),
            SimpleValue::AssetInfo { value: _ } => "application/json".into(),
            SimpleValue::Recipe { value: _ } => "application/json".into(),
            SimpleValue::CommandMetadata { value: _ } => "application/json".into(),
            SimpleValue::Query { value: _ } => "text/plain".into(),
            SimpleValue::Key { value: _ } => "text/plain".into(),
        }
    }

    fn from_string(txt: String) -> Self {
        SimpleValue::Text { value: txt }
    }

    fn from_i32(n: i32) -> Self {
        SimpleValue::I32 { value: n }
    }

    fn from_i64(n: i64) -> Self {
        SimpleValue::I64 { value: n }
    }

    fn from_f64(n: f64) -> Self {
        SimpleValue::F64 { value: n }
    }

    fn from_bool(b: bool) -> Self {
        SimpleValue::Bool { value: b }
    }

    fn from_bytes(b: Vec<u8>) -> Self {
        SimpleValue::Bytes { value: b }
    }

    fn try_from_json_value(value: &serde_json::Value) -> Result<Self, Error> {
        match value {
            serde_json::Value::Null => Ok(SimpleValue::None {}),
            serde_json::Value::Bool(b) => Ok(SimpleValue::Bool { value: *b }),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Ok(SimpleValue::I64 { value: i })
                } else if let Some(f) = n.as_f64() {
                    Ok(SimpleValue::F64 { value: f })
                } else {
                    Err(Error::conversion_error_with_message(
                        value,
                        "i64 or f64",
                        "Invalid JSON number",
                    ))
                }
            }
            serde_json::Value::String(s) => Ok(SimpleValue::Text {
                value: s.to_owned(),
            }),
            serde_json::Value::Array(a) => {
                let mut v = Vec::new();
                for x in a {
                    v.push(SimpleValue::try_from_json_value(x)?);
                }
                Ok(SimpleValue::Array { value: v })
            }
            serde_json::Value::Object(o) => {
                let mut m = BTreeMap::new();
                for (k, v) in o {
                    m.insert(k.to_owned(), SimpleValue::try_from_json_value(v)?);
                }
                Ok(SimpleValue::Object { value: m })
            }
        }
    }

    fn try_into_i64(&self) -> Result<i64, Error> {
        match self {
            SimpleValue::I32 { value: n } => Ok(*n as i64),
            SimpleValue::I64 { value: n } => Ok(*n),
            _ => Err(Error::conversion_error(self.identifier(), "i64")),
        }
    }

    fn try_into_bool(&self) -> Result<bool, Error> {
        match self {
            SimpleValue::Bool { value: b } => Ok(*b),
            SimpleValue::I32 { value: n } => Ok(*n != 0),
            SimpleValue::I64 { value: n } => Ok(*n != 0),
            _ => Err(Error::conversion_error(self.identifier(), "bool")),
        }
    }

    fn try_into_f64(&self) -> Result<f64, Error> {
        match self {
            SimpleValue::I32 { value: n } => Ok(*n as f64),
            SimpleValue::I64 { value: n } => Ok(*n as f64),
            SimpleValue::F64 { value: n } => Ok(*n),
            _ => Err(Error::conversion_error(self.identifier(), "f64")),
        }
    }
    fn try_into_key(&self) -> Result<liquers_core::query::Key, Error> {
        match self {
            SimpleValue::Text { value } => Ok(liquers_core::parse::parse_key(value)?),
            SimpleValue::Query{ value: q } => q.key().ok_or(Error::conversion_error(self.identifier(), "key")),
            SimpleValue::Key{ value: k } => Ok(k.clone()),
            _ => Err(Error::conversion_error(self.identifier(), "key")),
        }
    }

    fn from_metadata(metadata: liquers_core::metadata::MetadataRecord) -> Self {
        SimpleValue::Metadata { value: metadata }
    }

    fn from_asset_info(asset_info: Vec<AssetInfo>) -> Self {
        SimpleValue::AssetInfo { value: asset_info }
    }

    fn from_recipe(recipe: liquers_core::recipes::Recipe) -> Self {
        SimpleValue::Recipe { value: recipe }
    }

    fn from_query(query: &liquers_core::query::Query) -> Self {
        SimpleValue::Query {
            value: query.clone(),
        }
    }

    fn from_key(key: &liquers_core::query::Key) -> Self {
        SimpleValue::Key { value: key.clone() }
    }
    
}

impl TryFrom<&SimpleValue> for i32 {
    type Error = Error;
    fn try_from(value: &SimpleValue) -> Result<Self, Self::Error> {
        match value {
            SimpleValue::I32 { value: x } => Ok(*x),
            SimpleValue::I64 { value: x } => i32::try_from(*x)
                .map_err(|e| Error::conversion_error_with_message("I64", "i32", &e.to_string())),
            _ => Err(Error::conversion_error(value.type_name(), "i32")),
        }
    }
}

impl TryFrom<SimpleValue> for i32 {
    type Error = Error;
    fn try_from(value: SimpleValue) -> Result<Self, Self::Error> {
        match value {
            SimpleValue::I32 { value: x } => Ok(x),
            SimpleValue::I64 { value: x } => i32::try_from(x)
                .map_err(|e| Error::conversion_error_with_message("I64", "i32", &e.to_string())),
            _ => Err(Error::conversion_error(value.type_name(), "i32")),
        }
    }
}

impl From<i32> for SimpleValue {
    fn from(value: i32) -> SimpleValue {
        SimpleValue::I32 { value }
    }
}

impl From<()> for SimpleValue {
    fn from(_value: ()) -> SimpleValue {
        SimpleValue::none()
    }
}

impl TryFrom<SimpleValue> for i64 {
    type Error = Error;
    fn try_from(value: SimpleValue) -> Result<Self, Self::Error> {
        match value {
            SimpleValue::I32 { value: x } => Ok(x as i64),
            SimpleValue::I64 { value: x } => Ok(x),
            _ => Err(Error::conversion_error(value.type_name(), "i64")),
        }
    }
}
impl From<i64> for SimpleValue {
    fn from(value: i64) -> SimpleValue {
        SimpleValue::I64 { value }
    }
}

impl TryFrom<SimpleValue> for f64 {
    type Error = Error;
    fn try_from(value: SimpleValue) -> Result<Self, Self::Error> {
        match value {
            SimpleValue::I32 { value: x } => Ok(x as f64),
            SimpleValue::I64 { value: x } => Ok(x as f64),
            SimpleValue::F64 { value: x } => Ok(x),
            _ => Err(Error::conversion_error(value.type_name(), "f64")),
        }
    }
}
impl From<f64> for SimpleValue {
    fn from(value: f64) -> SimpleValue {
        SimpleValue::F64 { value }
    }
}

impl TryFrom<SimpleValue> for f32 {
    type Error = Error;
    fn try_from(value: SimpleValue) -> Result<Self, Self::Error> {
        match value {
            SimpleValue::I32 { value: x } => Ok(x as f32),
            SimpleValue::I64 { value: x } => Ok(x as f32),
            SimpleValue::F64 { value: x } => Ok(x as f32),
            _ => Err(Error::conversion_error(value.type_name(), "f32")),
        }
    }
}
impl From<f32> for SimpleValue {
    fn from(value: f32) -> SimpleValue {
        SimpleValue::F64 {
            value: value as f64,
        }
    }
}

impl TryFrom<SimpleValue> for bool {
    type Error = Error;
    fn try_from(value: SimpleValue) -> Result<Self, Self::Error> {
        match value {
            SimpleValue::Bool { value: x } => Ok(x),
            SimpleValue::I32 { value: x } => Ok(x != 0),
            SimpleValue::I64 { value: x } => Ok(x != 0),
            _ => Err(Error::conversion_error(value.type_name(), "bool")),
        }
    }
}
impl From<bool> for SimpleValue {
    fn from(value: bool) -> SimpleValue {
        SimpleValue::Bool { value }
    }
}

impl TryFrom<SimpleValue> for String {
    type Error = Error;
    fn try_from(value: SimpleValue) -> Result<Self, Self::Error> {
        match value {
            SimpleValue::Text { value: x } => Ok(x),
            SimpleValue::I32 { value: x } => Ok(format!("{}", x)),
            SimpleValue::I64 { value: x } => Ok(format!("{}", x)),
            SimpleValue::F64 { value: x } => Ok(format!("{}", x)),
            _ => Err(Error::conversion_error(value.type_name(), "string")),
        }
    }
}

impl From<String> for SimpleValue {
    fn from(value: String) -> SimpleValue {
        SimpleValue::Text { value }
    }
}
impl From<&str> for SimpleValue {
    fn from(value: &str) -> SimpleValue {
        SimpleValue::Text {
            value: value.to_owned(),
        }
    }
}

impl DefaultValueSerializer for SimpleValue {
    fn as_bytes(&self, format: &str) -> Result<Vec<u8>, Error> {
        match format {
            "txt" | "html" => match self {
                SimpleValue::None {} => Ok("none".as_bytes().to_vec()),
                SimpleValue::Bool { value: true } => Ok("true".as_bytes().to_vec()),
                SimpleValue::Bool { value: false } => Ok("false".as_bytes().to_vec()),
                SimpleValue::I32 { value: x } => Ok(format!("{x}").into_bytes()),
                SimpleValue::I64 { value: x } => Ok(format!("{x}").into_bytes()),
                SimpleValue::F64 { value: x } => Ok(format!("{x}").into_bytes()),
                SimpleValue::Text { value: x } => Ok(x.as_bytes().to_vec()),
                _ => Err(Error::new(
                    ErrorType::SerializationError,
                    format!(
                        "Serialization to {} not supported by {}",
                        format,
                        self.type_name()
                    ),
                )),
            },
            "json" => {
                match self {
                    SimpleValue::None {} =>{
                            serde_json::to_vec(&serde_json::Value::Null).map_err(|e| {
                                Error::new(
                                    ErrorType::SerializationError,
                                    format!("Failed to serialize to JSON: {}", e),
                                )
                            })
                        }
                    SimpleValue::Bool { value } => {
                        serde_json::to_vec(value).map_err(|e| {
                            Error::new(
                                ErrorType::SerializationError,
                                format!("Failed to serialize bool to JSON: {}", e),
                            )
                        })
                    },
                    SimpleValue::I32 { value } => {
                        serde_json::to_vec(value).map_err(|e| {
                            Error::new(
                                ErrorType::SerializationError,
                                format!("Failed to serialize i32 to JSON: {}", e),
                            )
                        })
                    },
                    SimpleValue::I64 { value } => {
                        serde_json::to_vec(value).map_err(|e| {
                            Error::new(
                                ErrorType::SerializationError,
                                format!("Failed to serialize i64 to JSON: {}", e),
                            )
                        })
                    },
                    SimpleValue::F64 { value } => {
                        serde_json::to_vec(value).map_err(|e| {
                            Error::new(
                                ErrorType::SerializationError,
                                format!("Failed to serialize f64 to JSON: {}", e),
                            )
                        })
                    },
                    SimpleValue::Text { value } => {
                        serde_json::to_vec(value).map_err(|e| {
                            Error::new(
                                ErrorType::SerializationError,
                                format!("Failed to serialize text to JSON: {}", e),
                            )
                        })
                    },
                    SimpleValue::Metadata { value } => {
                        serde_json::to_vec(value).map_err(|e| {
                            Error::new(
                                ErrorType::SerializationError,
                                format!("Failed to serialize metadata to JSON: {}", e),
                            )
                        })
                    },
                    SimpleValue::AssetInfo { value } => {
                        serde_json::to_vec(value).map_err(|e| {
                            Error::new(
                                ErrorType::SerializationError,
                                format!("Failed to serialize asset info to JSON: {}", e),
                            )
                        })
                    },
                    SimpleValue::Recipe { value } => {
                        serde_json::to_vec(value).map_err(|e| {
                            Error::new(
                                ErrorType::SerializationError,
                                format!("Failed to serialize recipe to JSON: {}", e),
                            )
                        })
                    },
                    SimpleValue::CommandMetadata { value } => {
                        serde_json::to_vec(value).map_err(|e| {
                            Error::new(
                                ErrorType::SerializationError,
                                format!("Failed to serialize command metadata to JSON: {}", e),
                            )
                        })
                    },
                    _ => {
                        serde_json::to_vec(&self).map_err(|e| {
                            Error::new(
                                ErrorType::SerializationError,
                                format!("Failed to serialize to JSON: {}", e),
                            )
                        })
                    }
                }
            }
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
                Ok(SimpleValue::Text { value: s })
            }
            _ => Err(Error::new(
                ErrorType::SerializationError,
                format!("Unsupported format in from_bytes:{}", fmt),
            )),
        }
    }
}

