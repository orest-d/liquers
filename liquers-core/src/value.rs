#![allow(unused_imports)]
#![allow(dead_code)]

use serde_json;

use std::{borrow::Cow, collections::BTreeMap, result::Result};

use crate::{
    command_metadata::CommandMetadata,
    error::{Error, ErrorType},
    metadata::{AssetInfo, MetadataRecord},
    recipes::Recipe,
};
use std::convert::{TryFrom, TryInto};

/// Basic built-in value type
/// Value type is the central data type of the system.
/// It is mainly used to represent a state (via [crate::state::State] ).
/// A custom value type can be used instead of [Value], but it must implement the [ValueInterface] trait.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(untagged)]
pub enum Value {
    None,
    Bool(bool),
    I32(i32),
    I64(i64),
    F64(f64),
    Text(String),
    Array(Vec<Value>),
    Object(BTreeMap<String, Value>),
    Bytes(Vec<u8>),
    Metadata(MetadataRecord),
    AssetInfo(Vec<AssetInfo>),
    Recipe(Recipe),
    CommandMetadata(CommandMetadata),
    Query(crate::query::Query),
    Key(crate::query::Key),
}

// TODO: Remove the serialization and deserialization from ValueInterface (is it there?)
/// ValueInterface is a trait that must be implemented by the value type.
/// This is a central trait that defines the minimum set of operations
/// that must be supported by the value type.
pub trait ValueInterface:
    core::fmt::Debug
    + Clone
    + Sized
    + DefaultValueSerializer
    + crate::maybe_send::MaybeSend
    + crate::maybe_send::MaybeSync
    + 'static
{
    /// Try to get a Query out
    fn try_into_query(&self) -> Result<crate::query::Query, Error>;
    /// Empty value
    fn none() -> Self;

    /// Test if value is empty
    fn is_none(&self) -> bool;

    /// From string
    fn new(txt: &str) -> Self;

    /// From string
    fn from_string(txt: String) -> Self;

    /// From query
    fn from_query(query: &crate::query::Query) -> Self;

    /// From key
    fn from_key(key: &crate::query::Key) -> Self;

    /// From integer
    fn from_i32(n: i32) -> Self;

    /// From integer string
    fn from_i32_str(n: &str) -> Result<Self, Error> {
        n.parse::<i32>()
            .map(|x| Self::from_i32(x))
            .map_err(|_| Error::conversion_error(n, "i32"))
    }

    /// From integer
    fn from_i64(n: i64) -> Self;

    /// From integer string
    fn from_i64_str(n: &str) -> Result<Self, Error> {
        n.parse::<i64>()
            .map(|x| Self::from_i64(x))
            .map_err(|_| Error::conversion_error(n, "i64"))
    }

    /// From float
    fn from_f64(n: f64) -> Self;

    /// From float string
    fn from_f64_str(n: &str) -> Result<Self, Error> {
        n.parse::<f64>()
            .map(|x| Self::from_f64(x))
            .map_err(|_| Error::conversion_error(n, "f64"))
    }

    /// From boolean
    fn from_bool(b: bool) -> Self;

    /// From boolean string
    fn from_bool_str(b: &str) -> Result<Self, Error> {
        match b.to_lowercase().as_str() {
            "true" => Ok(Self::from_bool(true)),
            "t" => Ok(Self::from_bool(true)),
            "yes" => Ok(Self::from_bool(true)),
            "y" => Ok(Self::from_bool(true)),
            "1" => Ok(Self::from_bool(true)),
            "false" => Ok(Self::from_bool(false)),
            "f" => Ok(Self::from_bool(false)),
            "no" => Ok(Self::from_bool(false)),
            "n" => Ok(Self::from_bool(false)),
            "0" => Ok(Self::from_bool(false)),
            _ => Err(Error::conversion_error(b, "bool")),
        }
    }

    /// From metadata
    fn from_metadata(metadata: MetadataRecord) -> Self;

    /// From asset info
    fn from_asset_info(asset_info: Vec<AssetInfo>) -> Self;

    /// From recipe
    fn from_recipe(recipe: Recipe) -> Self;

    /// From command metadata
    fn from_command_metadata(command_metadata: CommandMetadata) -> Self;

    /// From bytes
    fn from_bytes(b: Vec<u8>) -> Self;

    /// Try to get a string out
    fn try_into_string(&self) -> Result<String, Error>;

    /// Try to get a string out
    fn try_into_string_option(&self) -> Result<Option<String>, Error> {
        if self.is_none() {
            Ok(None)
        } else {
            self.try_into_string().map(Some)
        }
    }

    /// Try to get an integer
    fn try_into_i32(&self) -> Result<i32, Error>;

    /// Try to get a i64
    fn try_into_i64(&self) -> Result<i64, Error>;

    /// Try to get a i64 option
    fn try_into_i64_option(&self) -> Result<Option<i64>, Error> {
        if self.is_none() {
            Ok(None)
        } else {
            self.try_into_i64().map(Some)
        }
    }

    /// Try to get a i64
    fn try_into_f64(&self) -> Result<f64, Error>;

    /// Try to get a i64 option
    fn try_into_f64_option(&self) -> Result<Option<f64>, Error> {
        if self.is_none() {
            Ok(None)
        } else {
            self.try_into_f64().map(Some)
        }
    }

    /// Try into boolean
    fn try_into_bool(&self) -> Result<bool, Error>;

    /// Try into bytes
    fn try_into_bytes(&self) -> Result<Vec<u8>, Error>;

    /// Try into key
    fn try_into_key(&self) -> Result<crate::query::Key, Error>;

    /// Try into command metadata
    fn try_into_command_metadata(&self) -> Result<CommandMetadata, Error>;

    /// Whether *this* value can be serialized in `data_format`.
    ///
    /// Answered without a registry, so `State::as_bytes` and the state-level checks need no
    /// `Environment`. The default consults [`ValueInterface::type_info`], which is correct for
    /// any implementor whose descriptions are accurate.
    fn supports_data_format(&self, data_format: &str) -> bool {
        self.type_info().supports_data_format(data_format)
    }

    /// The description of this value's type.
    ///
    /// The default finds it among [`ValueInterface::type_descriptions`] by identifier, and falls
    /// back to a minimal description built from this value's own defaults so that an implementor
    /// which describes nothing still reports something coherent.
    fn type_info(&self) -> crate::type_system::TypeInfo {
        let identifier = self.identifier();
        Self::type_descriptions()
            .into_iter()
            .find(|info| info.type_identifier == identifier)
            .unwrap_or_else(|| {
                crate::type_system::TypeInfo::new(identifier)
                    .with_type_name(self.type_name())
                    .with_defaults(
                        self.default_data_format(),
                        self.default_extension(),
                        self.default_media_type(),
                        self.default_filename(),
                    )
            })
    }

    /// Static self-description of every type this value type can hold.
    ///
    /// Seeds a [`crate::type_system::TypeRegistry`]. The default is empty: an implementor that
    /// does not describe itself registers nothing, which degrades to "unknown type" rather than
    /// failing to compile — so adding this method breaks no existing implementor.
    fn type_descriptions() -> Vec<crate::type_system::TypeInfo> {
        Vec::new()
    }

    /// String identifier of the state type
    /// Several types can be linked to the same identifier.
    /// The identifier must be cross-platform
    fn identifier(&self) -> Cow<'static, str>; // TODO: Rename to type_identifier?

    /// String name of the stored type
    /// The type_name is more detailed than identifier.
    /// The identifier does not need to be cross-platform, it serves more for information and debugging
    fn type_name(&self) -> Cow<'static, str>; // TODO: Rename to detailed_type_identifier?

    /// Default file extension; determines the default data format
    /// Must be consistent with the default_media_type.
    fn default_extension(&self) -> Cow<'static, str>;

    /// Default data format; determines the default data format for serialization
    /// Data format is more specific than the file extension.
    /// For example, the default extension can be "csv", but the data format can
    /// specify what kind of CSV it is (e.g., "csv:comma" or "csv:tab").
    /// The DefaultValueSerializer trait must be able to unde
    /// Must be consistent with the default_media_type and default_extension.
    fn default_data_format(&self) -> Cow<'static, str> {
        self.default_extension()
    }

    /// Default file name
    fn default_filename(&self) -> Cow<'static, str>;

    /// Default mime type - must be consistent with the default_extension
    fn default_media_type(&self) -> Cow<'static, str>;

    /// Try to get a JSON-serializable value
    fn try_into_json_value(&self) -> Result<serde_json::Value, Error>;

    /// Construct list value from child values.
    fn from_array(_values: Vec<Self>) -> Result<Self, Error> {
        Err(Error::not_supported(
            "Array conversion not supported for this ValueInterface".to_string(),
        ))
    }

    /// Construct object value from child values.
    fn from_object(_values: BTreeMap<String, Self>) -> Result<Self, Error> {
        Err(Error::not_supported(
            "Object conversion not supported for this ValueInterface".to_string(),
        ))
    }

    /// Try to convert JSON value to value type
    fn try_from_json_value(value: &serde_json::Value) -> Result<Self, Error> {
        match value {
            serde_json::Value::Null => Ok(Self::none()),
            serde_json::Value::Bool(b) => Ok(Self::from_bool(*b)),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Ok(Self::from_i64(i))
                } else if let Some(f) = n.as_f64() {
                    Ok(Self::from_f64(f))
                } else {
                    Err(Error::conversion_error_with_message(
                        value,
                        "i64 or f64",
                        "Invalid JSON number",
                    ))
                }
            }
            serde_json::Value::String(s) => Ok(Self::new(s)),
            serde_json::Value::Array(a) => {
                let mut v = Vec::with_capacity(a.len());
                for x in a {
                    v.push(Self::try_from_json_value(x)?);
                }
                Self::from_array(v)
            }
            serde_json::Value::Object(o) => {
                let mut m = BTreeMap::new();
                for (k, v) in o {
                    m.insert(k.clone(), Self::try_from_json_value(v)?);
                }
                Self::from_object(m)
            }
        }
    }
}

