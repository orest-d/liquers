//! The value bridge: `liquers_lib::value::Value` to Python objects and back.
//!
//! liquers-py has no value type of its own. Structural Python values map onto `SimpleValue`
//! variants, DataFrames and images onto their `ExtValue` variants, and anything else is retained
//! as an opaque [`PyObjectValue`] inside `ExtValue::Foreign` (`py.Object`), or refused in strict
//! mode. See `specs/design/liquers-py/phase2-architecture.md` §Interfaces.

use std::borrow::Cow;
use std::collections::{BTreeMap, HashSet};
use std::io::Cursor;
use std::sync::Arc;

use liquers_core::error::{Error, ErrorType};
use liquers_core::type_system::TypeInfo;
use liquers_lib::value::foreign::ForeignValue;
use liquers_lib::value::simple::SimpleValue;
use liquers_lib::value::{ExtValue, ExtValueInterface, ValueExtension};
use pyo3::prelude::*;
use pyo3::types::{
    PyBool, PyByteArray, PyBytes, PyDict, PyFloat, PyInt, PyList, PyString, PyTuple,
};
use pyo3::IntoPyObjectExt;

pub use liquers_lib::value::Value;

/// The type identifier of a retained Python object.
///
/// `py.Object`, not a bare `Object`: a bare name asserts that Liquers owns the concept, and a
/// Python object is somebody else's type. One constant, read by both [`PyObjectValue::identifier`]
/// and [`py_type_info`], so the registry and the value cannot disagree.
pub const PY_OBJECT_TYPE_IDENTIFIER: &str = "py.Object";

/// The only data format a `py.Object` can be written in, and only with the codec enabled.
pub const PICKLE_FORMAT: &str = "pickle";

/// How an object with no structural mapping is treated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversionMode {
    /// Retained opaquely as `py.Object` (the default).
    Permissive,
    /// Refused with a `ConversionError` naming the type.
    Strict,
}

impl ConversionMode {
    pub fn parse(name: &str) -> Result<Self, Error> {
        match name {
            "permissive" => Ok(ConversionMode::Permissive),
            "strict" => Ok(ConversionMode::Strict),
            other => Err(Error::configuration_error(format!(
                "unknown conversion mode '{other}'; use 'permissive' or 'strict'"
            ))),
        }
    }
}

/// A Python object retained opaquely inside a Liquers value.
///
/// `Py<PyAny>` is `Send + Sync`, and dropping it without the GIL is deferred by PyO3, so the value
/// can travel through the asset manager like any other.
#[derive(Debug)]
pub struct PyObjectValue {
    object: Py<PyAny>,
    type_name: String,
    /// Whether the environment that created this value enabled the pickle codec.
    pickle: bool,
}

impl PyObjectValue {
    pub fn new(object: &Bound<'_, PyAny>, pickle: bool) -> Self {
        PyObjectValue {
            object: object.clone().unbind(),
            type_name: qualified_type_name(object),
            pickle,
        }
    }

    pub fn object(&self, py: Python<'_>) -> Py<PyAny> {
        self.object.clone_ref(py)
    }
}

/// `module.QualName` of an object's type, for messages and `type_name`.
fn qualified_type_name(object: &Bound<'_, PyAny>) -> String {
    let ty = object.get_type();
    let module = ty
        .module()
        .map(|m| m.to_string())
        .unwrap_or_else(|_| "builtins".to_string());
    let name = ty
        .qualname()
        .map(|n| n.to_string())
        .unwrap_or_else(|_| "object".to_string());
    format!("{module}.{name}")
}

/// The registry's description of `py.Object` for an environment, without an instance.
///
/// The instance side is [`PyObjectValue::type_info`]; a unit test asserts the two agree. With the
/// codec off the type declares **no** formats: the write path then persists such a value as
/// metadata only, and the read path never decodes stored bytes of it (AC-18).
pub fn py_type_info(pickle: bool) -> TypeInfo {
    let info = TypeInfo::new(PY_OBJECT_TYPE_IDENTIFIER)
        .with_type_name("python object")
        .with_defaults(
            PICKLE_FORMAT,
            PICKLE_FORMAT,
            "application/x-python-pickle",
            "data.pickle",
        );
    if pickle {
        info.with_data_formats([PICKLE_FORMAT])
    } else {
        info
    }
}

