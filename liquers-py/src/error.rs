use pyo3::{exceptions::PyException, prelude::*};

use crate::parse::Position;

#[pyclass]
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
    KeyReadError,
    KeyWriteError,
    UnexpectedError,
    ExecutionError,
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
            ErrorType::KeyReadError => liquers_core::error::ErrorType::KeyReadError,
            ErrorType::KeyWriteError => liquers_core::error::ErrorType::KeyWriteError,
            ErrorType::UnexpectedError => liquers_core::error::ErrorType::UnexpectedError,
            ErrorType::ExecutionError => liquers_core::error::ErrorType::ExecutionError,
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
            liquers_core::error::ErrorType::KeyReadError => ErrorType::KeyReadError,
            liquers_core::error::ErrorType::KeyWriteError => ErrorType::KeyWriteError,
            liquers_core::error::ErrorType::UnexpectedError => ErrorType::UnexpectedError,
            liquers_core::error::ErrorType::ExecutionError => ErrorType::ExecutionError,
        }
    }
}

#[pyclass]
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

impl From<Error> for PyErr {
    fn from(e: Error) -> Self {
        PyException::new_err((e.0.to_string(), e))
    }
}
