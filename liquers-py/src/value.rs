use serde_json;

use liquers_core::{
    error::ErrorType,
    value::{self, DefaultValueSerializer, ValueInterface},
};
use pyo3::prelude::*;

use liquers_core::error::Error;
use std::{
    borrow::Cow,
    collections::BTreeMap,
    convert::{TryFrom, TryInto},
    fmt::format,
    result::Result,
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
                    .map(|(k, v)| format!("\"{}\":{}", k.escape_unicode(), v.__str__().unwrap_or("?".into())))
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
            Value::Bytes { value } => Ok(format!("{:?}", value)),
            Value::Py { value } => {
                Python::with_gil(|py| {
                    Ok(value.bind(py).str()?.to_string())
                })

            },
        }
    }
    pub fn __repr__(&self) -> String {
        format!("{:?}", self)
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
            // TODO: Implement this properly
            Value::Py { value: _ } => "python_value".into(),
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
            // TODO: Implement this properly
            Value::Py { value: _ } => "python_value".into(),
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
