use pyo3::{exceptions::PyException, prelude::*, types::PyAnyMethods};

use crate::parse::Position;

#[pyclass(from_py_object, eq, eq_int, module = "liquers")]
#[derive(Debug, Clone, PartialEq, Eq, Hash, Copy)]
pub enum ErrorType {
    ArgumentMissing,
    ActionNotRegistered,
    CommandAlreadyRegistered,
    ParseError,
    ParameterError,
    TooManyParameters,
    ConversionError,
    SerializationError,
    General,
    CacheNotSupported,
    UnknownCommand,
    NotSupported,
    NotAvailable,
    KeyNotFound,
    KeyNotSupported,
    KeyNotAbsolute,
    KeyReadError,
    KeyWriteError,
    UnexpectedError,
    ExecutionError,
    DependencyVersionMismatch,
    DependencyCycle,
    StatusConflict,
    Cancelled,
    ConfigurationError,
}

impl From<ErrorType> for liquers_core::error::ErrorType {
    fn from(e: ErrorType) -> Self {
        match e {
            ErrorType::ArgumentMissing => liquers_core::error::ErrorType::ArgumentMissing,
            ErrorType::ActionNotRegistered => liquers_core::error::ErrorType::ActionNotRegistered,
            ErrorType::CommandAlreadyRegistered => {
                liquers_core::error::ErrorType::CommandAlreadyRegistered
            }
            ErrorType::ParseError => liquers_core::error::ErrorType::ParseError,
            ErrorType::ParameterError => liquers_core::error::ErrorType::ParameterError,
            ErrorType::TooManyParameters => liquers_core::error::ErrorType::TooManyParameters,
            ErrorType::ConversionError => liquers_core::error::ErrorType::ConversionError,
            ErrorType::SerializationError => liquers_core::error::ErrorType::SerializationError,
            ErrorType::General => liquers_core::error::ErrorType::General,
            ErrorType::CacheNotSupported => liquers_core::error::ErrorType::CacheNotSupported,
            ErrorType::UnknownCommand => liquers_core::error::ErrorType::UnknownCommand,
            ErrorType::NotSupported => liquers_core::error::ErrorType::NotSupported,
            ErrorType::NotAvailable => liquers_core::error::ErrorType::NotAvailable,
            ErrorType::KeyNotFound => liquers_core::error::ErrorType::KeyNotFound,
            ErrorType::KeyNotSupported => liquers_core::error::ErrorType::KeyNotSupported,
            ErrorType::KeyNotAbsolute => liquers_core::error::ErrorType::KeyNotAbsolute,
            ErrorType::KeyReadError => liquers_core::error::ErrorType::KeyReadError,
            ErrorType::KeyWriteError => liquers_core::error::ErrorType::KeyWriteError,
            ErrorType::UnexpectedError => liquers_core::error::ErrorType::UnexpectedError,
            ErrorType::ExecutionError => liquers_core::error::ErrorType::ExecutionError,
            ErrorType::DependencyVersionMismatch => {
                liquers_core::error::ErrorType::DependencyVersionMismatch
            }
            ErrorType::DependencyCycle => liquers_core::error::ErrorType::DependencyCycle,
            ErrorType::StatusConflict => liquers_core::error::ErrorType::StatusConflict,
            ErrorType::Cancelled => liquers_core::error::ErrorType::Cancelled,
            ErrorType::ConfigurationError => liquers_core::error::ErrorType::ConfigurationError,
        }
    }
}