impl ForeignValue for PyObjectValue {
    fn origin(&self) -> &'static str {
        "python"
    }
    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
    fn identifier(&self) -> Cow<'static, str> {
        PY_OBJECT_TYPE_IDENTIFIER.into()
    }
    fn type_name(&self) -> Cow<'static, str> {
        self.type_name.clone().into()
    }
    fn default_extension(&self) -> Cow<'static, str> {
        PICKLE_FORMAT.into()
    }
    fn default_filename(&self) -> Cow<'static, str> {
        "data.pickle".into()
    }
    fn default_media_type(&self) -> Cow<'static, str> {
        "application/x-python-pickle".into()
    }
    fn type_info(&self) -> TypeInfo {
        // The registry description with the instance's own type name; `type_name` is
        // informational and never a dispatch key.
        py_type_info(self.pickle).with_type_name(self.type_name.clone())
    }
    fn as_bytes(&self, format: &str) -> Result<Vec<u8>, Error> {
        if !self.pickle {
            return Err(Error::from_error(
                ErrorType::SerializationError,
                format!(
                    "{} cannot be serialized: the pickle codec is off in this environment \
                     (lq.Environment(pickle=True) enables it)",
                    self.type_name
                ),
            ));
        }
        if format != PICKLE_FORMAT {
            return Err(Error::from_error(
                ErrorType::SerializationError,
                format!(
                    "{} can only be serialized as '{PICKLE_FORMAT}', not '{format}'",
                    self.type_name
                ),
            ));
        }
        Python::attach(|py| {
            let dumped = py
                .import("pickle")
                .and_then(|pickle| pickle.call_method1("dumps", (self.object.bind(py),)))
                .and_then(|bytes| bytes.extract::<Vec<u8>>())
                .map_err(|e| {
                    Error::from_error(
                        ErrorType::SerializationError,
                        format!("pickling {} failed: {e}", self.type_name),
                    )
                })?;
            Ok(dumped)
        })
    }
}

/// Rebuilds a `py.Object` from pickle bytes. Registered once, at module init.
///
/// Reached only from an environment whose registry lists `pickle` for `py.Object`; a pickle-off
/// environment reads such bytes as degraded and never calls this (AC-18).
pub fn decode_pickle(bytes: &[u8], data_format: &str) -> Result<Arc<dyn ForeignValue>, Error> {
    if data_format != PICKLE_FORMAT {
        return Err(Error::from_error(
            ErrorType::SerializationError,
            format!("{PY_OBJECT_TYPE_IDENTIFIER} cannot be read from '{data_format}'"),
        ));
    }
    Python::attach(|py| {
        let object = py
            .import("pickle")
            .and_then(|pickle| pickle.call_method1("loads", (PyBytes::new(py, bytes),)))
            .map_err(|e| {
                Error::from_error(
                    ErrorType::SerializationError,
                    format!("unpickling {PY_OBJECT_TYPE_IDENTIFIER} failed: {e}"),
                )
            })?;
        let value: Arc<dyn ForeignValue> = Arc::new(PyObjectValue::new(&object, true));
        Ok(value)
    })
}

fn py_conversion_error(what: &str, e: PyErr) -> Error {
    Error::conversion_error_with_message(what, "Python", &e.to_string())
}

fn into_py<'py, T: IntoPyObjectExt<'py>>(
    py: Python<'py>,
    value: T,
    what: &str,
) -> Result<Py<PyAny>, Error> {
    value
        .into_py_any(py)
        .map_err(|e| py_conversion_error(what, e))
}

/// Converts a Liquers value to a Python object. Needs the GIL.
pub fn value_to_py(py: Python<'_>, value: &Value) -> Result<Py<PyAny>, Error> {
    match value {
        Value::Base(simple) => simple_to_py(py, simple),
        Value::Extended(ext) => ext_to_py(py, ext),
    }
}

