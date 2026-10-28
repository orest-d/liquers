use std::str::FromStr;

use pyo3::prelude::*;

use crate::error::Error;
use crate::query::quoted;

/// When an asset's value expires, relative to when it is made.
///
/// `Expires(spec)` parses a specification such as `"never"`, `"immediately"`, `"in 5 min"`,
/// `"at 18:00"`, `"on Monday"`, `"eod"`, a date, or a comma-separated combination, and raises a
/// `LiquersError` when it is not one. `Expires.never()` and `Expires.immediately()` are the two
/// fixed cases.
#[pyclass(from_py_object, module = "liquers")]
#[derive(Clone, Debug)]
pub struct Expires(pub liquers_core::expiration::Expires);

#[pymethods]
impl Expires {
    #[new]
    pub fn new(spec: &str) -> PyResult<Self> {
        liquers_core::expiration::Expires::from_str(spec)
            .map(Expires)
            .map_err(|e| PyErr::from(Error::from(e)))
    }

    /// The value never expires (the default).
    #[staticmethod]
    pub fn never() -> Self {
        Expires(liquers_core::expiration::Expires::Never)
    }

    /// The value expires at once: it is never reused, as for a volatile command.
    #[staticmethod]
    pub fn immediately() -> Self {
        Expires(liquers_core::expiration::Expires::Immediately)
    }

    /// The specification as text, in the form `Expires(spec)` reads back.
    pub fn encode(&self) -> String {
        self.0.to_string()
    }

    pub fn __repr__(&self, py: Python<'_>) -> String {
        format!("Expires({})", quoted(py, &self.0.to_string()))
    }

    pub fn __str__(&self) -> String {
        self.0.to_string()
    }

    pub fn __eq__(&self, other: &Self) -> bool {
        self.0 == other.0
    }

    pub fn __ne__(&self, other: &Self) -> bool {
        self.0 != other.0
    }
}

/// An absolute expiration time, in UTC: never, immediately, or a timestamp.
///
/// The resolved form of an `Expires`. `ExpirationTime(spec)` takes `"never"`, `"immediately"` or an
/// RFC 3339 timestamp, and raises an exception for anything else.
#[pyclass(from_py_object, module = "liquers")]
#[derive(Clone, Debug)]
pub struct ExpirationTime(pub liquers_core::expiration::ExpirationTime);

#[pymethods]
impl ExpirationTime {
    #[new]
    pub fn new(spec: &str) -> PyResult<Self> {
        let value = match spec.trim().to_lowercase().as_str() {
            "never" => liquers_core::expiration::ExpirationTime::Never,
            "immediately" => liquers_core::expiration::ExpirationTime::Immediately,
            _ => {
                let dt = chrono::DateTime::parse_from_rfc3339(spec)
                    .map_err(|e| {
                        PyErr::from(crate::error::Error::from(
                            liquers_core::error::Error::general_error(format!(
                                "Invalid expiration time {spec:?}: {e}"
                            )),
                        ))
                    })?
                    .with_timezone(&chrono::Utc);
                liquers_core::expiration::ExpirationTime::At(dt)
            }
        };
        Ok(ExpirationTime(value))
    }

    /// The time of a value that never expires.
    #[staticmethod]
    pub fn never() -> Self {
        ExpirationTime(liquers_core::expiration::ExpirationTime::Never)
    }

    /// The time of a value that expires at once.
    #[staticmethod]
    pub fn immediately() -> Self {
        ExpirationTime(liquers_core::expiration::ExpirationTime::Immediately)
    }

    /// The expiration time as text.
    pub fn encode(&self) -> String {
        self.0.to_string()
    }

    /// True when the value never expires.
    pub fn is_never(&self) -> bool {
        self.0.is_never()
    }

    /// True when the value expires at once.
    pub fn is_immediately(&self) -> bool {
        self.0.is_immediately()
    }

    pub fn __repr__(&self, py: Python<'_>) -> String {
        format!("ExpirationTime({})", quoted(py, &self.0.to_string()))
    }

    pub fn __str__(&self) -> String {
        self.0.to_string()
    }

    pub fn __eq__(&self, other: &Self) -> bool {
        self.0 == other.0
    }

    pub fn __ne__(&self, other: &Self) -> bool {
        self.0 != other.0
    }
}