impl ValueInterface for Value {
    fn try_into_query(&self) -> Result<crate::query::Query, Error> {
        match self {
            Value::Query(q) => Ok(q.clone()),
            Value::Text(s) => crate::parse::parse_query(s)
                .map_err(|e| Error::from_error(ErrorType::ParseError, e)),
            _ => Err(Error::conversion_error(self.identifier(), "Query")),
        }
    }
    fn none() -> Self {
        Value::None
    }
    fn is_none(&self) -> bool {
        if let Value::None = self {
            true
        } else {
            false
        }
    }

    fn new(txt: &str) -> Self {
        Value::Text(txt.to_owned())
    }

    fn try_into_string(&self) -> Result<String, Error> {
        match self {
            Value::None => Ok("None".to_string()),
            Value::Bool(b) => Ok(format!("{b}")),
            Value::I32(n) => Ok(format!("{n}")),
            Value::I64(n) => Ok(format!("{n}")),
            Value::F64(n) => Ok(format!("{n}")),
            Value::Text(t) => Ok(t.to_owned()),
            Value::Bytes(b) => Ok(String::from_utf8_lossy(b).to_string()),
            _ => Err(Error::conversion_error(self.identifier(), "string")),
        }
    }

    fn try_into_i32(&self) -> Result<i32, Error> {
        match self {
            Value::I32(n) => Ok(*n),
            _ => Err(Error::conversion_error(self.identifier(), "i32")),
        }
    }