fn simple_to_py(py: Python<'_>, value: &SimpleValue) -> Result<Py<PyAny>, Error> {
    match value {
        SimpleValue::None {} => Ok(py.None()),
        SimpleValue::Bool { value } => into_py(py, *value, "bool"),
        SimpleValue::I32 { value } => into_py(py, *value, "i32"),
        SimpleValue::I64 { value } => into_py(py, *value, "i64"),
        SimpleValue::F64 { value } => into_py(py, *value, "f64"),
        SimpleValue::Text { value } => into_py(py, value.as_str(), "text"),
        SimpleValue::Bytes { value } => Ok(PyBytes::new(py, value).into_any().unbind()),
        SimpleValue::Array { value } => {
            let list = PyList::empty(py);
            for item in value {
                list.append(simple_to_py(py, item)?)
                    .map_err(|e| py_conversion_error("list", e))?;
            }
            Ok(list.into_any().unbind())
        }
        SimpleValue::Object { value } => {
            let dict = PyDict::new(py);
            for (key, item) in value {
                dict.set_item(key, simple_to_py(py, item)?)
                    .map_err(|e| py_conversion_error("dict", e))?;
            }
            Ok(dict.into_any().unbind())
        }
        SimpleValue::Metadata { value } => into_py(
            py,
            crate::metadata::MetadataRecord {
                inner: value.clone(),
            },
            "metadata",
        ),
        SimpleValue::AssetInfo { value } => {
            let list = PyList::empty(py);
            for info in value {
                let wrapped = into_py(
                    py,
                    crate::metadata::AssetInfo {
                        inner: info.clone(),
                    },
                    "asset info",
                )?;
                list.append(wrapped)
                    .map_err(|e| py_conversion_error("asset info", e))?;
            }
            Ok(list.into_any().unbind())
        }
        SimpleValue::Recipe { value } => into_py(
            py,
            crate::recipes::Recipe {
                inner: value.clone(),
            },
            "recipe",
        ),
        SimpleValue::CommandMetadata { value } => into_py(
            py,
            crate::command_metadata::CommandMetadata(value.clone()),
            "command metadata",
        ),
        SimpleValue::Query { value } => into_py(py, crate::query::Query(value.clone()), "query"),
        SimpleValue::Key { value } => into_py(py, crate::query::Key(value.clone()), "key"),
    }
}

/// The extended variants Python understands.
///
/// Written as `if let` tests rather than one `match`: which `ExtValue` variants exist depends on
/// liquers-lib's features, and Cargo unifies those across a workspace build (a build that also
/// compiles an `egui` user turns `UiCommand` and `Widget` on here). A `match` would therefore be
/// exhaustive in one build and fail in another; every variant not handled below is refused with
/// its identifier, which is the documented behaviour for each of them.
fn ext_to_py(py: Python<'_>, value: &ExtValue) -> Result<Py<PyAny>, Error> {
    if let ExtValue::Foreign { value: foreign } = value {
        return match foreign.as_any().downcast_ref::<PyObjectValue>() {
            Some(object) => Ok(object.object(py)),
            None => Err(Error::conversion_error_with_message(
                foreign.identifier().as_ref(),
                "Python",
                &format!("the value belongs to the {} runtime", foreign.origin()),
            )),
        };
    }
    if let ExtValue::PolarsDataFrame { value: frame } = value {
        let mut parquet = Vec::new();
        liquers_lib::polars::serde::serialize_dataframe_to_writer(frame, "parquet", &mut parquet)?;
        let frame = py
            .import("polars")
            .and_then(|polars| {
                let buffer = py
                    .import("io")?
                    .call_method1("BytesIO", (PyBytes::new(py, &parquet),))?;
                polars.call_method1("read_parquet", (buffer,))
            })
            .map_err(|e| py_conversion_error("polars.DataFrame (needs polars)", e))?;
        return Ok(frame.unbind());
    }
    if let ExtValue::Image { value: image } = value {
        let png = liquers_lib::image::serde::serialize_image_to_bytes(image, "png")?;
        let opened = py
            .import("PIL.Image")
            .and_then(|module| {
                let buffer = py
                    .import("io")?
                    .call_method1("BytesIO", (PyBytes::new(py, &png),))?;
                let img = module.call_method1("open", (buffer,))?;
                // Force the read now, while the buffer is alive.
                img.call_method0("load")?;
                Ok(img)
            })
            .map_err(|e| py_conversion_error("Image (needs Pillow)", e))?;
        return Ok(opened.unbind());
    }
    Err(Error::conversion_error_with_message(
        value.identifier().as_ref(),
        "Python",
        "this value type is not exposed to Python",
    ))
}

/// Converts a Python object to a Liquers value. Needs the GIL.
pub fn py_to_value(
    object: &Bound<'_, PyAny>,
    mode: ConversionMode,
    pickle: bool,
) -> Result<Value, Error> {
    let mut path = HashSet::new();
    if let Some(simple) = py_to_simple(object, &mut path)? {
        return Ok(Value::Base(simple));
    }
    if let Some(ext) = py_to_ext(object)? {
        return Ok(Value::Extended(ext));
    }
    match mode {
        ConversionMode::Permissive => Ok(Value::Extended(ExtValue::Foreign {
            value: Arc::new(PyObjectValue::new(object, pickle)),
        })),
        ConversionMode::Strict => Err(Error::conversion_error_with_message(
            qualified_type_name(object),
            "Value",
            "strict conversion refuses objects with no structural mapping",
        )),
    }
}

