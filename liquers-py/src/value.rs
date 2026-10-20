use serde_json;

use liquers_core::{
    error::ErrorType,
    value::{DefaultValueSerializer, ValueInterface},
};
use pyo3::{
    prelude::*,
    types::{PyDict, PyList},
    IntoPyObjectExt,
};

use liquers_core::error::Error;
use std::{borrow::Cow, collections::BTreeMap, convert::TryFrom, result::Result};

use crate::{
    command_metadata::CommandMetadata,
    metadata::MetadataRecord,
    parse::{Key, Query},
    recipes::Recipe,
};

// `Clone` is implemented by hand below: `Py<PyAny>` is not `Clone` without PyO3's `py-clone`
// feature, because copying a Python reference needs the interpreter.
#[derive(Debug)]
#[pyclass(from_py_object)]
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
    AssetInfo {
        value: Vec<crate::metadata::AssetInfo>,
    },
    Py {
        value: Py<PyAny>,
    },
}

impl Clone for Value {
    fn clone(&self) -> Self {
        match self {
            Value::None {} => Value::None {},
            Value::Bool { value } => Value::Bool { value: *value },
            Value::I32 { value } => Value::I32 { value: *value },
            Value::I64 { value } => Value::I64 { value: *value },
            Value::F64 { value } => Value::F64 { value: *value },
            Value::Text { value } => Value::Text {
                value: value.clone(),
            },
            Value::Array { value } => Value::Array {
                value: value.clone(),
            },
            Value::Object { value } => Value::Object {
                value: value.clone(),
            },
            Value::Bytes { value } => Value::Bytes {
                value: value.clone(),
            },
            Value::Metadata { value } => Value::Metadata {
                value: value.clone(),
            },
            Value::Recipe { value } => Value::Recipe {
                value: value.clone(),
            },
            Value::CommandMetadata { value } => Value::CommandMetadata {
                value: value.clone(),
            },
            Value::Query { value } => Value::Query {
                value: value.clone(),
            },
            Value::Key { value } => Value::Key {
                value: value.clone(),
            },
            Value::AssetInfo { value } => Value::AssetInfo {
                value: value.clone(),
            },
            Value::Py { value } => Value::Py {
                value: Python::attach(|py| value.clone_ref(py)),
            },
        }
    }
}

#[pymethods]
impl Value {
    #[new]
    pub fn new(x: Bound<'_, PyAny>) -> Self {
        if x.is_none() {
            Value::None {}
        } else if let Ok(b) = x.extract::<bool>() {
            Value::Bool { value: b }
        } else if let Ok(n) = x.extract::<i32>() {
            Value::I32 { value: n }
        } else if let Ok(n) = x.extract::<i64>() {
            Value::I64 { value: n }
        } else if let Ok(n) = x.extract::<f64>() {
            Value::F64 { value: n }
        } else if let Ok(t) = x.extract::<String>() {
            Value::Text { value: t }
        } else if let Ok(b) = x.extract::<Vec<u8>>() {
            Value::Bytes { value: b }
        } else {
            Value::Py { value: x.unbind() }
        }
    }
    pub fn as_pyobject(&self, py: Python) -> PyResult<Py<PyAny>> {
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
            Value::Py { value } => Python::attach(|py| Ok(value.bind(py).str()?.to_string())),
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
            Value::Py { value } => Python::attach(|py| Ok(value.bind(py).repr()?.to_string())),
            Value::Metadata { value } => Ok(format!("{:?}", value)),
            Value::AssetInfo { value } => Ok(format!("{:?}", value)),
            Value::Recipe { value } => Ok(format!("{:?}", value)),
            Value::CommandMetadata { value } => Ok(format!("{:?}", value)),
            Value::Query { value } => Ok(value.__repr__()),
            Value::Key { value } => Ok(value.__repr__()),
        }
    }
}

