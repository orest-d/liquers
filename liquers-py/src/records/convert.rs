//! Cells to Python scalars and back.
//!
//! `FieldValue` has no Python class: a cell is `None`, `bool`, `int`, `float`, `str`, `bytes`,
//! `datetime.date`, a UTC `datetime.datetime` or a `list` of floats. Dates and timestamps are made
//! through the `datetime` module rather than PyO3's datetime types, which the limited API (the
//! abi3 wheel) does not offer.

use std::sync::Arc;

use liquers_core::error::Error;
use liquers_records::FieldValue;
use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::{IntoPyDict, PyBool, PyBytes, PyFloat, PyInt, PyList, PyString};

use crate::error::pyerr_to_error;

/// `date(1970, 1, 1).toordinal()`: a `Date` cell counts days from it.
const EPOCH_ORDINAL: i64 = 719_163;

struct DateTimeApi {
    date: Py<PyAny>,
    datetime: Py<PyAny>,
    timedelta: Py<PyAny>,
    epoch: Py<PyAny>,
    utc: Py<PyAny>,
}

fn api(py: Python<'_>) -> PyResult<&DateTimeApi> {
    static API: PyOnceLock<DateTimeApi> = PyOnceLock::new();
    API.get_or_try_init(py, || {
        let module = py.import("datetime")?;
        let utc = module.getattr("timezone")?.getattr("utc")?;
        let datetime = module.getattr("datetime")?;
        let epoch = datetime.call1((1970, 1, 1, 0, 0, 0, 0, &utc))?;
        Ok(DateTimeApi {
            date: module.getattr("date")?.unbind(),
            datetime: datetime.unbind(),
            timedelta: module.getattr("timedelta")?.unbind(),
            epoch: epoch.unbind(),
            utc: utc.unbind(),
        })
    })
}

/// A cell as a Python object.
pub fn field_value_to_py(py: Python<'_>, value: &FieldValue) -> Result<Py<PyAny>, Error> {
    let converted = (|| -> PyResult<Py<PyAny>> {
        Ok(match value {
            FieldValue::Null => py.None(),
            FieldValue::Bool(b) => PyBool::new(py, *b).to_owned().into_any().unbind(),
            FieldValue::Int(i) => i.into_pyobject(py)?.into_any().unbind(),
            FieldValue::UInt(u) => u.into_pyobject(py)?.into_any().unbind(),
            FieldValue::Float(f) => PyFloat::new(py, *f).into_any().unbind(),
            FieldValue::Text(text) => PyString::new(py, text).into_any().unbind(),
            FieldValue::Bytes(bytes) => PyBytes::new(py, bytes).into_any().unbind(),
            FieldValue::Date(days) => {
                api(py)?.date.bind(py).call_method1("fromordinal", (EPOCH_ORDINAL + i64::from(*days),))?.unbind()
            }
            FieldValue::Timestamp(micros) => {
                let api = api(py)?;
                let delta = api.timedelta.bind(py).call((), Some(&[("microseconds", *micros)].into_py_dict(py)?))?;
                api.epoch.bind(py).add(delta)?.unbind()
            }
            FieldValue::Vector(values) => PyList::new(py, values.iter().map(|v| f64::from(*v)))?.into_any().unbind(),
        })
    })();
    converted.map_err(|e| pyerr_to_error(py, &e))
}

/// The type a Python scalar infers to, and its cell, or `None` for a Python `None`.
///
/// `bool` is tested before `int`, and `datetime.datetime` before `datetime.date`, since each is a
/// subclass of the other. An `int` beyond 64 bits is a `ConversionError` naming `what`.
pub fn py_to_field_value(object: &Bound<'_, PyAny>, what: &dyn Fn() -> String) -> Result<FieldValue, Error> {
    let py = object.py();
    if object.is_none() {
        return Ok(FieldValue::Null);
    }
    if let Ok(b) = object.cast::<PyBool>() {
        return Ok(FieldValue::Bool(b.is_true()));
    }
    if object.cast::<PyInt>().is_ok() {
        return match object.extract::<i64>() {
            Ok(i) => Ok(FieldValue::Int(i)),
            Err(_) => Err(Error::conversion_error_with_message(
                object.to_string(),
                "Int",
                &format!("{} is outside the 64-bit range", what()),
            )),
        };
    }
    if let Ok(f) = object.cast::<PyFloat>() {
        return Ok(FieldValue::Float(f.value()));
    }
    if let Ok(s) = object.cast::<PyString>() {
        let text = s.to_str().map_err(|e| pyerr_to_error(py, &e))?;
        return Ok(FieldValue::Text(Arc::from(text)));
    }
    if let Ok(b) = object.cast::<PyBytes>() {
        return Ok(FieldValue::Bytes(Arc::from(b.as_bytes())));
    }
    let api = api(py).map_err(|e| pyerr_to_error(py, &e))?;
    let is = |class: &Py<PyAny>| object.is_instance(class.bind(py)).unwrap_or(false);
    if is(&api.datetime) {
        // A naive datetime is taken as UTC, as the IPC reader takes a naive timestamp.
        let aware = if object.getattr("tzinfo").map(|t| t.is_none()).unwrap_or(true) {
            object
                .call_method("replace", (), Some(&[("tzinfo", api.utc.bind(py))].into_py_dict(py).map_err(|e| pyerr_to_error(py, &e))?))
                .map_err(|e| pyerr_to_error(py, &e))?
        } else {
            object.clone()
        };
        let delta = aware.sub(api.epoch.bind(py)).map_err(|e| pyerr_to_error(py, &e))?;
        let days: i64 = delta.getattr("days").and_then(|v| v.extract()).map_err(|e| pyerr_to_error(py, &e))?;
        let seconds: i64 = delta.getattr("seconds").and_then(|v| v.extract()).map_err(|e| pyerr_to_error(py, &e))?;
        let micros: i64 =
            delta.getattr("microseconds").and_then(|v| v.extract()).map_err(|e| pyerr_to_error(py, &e))?;
        let total = days
            .checked_mul(86_400_000_000)
            .and_then(|d| d.checked_add(seconds * 1_000_000 + micros))
            .ok_or_else(|| Error::conversion_error_with_message(object.to_string(), "Timestamp", &format!("{} is out of range", what())))?;
        return Ok(FieldValue::Timestamp(total));
    }
    if is(&api.date) {
        let ordinal: i64 = object.call_method0("toordinal").and_then(|v| v.extract()).map_err(|e| pyerr_to_error(py, &e))?;
        let days = i32::try_from(ordinal - EPOCH_ORDINAL).map_err(|_| {
            Error::conversion_error_with_message(object.to_string(), "Date", &format!("{} is out of range", what()))
        })?;
        return Ok(FieldValue::Date(days));
    }
    if let Ok(list) = object.cast::<PyList>() {
        let mut values = Vec::with_capacity(list.len());
        for item in list.iter() {
            let number: f64 = item.extract().map_err(|_| {
                Error::conversion_error_with_message(
                    crate::records::schema::type_name(&item),
                    "Vector",
                    &format!("{}: a vector holds numbers", what()),
                )
            })?;
            values.push(number as f32);
        }
        return Ok(FieldValue::Vector(Arc::from(values)));
    }
    Err(Error::conversion_error_with_message(
        crate::records::schema::type_name(object),
        "a record cell",
        &format!("{}: no field type holds this value", what()),
    ))
}