impl From<liquers_core::error::ErrorType> for ErrorType {
    fn from(e: liquers_core::error::ErrorType) -> Self {
        match e {
            liquers_core::error::ErrorType::ArgumentMissing => ErrorType::ArgumentMissing,
            liquers_core::error::ErrorType::ActionNotRegistered => ErrorType::ActionNotRegistered,
            liquers_core::error::ErrorType::CommandAlreadyRegistered => {
                ErrorType::CommandAlreadyRegistered
            }
            liquers_core::error::ErrorType::ParseError => ErrorType::ParseError,
            liquers_core::error::ErrorType::ParameterError => ErrorType::ParameterError,
            liquers_core::error::ErrorType::TooManyParameters => ErrorType::TooManyParameters,
            liquers_core::error::ErrorType::ConversionError => ErrorType::ConversionError,
            liquers_core::error::ErrorType::SerializationError => ErrorType::SerializationError,
            liquers_core::error::ErrorType::General => ErrorType::General,
            liquers_core::error::ErrorType::CacheNotSupported => ErrorType::CacheNotSupported,
            liquers_core::error::ErrorType::UnknownCommand => ErrorType::UnknownCommand,
            liquers_core::error::ErrorType::NotSupported => ErrorType::NotSupported,
            liquers_core::error::ErrorType::NotAvailable => ErrorType::NotAvailable,
            liquers_core::error::ErrorType::KeyNotFound => ErrorType::KeyNotFound,
            liquers_core::error::ErrorType::KeyNotSupported => ErrorType::KeyNotSupported,
            liquers_core::error::ErrorType::KeyNotAbsolute => ErrorType::KeyNotAbsolute,
            liquers_core::error::ErrorType::KeyReadError => ErrorType::KeyReadError,
            liquers_core::error::ErrorType::KeyWriteError => ErrorType::KeyWriteError,
            liquers_core::error::ErrorType::UnexpectedError => ErrorType::UnexpectedError,
            liquers_core::error::ErrorType::ExecutionError => ErrorType::ExecutionError,
            liquers_core::error::ErrorType::DependencyVersionMismatch => {
                ErrorType::DependencyVersionMismatch
            }
            liquers_core::error::ErrorType::DependencyCycle => ErrorType::DependencyCycle,
            liquers_core::error::ErrorType::StatusConflict => ErrorType::StatusConflict,
            liquers_core::error::ErrorType::Cancelled => ErrorType::Cancelled,
            liquers_core::error::ErrorType::ConfigurationError => ErrorType::ConfigurationError,
        }
    }
}

#[pyclass(from_py_object, module = "liquers")]
#[derive(Debug, Clone)]
pub struct Error(pub liquers_core::error::Error);

#[pymethods]
impl Error {
    #[new]
    pub fn new(error_type: ErrorType, message: String) -> Self {
        Error(liquers_core::error::Error::new(
            error_type.into(),
            message.clone(),
        ))
    }

    pub fn with_position(&self, position: Position) -> Self {
        Error(self.0.clone().with_position(&position.0))
    }

    pub fn throw(&self) -> PyResult<()> {
        Err(self.clone().into())
    }
}

impl From<Error> for liquers_core::error::Error {
    fn from(e: Error) -> Self {
        e.0
    }
}

impl From<liquers_core::error::Error> for Error {
    fn from(e: liquers_core::error::Error) -> Self {
        Error(e)
    }
}

/// A wrapped Liquers error raised from a binding becomes the `LiquersError` subclass of its type,
/// the same exception an evaluation raises (OBJECT04).
impl From<Error> for PyErr {
    fn from(e: Error) -> Self {
        Python::attach(|py| error_to_pyerr(py, &e.0))
    }
}

/// Why a call made from Python failed: a Liquers error, or a Python exception raised while it
/// waited (`KeyboardInterrupt` from a signal handler), which is re-raised unchanged rather than
/// becoming an `ExecutionError`.
pub enum PyFailure {
    Liquers(liquers_core::error::Error),
    Python(PyErr),
}

impl From<liquers_core::error::Error> for PyFailure {
    fn from(e: liquers_core::error::Error) -> Self {
        PyFailure::Liquers(e)
    }
}

impl PyFailure {
    pub fn into_pyerr(self, py: Python<'_>) -> PyErr {
        match self {
            PyFailure::Liquers(error) => error_to_pyerr(py, &error),
            PyFailure::Python(err) => err,
        }
    }
}

/// The Python exception for a Liquers error: an instance of the `LiquersError` subclass for its
/// `ErrorType`, carrying every field (ERROR01, ERROR02).
///
/// Falls back to a plain `Exception` with the rendered message only if the `liquers._errors`
/// module cannot be imported, which happens only in a broken installation.
pub fn error_to_pyerr(py: Python<'_>, error: &liquers_core::error::Error) -> PyErr {
    let built = py.import("liquers._errors").and_then(|module| {
        let position = if error.position.is_unknown() {
            None
        } else {
            Some(Position(error.position.clone()))
        };
        module.call_method1(
            "_from_native",
            (
                ErrorType::from(error.error_type),
                error.message.clone(),
                error.query.clone(),
                error.key.clone(),
                position,
                error.traceback.clone(),
            ),
        )
    });
    match built {
        Ok(exception) => PyErr::from_value(exception),
        Err(_) => PyException::new_err(error.to_string()),
    }
}