/// Convert a Rust value to a Python object, reporting a failure as a Liquers error.
fn into_py_any<'py, T: IntoPyObjectExt<'py>>(
    value: T,
    py: Python<'py>,
    what: &str,
) -> Result<Py<PyAny>, liquers_core::error::Error> {
    value.into_py_any(py).map_err(|e| {
        liquers_core::error::Error::execution_error(format!(
            "Error converting {what} to a python object: {e}"
        ))
    })
}

pub fn value_to_pyobject(v: &Value, py: Python) -> Result<Py<PyAny>, liquers_core::error::Error> {
    match v {
        Value::None {} => Ok(py.None()),
        Value::Bool { value } => into_py_any(*value, py, "bool"),
        Value::I32 { value } => into_py_any(*value, py, "i32"),
        Value::I64 { value } => into_py_any(*value, py, "i64"),
        Value::F64 { value } => into_py_any(*value, py, "f64"),
        Value::Text { value } => into_py_any(value.as_str(), py, "text"),
        Value::Array { value } => {
            let list = PyList::empty(py);
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
            let dict = PyDict::new(py);
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
        Value::Bytes { value } => into_py_any(value.as_slice(), py, "bytes"),
        Value::Py { value } => Ok(value.clone_ref(py)),
        Value::Metadata { value } => into_py_any(value.clone(), py, "metadata"),
        Value::AssetInfo { value } => into_py_any(value.clone(), py, "asset info"),
        Value::Recipe { value } => into_py_any(value.clone(), py, "recipe"),
        Value::CommandMetadata { value } => into_py_any(value.clone(), py, "command metadata"),
        Value::Query { value } => into_py_any(value.clone(), py, "query"),
        Value::Key { value } => into_py_any(value.clone(), py, "key"),
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

        // The shared variants are described by `liquers-core`, whose codec this crate's
        // serializer delegates to, so the declared formats are exactly the ones it accepts.
        let mut descriptions = CoreValue::type_descriptions();
        // A retained Python object has no byte form: it declares no data format, so the asset
        // layer stores it as metadata only.
        descriptions.push(
            TypeInfo::new(PY_OBJECT_TYPE_IDENTIFIER)
                .with_type_name("python_value")
                .with_defaults(
                    "pickle",
                    "pickle",
                    "application/octet-stream",
                    "data.pickle",
                ),
        );
        descriptions
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
                liquers_core::value::json_f64::to_number(*n)?,
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

type CoreValue = liquers_core::value::Value;

/// The error for a value that holds a retained Python object, which has no byte form
/// (`PY-OBJECT-VALUE-HAS-NO-BYTE-FORM`).
fn python_object_has_no_byte_form(action: &str) -> Error {
    Error::from_error(
        ErrorType::SerializationError,
        format!("{PY_OBJECT_TYPE_IDENTIFIER} has no byte form: cannot {action} a Python object"),
    )
}

/// The `liquers-core` twin of `value`, whose codec this crate's serializer is. Fails on a
/// `Value::Py` anywhere inside, since core has no variant for it.
fn to_core_value(value: &Value) -> Result<CoreValue, Error> {
    Ok(match value {
        Value::None {} => CoreValue::None,
        Value::Bool { value } => CoreValue::Bool(*value),
        Value::I32 { value } => CoreValue::I32(*value),
        Value::I64 { value } => CoreValue::I64(*value),
        Value::F64 { value } => CoreValue::F64(*value),
        Value::Text { value } => CoreValue::Text(value.clone()),
        Value::Array { value } => {
            CoreValue::Array(value.iter().map(to_core_value).collect::<Result<_, _>>()?)
        }
        Value::Object { value } => CoreValue::Object(
            value
                .iter()
                .map(|(k, v)| Ok((k.clone(), to_core_value(v)?)))
                .collect::<Result<_, Error>>()?,
        ),
        Value::Bytes { value } => CoreValue::Bytes(value.clone()),
        Value::Metadata { value } => CoreValue::Metadata(value.inner.clone()),
        Value::Recipe { value } => CoreValue::Recipe(value.inner.clone()),
        Value::CommandMetadata { value } => CoreValue::CommandMetadata(value.0.clone()),
        Value::Query { value } => CoreValue::Query(value.0.clone()),
        Value::Key { value } => CoreValue::Key(value.0.clone()),
        Value::AssetInfo { value } => {
            CoreValue::AssetInfo(value.iter().map(|info| info.inner.clone()).collect())
        }
        Value::Py { value: _ } => return Err(python_object_has_no_byte_form("serialize")),
    })
}

/// The inverse of [`to_core_value`]; total, since every core variant has a twin here.
fn from_core_value(value: CoreValue) -> Value {
    match value {
        CoreValue::None => Value::None {},
        CoreValue::Bool(value) => Value::Bool { value },
        CoreValue::I32(value) => Value::I32 { value },
        CoreValue::I64(value) => Value::I64 { value },
        CoreValue::F64(value) => Value::F64 { value },
        CoreValue::Text(value) => Value::Text { value },
        CoreValue::Array(value) => Value::Array {
            value: value.into_iter().map(from_core_value).collect(),
        },
        CoreValue::Object(value) => Value::Object {
            value: value
                .into_iter()
                .map(|(k, v)| (k, from_core_value(v)))
                .collect(),
        },
        CoreValue::Bytes(value) => Value::Bytes { value },
        CoreValue::Metadata(inner) => Value::Metadata {
            value: MetadataRecord { inner },
        },
        CoreValue::AssetInfo(value) => Value::AssetInfo {
            value: value
                .into_iter()
                .map(|inner| crate::metadata::AssetInfo { inner })
                .collect(),
        },
        CoreValue::Recipe(inner) => Value::Recipe {
            value: Recipe { inner },
        },
        CoreValue::CommandMetadata(value) => Value::CommandMetadata {
            value: CommandMetadata(value),
        },
        CoreValue::Query(value) => Value::Query {
            value: Query(value),
        },
        CoreValue::Key(value) => Value::Key { value: Key(value) },
    }
}

/// The codec is `liquers-core`'s, reached through [`to_core_value`] / [`from_core_value`], so a
/// store written from Python holds the same bytes as one written from Rust, and the two cannot
/// drift. A Python object (`py.Object`) has no byte form and is refused.
impl DefaultValueSerializer for Value {
    fn as_bytes(&self, format: &str) -> Result<Vec<u8>, Error> {
        to_core_value(self)?.as_bytes(format)
    }
    fn deserialize_from_bytes(b: &[u8], type_identifier: &str, fmt: &str) -> Result<Self, Error> {
        if type_identifier == PY_OBJECT_TYPE_IDENTIFIER {
            return Err(python_object_has_no_byte_form("deserialize"));
        }
        CoreValue::deserialize_from_bytes(b, type_identifier, fmt).map(from_core_value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use liquers_core::type_system::TypeRegistry;

    /// Every variant this crate can hold, except `Py`.
    ///
    /// **Interpreter-free by necessity.** `pyo3` is built here without
    /// `auto-initialize`, so a test calling `Python::attach` has no interpreter to attach to.
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

    /// Samples with content, for the codec tests: `samples()` plus non-empty containers and a
    /// real query and key (an empty query encodes as the empty string).
    fn codec_samples() -> Vec<Value> {
        let mut values = samples();
        values.push(Value::Array {
            value: vec![
                Value::I32 { value: 1 },
                Value::Text {
                    value: "a".to_owned(),
                },
            ],
        });
        values.push(Value::Object {
            value: BTreeMap::from([("a".to_owned(), Value::I32 { value: 1 })]),
        });
        values.push(Value::Bytes {
            value: vec![1, 2, 3],
        });
        values.push(Value::Query {
            value: crate::parse::Query(
                liquers_core::parse::parse_query("a/b-c").expect("query"),
            ),
        });
        values.push(Value::Key {
            value: crate::query::Key(liquers_core::parse::parse_key("x/y.txt").expect("key")),
        });
        values
    }

    /// The formats `value`'s type declares in `Value::type_descriptions()`.
    fn declared_formats(value: &Value) -> Vec<String> {
        let identifier = value.identifier();
        Value::type_descriptions()
            .into_iter()
            .find(|info| info.type_identifier == identifier)
            .unwrap_or_else(|| panic!("no description for {identifier}"))
            .supported_data_formats
            .iter()
            .map(|format| format.to_string())
            .collect()
    }

    /// `pyser01` (AC-1) — every declared format writes.
    #[test]
    fn pyser01_every_declared_format_writes() {
        for value in codec_samples() {
            for format in declared_formats(&value) {
                let result = value.as_bytes(&format);
                assert!(
                    result.is_ok(),
                    "{} as {format}: {:?}",
                    value.identifier(),
                    result.err()
                );
            }
        }
    }

    /// `pyser02` (AC-2) — a value round-trips wherever core's codec does: the identifier is kept
    /// and the bytes re-encode identically. Everywhere else liquers-py reads back exactly what
    /// core reads back (core's JSON read is identifier-blind for the structured variants,
    /// `CORE-VALUE-JSON-READ-IGNORES-STRUCTURED-IDENTIFIERS`), which `pyser03` asserts.
    #[test]
    fn pyser02_round_trip_where_core_round_trips() -> Result<(), Error> {
        let text = |s: &str| Value::Text {
            value: s.to_owned(),
        };
        let cases: Vec<(Value, &[&str])> = vec![
            (Value::None {}, &["json"]),
            (Value::Bool { value: true }, &["json", "txt"]),
            (Value::I32 { value: 7 }, &["json", "txt"]),
            (
                Value::I64 {
                    value: -9_000_000_000,
                },
                &["json", "txt"],
            ),
            (Value::F64 { value: 2.5 }, &["json", "txt"]),
            (text("hello"), &["json", "txt", "html", "md"]),
            (
                Value::Bytes {
                    value: vec![1, 2, 3],
                },
                &["json", "b"],
            ),
            (
                Value::Array {
                    value: vec![Value::I32 { value: 1 }, text("a")],
                },
                &["json"],
            ),
            (
                Value::Object {
                    value: BTreeMap::from([("a".to_owned(), Value::I32 { value: 1 })]),
                },
                &["json"],
            ),
            (
                Value::Query {
                    value: crate::parse::Query(
                        liquers_core::parse::parse_query("a/b-c").expect("query"),
                    ),
                },
                &["txt"],
            ),
            (
                Value::Key {
                    value: crate::query::Key(
                        liquers_core::parse::parse_key("x/y.txt").expect("key"),
                    ),
                },
                &["txt"],
            ),
        ];
        for (value, formats) in cases {
            let identifier = value.identifier();
            for format in formats {
                let bytes = value.as_bytes(format)?;
                let back = Value::deserialize_from_bytes(&bytes, &identifier, format)?;
                assert_eq!(back.identifier(), identifier, "{identifier} as {format}");
                assert_eq!(back.as_bytes(format)?, bytes, "{identifier} as {format}");
            }
        }

        // The problem example: an object as JSON.
        let object = Value::Object {
            value: BTreeMap::from([("a".to_owned(), Value::I32 { value: 1 })]),
        };
        assert_eq!(object.default_extension(), "json");
        assert_eq!(object.as_bytes("json")?, b"{\"a\":1}".to_vec());
        let back = Value::deserialize_from_bytes(b"{\"a\":1}", "Object", "json")?;
        assert_eq!(back.identifier(), "Object");
        Ok(())
    }

    /// `pyser03` (AC-3) — liquers-py writes the bytes core writes for the same value, and each side
    /// reads the other's bytes to the same value (or both refuse them).
    #[test]
    fn pyser03_bytes_match_core_both_ways() -> Result<(), Error> {
        for value in codec_samples() {
            let core = to_core_value(&value)?;
            let identifier = value.identifier();
            for format in declared_formats(&value) {
                let py_bytes = value.as_bytes(&format)?;
                let core_bytes = core.as_bytes(&format)?;
                assert_eq!(py_bytes, core_bytes, "{identifier} as {format}");

                let py_read = Value::deserialize_from_bytes(&core_bytes, &identifier, &format);
                let core_read = CoreValue::deserialize_from_bytes(&py_bytes, &identifier, &format);
                match (py_read, core_read) {
                    (Ok(py), Ok(core)) => {
                        assert_eq!(to_core_value(&py)?, core, "{identifier} as {format}")
                    }
                    (Err(_), Err(_)) => {}
                    (py, core) => panic!(
                        "{identifier} as {format}: liquers-py read {:?}, core read {:?}",
                        py.map(|v| v.identifier()),
                        core.map(|v| v.identifier())
                    ),
                }
            }
        }
        Ok(())
    }

    /// `pyser04` (AC-4) — the shared descriptions are core's, and the only extra one is
    /// `py.Object`.
    #[test]
    fn pyser04_descriptions_match_core() {
        let core = CoreValue::type_descriptions();
        let py = Value::type_descriptions();
        assert_eq!(py.len(), core.len() + 1);
        for info in &py {
            if info.type_identifier == PY_OBJECT_TYPE_IDENTIFIER {
                continue;
            }
            let twin = core
                .iter()
                .find(|c| c.type_identifier == info.type_identifier)
                .unwrap_or_else(|| panic!("{} is not in core", info.type_identifier));
            assert_eq!(info.type_name, twin.type_name, "{}", info.type_identifier);
            assert_eq!(info.default_extension, twin.default_extension);
            assert_eq!(info.default_data_format, twin.default_data_format);
            assert_eq!(info.default_media_type, twin.default_media_type);
            assert_eq!(info.default_filename, twin.default_filename);
            assert_eq!(info.supported_data_formats, twin.supported_data_formats);
        }
    }

    /// `pyser05` (AC-5) — reading a Python object is refused, and its type declares no format.
    #[test]
    fn pyser05_python_object_read_is_refused() {
        let error = Value::deserialize_from_bytes(b"\x80\x04N.", PY_OBJECT_TYPE_IDENTIFIER, "pickle")
            .expect_err("a Python object has no byte form");
        assert_eq!(error.error_type, ErrorType::SerializationError);
        assert!(error.message.contains(PY_OBJECT_TYPE_IDENTIFIER), "{}", error.message);

        let info = Value::type_descriptions()
            .into_iter()
            .find(|info| info.type_identifier == PY_OBJECT_TYPE_IDENTIFIER)
            .expect("py.Object is described");
        assert!(info.supported_data_formats.is_empty());
    }

    /// `pyser06` (AC-5) — writing a Python object, alone or inside an array, is refused.
    #[test]
    fn pyser06_python_object_write_is_refused() {
        Python::initialize();
        Python::attach(|py| {
            let object = Value::Py { value: py.None() };
            let array = Value::Array {
                value: vec![Value::I32 { value: 1 }, object.clone()],
            };
            for value in [object, array] {
                let error = value
                    .as_bytes("json")
                    .expect_err("a Python object has no byte form");
                assert_eq!(error.error_type, ErrorType::SerializationError);
                assert!(error.message.contains(PY_OBJECT_TYPE_IDENTIFIER), "{}", error.message);
            }
        });
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
        assert_eq!(
            Value::from_bytes(vec![1, 2]).try_into_bytes(),
            Ok(vec![1, 2])
        );
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

    /// AC-7 of `specs/design/value-to-json-non-finite-float/`: refused, not a panic.
    #[test]
    fn try_into_json_value_refuses_non_finite() {
        use liquers_core::error::ErrorType;
        for x in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let result = Value::F64 { value: x }.try_into_json_value();
            assert!(matches!(result, Err(ref e) if e.error_type == ErrorType::ConversionError));
        }
    }
}
