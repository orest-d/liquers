use serde_json;

use liquers_core::{
    error::ErrorType,
    value::{self, DefaultValueSerializer, ValueInterface},
};
use pyo3::{
    prelude::*,
    types::{PyDict, PyList},
};

use liquers_core::error::Error;
use std::{
    borrow::Cow,
    collections::BTreeMap,
    convert::{TryFrom, TryInto},
    fmt::format,
    result::Result,
};

use crate::{
    command_metadata::CommandMetadata,
    metadata::MetadataRecord,
    parse::{Key, Query},
    recipes::Recipe,
};

#[derive(Debug, Clone)]
#[pyclass]
pub enum Value {
    None {},
    Bool { value: bool },
    I32 { value: i32 },
    I64 { value: i64 },
    F64 { value: f64 },
    Text { value: String },
    Array { value: Vec<Value> },
    Object { value: BTreeMap<String, Value> },
    Bytes { value: Vec<u8> },
    Metadata { value: MetadataRecord },
    Recipe { value: Recipe },
    CommandMetadata { value: CommandMetadata },
    Query { value: Query },
    Key { value: Key },
    AssetInfo { value: Vec<crate::metadata::AssetInfo> },
    Py { value: Py<PyAny> },
}

#[pymethods]
impl Value {
    #[new]
    pub fn new(x: PyObject) -> Self {
        Python::with_gil(|py| {
            if x.is_none(py) {
                Value::None {}
            } else if let Ok(b) = x.extract::<bool>(py) {
                Value::Bool { value: b }
            } else if let Ok(n) = x.extract::<i32>(py) {
                Value::I32 { value: n }
            } else if let Ok(n) = x.extract::<i64>(py) {
                Value::I64 { value: n }
            } else if let Ok(n) = x.extract::<f64>(py) {
                Value::F64 { value: n }
            } else if let Ok(t) = x.extract::<String>(py) {
                Value::Text { value: t }
            } else if let Ok(b) = x.extract::<Vec<u8>>(py) {
                Value::Bytes { value: b }
            } else {
                Value::Py { value: x.into() }
            }
        })
    }
    pub fn as_pyobject(&self, py: Python) -> PyResult<PyObject> {
        Ok(value_to_pyobject(self, py).map_err(|e| crate::error::Error::from(e))?)
    }
    pub fn __str__(&self) -> PyResult<String> {
        match self {
            Value::None {} => Ok("None".into()),
            Value::Bool { value } => {
                if *value {
                    Ok("True".into())
                } else {
                    Ok("False".into())
                }
            }
            Value::I32 { value } => Ok(format!("{value}")),
            Value::I64 { value } => Ok(format!("{value}")),
            Value::F64 { value } => Ok(format!("{value}")),
            Value::Text { value } => Ok(value.to_owned()),
            Value::Array { value } => Ok(format!(
                "[{}]",
                value
                    .iter()
                    .map(|x| x.__str__().unwrap_or("?".into()))
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
            Value::Object { value } => Ok(format!(
                "{{{}}}",
                value
                    .iter()
                    .map(|(k, v)| format!(
                        "\"{}\":{}",
                        k.escape_default(),
                        v.__str__().unwrap_or("?".into())
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
            Value::Bytes { value } => Ok(format!("{:?}", value)),
            Value::Py { value } => Python::with_gil(|py| Ok(value.bind(py).str()?.to_string())),
            Value::Metadata { value } => Ok(format!("{:?}", value)),
            Value::AssetInfo { value } => Ok(format!("{:?}", value)),
            Value::Recipe { value } => Ok(format!("{:?}", value)),
            Value::CommandMetadata { value } => Ok(format!("{:?}", value)),
            Value::Query { value } => Ok(value.encode()),
            Value::Key { value } => Ok(value.encode()),
        }
    }
    pub fn __repr__(&self) -> PyResult<String> {
        match self {
            Value::None {} => Ok("None".into()),
            Value::Bool { value } => {
                if *value {
                    Ok("True".into())
                } else {
                    Ok("False".into())
                }
            }
            Value::I32 { value } => Ok(format!("{value}")),
            Value::I64 { value } => Ok(format!("{value}")),
            Value::F64 { value } => Ok(format!("{value}")),
            Value::Text { value } => Ok(format!("\"{}\"", value.escape_default())),
            Value::Array { value } => Ok(format!(
                "[{}]",
                value
                    .iter()
                    .map(|x| x.__repr__().unwrap_or("?".into()))
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
            Value::Object { value } => Ok(format!(
                "{{{}}}",
                value
                    .iter()
                    .map(|(k, v)| format!(
                        "\"{}\":{}",
                        k.escape_default(),
                        v.__repr__().unwrap_or("?".into())
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
            Value::Bytes { value } => Ok(format!("{:?}", value)),
            Value::Py { value } => Python::with_gil(|py| Ok(value.bind(py).repr()?.to_string())),
            Value::Metadata { value } => Ok(format!("{:?}", value)),
            Value::AssetInfo { value } => Ok(format!("{:?}", value)),
            Value::Recipe { value } => Ok(format!("{:?}", value)),
            Value::CommandMetadata { value } => Ok(format!("{:?}", value)),
            Value::Query { value } => Ok(value.__repr__()),
            Value::Key { value } => Ok(value.__repr__()),
        }
    }
}

pub fn value_to_pyobject(v: &Value, py: Python) -> Result<PyObject, liquers_core::error::Error> {
    match v {
        Value::None {} => Ok(py.None()),
        Value::Bool { value } => Ok(value.to_object(py)),
        Value::I32 { value } => Ok(value.to_object(py)),
        Value::I64 { value } => Ok(value.to_object(py)),
        Value::F64 { value } => Ok(value.to_object(py)),
        Value::Text { value } => Ok(value.to_object(py)),
        Value::Array { value } => {
            let list = PyList::empty_bound(py);
            for x in value {
                list.append(value_to_pyobject(x, py)?).map_err(|e| {
                    liquers_core::error::Error::execution_error(format!(
                        "Error appending to python list: {e}"
                    ))
                })?;
            }
            Ok(list.into())
        }
        Value::Object { value } => {
            let dict = PyDict::new_bound(py);
            for (k, v) in value {
                let x = value_to_pyobject(v, py)?;
                dict.set_item(k, x).map_err(|e| {
                    liquers_core::error::Error::execution_error(format!(
                        "Error setting item '{k}' in python dictionary: {e}"
                    ))
                })?;
            }
            Ok(dict.into())
        }
        Value::Bytes { value } => Ok(value.to_object(py)),
        Value::Py { value } => Ok(value.clone_ref(py)),
        Value::Metadata { value } => Ok(value.clone().into_py(py)),
        Value::AssetInfo { value } => Ok(value.clone().into_py(py)),
        Value::Recipe { value } => Ok(value.clone().into_py(py)),
        Value::CommandMetadata { value } => Ok(value.clone().into_py(py)),
        Value::Query { value } => Ok(value.clone().into_py(py)),
        Value::Key { value } => Ok(value.clone().into_py(py)),
    }
}

/// The type identifier of a retained Python object.
///
/// `py.Object`, not a bare `Object`: a bare name asserts that Liquers owns the concept, and a
/// Python object is somebody else's type. It is a **constant** because the identifier appears in
/// both `identifier()` and `type_descriptions()`, and a registry entry that does not match what a
/// value reports is exactly the failure this crate's descriptions exist to prevent.
pub const PY_OBJECT_TYPE_IDENTIFIER: &str = "py.Object";

impl ValueInterface for Value {
    /// One description per variant, identifiers shared with `liquers_core::value::Value`.
    ///
    /// Without this the registry holds only the `error` pseudo-type, and **every** write through
    /// a `PyEnvironment` is refused — `validate_metadata_hard` rejects any identifier the registry
    /// does not contain. See `specs/issues/PY-VALUE-TYPE-DESCRIPTIONS-MISSING.md`.
    fn type_descriptions() -> Vec<liquers_core::type_system::TypeInfo> {
        use liquers_core::type_system::TypeInfo;
        vec![
            TypeInfo::new("None")
                .with_type_name("none")
                .with_defaults("json", "json", "application/json", "data.json")
                .with_data_formats(["json"]),
            TypeInfo::new("Bool")
                .with_type_name("bool")
                .with_defaults("json", "json", "application/json", "data.json")
                .with_data_formats(["json"]),
            TypeInfo::new("I32")
                .with_type_name("i32")
                .with_defaults("json", "json", "application/json", "data.json")
                .with_data_formats(["json"]),
            TypeInfo::new("I64")
                .with_type_name("i64")
                .with_defaults("json", "json", "application/json", "data.json")
                .with_data_formats(["json"]),
            TypeInfo::new("F64")
                .with_type_name("f64")
                .with_defaults("json", "json", "application/json", "data.json")
                .with_data_formats(["json"]),
            TypeInfo::new("Text")
                .with_type_name("text")
                .with_defaults("txt", "txt", "text/plain", "text.txt")
                .with_data_formats(["txt", "json"]),
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
                .with_data_formats(["b", "json"]),
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
                .with_data_formats(["txt", "json"]),
            TypeInfo::new("Key")
                .with_type_name("key")
                .with_defaults("txt", "txt", "text/plain", "key.txt")
                .with_data_formats(["txt", "json"]),
            // Declares **no** data formats: a retained Python object has no byte form here.
            // `pickle` is the extension it would use, but nothing in this crate serializes one,
            // and declaring a format the code cannot produce would let `set_binary` accept bytes
            // that can never be materialized.
            TypeInfo::new(PY_OBJECT_TYPE_IDENTIFIER)
                .with_type_name("python_value")
                .with_defaults(
                    "pickle",
                    "pickle",
                    "application/octet-stream",
                    "data.pickle",
                ),
        ]
    }

    fn try_into_query(&self) -> Result<liquers_core::query::Query, Error> {
        match self {
            // `crate::parse::Query` is the `#[pyclass]` wrapper; the trait wants the core type,
            // which is the newtype's single field.
            Value::Query { value } => Ok(value.0.clone()),
            Value::Text { value: s } => crate::parse::parse(s)
                .map(|q| q.0)
                .map_err(|e| Error::from_error(ErrorType::ParseError, e)),
            _ => Err(Error::conversion_error(self.identifier(), "Query")),
        }
    }
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
            // TODO: Implement this properly
            Value::Py { value: _ } => Err(Error::not_supported(
                "Py value conversion to JSON".to_owned(),
            )),
            _ => Err(Error::conversion_error(self.identifier(), "JSON value")),
        }
    }

    /// Bare CamelCase names, matching `liquers_core::value::Value` variant for variant, so a
    /// store written from Python is readable from Rust and back.
    ///
    /// These used to collapse six variants onto `"generic"` and spell the rest in lowercase —
    /// the model `value-type-system` removed from `liquers-core`, left behind here because this
    /// file was never part of the crate. `python_value` could not have been registered at all:
    /// `_` is a reserved character, and `identifier_naming_rule_holds` rejects it.
    fn identifier(&self) -> Cow<'static, str> {
        match self {
            Value::None {} => "None".into(),
            Value::Bool { value: _ } => "Bool".into(),
            Value::I32 { value: _ } => "I32".into(),
            Value::I64 { value: _ } => "I64".into(),
            Value::F64 { value: _ } => "F64".into(),
            Value::Text { value: _ } => "Text".into(),
            Value::Array { value: _ } => "Array".into(),
            Value::Object { value: _ } => "Object".into(),
            Value::Bytes { value: _ } => "Bytes".into(),
            Value::Py { value: _ } => PY_OBJECT_TYPE_IDENTIFIER.into(),
            Value::Metadata { value: _ } => "Metadata".into(),
            Value::AssetInfo { value: _ } => "AssetInfo".into(),
            Value::Recipe { value: _ } => "Recipe".into(),
            Value::CommandMetadata { value: _ } => "CommandMetadata".into(),
            Value::Query { value: _ } => "Query".into(),
            Value::Key { value: _ } => "Key".into(),
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
            Value::Py { value: _ } => "python_value".into(),
            Value::Metadata { value: _ } => "metadata".into(),
            Value::AssetInfo { value: _ } => "asset_info".into(),
            Value::Recipe { value: _ } => "recipe".into(),
            Value::CommandMetadata { value: _ } => "command_metadata".into(),
            Value::Query { value: _ } => "query".into(),
            Value::Key { value: _ } => "key".into(),
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
            Value::Py { value: _ } => "pickle".into(),
            Value::Metadata { value: _ } => "json".into(),
            Value::AssetInfo { value: _ } => "json".into(),
            Value::Recipe { value: _ } => "json".into(),
            Value::CommandMetadata { value: _ } => "json".into(),
            Value::Query { value: _ } => "txt".into(),
            Value::Key { value: _ } => "txt".into(),
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
            Value::Py { value: _ } => "data.pickle".into(),
            Value::Metadata { value: _ } => "metadata.json".into(),
            Value::AssetInfo { value: _ } => "asset_info.json".into(),
            Value::Recipe { value: _ } => "recipe.json".into(),
            Value::CommandMetadata { value: _ } => "command_metadata.json".into(),
            Value::Query { value: _ } => "query.txt".into(),
            Value::Key { value: _ } => "key.txt".into(),
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
            Value::Py { value: _ } => "application/octet-stream".into(),
            Value::Metadata { value: _ } => "application/json".into(),
            Value::AssetInfo { value: _ } => "application/json".into(),
            Value::Recipe { value: _ } => "application/json".into(),
            Value::CommandMetadata { value: _ } => "application/json".into(),
            Value::Query { value: _ } => "text/plain".into(),
            Value::Key { value: _ } => "text/plain".into(),
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
        Value::Metadata {
            value: MetadataRecord { inner: metadata },
        }
    }

    fn from_recipe(recipe: liquers_core::recipes::Recipe) -> Self {
        Value::Recipe {
            value: Recipe { inner: recipe },
        }
    }

    fn from_query(query: &liquers_core::query::Query) -> Self {
        Value::Query {
            value: Query(query.clone()),
        }
    }

    fn from_key(key: &liquers_core::query::Key) -> Self {
        Value::Key {
            value: Key(key.clone()),
        }
    }

    fn from_asset_info(asset_info: Vec<liquers_core::metadata::AssetInfo>) -> Self {
        Value::AssetInfo {
            value: asset_info
                .into_iter()
                .map(|inner| crate::metadata::AssetInfo { inner })
                .collect(),
        }
    }

    fn from_command_metadata(
        command_metadata: liquers_core::command_metadata::CommandMetadata,
    ) -> Self {
        Value::CommandMetadata {
            value: CommandMetadata(command_metadata),
        }
    }

    fn try_into_bytes(&self) -> Result<Vec<u8>, Error> {
        match self {
            Value::Bytes { value: b } => Ok(b.clone()),
            _ => Err(Error::conversion_error(self.identifier(), "bytes")),
        }
    }

    fn try_into_key(&self) -> Result<liquers_core::query::Key, Error> {
        match self {
            // As with `try_into_query`: unwrap the `#[pyclass]` newtype.
            Value::Key { value } => Ok(value.0.clone()),
            Value::Text { value: s } => liquers_core::parse::parse_key(s),
            _ => Err(Error::conversion_error(self.identifier(), "key")),
        }
    }

    fn try_into_command_metadata(
        &self,
    ) -> Result<liquers_core::command_metadata::CommandMetadata, Error> {
        match self {
            Value::CommandMetadata { value } => Ok(value.0.clone()),
            _ => Err(Error::conversion_error(
                self.identifier(),
                "command metadata",
            )),
        }
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
                Value::Query { value: x } => Ok(x.encode().as_bytes().to_vec()),
                Value::Key { value: x } => Ok(x.encode().as_bytes().to_vec()),
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
    fn deserialize_from_bytes(b: &[u8], _type_identifier: &str, fmt: &str) -> Result<Self, Error> {
        match fmt {
            _ => Err(Error::new(
                ErrorType::SerializationError,
                format!("Unsupported format in from_bytes:{}", fmt),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use liquers_core::type_system::TypeRegistry;

    /// Every variant this crate can hold, except `Py`.
    ///
    /// **GIL-free by necessity.** `pyo3` is built here with `extension-module` and without
    /// `auto-initialize`, so a test calling `Python::with_gil` has no interpreter to attach to.
    /// `Value::Py` is therefore covered by [`PY_OBJECT_TYPE_IDENTIFIER`] appearing in both
    /// `identifier()` and `type_descriptions()` rather than by a constructed sample.
    fn samples() -> Vec<Value> {
        vec![
            Value::None {},
            Value::Bool { value: true },
            Value::I32 { value: 1 },
            Value::I64 { value: 1 },
            Value::F64 { value: 1.0 },
            Value::Text {
                value: "x".to_owned(),
            },
            Value::Array { value: Vec::new() },
            Value::Object {
                value: BTreeMap::new(),
            },
            Value::Bytes { value: Vec::new() },
            Value::Metadata {
                value: MetadataRecord {
                    inner: liquers_core::metadata::MetadataRecord::new(),
                },
            },
            Value::AssetInfo { value: Vec::new() },
            Value::Recipe {
                value: Recipe {
                    inner: liquers_core::recipes::Recipe::default(),
                },
            },
            Value::CommandMetadata {
                value: CommandMetadata(liquers_core::command_metadata::CommandMetadata::new(
                    "test",
                )),
            },
            Value::Query {
                value: crate::parse::Query(liquers_core::query::Query::new()),
            },
            Value::Key {
                value: crate::query::Key(liquers_core::query::Key::new()),
            },
        ]
    }

    /// `fvt6.1` — every variant has a description, and the two agree.
    ///
    /// Mirrors `liquers-core`'s `vts7.1`. Before this, `type_descriptions()` was the empty
    /// default, so the registry held only `error` and every write through a `PyEnvironment`
    /// would have been refused.
    #[test]
    fn type_descriptions_match_identifier() {
        let descriptions = Value::type_descriptions();

        for value in samples() {
            let identifier = value.identifier();
            let info = descriptions
                .iter()
                .find(|info| info.type_identifier == identifier)
                .unwrap_or_else(|| panic!("no description for identifier {identifier:?}"));
            assert_eq!(info.type_name, value.type_name(), "for {identifier}");
            assert_eq!(
                info.default_extension,
                value.default_extension(),
                "for {identifier}"
            );
            assert_eq!(
                info.default_media_type,
                value.default_media_type(),
                "for {identifier}"
            );
            assert_eq!(
                info.default_filename,
                value.default_filename(),
                "for {identifier}"
            );
        }

        // The samples are every variant but `Py`, which needs an interpreter to construct.
        assert_eq!(
            descriptions.len(),
            samples().len() + 1,
            "one description per variant, no more and no less"
        );
        assert!(
            descriptions
                .iter()
                .any(|info| info.type_identifier == PY_OBJECT_TYPE_IDENTIFIER),
            "and the extra one is the Python object"
        );
    }

    /// `fvt6.2` — every identifier satisfies the naming rule.
    ///
    /// Mirrors `identifier_naming_rule_holds`. `python_value` failed this: `_` is a reserved
    /// character, so that identifier could not have been registered even had it been described.
    #[test]
    fn identifiers_follow_the_naming_rule() {
        for info in Value::type_descriptions() {
            let id = &info.type_identifier;
            assert!(!id.is_empty(), "an identifier must not be empty");
            assert!(
                id.chars().all(|c| c.is_ascii_alphanumeric() || c == '.'),
                "identifier {id:?} may contain only alphanumerics and a single dot"
            );
            assert!(
                id.matches('.').count() <= 1,
                "identifier {id:?} may carry at most one dot"
            );
            if let Some((provider, local)) = id.split_once('.') {
                assert!(
                    !provider.is_empty()
                        && provider
                            .chars()
                            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()),
                    "provider of {id:?} must be lowercase"
                );
                assert!(!local.is_empty(), "local name of {id:?} must not be empty");
            }
        }
    }

    /// `fvt6.3` — the shared variants report the same identifiers as `liquers-core`.
    ///
    /// This is the cross-language guarantee: a store written from Python is readable from Rust,
    /// because both sides name a text value `Text` rather than one saying `text` and the other
    /// `Text`. `py.Object` is this crate's alone and is excluded.
    #[test]
    fn shared_variants_match_the_core_identifiers() {
        let core: Vec<String> = liquers_core::value::Value::type_descriptions()
            .into_iter()
            .map(|info| info.type_identifier.to_string())
            .collect();

        for info in Value::type_descriptions() {
            let id = info.type_identifier.to_string();
            if id == PY_OBJECT_TYPE_IDENTIFIER {
                continue;
            }
            assert!(
                core.contains(&id),
                "identifier {id:?} is not one liquers-core knows: {core:?}"
            );
        }
    }

    /// `fvt6.4` — the repaired conversions refuse rather than panic.
    ///
    /// `from_asset_info` was `todo!()` — a panic on a supported path, harmless only because this
    /// file was never compiled. The four methods added with it must error on the wrong variant.
    #[test]
    fn repaired_conversions_error_rather_than_panic() {
        let text = Value::Text {
            value: "x".to_owned(),
        };

        assert!(text.try_into_bytes().is_err());
        assert!(text.try_into_command_metadata().is_err());
        assert!(
            Value::Bytes { value: Vec::new() }.try_into_key().is_err(),
            "bytes are not a key"
        );

        // And they succeed on the right one.
        assert_eq!(Value::from_bytes(vec![1, 2]).try_into_bytes(), Ok(vec![1, 2]));
        assert!(
            Value::from_asset_info(Vec::new()).identifier() == "AssetInfo",
            "from_asset_info builds the AssetInfo variant instead of panicking"
        );
    }

    /// The registry a `PyEnvironment` builds now describes the whole value type.
    #[test]
    fn the_registry_describes_every_variant() {
        let registry = TypeRegistry::from_value_type::<Value>();

        assert!(registry.contains("Text"));
        assert!(registry.contains(PY_OBJECT_TYPE_IDENTIFIER));
        assert!(
            !registry.contains("error"),
            "there is no error type: an errored state is typed by the value it holds, which is none"
        );
        assert!(
            !registry.contains("generic"),
            "the collapsed identifier is gone"
        );
    }
}