    fn try_into_json_value(&self) -> Result<serde_json::Value, Error> {
        match self {
            Value::None => Ok(serde_json::Value::Null),
            Value::Bool(b) => Ok(serde_json::Value::Bool(*b)),
            Value::I32(n) => Ok(serde_json::Value::Number(serde_json::Number::from(*n))),
            Value::I64(n) => Ok(serde_json::Value::Number(serde_json::Number::from(*n))),
            Value::F64(n) => Ok(serde_json::Value::Number(
                serde_json::Number::from_f64(*n).unwrap(),
            )),
            Value::Text(t) => Ok(serde_json::Value::String(t.to_owned())),
            Value::Array(a) => {
                let mut v = Vec::new();
                for x in a {
                    v.push(x.try_into_json_value()?);
                }
                Ok(serde_json::Value::Array(v))
            }
            Value::Object(o) => {
                let mut m = serde_json::Map::new();
                for (k, v) in o {
                    m.insert(k.to_owned(), v.try_into_json_value()?);
                }
                Ok(serde_json::Value::Object(m))
            }
            Value::Metadata(metadata_record) => {
                serde_json::to_value(metadata_record).map_err(|e| {
                    Error::conversion_error_with_message("metadata", "json value", &e.to_string())
                })
            }
            Value::Recipe(recipe) => serde_json::to_value(recipe).map_err(|e| {
                Error::conversion_error_with_message("recipe", "json value", &e.to_string())
            }),
            _ => Err(Error::conversion_error(self.identifier(), "JSON value")),
        }
    }

    fn from_array(values: Vec<Self>) -> Result<Self, Error> {
        Ok(Value::Array(values))
    }

    fn from_object(values: BTreeMap<String, Self>) -> Result<Self, Error> {
        Ok(Value::Object(values))
    }

    fn type_descriptions() -> Vec<crate::type_system::TypeInfo> {
        use crate::type_system::TypeInfo;
        // `supported_data_formats` lists the formats a value of this type can be **written** in,
        // which is what the write path checks. Reading back is narrower in two places, recorded
        // rather than hidden: `None` written as text produces `none`, which the text branch has
        // no rule to parse; and `Text` written as bytes reads back as `Bytes`. Both are
        // legitimate writes, so both stay in the list.
        // Bare literals, repeated, with no shared vocabulary — tracked as
        // `specs/issues/DATA-FORMAT-CONSTANTS-AND-TOOLING.md`, which also covers recognising an
        // unknown format and giving serde-capable types their formats generically.
        const TEXTUAL: [&str; 7] = ["txt", "html", "css", "js", "py", "rs", "json"];
        vec![
            TypeInfo::new("None")
                .with_type_name("none")
                .with_defaults("json", "json", "application/json", "data.json")
                .with_data_formats(TEXTUAL),
            TypeInfo::new("Bool")
                .with_type_name("bool")
                .with_defaults("json", "json", "application/json", "data.json")
                .with_data_formats(TEXTUAL),
            TypeInfo::new("I32")
                .with_type_name("i32")
                .with_defaults("json", "json", "application/json", "data.json")
                .with_data_formats(TEXTUAL),
            TypeInfo::new("I64")
                .with_type_name("i64")
                .with_defaults("json", "json", "application/json", "data.json")
                .with_data_formats(TEXTUAL),
            TypeInfo::new("F64")
                .with_type_name("f64")
                .with_defaults("json", "json", "application/json", "data.json")
                .with_data_formats(TEXTUAL),
            TypeInfo::new("Text")
                .with_type_name("text")
                .with_defaults("txt", "txt", "text/plain", "text.txt")
                .with_data_formats(TEXTUAL)
                .with_data_formats(["b", "bin", "bytes"]),
            TypeInfo::new("Array")
                .with_type_name("array")
                .with_defaults("json", "json", "application/json", "data.json")
                .with_data_formats(["json"]),
            TypeInfo::new("Object")
                .with_type_name("object")
                .with_defaults("json", "json", "application/json", "data.json")
                .with_data_formats(["json"]),
            TypeInfo::new("Bytes")
                .with_type_name("bytes")
                .with_defaults("b", "b", "application/octet-stream", "binary.b")
                .with_data_formats(["b", "bin", "bytes", "json"]),
            TypeInfo::new("Metadata")
                .with_type_name("metadata")
                .with_defaults("json", "json", "application/json", "metadata.json")
                .with_data_formats(["json"]),
            TypeInfo::new("AssetInfo")
                .with_type_name("asset_info")
                .with_defaults("json", "json", "application/json", "asset_info.json")
                .with_data_formats(["json"]),
            TypeInfo::new("Recipe")
                .with_type_name("recipe")
                .with_defaults("json", "json", "application/json", "recipe.json")
                .with_data_formats(["json"]),
            TypeInfo::new("CommandMetadata")
                .with_type_name("command_metadata")
                .with_defaults("json", "json", "application/json", "command_metadata.json")
                .with_data_formats(["json"]),
            TypeInfo::new("Query")
                .with_type_name("query")
                .with_defaults("txt", "txt", "text/plain", "query.txt")
                .with_data_formats(TEXTUAL),
            TypeInfo::new("Key")
                .with_type_name("key")
                .with_defaults("txt", "txt", "text/plain", "key.txt")
                .with_data_formats(TEXTUAL),
        ]
    }

