use itertools::Itertools;

use crate::query::ActionRequest;
use crate::query::Key;
use crate::query::Position;
use std::error;
use std::fmt;
use std::fmt::Display;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash, Copy)]
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

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Error {
    pub error_type: ErrorType,
    pub message: String,
    pub position: Position,
    // TODO: deal with the query and key positions not starting at 0
    pub query: Option<String>,
    pub key: Option<String>,
}

impl Error {
    pub fn new(error_type: ErrorType, message: String) -> Self {
        Error {
            error_type,
            message,
            position: Position::unknown(),
            query: None,
            key: None,
        }
    }

    pub fn from_error<E:Display>(error_type:ErrorType, error: E) -> Self {
        Error {
            error_type,
            message: error.to_string(),
            position: Position::unknown(),
            query: None,
            key: None,
        }
    }

    pub fn from_result<T,E:Display>(error_type:ErrorType, result: Result<T,E>) -> Result<T,Self> {
        match result {
            Ok(value) => Ok(value),
            Err(e) => Err(Error::from_error(error_type, e))
        }
    }
    
    pub fn with_position(mut self, position: &Position) -> Self {
        self.position = position.clone();
        self
    }
    pub fn with_query(mut self, query: &crate::query::Query) -> Self {
        self.query = Some(query.encode());
        self
    }
    pub fn with_key(mut self, key: &crate::query::Key) -> Self {
        self.query = Some(key.encode());
        self
    }
    /// Constructs an error with the `NotAvailable` error type.
    /// This can be used when Option is converted to a result type.
    /// This is used e.g. in cache or store when the requested data is not available.    
    pub fn not_available() -> Self {
        Error {
            error_type: ErrorType::NotAvailable,
            message: "Not available".to_string(),
            position: Position::unknown(),
            query: None,
            key: None,
        }
    }
    /// Returns true if the requested item is not available.
    /// This can be used when Option is converted to a result type.
    /// This is used e.g. in cache or store when the requested data is not available.    
    pub fn is_not_available(&self) -> bool {
        self.error_type == ErrorType::NotAvailable
    }
    pub fn cache_not_supported() -> Self {
        Error {
            error_type: ErrorType::CacheNotSupported,
            message: "Cache not supported".to_string(),
            position: Position::unknown(),
            query: None,
            key: None,
        }
    }
    pub fn not_supported(message: String) -> Self {
        Error {
            error_type: ErrorType::NotSupported,
            message,
            position: Position::unknown(),
            query: None,
            key: None,
        }
    }
    pub fn action_not_registered(action: &ActionRequest, namespaces: &Vec<String>) -> Self {
        Error {
            error_type: ErrorType::ActionNotRegistered,
            message: format!(
                "Action '{}' not registered in namespaces {}",
                action.name,
                namespaces.iter().map(|ns| format!("'{}'", ns)).join(", ")
            ),
            position: action.position.clone(),
            query: None,
            key: None,
        }
    }
    pub fn missing_argument(i: usize, name: &str, position: &Position) -> Self {
        Error {
            error_type: ErrorType::ArgumentMissing,
            message: format!("Missing argument #{}:{}", i, name),
            position: position.clone(),
            query: None,
            key: None,
        }
    }
    pub fn conversion_error<W: Display, T: Display>(what: W, to: T) -> Self {
        Error {
            error_type: ErrorType::ConversionError,
            message: format!("Can't convert '{}' to {}", what, to),
            position: Position::unknown(),
            query: None,
            key: None,
        }
    }
    pub fn conversion_error_with_message<W: Display, T: Display>(
        what: W,
        to: T,
        message: &str,
    ) -> Self {
        Error {
            error_type: ErrorType::ConversionError,
            message: format!("Can't convert '{}' to {}: {}", what, to, message),
            position: Position::unknown(),
            query: None,
            key: None,
        }
    }
    pub fn conversion_error_at_position<W: Display, T: Display>(
        what: W,
        to: T,
        position: &Position,
    ) -> Self {
        Error {
            error_type: ErrorType::ConversionError,
            message: format!("Can't convert '{}' to {}", what, to),
            position: position.clone(),
            query: None,
            key: None,
        }
    }
    pub fn key_parse_error(key: &str, err: &str, position: &Position) -> Self {
        Error {
            error_type: ErrorType::ParseError,
            message: format!("Can't parse key '{}': {}", key, err),
            position: position.clone(),
            query: None,
            key: None,
        }
    }
    pub fn query_parse_error(query: &str, err: &str, position: &Position) -> Self {
        Error {
            error_type: ErrorType::ParseError,
            message: format!("Can't parse query '{}': {}", query, err),
            position: position.clone(),
            query: None,
            key: None,
        }
    }
    pub fn general_error(message: String) -> Self {
        Error {
            error_type: ErrorType::General,
            message,
            position: Position::unknown(),
            query: None,
            key: None,
        }
    }
    pub fn unexpected_error(message: String) -> Self {
        Error {
            error_type: ErrorType::UnexpectedError,
            message,
            position: Position::unknown(),
            query: None,
            key: None,
        }
    }

    pub(crate) fn unknown_command_executor(
        realm: &str,
        namespace: &str,
        command_name: &str,
        action_position: &Position,
    ) -> Error {
        Error {
            error_type: ErrorType::UnknownCommand,
            message: format!(
                "Unknown command executor - realm:'{}' namespace:'{}' command:'{}'",
                realm, namespace, command_name
            ),
            position: action_position.clone(),
            query: None,
            key: None,
        }
    }
    pub fn key_not_found(key: &Key) -> Self {
        Error {
            error_type: ErrorType::KeyNotFound,
            message: format!("Key not found: '{}'", key),
            position: Position::unknown(),
            query: None,
            key: None,
        }
    }
    pub fn key_not_supported(key: &Key, store_name:&str) -> Self {
        Error {
            error_type: ErrorType::KeyNotSupported,
            message: format!("Key '{}' not supported by store {}", key, store_name),
            position: Position::unknown(),
            query: None,
            key: Some(key.encode()),
        }
    }
    pub fn key_read_error(key: &Key, store_name:&str, message: &(impl Display + ?Sized)) -> Self {
        Error {
            error_type: ErrorType::KeyReadError,
            message: format!("Key '{}' read error by store {}: {}", key, store_name, message),
            position: Position::unknown(),
            query: None,
            key: Some(key.encode()),
        }
    }
    pub fn key_write_error(key: &Key, store_name:&str, message: &(impl Display + ?Sized)) -> Self {
        Error {
            error_type: ErrorType::KeyWriteError,
            message: format!("Key '{}' write error by store {}: {}", key, store_name, message),
            position: Position::unknown(),
            query: None,
            key: Some(key.encode()),
        }
    }
    pub fn execution_error(message: String) -> Self {
        Error {
            error_type: ErrorType::ExecutionError,
            message,
            position: Position::unknown(),
            query: None,
            key: None,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.position.is_unknown() {
            write!(f, "{}", self.message)
        } else {
            write!(f, "{} at {}", self.message, self.position)
        }
    }
}

impl error::Error for Error {
    fn description(&self) -> &str {
        &self.message
    }
}