/// The Liquers error for a Python exception.
///
/// A `LiquersError` keeps its own type and fields (ERROR06), with the Python frames it was raised
/// through appended to its traceback. Anything else, `BaseException`
/// subclasses such as `KeyboardInterrupt` included, becomes an `ExecutionError` whose message
/// names the exception class and whose traceback is the formatted Python stack (ERROR03, ERROR04).
pub fn pyerr_to_error(py: Python<'_>, err: &PyErr) -> liquers_core::error::Error {
    let value = err.value(py);
    if let Some(mut error) = liquers_error_fields(py, value) {
        // A `LiquersError` that passed through Python frames (an `lq.Context.evaluate` failure a
        // command let propagate) gains those frames after the traceback it already carries, so a
        // Python -> Rust -> Python chain reports every Python stack it crossed. One that was never
        // raised (`__traceback__` is `None`) is passed back unchanged.
        if err.traceback(py).is_some() {
            if let Some(frames) = format_traceback(py, err) {
                error.traceback = Some(match error.traceback.take() {
                    Some(inner) if !inner.is_empty() => format!("{inner}\n{frames}"),
                    Some(_) | None => frames,
                });
            }
        }
        return error;
    }
    let class = value
        .get_type()
        .qualname()
        .map(|n| n.to_string())
        .unwrap_or_else(|_| "Exception".to_string());
    let message = value
        .str()
        .map(|s| s.to_string())
        .unwrap_or_else(|_| String::new());
    let rendered = if message.is_empty() {
        class
    } else {
        format!("{class}: {message}")
    };
    let error = liquers_core::error::Error::execution_error(rendered);
    match format_traceback(py, err) {
        Some(traceback) => error.with_traceback(traceback),
        None => error,
    }
}

fn liquers_error_fields(
    py: Python<'_>,
    value: &Bound<'_, pyo3::exceptions::PyBaseException>,
) -> Option<liquers_core::error::Error> {
    let base = py
        .import("liquers._errors")
        .and_then(|m| m.getattr("LiquersError"))
        .ok()?;
    if !value.is_instance(&base).unwrap_or(false) {
        return None;
    }
    let error_type: ErrorType = value.getattr("type").ok()?.extract().ok()?;
    let message: String = value
        .getattr("message")
        .and_then(|m| m.extract())
        .unwrap_or_default();
    let mut error = liquers_core::error::Error::from_error(error_type.into(), message);
    error.query = optional_string(value, "query");
    error.key = optional_string(value, "key");
    error.traceback = optional_string(value, "traceback");
    if let Some(position) = value
        .getattr("position")
        .ok()
        .and_then(|p| p.extract::<Position>().ok())
    {
        error.position = position.0;
    }
    Some(error)
}

fn optional_string(value: &Bound<'_, pyo3::exceptions::PyBaseException>, name: &str) -> Option<String> {
    value
        .getattr(name)
        .ok()
        .and_then(|v| if v.is_none() { None } else { v.str().ok() })
        .map(|s| s.to_string())
}