    fn identifier(&self) -> Cow<'static, str> {
        // Bare CamelCase names: `liquers-core` owns every one of these concepts, and a bare
        // identifier is reserved for exactly that. See `specs/reference/VALUE_TYPE_SYSTEM.md`.
        //
        // These changed from the previous scheme, in which five distinct variants all reported
        // `"generic"` while the deserializer branched on `"i32"`/`"i64"`/`"f64"`/`"bool"` — so an
        // integer written as text read back as text, silently. Stored identifiers written by an
        // older build are deliberately not migrated.
        match self {
            Value::None => "None".into(),
            Value::Bool(_) => "Bool".into(),
            Value::I32(_) => "I32".into(),
            Value::I64(_) => "I64".into(),
            Value::F64(_) => "F64".into(),
            Value::Text(_) => "Text".into(),
            Value::Array(_) => "Array".into(),
            Value::Object(_) => "Object".into(),
            Value::Bytes(_) => "Bytes".into(),
            Value::Metadata(_) => "Metadata".into(),
            Value::AssetInfo(_) => "AssetInfo".into(),
            Value::Recipe(_) => "Recipe".into(),
            Value::CommandMetadata(_) => "CommandMetadata".into(),
            Value::Query(_) => "Query".into(),
            Value::Key(_) => "Key".into(),
        }
    }

    fn type_name(&self) -> Cow<'static, str> {
        match self {
            Value::None => "none".into(),
            Value::Bool(_) => "bool".into(),
            Value::I32(_) => "i32".into(),
            Value::I64(_) => "i64".into(),
            Value::F64(_) => "f64".into(),
            Value::Text(_) => "text".into(),
            Value::Array(_) => "array".into(),
            Value::Object(_) => "object".into(),
            Value::Bytes(_) => "bytes".into(),
            Value::Metadata(_) => "metadata".into(),
            Value::AssetInfo(_) => "asset_info".into(),
            Value::Recipe(_) => "recipe".into(),
            Value::CommandMetadata(_) => "command_metadata".into(),
            Value::Query(_) => "query".into(),
            Value::Key(_) => "key".into(),
        }
    }

    fn default_extension(&self) -> Cow<'static, str> {
        match self {
            Value::None => "json".into(),
            Value::Bool(_) => "json".into(),
            Value::I32(_) => "json".into(),
            Value::I64(_) => "json".into(),
            Value::F64(_) => "json".into(),
            Value::Text(_) => "txt".into(),
            Value::Array(_) => "json".into(),
            Value::Object(_) => "json".into(),
            Value::Bytes(_) => "b".into(),
            Value::Metadata(_) => "json".into(),
            Value::AssetInfo(_) => "json".into(),
            Value::Recipe(_) => "json".into(),
            Value::CommandMetadata(_) => "json".into(),
            Value::Query(_) => "txt".into(),
            Value::Key(_) => "txt".into(),
        }
    }

    fn default_filename(&self) -> Cow<'static, str> {
        match self {
            Value::None => "data.json".into(),
            Value::Bool(_) => "data.json".into(),
            Value::I32(_) => "data.json".into(),
            Value::I64(_) => "data.json".into(),
            Value::F64(_) => "data.json".into(),
            Value::Text(_) => "text.txt".into(),
            Value::Array(_) => "data.json".into(),
            Value::Object(_) => "data.json".into(),
            Value::Bytes(_) => "binary.b".into(),
            Value::Metadata(_) => "metadata.json".into(),
            Value::AssetInfo(_) => "asset_info.json".into(),
            Value::Recipe(_) => "recipe.json".into(),
            Value::CommandMetadata(_) => "command_metadata.json".into(),
            Value::Query(_) => "query.txt".into(),
            Value::Key(_) => "key.txt".into(),
        }
    }

    fn default_media_type(&self) -> Cow<'static, str> {
        match self {
            Value::None => "application/json".into(),
            Value::Bool(_) => "application/json".into(),
            Value::I32(_) => "application/json".into(),
            Value::I64(_) => "application/json".into(),
            Value::F64(_) => "application/json".into(),
            Value::Text(_) => "text/plain".into(),
            Value::Array(_) => "application/json".into(),
            Value::Object(_) => "application/json".into(),
            Value::Bytes(_) => "application/octet-stream".into(),
            Value::Metadata(_) => "application/json".into(),
            Value::AssetInfo(_) => "application/json".into(),
            Value::Recipe(_) => "application/json".into(),
            Value::CommandMetadata(_) => "application/json".into(),
            Value::Query(_) => "text/plain".into(),
            Value::Key(_) => "text/plain".into(),
        }
    }

    fn from_string(txt: String) -> Self {
        Value::Text(txt)
    }

    fn from_i32(n: i32) -> Self {
        Value::I32(n)
    }

    fn from_i64(n: i64) -> Self {
        Value::I64(n)
    }

    fn from_f64(n: f64) -> Self {
        Value::F64(n)
    }

    fn from_bool(b: bool) -> Self {
        Value::Bool(b)
    }

    fn from_bytes(b: Vec<u8>) -> Self {
        Value::Bytes(b)
    }

    fn try_from_json_value(value: &serde_json::Value) -> Result<Self, Error> {
        match value {
            serde_json::Value::Null => Ok(Value::None),
            serde_json::Value::Bool(b) => Ok(Value::Bool(*b)),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Ok(Value::I64(i))
                } else if let Some(f) = n.as_f64() {
                    Ok(Value::F64(f))
                } else {
                    Err(Error::conversion_error_with_message(
                        value,
                        "i64 or f64",
                        "Invalid JSON number",
                    ))
                }
            }
            serde_json::Value::String(s) => Ok(Value::Text(s.to_owned())),
            serde_json::Value::Array(a) => {
                let mut v = Vec::new();
                for x in a {
                    v.push(Value::try_from_json_value(x)?);
                }
                Ok(Value::Array(v))
            }
            serde_json::Value::Object(o) => {
                let mut m = BTreeMap::new();
                for (k, v) in o {
                    m.insert(k.to_owned(), Value::try_from_json_value(v)?);
                }
                Ok(Value::Object(m))
            }
        }
    }

    fn try_into_i64(&self) -> Result<i64, Error> {
        match self {
            Value::I32(n) => Ok(*n as i64),
            Value::I64(n) => Ok(*n),
            _ => Err(Error::conversion_error(self.identifier(), "i64")),
        }
    }

    fn try_into_bool(&self) -> Result<bool, Error> {
        match self {
            Value::Bool(b) => Ok(*b),
            Value::I32(n) => Ok(*n != 0),
            Value::I64(n) => Ok(*n != 0),
            _ => Err(Error::conversion_error(self.identifier(), "bool")),
        }
    }

    fn try_into_f64(&self) -> Result<f64, Error> {
        match self {
            Value::I32(n) => Ok(*n as f64),
            Value::I64(n) => Ok(*n as f64),
            Value::F64(n) => Ok(*n),
            _ => Err(Error::conversion_error(self.identifier(), "f64")),
        }
    }
    fn try_into_key(&self) -> Result<crate::query::Key, Error> {
        match self {
            Value::Text(s) => Ok(crate::parse::parse_key(s)?),
            Value::Query(q) => q
                .key()
                .ok_or(Error::conversion_error(self.identifier(), "key")),
            Value::Key(k) => Ok(k.clone()),
            _ => Err(Error::conversion_error(self.identifier(), "key")),
        }
    }

    fn try_into_command_metadata(&self) -> Result<CommandMetadata, Error> {
        match self {
            Value::CommandMetadata(command_metadata) => Ok(command_metadata.clone()),
            _ => Err(Error::conversion_error(
                self.identifier(),
                "command metadata",
            )),
        }
    }

    fn try_into_bytes(&self) -> Result<Vec<u8>, Error> {
        match self {
            Value::Bytes(b) => Ok(b.clone()),
            Value::Text(t) => Ok(t.as_bytes().to_vec()),
            _ => Err(Error::conversion_error(self.identifier(), "bytes")),
        }
    }

    fn from_metadata(metadata: MetadataRecord) -> Self {
        Value::Metadata(metadata)
    }

    fn from_asset_info(asset_info: Vec<AssetInfo>) -> Self {
        Value::AssetInfo(asset_info)
    }

    fn from_recipe(recipe: Recipe) -> Self {
        Value::Recipe(recipe)
    }

    fn from_command_metadata(command_metadata: CommandMetadata) -> Self {
        Value::CommandMetadata(command_metadata)
    }

    fn from_query(query: &crate::query::Query) -> Self {
        Value::Query(query.clone())
    }

    fn from_key(key: &crate::query::Key) -> Self {
        Value::Key(key.clone())
    }
}

