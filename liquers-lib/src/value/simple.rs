use serde::{Deserialize, Serialize};
use serde_json;
use serde_yaml;

use liquers_core::{
    command_metadata::CommandMetadata,
    error::ErrorType,
    metadata::{AssetInfo, MetadataRecord},
    query::{Key, Query},
    recipes::Recipe,
    value::{DefaultValueSerializer, ValueInterface},
};

use liquers_core::error::Error;
use std::{borrow::Cow, collections::BTreeMap, convert::TryFrom, result::Result, sync::Arc};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    },
}
impl Default for SimpleValue {
    fn default() -> Self {
        SimpleValue::None {}
    }
}

impl ValueInterface for SimpleValue {
    fn try_into_query(&self) -> Result<Query, Error> {
        match self {
            SimpleValue::Query { value } => Ok(value.clone()),
            SimpleValue::Text { value: s } => liquers_core::parse::parse_query(s)
                .map_err(|e| Error::from_error(ErrorType::ParseError, e)),
            _ => Err(Error::conversion_error(self.identifier(), "Query")),
        }
    }
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
            SimpleValue::I32 { value: n } => {
                Ok(serde_json::Value::Number(serde_json::Number::from(*n)))
            }
            SimpleValue::I64 { value: n } => {
                Ok(serde_json::Value::Number(serde_json::Number::from(*n)))
            }
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

    fn from_array(values: Vec<Self>) -> Result<Self, Error> {
        Ok(SimpleValue::Array { value: values })
    }

    fn from_object(values: BTreeMap<String, Self>) -> Result<Self, Error> {
        Ok(SimpleValue::Object { value: values })
    }

    fn type_descriptions() -> Vec<liquers_core::type_system::TypeInfo> {
        // `SimpleValue` mirrors `liquers_core::value::Value` variant for variant, so it shares its
        // identifiers and descriptions rather than maintaining a second, drifting copy.
        liquers_core::value::Value::type_descriptions()
    }

