#![allow(unused_imports)]
#![allow(dead_code)]

use serde_json;

use std::{borrow::Cow, collections::BTreeMap, result::Result};

use crate::error::{Error, ErrorType};
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
}

/// ValueInterface is a trait that must be implemented by the value type.
/// This is a central trait that defines the minimum set of operations
/// that must be supported by the value type.
pub trait ValueInterface: core::fmt::Debug + Clone + Sized + DefaultValueSerializer{
    /// Empty value
    fn none() -> Self;

    /// Test if value is empty
    fn is_none(&self) -> bool;

    /// From string
    fn new(txt: &str) -> Self;

    /// From string
    fn from_string(txt: String) -> Self;

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

    /// From bytes
    fn from_bytes(b: Vec<u8>) -> Self;

    /// Try to get a string out
    fn try_into_string(&self) -> Result<String, Error>;

    /// Try to get a string out
    fn try_into_string_option(&self) -> Result<Option<String>, Error> {
        if self.is_none() {
            Ok(None)
        } else {
            self.try_into_string().map(|x| Some(x))
        }
    }

    /// Try to get a string out
    fn try_into_i32(&self) -> Result<i32, Error>;

    /// String identifier of the state type
    /// Several types can be linked to the same identifier.
    /// The identifier must be cross-platform
    fn identifier(&self) -> Cow<'static, str>;

    /// String name of the stored type
    /// The type_name is more detailed than identifier.
    /// The identifier does not need to be cross-platform, it serves more for information and debugging
    fn type_name(&self) -> Cow<'static, str>;

    /// Default file extension; determines the default data format
    /// Must be consistent with the default_media_type.
    fn default_extension(&self) -> Cow<'static, str>;

    /// Default file name
    fn default_filename(&self) -> Cow<'static, str>;

    /// Default mime type - must be consistent with the default_extension
    fn default_media_type(&self) -> Cow<'static, str>;

    /// Try to get a JSON-serializable value
    fn try_into_json_value(&self) -> Result<serde_json::Value, Error>;

    /// Try to convert JSON value to value type
    fn try_from_json_value(value:&serde_json::Value) -> Result<Self, Error>{
        match value {
            serde_json::Value::Null => Ok(Self::none()),
            serde_json::Value::Bool(b) => Ok(Self::from_bool(*b)),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Ok(Self::from_i64(i))
                } else if let Some(f) = n.as_f64() {
                    Ok(Self::from_f64(f))
                } else {
                    Err(Error::conversion_error_with_message(value, "i64 or f64", "Invalid JSON number"))
                }
            }
            serde_json::Value::String(s) => Ok(Self::new(s)),
            serde_json::Value::Array(a) => {
                Err(Error::not_supported("JSON Array conversion not supported by default for a generic ValueInterface".to_string()))
            }
            serde_json::Value::Object(o) => {
                Err(Error::not_supported("JSON Object conversion not supported by default for a generic ValueInterface".to_string()))
            }
        }
    }
}

impl ValueInterface for Value {
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
            Value::I32(n) => Ok(format!("{n}")),
            Value::I64(n) => Ok(format!("{n}")),
            Value::F64(n) => Ok(format!("{n}")),
            Value::Text(t) => Ok(t.to_owned()),
            Value::Bytes(b) => {
                Ok(String::from_utf8_lossy(b).to_string())
            },
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
            _ => Err(Error::conversion_error(self.identifier(), "JSON value")),
        }
    }

    fn identifier(&self) -> Cow<'static, str> {
        match self {
            Value::None => "generic".into(),
            Value::Bool(_) => "generic".into(),
            Value::I32(_) => "generic".into(),
            Value::I64(_) => "generic".into(),
            Value::F64(_) => "generic".into(),
            Value::Text(_) => "text".into(),
            Value::Array(_) => "generic".into(),
            Value::Object(_) => "dictionary".into(),
            Value::Bytes(_) => "bytes".into(),
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
    
    fn try_from_json_value(value:&serde_json::Value) -> Result<Self, Error> {
        match value {
            serde_json::Value::Null => Ok(Value::None),
            serde_json::Value::Bool(b) => Ok(Value::Bool(*b)),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Ok(Value::I64(i))
                } else if let Some(f) = n.as_f64() {
                    Ok(Value::F64(f))
                } else {
                    Err(Error::conversion_error_with_message(value, "i64 or f64", "Invalid JSON number"))
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
}

impl TryFrom<&Value> for i32 {
    type Error = Error;
    fn try_from(value: &Value) -> Result<Self, Self::Error> {
        match value {
            Value::I32(x) => Ok(*x),
            Value::I64(x) => i32::try_from(*x).map_err(|e| Error::conversion_error_with_message("I64", "i32", &e.to_string())),
            _ => Err(Error::conversion_error(value.type_name(), "i32")),
        }
    }
}

impl TryFrom<Value> for i32 {
    type Error = Error;
    fn try_from(value: Value) -> Result<Self, Self::Error> {
        match value {
            Value::I32(x) => Ok(x),
            Value::I64(x) => i32::try_from(x).map_err(|e| Error::conversion_error_with_message("I64", "i32", &e.to_string())),
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
impl From<bool> for Value {
    fn from(value: bool) -> Value {
        Value::Bool(value)
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
    fn as_bytes(&self, format: &str) -> Result<Vec<u8>, Error>;
    fn deserialize_from_bytes(b: &[u8], type_identifier:&str, format: &str) -> Result<Self, Error>;
}

impl DefaultValueSerializer for Value {
    fn as_bytes(&self, format: &str) -> Result<Vec<u8>, Error> {
        match format {
            "json" => serde_json::to_vec(self).map_err(|e| {                
                Error::new(ErrorType::SerializationError, format!("JSON error {}", e))
            }),
            "txt" | "html" => match self {
                Value::None => Ok("none".as_bytes().to_vec()),
                Value::Bool(true) => Ok("true".as_bytes().to_vec()),
                Value::Bool(false) => Ok("false".as_bytes().to_vec()),
                Value::I32(x) => Ok(format!("{x}").into_bytes()),
                Value::I64(x) => Ok(format!("{x}").into_bytes()),
                Value::F64(x) => Ok(format!("{x}").into_bytes()),
                Value::Text(x) => Ok(x.as_bytes().to_vec()),
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
    fn deserialize_from_bytes(b: &[u8], _type_identifier:&str, fmt: &str) -> Result<Self, Error> {
        match fmt {
            "json" => serde_json::from_slice(b).map_err(|e| {
                Error::new(
                    ErrorType::SerializationError,
                    format!("JSON error in from_bytes:{}", e),
                )
            }),
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

    #[test]
    fn test1() -> Result<(), Box<dyn std::error::Error>> {
        println!("Hello.");
        let v = Value::I32(123);
        let b = v.as_bytes("json")?;
        println!("Serialized    {:?}: {}", v, std::str::from_utf8(&b)?);
        let w: Value = DefaultValueSerializer::deserialize_from_bytes(&b, "generic", "json")?;
        println!("De-Serialized {:?}", w);
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
}