impl TryFrom<&Value> for i32 {
    type Error = Error;
    fn try_from(value: &Value) -> Result<Self, Self::Error> {
        match value {
            Value::I32(x) => Ok(*x),
            Value::I64(x) => i32::try_from(*x)
                .map_err(|e| Error::conversion_error_with_message("I64", "i32", &e.to_string())),
            _ => Err(Error::conversion_error(value.type_name(), "i32")),
        }
    }
}

impl TryFrom<Value> for i32 {
    type Error = Error;
    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::I32(x) => Ok(x),
            Value::I64(x) => i32::try_from(x)
                .map_err(|e| Error::conversion_error_with_message("I64", "i32", &e.to_string())),
            _ => Err(Error::conversion_error(value.type_name(), "i32")),
        }
    }
}

impl From<i32> for Value {
    fn from(value: i32) -> Value {
        Value::I32(value)
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
            Value::I32(x) => Ok(x as i64),
            Value::I64(x) => Ok(x),
            _ => Err(Error::conversion_error(value.type_name(), "i64")),
        }
    }
}
impl From<i64> for Value {
    fn from(value: i64) -> Value {
        Value::I64(value)
    }
}

impl From<Vec<i64>> for Value {
    fn from(value: Vec<i64>) -> Value {
        Value::Array(value.into_iter().map(Value::from).collect())
    }
}

impl TryFrom<Value> for f64 {
    type Error = Error;
    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::I32(x) => Ok(x as f64),
            Value::I64(x) => Ok(x as f64),
            Value::F64(x) => Ok(x),
            _ => Err(Error::conversion_error(value.type_name(), "f64")),
        }
    }
}
impl From<f64> for Value {
    fn from(value: f64) -> Value {
        Value::F64(value)
    }
}