fn format_traceback(py: Python<'_>, err: &PyErr) -> Option<String> {
    let lines = py
        .import("traceback")
        .and_then(|tb| tb.call_method1("format_exception", (err.value(py),)))
        .ok()?;
    let joined = py
        .eval(c"''.join", None, None)
        .and_then(|join| join.call1((lines,)))
        .ok()?;
    joined.extract::<String>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use liquers_core::error::ErrorType as CoreErrorType;

    /// Loads `liquers._errors` from the source tree and installs its subclasses, without the
    /// native extension (which a Rust test binary cannot import).
    /// Loads `liquers._errors` once per test process.
    ///
    /// Tests run on parallel threads, and executing the module can release the GIL, so a second
    /// install could replace `sys.modules["liquers._errors"]` between another test's raise and
    /// its `isinstance` check against the replaced `LiquersError` — which made
    /// `every_error_type_round_trips_with_its_fields` fail intermittently.
    fn install_errors_module(py: Python<'_>) -> PyResult<()> {
        // `PyOnceLock` detaches while it waits, so a test blocked here cannot hold the GIL that
        // the initializing test needs.
        static INSTALLED: pyo3::sync::PyOnceLock<()> = pyo3::sync::PyOnceLock::new();
        INSTALLED.get_or_try_init(py, || load_errors_module(py)).map(|_| ())
    }

    fn load_errors_module(py: Python<'_>) -> PyResult<()> {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/python/liquers/_errors.py");
        let util = py.import("importlib.util")?;
        let spec = util.call_method1("spec_from_file_location", ("liquers._errors", path))?;
        let module = util.call_method1("module_from_spec", (&spec,))?;
        let modules = py.import("sys")?.getattr("modules")?;
        // Importing `liquers._errors` imports its parent package first; an empty stand-in keeps
        // the real `liquers/__init__.py`, which needs the native extension, out of the test.
        if !modules.contains("liquers")? {
            modules.set_item("liquers", pyo3::types::PyModule::new(py, "liquers")?)?;
        }
        modules.set_item("liquers._errors", &module)?;
        spec.getattr("loader")?.call_method1("exec_module", (&module,))?;
        module.call_method1("_install", (py.get_type::<ErrorType>(),))?;
        Ok(())
    }

    const ALL: [CoreErrorType; 25] = [
        CoreErrorType::ArgumentMissing,
        CoreErrorType::ActionNotRegistered,
        CoreErrorType::CommandAlreadyRegistered,
        CoreErrorType::ParseError,
        CoreErrorType::ParameterError,
        CoreErrorType::TooManyParameters,
        CoreErrorType::ConversionError,
        CoreErrorType::SerializationError,
        CoreErrorType::General,
        CoreErrorType::CacheNotSupported,
        CoreErrorType::UnknownCommand,
        CoreErrorType::NotSupported,
        CoreErrorType::NotAvailable,
        CoreErrorType::KeyNotFound,
        CoreErrorType::KeyNotSupported,
        CoreErrorType::KeyNotAbsolute,
        CoreErrorType::KeyReadError,
        CoreErrorType::KeyWriteError,
        CoreErrorType::UnexpectedError,
        CoreErrorType::ExecutionError,
        CoreErrorType::DependencyVersionMismatch,
        CoreErrorType::DependencyCycle,
        CoreErrorType::StatusConflict,
        CoreErrorType::Cancelled,
        CoreErrorType::ConfigurationError,
    ];

    #[test]
    fn every_error_type_round_trips_with_its_fields() -> Result<(), Box<dyn std::error::Error>> {
        Python::initialize();
        Python::attach(|py| -> Result<(), Box<dyn std::error::Error>> {
            install_errors_module(py)?;
            for error_type in ALL {
                let mut original =
                    liquers_core::error::Error::from_error(error_type, "the message".to_string())
                        .with_traceback("stack");
                original.query = Some("a/b".to_string());
                original.key = Some("k/v".to_string());
                let py_err = error_to_pyerr(py, &original);
                let class = py_err.value(py).get_type().name()?.to_string();
                assert!(class.ends_with("Error"), "{error_type:?} became {class}");
                let back = pyerr_to_error(py, &py_err);
                assert_eq!(back.error_type, error_type);
                assert_eq!(back.message, "the message");
                assert_eq!(back.query.as_deref(), Some("a/b"));
                assert_eq!(back.key.as_deref(), Some("k/v"));
                assert_eq!(back.traceback.as_deref(), Some("stack"));
            }
            Ok(())
        })
    }

    #[test]
    fn a_plain_exception_becomes_an_execution_error_with_class_and_stack(
    ) -> Result<(), Box<dyn std::error::Error>> {
        Python::initialize();
        Python::attach(|py| -> Result<(), Box<dyn std::error::Error>> {
            install_errors_module(py)?;
            for (code, class) in [
                (c"raise ValueError('bad')", "ValueError: bad"),
                (c"raise KeyboardInterrupt()", "KeyboardInterrupt"),
            ] {
                let err = py.run(code, None, None).err().ok_or("the code raises")?;
                let error = pyerr_to_error(py, &err);
                assert_eq!(error.error_type, CoreErrorType::ExecutionError);
                assert!(error.message.starts_with(class), "{}", error.message);
                assert!(error.traceback.is_some());
            }
            Ok(())
        })
    }
}