    fn identifier(&self) -> Cow<'static, str> {
        // Bare CamelCase names, matching `liquers_core::value::Value` variant for variant.
        match self {
            SimpleValue::None {} => "None".into(),
            SimpleValue::Bool { value: _ } => "Bool".into(),
            SimpleValue::I32 { value: _ } => "I32".into(),
            SimpleValue::I64 { value: _ } => "I64".into(),
            SimpleValue::F64 { value: _ } => "F64".into(),
            SimpleValue::Text { value: _ } => "Text".into(),
            SimpleValue::Array { value: _ } => "Array".into(),
            SimpleValue::Object { value: _ } => "Object".into(),
            SimpleValue::Bytes { value: _ } => "Bytes".into(),
            SimpleValue::Metadata { value: _ } => "Metadata".into(),
            SimpleValue::AssetInfo { value: _ } => "AssetInfo".into(),
            SimpleValue::Recipe { value: _ } => "Recipe".into(),
            SimpleValue::CommandMetadata { value: _ } => "CommandMetadata".into(),
            SimpleValue::Query { value: _ } => "Query".into(),
            SimpleValue::Key { value: _ } => "Key".into(),
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
            SimpleValue::Query { value: q } => q
                .key()
                .ok_or(Error::conversion_error(self.identifier(), "key")),
            SimpleValue::Key { value: k } => Ok(k.clone()),
            _ => Err(Error::conversion_error(self.identifier(), "key")),
        }
    }

    fn try_into_command_metadata(&self) -> Result<CommandMetadata, Error> {
        match self {
            SimpleValue::CommandMetadata { value } => Ok(value.clone()),
            _ => Err(Error::conversion_error(
                self.identifier(),
                "command metadata",
            )),
        }
    }

    fn try_into_bytes(&self) -> Result<Vec<u8>, Error> {
        match self {
            SimpleValue::Bytes { value: b } => Ok(b.clone()),
            SimpleValue::Text { value: t } => Ok(t.as_bytes().to_vec()),
            _ => Err(Error::conversion_error(self.identifier(), "bytes")),
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

    fn from_command_metadata(command_metadata: CommandMetadata) -> Self {
        SimpleValue::CommandMetadata {
            value: command_metadata,
        }
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

impl From<Vec<i64>> for SimpleValue {
    fn from(value: Vec<i64>) -> SimpleValue {
        SimpleValue::Array {
            value: value.into_iter().map(SimpleValue::from).collect(),
        }
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

impl SimpleValue {
    /// A JSON document read by its shape: objects, arrays and scalars as `SimpleValue`'s own.
    fn from_plain_json(b: &[u8]) -> Result<Self, Error> {
        let json_value: serde_json::Value =
            serde_json::from_slice(b).map_err(|e| Error::from_error(ErrorType::ParseError, e))?;
        SimpleValue::try_from_json_value(&json_value)
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
            "json" => match self {
                SimpleValue::None {} => serde_json::to_vec(&serde_json::Value::Null).map_err(|e| {
                    Error::new(
                        ErrorType::SerializationError,
                        format!("Failed to serialize to JSON: {}", e),
                    )
                }),
                SimpleValue::Bool { value } => serde_json::to_vec(value).map_err(|e| {
                    Error::new(
                        ErrorType::SerializationError,
                        format!("Failed to serialize bool to JSON: {}", e),
                    )
                }),
                SimpleValue::I32 { value } => serde_json::to_vec(value).map_err(|e| {
                    Error::new(
                        ErrorType::SerializationError,
                        format!("Failed to serialize i32 to JSON: {}", e),
                    )
                }),
                SimpleValue::I64 { value } => serde_json::to_vec(value).map_err(|e| {
                    Error::new(
                        ErrorType::SerializationError,
                        format!("Failed to serialize i64 to JSON: {}", e),
                    )
                }),
                SimpleValue::F64 { value } => serde_json::to_vec(value).map_err(|e| {
                    Error::new(
                        ErrorType::SerializationError,
                        format!("Failed to serialize f64 to JSON: {}", e),
                    )
                }),
                SimpleValue::Text { value } => serde_json::to_vec(value).map_err(|e| {
                    Error::new(
                        ErrorType::SerializationError,
                        format!("Failed to serialize text to JSON: {}", e),
                    )
                }),
                SimpleValue::Metadata { value } => serde_json::to_vec(value).map_err(|e| {
                    Error::new(
                        ErrorType::SerializationError,
                        format!("Failed to serialize metadata to JSON: {}", e),
                    )
                }),
                SimpleValue::AssetInfo { value } => serde_json::to_vec(value).map_err(|e| {
                    Error::new(
                        ErrorType::SerializationError,
                        format!("Failed to serialize asset info to JSON: {}", e),
                    )
                }),
                SimpleValue::Recipe { value } => serde_json::to_vec(value).map_err(|e| {
                    Error::new(
                        ErrorType::SerializationError,
                        format!("Failed to serialize recipe to JSON: {}", e),
                    )
                }),
                SimpleValue::CommandMetadata { value } => serde_json::to_vec(value).map_err(|e| {
                    Error::new(
                        ErrorType::SerializationError,
                        format!("Failed to serialize command metadata to JSON: {}", e),
                    )
                }),
                _ => serde_json::to_vec(&self).map_err(|e| {
                    Error::new(
                        ErrorType::SerializationError,
                        format!("Failed to serialize to JSON: {}", e),
                    )
                }),
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
                Ok(SimpleValue::Text { value: s })
            }
            // JSON alone does not say which variant it came from, so the declared type identifier
            // is consulted first, as `liquers_core::value::Value` does. The structured variants
            // are read as their own types; the variants `as_bytes` writes in serde's tagged form
            // (`{"Array": {"value": …}}`) are read in that form, and otherwise as plain JSON, which
            // is what a hand-written file holds. Everything else is plain JSON.
            "json" => match type_identifier {
                "Metadata" => serde_json::from_slice::<MetadataRecord>(b)
                    .map(|value| SimpleValue::Metadata { value })
                    .map_err(|e| Error::from_error(ErrorType::ParseError, e)),
                "AssetInfo" => serde_json::from_slice::<Vec<AssetInfo>>(b)
                    .map(|value| SimpleValue::AssetInfo { value })
                    .map_err(|e| Error::from_error(ErrorType::ParseError, e)),
                "Recipe" => serde_json::from_slice::<Recipe>(b)
                    .map(|value| SimpleValue::Recipe { value })
                    .map_err(|e| Error::from_error(ErrorType::ParseError, e)),
                "CommandMetadata" => serde_json::from_slice::<CommandMetadata>(b)
                    .map(|value| SimpleValue::CommandMetadata { value })
                    .map_err(|e| Error::from_error(ErrorType::ParseError, e)),
                "Array" | "Object" | "Bytes" | "Query" | "Key" => {
                    match serde_json::from_slice::<SimpleValue>(b) {
                        Ok(value) => Ok(value),
                        Err(_) => Self::from_plain_json(b),
                    }
                }
                _ => Self::from_plain_json(b),
            },
            "yaml" | "yml" => {
                let json_value: serde_json::Value = serde_yaml::from_slice(b)
                    .map_err(|e| Error::from_error(ErrorType::ParseError, e))?;
                SimpleValue::try_from_json_value(&json_value)
            }
            _ => Err(Error::from_error(
                ErrorType::SerializationError,
                format!("Unsupported format in deserialize_from_bytes: {}", fmt),
            )),
        }
    }
}

impl TryFrom<SimpleValue> for u32 {
    type Error = Error;
    fn try_from(value: SimpleValue) -> Result<Self, Self::Error> {
        match value {
            SimpleValue::I32 { value: x } => u32::try_from(x)
                .map_err(|e| Error::conversion_error_with_message("I32", "u32", &e.to_string())),
            SimpleValue::I64 { value: x } => u32::try_from(x)
                .map_err(|e| Error::conversion_error_with_message("I64", "u32", &e.to_string())),
            _ => Err(Error::conversion_error(value.type_name(), "u32")),
        }
    }
}

impl TryFrom<SimpleValue> for u8 {
    type Error = Error;
    fn try_from(value: SimpleValue) -> Result<Self, Self::Error> {
        match value {
            SimpleValue::I32 { value: x } => u8::try_from(x)
                .map_err(|e| Error::conversion_error_with_message("I32", "u8", &e.to_string())),
            SimpleValue::I64 { value: x } => u8::try_from(x)
                .map_err(|e| Error::conversion_error_with_message("I64", "u8", &e.to_string())),
            _ => Err(Error::conversion_error(value.type_name(), "u8")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;
    use serde_yaml;

    #[test]
    fn test_json_roundtrip_bool() -> Result<(), Box<dyn std::error::Error>> {
        let value = SimpleValue::Bool { value: true };
        let bytes = value.as_bytes("json")?;
        let deserialized =
            SimpleValue::deserialize_from_bytes(&bytes, "Bool", "json")?;
        assert_eq!(value, deserialized);
        Ok(())
    }

    #[test]
    fn test_json_roundtrip_i32() -> Result<(), Box<dyn std::error::Error>> {
        let value = SimpleValue::I32 { value: 42 };
        let bytes = value.as_bytes("json")?;
        let deserialized =
            SimpleValue::deserialize_from_bytes(&bytes, "I32", "json")?;
        match deserialized {
            SimpleValue::I64 { value: 42 } => Ok(()),
            _ => Err("Expected I64 with value 42".into()),
        }
    }

    #[test]
    fn test_json_roundtrip_i64() -> Result<(), Box<dyn std::error::Error>> {
        let value = SimpleValue::I64 { value: 123456789 };
        let bytes = value.as_bytes("json")?;
        let deserialized =
            SimpleValue::deserialize_from_bytes(&bytes, "I64", "json")?;
        assert_eq!(value, deserialized);
        Ok(())
    }

    #[test]
    fn test_json_roundtrip_f64() -> Result<(), Box<dyn std::error::Error>> {
        let value = SimpleValue::F64 { value: 3.14159 };
        let bytes = value.as_bytes("json")?;
        let deserialized =
            SimpleValue::deserialize_from_bytes(&bytes, "F64", "json")?;
        assert_eq!(value, deserialized);
        Ok(())
    }

    #[test]
    fn test_json_roundtrip_text() -> Result<(), Box<dyn std::error::Error>> {
        let value = SimpleValue::Text {
            value: "hello world".to_string(),
        };
        let bytes = value.as_bytes("json")?;
        let deserialized =
            SimpleValue::deserialize_from_bytes(&bytes, "Text", "json")?;
        assert_eq!(value, deserialized);
        Ok(())
    }

    #[test]
    fn test_json_roundtrip_none() -> Result<(), Box<dyn std::error::Error>> {
        let value = SimpleValue::None {};
        let bytes = value.as_bytes("json")?;
        let deserialized =
            SimpleValue::deserialize_from_bytes(&bytes, "None", "json")?;
        assert_eq!(value, deserialized);
        Ok(())
    }

    #[test]
    fn test_json_roundtrip_array() -> Result<(), Box<dyn std::error::Error>> {
        let value = SimpleValue::Array {
            value: vec![
                SimpleValue::I64 { value: 1 },
                SimpleValue::I64 { value: 2 },
                SimpleValue::Text {
                    value: "three".to_string(),
                },
            ],
        };
        // Plain JSON, as a hand-written file holds it (as_bytes writes the tagged form).
        let json_value = value.try_into_json_value()?;
        let bytes = serde_json::to_vec(&json_value)?;
        let deserialized =
            SimpleValue::deserialize_from_bytes(&bytes, "Array", "json")?;
        assert_eq!(value, deserialized);
        Ok(())
    }

    #[test]
    fn test_json_roundtrip_object() -> Result<(), Box<dyn std::error::Error>> {
        let mut obj = BTreeMap::new();
        obj.insert("name".to_string(), SimpleValue::Text {
            value: "Alice".to_string(),
        });
        obj.insert("age".to_string(), SimpleValue::I64 { value: 30 });
        let value = SimpleValue::Object { value: obj };

        // Plain JSON, as a hand-written file holds it (as_bytes writes the tagged form).
        let json_value = value.try_into_json_value()?;
        let bytes = serde_json::to_vec(&json_value)?;
        let deserialized =
            SimpleValue::deserialize_from_bytes(&bytes, "Object", "json")?;
        assert_eq!(value, deserialized);
        Ok(())
    }

    #[test]
    fn test_yaml_roundtrip_bool() -> Result<(), Box<dyn std::error::Error>> {
        let value = SimpleValue::Bool { value: false };
        // Plain JSON, as a hand-written file holds it (as_bytes writes the tagged form).
        let json_val = value.try_into_json_value()?;
        let yaml_str = serde_yaml::to_string(&json_val)?;
        let yaml_bytes = yaml_str.into_bytes();
        let deserialized =
            SimpleValue::deserialize_from_bytes(&yaml_bytes, "Bool", "yaml")?;
        assert_eq!(value, deserialized);
        Ok(())
    }

    #[test]
    fn test_yaml_roundtrip_i64() -> Result<(), Box<dyn std::error::Error>> {
        let value = SimpleValue::I64 { value: 999 };
        // Plain JSON, as a hand-written file holds it (as_bytes writes the tagged form).
        let json_val = value.try_into_json_value()?;
        let yaml_str = serde_yaml::to_string(&json_val)?;
        let yaml_bytes = yaml_str.into_bytes();
        let deserialized =
            SimpleValue::deserialize_from_bytes(&yaml_bytes, "I64", "yaml")?;
        assert_eq!(value, deserialized);
        Ok(())
    }

    #[test]
    fn test_yaml_roundtrip_text() -> Result<(), Box<dyn std::error::Error>> {
        let value = SimpleValue::Text {
            value: "yaml test".to_string(),
        };
        // Plain JSON, as a hand-written file holds it (as_bytes writes the tagged form).
        let json_val = value.try_into_json_value()?;
        let yaml_str = serde_yaml::to_string(&json_val)?;
        let yaml_bytes = yaml_str.into_bytes();
        let deserialized =
            SimpleValue::deserialize_from_bytes(&yaml_bytes, "Text", "yaml")?;
        assert_eq!(value, deserialized);
        Ok(())
    }

    #[test]
    fn test_yaml_roundtrip_array() -> Result<(), Box<dyn std::error::Error>> {
        let value = SimpleValue::Array {
            value: vec![
                SimpleValue::I64 { value: 10 },
                SimpleValue::I64 { value: 20 },
            ],
        };
        // Plain JSON, as a hand-written file holds it (as_bytes writes the tagged form).
        let json_val = value.try_into_json_value()?;
        let yaml_str = serde_yaml::to_string(&json_val)?;
        let yaml_bytes = yaml_str.into_bytes();
        let deserialized =
            SimpleValue::deserialize_from_bytes(&yaml_bytes, "Array", "yaml")?;
        assert_eq!(value, deserialized);
        Ok(())
    }

    #[test]
    fn test_yaml_roundtrip_object() -> Result<(), Box<dyn std::error::Error>> {
        let mut obj = BTreeMap::new();
        obj.insert("key1".to_string(), SimpleValue::Text {
            value: "value1".to_string(),
        });
        obj.insert("key2".to_string(), SimpleValue::I64 { value: 42 });
        let value = SimpleValue::Object { value: obj };

        // Plain JSON, as a hand-written file holds it (as_bytes writes the tagged form).
        let json_val = value.try_into_json_value()?;
        let yaml_str = serde_yaml::to_string(&json_val)?;
        let yaml_bytes = yaml_str.into_bytes();
        let deserialized =
            SimpleValue::deserialize_from_bytes(&yaml_bytes, "Object", "yaml")?;
        assert_eq!(value, deserialized);
        Ok(())
    }

    #[test]
    fn test_yml_alias_same_as_yaml() -> Result<(), Box<dyn std::error::Error>> {
        let value = SimpleValue::I64 { value: 2026 };
        // Plain JSON, as a hand-written file holds it (as_bytes writes the tagged form).
        let json_val = value.try_into_json_value()?;
        let yaml_str = serde_yaml::to_string(&json_val)?;
        let yaml_bytes = yaml_str.into_bytes();

        // Both "yaml" and "yml" should work
        let deserialized_yaml =
            SimpleValue::deserialize_from_bytes(&yaml_bytes, "I64", "yaml")?;
        let deserialized_yml =
            SimpleValue::deserialize_from_bytes(&yaml_bytes, "I64", "yml")?;

        assert_eq!(value, deserialized_yaml);
        assert_eq!(value, deserialized_yml);
        assert_eq!(deserialized_yaml, deserialized_yml);
        Ok(())
    }

    #[test]
    fn test_json_parse_error() {
        let invalid_json = b"{ invalid json }";
        let result = SimpleValue::deserialize_from_bytes(invalid_json, "Object", "json");
        assert!(result.is_err());
    }

    #[test]
    fn test_yaml_parse_error() {
        let invalid_yaml = b"{ [ : }";
        let result = SimpleValue::deserialize_from_bytes(invalid_yaml, "Object", "yaml");
        assert!(result.is_err());
    }

    #[test]
    fn test_unsupported_format() {
        let bytes = b"some data";
        let result = SimpleValue::deserialize_from_bytes(bytes, "Text", "unknown_format");
        assert!(result.is_err());
    }

    #[test]
    fn test_txt_format_still_works() -> Result<(), Box<dyn std::error::Error>> {
        let input_bytes = b"hello from txt";
        let deserialized =
            SimpleValue::deserialize_from_bytes(input_bytes, "Text", "txt")?;
        match deserialized {
            SimpleValue::Text { value } => {
                assert_eq!(value, "hello from txt");
                Ok(())
            }
            _ => Err("Expected Text value".into()),
        }
    }

    #[test]
    fn test_html_format_still_works() -> Result<(), Box<dyn std::error::Error>> {
        let input_bytes = b"<html>content</html>";
        let deserialized =
            SimpleValue::deserialize_from_bytes(input_bytes, "Text", "html")?;
        match deserialized {
            SimpleValue::Text { value } => {
                assert_eq!(value, "<html>content</html>");
                Ok(())
            }
            _ => Err("Expected Text value".into()),
        }
    }

    #[test]
    fn test_toml_format_still_works() -> Result<(), Box<dyn std::error::Error>> {
        let input_bytes = b"key = value";
        let deserialized =
            SimpleValue::deserialize_from_bytes(input_bytes, "Text", "toml")?;
        match deserialized {
            SimpleValue::Text { value } => {
                assert_eq!(value, "key = value");
                Ok(())
            }
            _ => Err("Expected Text value".into()),
        }
    }
    /// A sample of every variant, by type identifier. A new identifier in `type_descriptions`
    /// without a sample here fails the round-trip test below.
    fn sample(identifier: &str) -> Option<SimpleValue> {
        let value = match identifier {
            "None" => SimpleValue::None {},
            "Bool" => SimpleValue::Bool { value: true },
            "I32" => SimpleValue::I32 { value: 7 },
            "I64" => SimpleValue::I64 { value: 1 << 40 },
            "F64" => SimpleValue::F64 { value: 1.5 },
            "Text" => SimpleValue::Text { value: "hello".to_string() },
            "Array" => SimpleValue::Array {
                value: vec![SimpleValue::I64 { value: 1 }, SimpleValue::Text { value: "two".to_string() }],
            },
            "Object" => {
                let mut map = BTreeMap::new();
                map.insert("a".to_string(), SimpleValue::Bool { value: false });
                SimpleValue::Object { value: map }
            }
            "Bytes" => SimpleValue::Bytes { value: vec![0, 1, 254] },
            "Metadata" => SimpleValue::Metadata { value: MetadataRecord::new() },
            "AssetInfo" => SimpleValue::AssetInfo { value: vec![AssetInfo::new()] },
            "Recipe" => SimpleValue::Recipe { value: Recipe::default() },
            "CommandMetadata" => SimpleValue::CommandMetadata { value: CommandMetadata::default() },
            "Query" => SimpleValue::Query { value: liquers_core::parse::parse_query("a/b").ok()? },
            "Key" => SimpleValue::Key { value: liquers_core::parse::parse_key("a/b").ok()? },
            _ => return None,
        };
        Some(value)
    }

    /// Every (type, format) pair `SimpleValue`'s `TypeInfo` declares is written and read back.
    /// A text format carries no type, so it reads back as `Text`; JSON numbers read back as `I64`.
    /// Pairs the writer refuses are collected and compared with a recorded list, so a new gap
    /// fails here instead of passing silently.
    #[test]
    fn every_declared_format_round_trips_or_is_recorded_as_unwritable(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut unwritable: Vec<String> = Vec::new();
        for info in SimpleValue::type_descriptions() {
            let id = info.type_identifier.to_string();
            let value = sample(&id).ok_or(format!("no sample for type identifier {id}"))?;
            for format in &info.supported_data_formats {
                let bytes = match value.as_bytes(format) {
                    Ok(bytes) => bytes,
                    Err(_) => {
                        unwritable.push(format!("{id}:{format}"));
                        continue;
                    }
                };
                let back = SimpleValue::deserialize_from_bytes(&bytes, &id, format)?;
                let expected = match format.as_ref() {
                    "txt" | "html" => SimpleValue::Text { value: String::from_utf8(bytes.clone())? },
                    "json" => match &value {
                        SimpleValue::I32 { value } => SimpleValue::I64 { value: i64::from(*value) },
                        other => other.clone(),
                    },
                    _ => value.clone(),
                };
                assert_eq!(back, expected, "round trip of {id} as {format}");
            }
        }
        unwritable.sort();
        let recorded: Vec<String> = UNWRITABLE.iter().map(|s| s.to_string()).collect();
        assert_eq!(unwritable, recorded, "declared but unwritable (type:format) pairs changed");
        Ok(())
    }

    /// Declared in `liquers_core::value::Value`'s `TypeInfo`, which `SimpleValue` shares, but
    /// refused by `SimpleValue::as_bytes` — `SIMPLE-VALUE-WRITES-FEWER-FORMATS-THAN-DECLARED`.
    const UNWRITABLE: &[&str] = &[
        "Bool:css",
        "Bool:js",
        "Bool:py",
        "Bool:rs",
        "Bytes:b",
        "Bytes:bin",
        "Bytes:bytes",
        "F64:css",
        "F64:js",
        "F64:py",
        "F64:rs",
        "I32:css",
        "I32:js",
        "I32:py",
        "I32:rs",
        "I64:css",
        "I64:js",
        "I64:py",
        "I64:rs",
        "Key:css",
        "Key:html",
        "Key:js",
        "Key:py",
        "Key:rs",
        "Key:txt",
        "None:css",
        "None:js",
        "None:py",
        "None:rs",
        "Query:css",
        "Query:html",
        "Query:js",
        "Query:py",
        "Query:rs",
        "Query:txt",
        "Text:b",
        "Text:bin",
        "Text:bytes",
        "Text:css",
        "Text:js",
        "Text:py",
        "Text:rs",
    ];
}