impl TryFrom<Value> for f32 {
    type Error = Error;
    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::I32(x) => Ok(x as f32),
            Value::I64(x) => Ok(x as f32),
            Value::F64(x) => Ok(x as f32),
            _ => Err(Error::conversion_error(value.type_name(), "f32")),
        }
    }
}
impl From<f32> for Value {
    fn from(value: f32) -> Value {
        Value::F64(value as f64)
    }
}

impl TryFrom<Value> for bool {
    type Error = Error;
    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::I32(x) => Ok(x != 0),
            Value::I64(x) => Ok(x != 0),
            _ => Err(Error::conversion_error(value.type_name(), "bool")),
        }
    }
}

impl TryFrom<Value> for isize {
    type Error = Error;
    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::I32(x) => Ok(x as isize),
            Value::I64(x) => Ok(x as isize),
            _ => Err(Error::conversion_error(value.type_name(), "bool")),
        }
    }
}

impl TryFrom<Value> for u32 {
    type Error = Error;
    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::I32(x) => Ok(x as u32),
            Value::I64(x) => Ok(x as u32),
            _ => Err(Error::conversion_error(value.type_name(), "bool")),
        }
    }
}

impl TryFrom<Value> for u64 {
    type Error = Error;
    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::I32(x) => Ok(x as u64),
            Value::I64(x) => Ok(x as u64),
            _ => Err(Error::conversion_error(value.type_name(), "bool")),
        }
    }
}

impl TryFrom<Value> for usize {
    type Error = Error;
    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::I32(x) => Ok(x as usize),
            Value::I64(x) => Ok(x as usize),
            _ => Err(Error::conversion_error(value.type_name(), "bool")),
        }
    }
}

impl From<bool> for Value {
    fn from(value: bool) -> Value {
        Value::Bool(value)
    }
}

/// A key-valued link argument, produced by a `-R-key/<key>` query.
///
/// This is how a command receives a *location* rather than a location's contents — most notably
/// the current directory, via a `-R-key/.` default link. It is the supported replacement for
/// reading the working key out of `Context`: explicit in the query, overridable per call, and
/// visible to the planner.
impl TryFrom<Value> for crate::query::Key {
    type Error = Error;
    fn try_from(value: Value) -> Result<Self, Self::Error> {
        value.try_into_key()
    }
}

impl TryFrom<Value> for String {
    type Error = Error;
    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::Text(x) => Ok(x),
            Value::I32(x) => Ok(format!("{}", x)),
            Value::I64(x) => Ok(format!("{}", x)),
            Value::F64(x) => Ok(format!("{}", x)),
            _ => Err(Error::conversion_error(value.type_name(), "string")),
        }
    }
}

impl From<String> for Value {
    fn from(value: String) -> Value {
        Value::Text(value)
    }
}
impl From<&str> for Value {
    fn from(value: &str) -> Value {
        Value::Text(value.to_owned())
    }
}

// TODO: Turn this into a separate object to make it configurable
pub trait DefaultValueSerializer
where
    Self: Sized,
{
    /// Serialize to bytes using the specified data format
    /// data format typically is a file extension like "json", "txt",
    /// but it can be more specific like "csv:comma"
    fn as_bytes(&self, data_format: &str) -> Result<Vec<u8>, Error>;

    /// Deserialize from bytes using the specified data format and type identifier
    /// An empty type identifier means that the type identifier is not known
    /// and the deserializer should try to infer the type from the data format.
    fn deserialize_from_bytes(
        b: &[u8],
        type_identifier: &str,
        data_format: &str,
    ) -> Result<Self, Error>;
}

impl DefaultValueSerializer for Value {
    fn as_bytes(&self, data_format: &str) -> Result<Vec<u8>, Error> {
        match data_format {
            "json" => serde_json::to_vec(self).map_err(|e| {
                Error::new(ErrorType::SerializationError, format!("JSON error {}", e))
            }),
            "txt" | "html" | "rs" | "py" | "css" | "js" => match self {
                // TODO: handle various extensions better, rs is only to test assets
                Value::None => Ok("none".as_bytes().to_vec()),
                Value::Bool(true) => Ok("true".as_bytes().to_vec()),
                Value::Bool(false) => Ok("false".as_bytes().to_vec()),
                Value::I32(x) => Ok(format!("{x}").into_bytes()),
                Value::I64(x) => Ok(format!("{x}").into_bytes()),
                Value::F64(x) => Ok(format!("{x}").into_bytes()),
                Value::Text(x) => Ok(x.as_bytes().to_vec()),
                Value::Bytes(x) => Ok(x.clone()), // TODO: handle bytes better - this is only to test rs
                Value::Query(x) => Ok(x.encode().as_bytes().to_vec()), // TODO: not for languages
                Value::Key(x) => Ok(x.encode().as_bytes().to_vec()), // TODO: not for languages
                _ => Err(Error::new(
                    ErrorType::SerializationError,
                    format!(
                        "Serialization to {} not supported by {}",
                        data_format,
                        self.type_name()
                    ),
                )),
            },
            "bytes" | "b" | "bin" => match self {
                Value::Bytes(x) => Ok(x.clone()),
                Value::Text(x) => Ok(x.as_bytes().to_vec()),
                _ => Err(Error::new(
                    ErrorType::SerializationError,
                    format!(
                        "Serialization to bytes not supported by {}",
                        self.type_name()
                    ),
                )),
            },
            _ => Err(Error::new(
                ErrorType::SerializationError,
                format!("Unsupported format {}", data_format),
            )),
        }
    }