/// A structural value, `None` when the object (or something inside it) has no structural form.
///
/// `path` holds the identities of the containers being converted, so a container that contains
/// itself is refused instead of recursing forever (VALUE07). Shared, non-cyclic references are
/// simply copied.
fn py_to_simple(
    object: &Bound<'_, PyAny>,
    path: &mut HashSet<usize>,
) -> Result<Option<SimpleValue>, Error> {
    if object.is_none() {
        return Ok(Some(SimpleValue::None {}));
    }
    // `bool` is a subclass of `int`, so it is tested first.
    if let Ok(b) = object.cast::<PyBool>() {
        return Ok(Some(SimpleValue::Bool { value: b.is_true() }));
    }
    if object.cast::<PyInt>().is_ok() {
        let value = object.extract::<i64>().map_err(|_| {
            Error::conversion_error_with_message(
                "int",
                "I64",
                &format!("{} is outside the 64-bit range", object),
            )
        })?;
        return Ok(Some(SimpleValue::I64 { value }));
    }
    if let Ok(f) = object.cast::<PyFloat>() {
        return Ok(Some(SimpleValue::F64 { value: f.value() }));
    }
    if let Ok(s) = object.cast::<PyString>() {
        let text = s
            .to_str()
            .map_err(|e| py_conversion_error("str", e))?
            .to_string();
        return Ok(Some(SimpleValue::Text { value: text }));
    }
    if let Ok(b) = object.cast::<PyBytes>() {
        return Ok(Some(SimpleValue::Bytes {
            value: b.as_bytes().to_vec(),
        }));
    }
    if let Ok(b) = object.cast::<PyByteArray>() {
        return Ok(Some(SimpleValue::Bytes { value: b.to_vec() }));
    }
    if let Some(wrapped) = wrapper_to_simple(object) {
        return Ok(Some(wrapped));
    }
    let is_sequence = object.cast::<PyList>().is_ok() || object.cast::<PyTuple>().is_ok();
    let is_dict = object.cast::<PyDict>().is_ok();
    if !(is_sequence || is_dict) {
        return Ok(None);
    }
    let identity = object.as_ptr() as usize;
    if !path.insert(identity) {
        return Err(Error::conversion_error_with_message(
            qualified_type_name(object),
            "Value",
            "the container contains itself",
        ));
    }
    let converted = if is_dict {
        dict_to_simple(object, path)
    } else {
        sequence_to_simple(object, path)
    };
    path.remove(&identity);
    converted
}

fn sequence_to_simple(
    object: &Bound<'_, PyAny>,
    path: &mut HashSet<usize>,
) -> Result<Option<SimpleValue>, Error> {
    let mut items = Vec::new();
    let iterator = object
        .try_iter()
        .map_err(|e| py_conversion_error("sequence", e))?;
    for item in iterator {
        let item = item.map_err(|e| py_conversion_error("sequence", e))?;
        match py_to_simple(&item, path)? {
            Some(simple) => items.push(simple),
            None => return Ok(None),
        }
    }
    Ok(Some(SimpleValue::Array { value: items }))
}

fn dict_to_simple(
    object: &Bound<'_, PyAny>,
    path: &mut HashSet<usize>,
) -> Result<Option<SimpleValue>, Error> {
    let dict = object
        .cast::<PyDict>()
        .map_err(|e| Error::conversion_error_with_message("dict", "Object", &e.to_string()))?;
    let mut entries = BTreeMap::new();
    for (key, item) in dict.iter() {
        let Ok(key) = key.cast::<PyString>() else {
            // Only string keys have a structural form.
            return Ok(None);
        };
        let key = key
            .to_str()
            .map_err(|e| py_conversion_error("dict key", e))?
            .to_string();
        match py_to_simple(&item, path)? {
            Some(simple) => {
                entries.insert(key, simple);
            }
            None => return Ok(None),
        }
    }
    Ok(Some(SimpleValue::Object { value: entries }))
}