    fn deserialize_from_bytes(
        b: &[u8],
        type_identifier: &str,
        data_format: &str,
    ) -> Result<Self, Error> {
        match data_format {
            // `Value` is `#[serde(untagged)]`, so JSON alone cannot always say which variant it
            // came from: `Value::Bytes(vec![1, 2, 3])` serializes to `[1, 2, 3]` and reads back as
            // `Value::Array`. The declared type identifier is exactly the discriminator serde
            // lacks, so it is consulted for the ambiguous variants before falling back to shape
            // inference. See `specs/issues/COMBINED-VALUE-DISCRIMINATION.md`.
            "json" => match type_identifier {
                "Bytes" => serde_json::from_slice::<Vec<u8>>(b)
                    .map(Value::Bytes)
                    .map_err(|e| {
                        Error::from_error(
                            ErrorType::SerializationError,
                            format!("JSON error reading Bytes in from_bytes: {}", e),
                        )
                    }),
                _ => serde_json::from_slice(b).map_err(|e| {
                    Error::from_error(
                        ErrorType::SerializationError,
                        format!("JSON error in from_bytes:{}", e),
                    )
                }),
            },
            "txt" | "html" | "rs" | "py" | "css" | "js" => {
                let s = String::from_utf8_lossy(b);
                match type_identifier {
                    // An empty identifier means "not known"; text is the only safe assumption.
                    "" => Ok(Value::Text(s.to_string())),
                    "Text" => Ok(Value::Text(s.to_string())),
                    "I32" => s.parse::<i32>().map(Value::I32).map_err(|e| {
                        Error::conversion_error_with_message(&s, "i32", &e.to_string())
                    }),
                    "I64" => s.parse::<i64>().map(Value::I64).map_err(|e| {
                        Error::conversion_error_with_message(&s, "i64", &e.to_string())
                    }),
                    "F64" => s.parse::<f64>().map(Value::F64).map_err(|e| {
                        Error::conversion_error_with_message(&s, "f64", &e.to_string())
                    }),
                    "Bool" => Value::from_bool_str(&s),
                    "Query" => crate::parse::parse_query(&s).map(Value::Query),
                    "Key" => crate::parse::parse_key(&s).map(Value::Key),
                    _ => Err(Error::new(
                        ErrorType::SerializationError,
                        format!(
                            "Unsupported type identifier in from_bytes:{}",
                            type_identifier
                        ),
                    )),
                }
            }
            "bytes" | "b" | "bin" => Ok(Value::Bytes(b.to_vec())),
            _ => Err(Error::new(
                ErrorType::SerializationError,
                format!("Unsupported format in from_bytes:{}", data_format),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test1() -> Result<(), Box<dyn std::error::Error>> {
        eprintln!("Hello.");
        let v = Value::I32(123);
        let b = v.as_bytes("json")?;
        eprintln!("Serialized    {:?}: {}", v, std::str::from_utf8(&b)?);
        let w: Value = DefaultValueSerializer::deserialize_from_bytes(&b, "I32", "json")?;
        eprintln!("De-Serialized {:?}", w);
        Ok(())
    }
    #[test]
    fn test_convert_int() -> Result<(), Box<dyn std::error::Error>> {
        let v = Value::I32(123);
        let x: i32 = v.try_into()?;
        assert_eq!(x, 123);
        Ok(())
    }
    #[test]
    fn test_convert_text() -> Result<(), Box<dyn std::error::Error>> {
        let v = Value::from("abc");
        assert_eq!(v, Value::Text("abc".to_owned()));
        let x: String = v.try_into()?;
        assert_eq!(x, "abc");
        Ok(())
    }
    #[test]
    fn test_serde_to_json() -> Result<(), Box<dyn std::error::Error>> {
        let v = Value::None;
        let s = serde_json::to_string(&v)?;
        assert_eq!(s, "null");
        let v = Value::Bool(true);
        let s = serde_json::to_string(&v)?;
        assert_eq!(s, "true");
        let v = Value::I32(123);
        let s = serde_json::to_string(&v)?;
        assert_eq!(s, "123");
        let v = Value::I64(123);
        let s = serde_json::to_string(&v)?;
        assert_eq!(s, "123");
        let v = Value::F64(123.456);
        let s = serde_json::to_string(&v)?;
        assert_eq!(s, "123.456");
        let v = Value::from("abc");
        let s = serde_json::to_string(&v)?;
        assert_eq!(s, "\"abc\"");
        let v = Value::Array(vec![Value::None, Value::Bool(false), Value::I32(123)]);
        let s = serde_json::to_string(&v)?;
        assert_eq!(s, "[null,false,123]");
        let mut m = BTreeMap::new();
        m.insert("test".to_owned(), Value::None);
        m.insert("a".to_owned(), Value::I32(123));
        let v = Value::Object(m);
        let s = serde_json::to_string(&v)?;
        assert_eq!(s, "{\"a\":123,\"test\":null}");
        Ok(())
    }
    #[test]
    fn test_serde_from_json() -> Result<(), Box<dyn std::error::Error>> {
        let v: Value = serde_json::from_str("null")?;
        assert_eq!(v, Value::None);
        let v: Value = serde_json::from_str("true")?;
        assert_eq!(v, Value::Bool(true));
        let v: Value = serde_json::from_str("123")?;
        assert_eq!(v, Value::I32(123));
        let v: Value = serde_json::from_str("123456789123456789")?;
        assert_eq!(v, Value::I64(123456789123456789));
        let v: Value = serde_json::from_str("123.456")?;
        assert_eq!(v, Value::F64(123.456));
        let v: Value = serde_json::from_str("[null, true, 123]")?;
        assert_eq!(
            v,
            Value::Array(vec![Value::None, Value::Bool(true), Value::I32(123)])
        );
        let v: Value = serde_json::from_str("{\"a\":123,\"test\":null}")?;
        if let Value::Object(x) = v {
            assert_eq!(x.len(), 2);
            assert_eq!(x["a"], Value::I32(123));
            assert_eq!(x["test"], Value::None);
        } else {
            assert!(false);
        }
        Ok(())
    }

    #[test]
    fn test_from_vec_i64() -> Result<(), Box<dyn std::error::Error>> {
        let v = Value::from(vec![1_i64, 2_i64, 3_i64]);
        let json = v.try_into_json_value()?;
        assert_eq!(json, serde_json::json!([1, 2, 3]));
        Ok(())
    }

    /// `vts7.1` — every variant's identifier has a description, and they agree.
    #[test]
    fn type_descriptions_match_identifier() {
        let samples = sample_values();
        let descriptions = Value::type_descriptions();
        for value in &samples {
            let identifier = value.identifier();
            let info = descriptions
                .iter()
                .find(|info| info.type_identifier == identifier)
                .unwrap_or_else(|| panic!("no description for identifier {identifier:?}"));
            assert_eq!(info.type_name, value.type_name());
            assert_eq!(info.default_extension, value.default_extension());
            assert_eq!(info.default_media_type, value.default_media_type());
            assert_eq!(info.default_filename, value.default_filename());
        }
        assert_eq!(
            descriptions.len(),
            samples.len(),
            "one description per variant, no more and no less"
        );
    }

    /// `vts7.2` — a value round-trips through its own declared identifier.
    ///
    /// **This test fails before the identifier change.** `Value::I32(7).identifier()` used to be
    /// `"generic"` while the text deserializer branched on `"i32"`, so an integer written as text
    /// came back as `Value::Text("7")` — silent corruption, and the substance of
    /// `CORE-METADATA-FORMAT-TYPE-CONSISTENCY`.
    #[test]
    fn scalar_identifiers_round_trip_through_the_serializer(
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Formats that round-trip, which is narrower than what a type may be *written* as: `None`
        // as text produces `none`, which the text branch has no rule to read, and `Text` as bytes
        // legitimately reads back as `Bytes`.
        let cases: Vec<(Value, &[&str])> = vec![
            (Value::None, &["json"]),
            (Value::Bool(true), &["json", "txt"]),
            (Value::I32(7), &["json", "txt"]),
            (Value::I64(-9_000_000_000), &["json", "txt"]),
            (Value::F64(2.5), &["json", "txt"]),
            (Value::Text("hello".to_string()), &["json", "txt", "html"]),
            (Value::Bytes(vec![1, 2, 3]), &["json", "b"]),
        ];
        for (value, formats) in cases {
            let identifier = value.identifier();
            for format in formats {
                assert!(
                    value.supports_data_format(format),
                    "{identifier} must declare support for {format}"
                );
                let bytes = value.as_bytes(format)?;
                let back: Value =
                    DefaultValueSerializer::deserialize_from_bytes(&bytes, &identifier, format)?;
                assert_eq!(
                    back.identifier(),
                    identifier,
                    "{identifier} written as {format} must read back as the same type"
                );
                assert_eq!(back, value, "{identifier} as {format} must round-trip");
            }
        }
        Ok(())
    }

    /// `vts7.3` — a type must support its own default data format.
    #[test]
    fn every_default_is_in_supported_formats() {
        for info in Value::type_descriptions() {
            assert!(
                info.supports_data_format(&info.default_data_format),
                "{} does not support its own default format {}",
                info.type_identifier,
                info.default_data_format
            );
        }
    }

    /// `vts7.4` — the instance method and the registry give the same answer.
    #[test]
    fn supports_data_format_agrees_with_the_registry(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let registry = crate::type_system::TypeRegistry::from_value_type::<Value>();
        for value in sample_values() {
            for format in ["json", "txt", "b", "parquet"] {
                assert_eq!(
                    value.supports_data_format(format),
                    registry.supports_data_format(&value.identifier(), format),
                    "{} / {format} must agree between value and registry",
                    value.identifier()
                );
            }
        }
        Ok(())
    }

    /// One value per `Value` variant. A new variant makes this fail to compile, which is the
    /// point: the description list must grow with the enum.
    fn sample_values() -> Vec<Value> {
        vec![
            Value::None,
            Value::Bool(true),
            Value::I32(1),
            Value::I64(1),
            Value::F64(1.0),
            Value::Text(String::new()),
            Value::Array(Vec::new()),
            Value::Object(BTreeMap::new()),
            Value::Bytes(Vec::new()),
            Value::Metadata(MetadataRecord::new()),
            Value::AssetInfo(Vec::new()),
            Value::Recipe(Recipe::default()),
            Value::CommandMetadata(CommandMetadata::new("test")),
            Value::Query(crate::query::Query::new()),
            Value::Key(crate::query::Key::new()),
        ]
    }
}