/// The liquers wrappers that carry a structural Liquers value.
fn wrapper_to_simple(object: &Bound<'_, PyAny>) -> Option<SimpleValue> {
    if let Ok(q) = object.extract::<crate::query::Query>() {
        return Some(SimpleValue::Query { value: q.0 });
    }
    if let Ok(k) = object.extract::<crate::query::Key>() {
        return Some(SimpleValue::Key { value: k.0 });
    }
    if let Ok(r) = object.extract::<crate::recipes::Recipe>() {
        return Some(SimpleValue::Recipe { value: r.inner });
    }
    if let Ok(m) = object.extract::<crate::metadata::MetadataRecord>() {
        return Some(SimpleValue::Metadata { value: m.inner });
    }
    if let Ok(c) = object.extract::<crate::command_metadata::CommandMetadata>() {
        return Some(SimpleValue::CommandMetadata { value: c.0 });
    }
    None
}

/// DataFrames, images and numpy arrays, recognised by type without importing their libraries.
fn py_to_ext(object: &Bound<'_, PyAny>) -> Result<Option<ExtValue>, Error> {
    let type_name = qualified_type_name(object);
    let py = object.py();
    if type_name == "polars.dataframe.frame.DataFrame" || type_name == "polars.DataFrame" {
        let buffer = py
            .import("io")
            .and_then(|io| io.call_method0("BytesIO"))
            .map_err(|e| py_conversion_error("polars.DataFrame", e))?;
        let bytes = object
            .call_method1("write_parquet", (&buffer,))
            .and_then(|_| buffer.call_method0("getvalue"))
            .and_then(|b| b.extract::<Vec<u8>>())
            .map_err(|e| py_conversion_error("polars.DataFrame", e))?;
        let frame = liquers_lib::polars::serde::deserialize_dataframe_from_reader(
            Cursor::new(bytes),
            "parquet",
        )?;
        return Ok(Some(ExtValue::from_polars_dataframe(frame)));
    }
    if is_pil_image(object) {
        let buffer = py
            .import("io")
            .and_then(|io| io.call_method0("BytesIO"))
            .map_err(|e| py_conversion_error("PIL.Image.Image", e))?;
        let bytes = object
            .call_method1("save", (&buffer, "PNG"))
            .and_then(|_| buffer.call_method0("getvalue"))
            .and_then(|b| b.extract::<Vec<u8>>())
            .map_err(|e| py_conversion_error("PIL.Image.Image", e))?;
        let image = liquers_lib::image::serde::deserialize_image_from_bytes(&bytes, "png")?;
        return Ok(Some(ExtValue::from_image(Arc::new(image))));
    }
    Ok(None)
}

/// `isinstance(object, PIL.Image.Image)`, false when Pillow is not imported at all.
fn is_pil_image(object: &Bound<'_, PyAny>) -> bool {
    let py = object.py();
    let Ok(modules) = py.import("sys").and_then(|sys| sys.getattr("modules")) else {
        return false;
    };
    let Ok(image_module) = modules.get_item("PIL.Image") else {
        return false;
    };
    match image_module.getattr("Image") {
        Ok(class) => object.is_instance(&class).unwrap_or(false),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use liquers_core::value::ValueInterface;

    fn roundtrip(py: Python<'_>, code: &str) -> Result<Py<PyAny>, Box<dyn std::error::Error>> {
        let object = py.eval(&std::ffi::CString::new(code)?, None, None)?;
        let value = py_to_value(&object, ConversionMode::Strict, false)?;
        Ok(value_to_py(py, &value)?)
    }

    #[test]
    fn py_type_info_agrees_with_instance() -> Result<(), Box<dyn std::error::Error>> {
        Python::initialize();
        Python::attach(|py| -> Result<(), Box<dyn std::error::Error>> {
            for pickle in [false, true] {
                let object = PyObjectValue::new(&py.None().into_bound(py), pickle);
                let instance = object.type_info();
                let registry = py_type_info(pickle);
                assert_eq!(instance.type_identifier, registry.type_identifier);
                assert_eq!(instance.default_data_format, registry.default_data_format);
                assert_eq!(instance.default_extension, registry.default_extension);
                assert_eq!(
                    instance.supported_data_formats,
                    registry.supported_data_formats
                );
                assert_eq!(object.identifier(), PY_OBJECT_TYPE_IDENTIFIER);
            }
            assert!(py_type_info(false).supported_data_formats.is_empty());
            assert!(py_type_info(true).supports_data_format(PICKLE_FORMAT));
            Ok(())
        })
    }

    #[test]
    fn structural_values_round_trip_with_their_types() -> Result<(), Box<dyn std::error::Error>> {
        Python::initialize();
        Python::attach(|py| -> Result<(), Box<dyn std::error::Error>> {
            for code in [
                "None",
                "True",
                "-9223372036854775808",
                "9223372036854775807",
                "1.5",
                "'text'",
                "b'bytes'",
                "[1, 'a', [None]]",
                "{'a': {'b': [1.0]}}",
            ] {
                let original = py.eval(&std::ffi::CString::new(code)?, None, None)?;
                let back = roundtrip(py, code)?.into_bound(py);
                assert!(back.get_type().is(&original.get_type()), "type of {code}");
                assert!(back.eq(&original)?, "value of {code}");
            }
            Ok(())
        })
    }

    #[test]
    fn bool_is_not_an_int_and_big_ints_are_refused() -> Result<(), Box<dyn std::error::Error>> {
        Python::initialize();
        Python::attach(|py| -> Result<(), Box<dyn std::error::Error>> {
            let t = py.eval(c"True", None, None)?;
            assert!(matches!(
                py_to_value(&t, ConversionMode::Strict, false)?,
                Value::Base(SimpleValue::Bool { value: true })
            ));
            let big = py.eval(c"2**63", None, None)?;
            let err = py_to_value(&big, ConversionMode::Strict, false)
                .err()
                .ok_or("2**63 should be refused")?;
            assert_eq!(err.error_type, ErrorType::ConversionError);
            Ok(())
        })
    }

    #[test]
    fn unknown_objects_follow_the_mode() -> Result<(), Box<dyn std::error::Error>> {
        Python::initialize();
        Python::attach(|py| -> Result<(), Box<dyn std::error::Error>> {
            let object = py.eval(c"object()", None, None)?;
            let value = py_to_value(&object, ConversionMode::Permissive, false)?;
            assert_eq!(value.identifier(), PY_OBJECT_TYPE_IDENTIFIER);
            let back = value_to_py(py, &value)?.into_bound(py);
            assert!(back.is(&object), "permissive mode keeps the same object");
            let err = py_to_value(&object, ConversionMode::Strict, false)
                .err()
                .ok_or("strict mode should refuse")?;
            assert_eq!(err.error_type, ErrorType::ConversionError);
            assert!(err.message.contains("builtins.object"), "{}", err.message);
            Ok(())
        })
    }

    #[test]
    fn a_list_holding_an_object_is_opaque_as_a_whole() -> Result<(), Box<dyn std::error::Error>> {
        Python::initialize();
        Python::attach(|py| -> Result<(), Box<dyn std::error::Error>> {
            let object = py.eval(c"[1, object()]", None, None)?;
            let value = py_to_value(&object, ConversionMode::Permissive, false)?;
            assert_eq!(value.identifier(), PY_OBJECT_TYPE_IDENTIFIER);
            Ok(())
        })
    }

    #[test]
    fn a_cycle_is_refused() -> Result<(), Box<dyn std::error::Error>> {
        Python::initialize();
        Python::attach(|py| -> Result<(), Box<dyn std::error::Error>> {
            let list = PyList::empty(py);
            list.append(&list)?;
            let err = py_to_value(list.as_any(), ConversionMode::Permissive, false)
                .err()
                .ok_or("a cycle should be refused")?;
            assert_eq!(err.error_type, ErrorType::ConversionError);
            // A shared but acyclic reference is fine.
            let shared = py.eval(c"(lambda x: [x, x])([1])", None, None)?;
            py_to_value(&shared, ConversionMode::Strict, false)?;
            Ok(())
        })
    }

    #[test]
    fn pickle_is_refused_unless_enabled_and_round_trips_when_it_is(
    ) -> Result<(), Box<dyn std::error::Error>> {
        Python::initialize();
        Python::attach(|py| -> Result<(), Box<dyn std::error::Error>> {
            let object = py.eval(c"{1, 2, 3}", None, None)?;
            let off = PyObjectValue::new(&object, false);
            let err = off.as_bytes(PICKLE_FORMAT).err().ok_or("pickle is off")?;
            assert_eq!(err.error_type, ErrorType::SerializationError);
            let on = PyObjectValue::new(&object, true);
            let bytes = on.as_bytes(PICKLE_FORMAT)?;
            let decoded = decode_pickle(&bytes, PICKLE_FORMAT)?;
            let decoded = decoded
                .as_any()
                .downcast_ref::<PyObjectValue>()
                .ok_or("decoded value is a PyObjectValue")?;
            assert!(decoded.object(py).into_bound(py).eq(&object)?);
            Ok(())
        })
    }
}
