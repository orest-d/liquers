use itertools::Itertools;

use crate::command_metadata::CommandKey;
use crate::metadata::DependencyKey;
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
    /// A store was given a key that is not absolute — some segment is `.` or `..`.
    ///
    /// Relative keys are a plan-level construct, resolved against a current working directory
    /// while the plan is built. A store never resolves them, so one that arrives at a store is a
    /// malformed address rather than a routing miss — which is why this is distinct from
    /// [`ErrorType::KeyNotSupported`]. See [`crate::query::Key::as_absolute`].
    KeyNotAbsolute,
    KeyReadError,
    KeyWriteError,
    UnexpectedError,
    ExecutionError,
    DependencyVersionMismatch,
    DependencyCycle,
    /// The error type returned when a *value* is requested from a cancelled asset/state.
    /// It is NOT stored as an asset's computed error; being in `Status::Cancelled` is a
    /// legitimate terminal state, and this error is synthesized only at value extraction.
    Cancelled,
}

/// The payload of an [`Error`], held behind a single [`Box`].
///
/// `Error` is returned by nearly every function in the workspace, so its size is paid by every
/// `Result` in it. Keeping the fields here, one pointer away, makes `Error` word-sized: see the
/// module-level note on [`Error`].
///
/// The fields are the ones `Error` itself used to carry, and they stay public and directly
/// reachable — `Error` dereferences to this type, so `err.message` and `err.position = pos`
/// continue to work unchanged.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ErrorPayload {
    pub error_type: ErrorType,
    pub message: String,
    pub position: Position,
    // TODO: deal with the query and key positions not starting at 0
    pub query: Option<String>,
    pub key: Option<String>,
    #[serde(skip)]
    pub command_key: Option<CommandKey>,
}

impl ErrorPayload {
    /// A payload with only a type and a message; every other field unset.
    fn new(error_type: ErrorType, message: String) -> Self {
        ErrorPayload {
            error_type,
            message,
            position: Position::unknown(),
            query: None,
            key: None,
            command_key: None,
        }
    }
}

/// The workspace-wide error type.
///
/// The fields live in a boxed [`ErrorPayload`], so `Error` is one pointer wide rather than the
/// 176 bytes the inline fields used to occupy. This matters because almost every function in
/// Liquers is fallible: a `Result<T, Error>` is at least as wide as its error, and clippy's
/// `result_large_err` flagged 715 of them before the payload was boxed.
///
/// The indirection is invisible to callers. `Error` derefs to its payload, so field access and
/// assignment read exactly as before, and `#[serde(transparent)]` keeps the serialized form a
/// flat object with the same keys — which matters because `Metadata::error_data` persists it.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(transparent)]
pub struct Error(Box<ErrorPayload>);

impl std::ops::Deref for Error {
    type Target = ErrorPayload;

    fn deref(&self) -> &ErrorPayload {
        &self.0
    }
}

impl std::ops::DerefMut for Error {
    fn deref_mut(&mut self) -> &mut ErrorPayload {
        &mut self.0
    }
}

impl From<ErrorPayload> for Error {
    fn from(payload: ErrorPayload) -> Self {
        Error(Box::new(payload))
    }
}

impl Error {
    /// Consumes the error and returns its payload, unboxed.
    pub fn into_payload(self) -> ErrorPayload {
        *self.0
    }

    /// Every constructor below funnels through here, so the single allocation an `Error` costs
    /// happens in exactly one place.
    pub fn new(error_type: ErrorType, message: String) -> Self {
        Error::from(ErrorPayload::new(error_type, message))
    }

    pub fn from_error<E: Display>(error_type: ErrorType, error: E) -> Self {
        Error::new(error_type, error.to_string())
    }

    pub fn from_result<T, E: Display>(
        error_type: ErrorType,
        result: Result<T, E>,
    ) -> Result<T, Self> {
        match result {
            Ok(value) => Ok(value),
            Err(e) => Err(Error::from_error(error_type, e)),
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
    /// Enriches an error with command execution context.
    /// This is typically called by the interpreter to add command information to errors
    /// returned from command execution.
    pub fn with_command_key(mut self, command_key: &CommandKey) -> Self {
        self.command_key = Some(command_key.clone());
        self
    }
    /// Constructs an error with the `NotAvailable` error type.
    /// This can be used when Option is converted to a result type.
    /// This is used e.g. in cache or store when the requested data is not available.    
    pub fn not_available() -> Self {
        Error::new(ErrorType::NotAvailable, "Not available".to_string())
    }
    /// Returns true if the requested item is not available.
    /// This can be used when Option is converted to a result type.
    /// This is used e.g. in cache or store when the requested data is not available.    
    pub fn is_not_available(&self) -> bool {
        self.error_type == ErrorType::NotAvailable
    }
    /// Constructs a cancellation error (`ErrorType::Cancelled`).
    /// Used when a value is requested from an asset/state in `Status::Cancelled`.
    pub fn cancelled(message: impl Into<String>) -> Self {
        Error::new(ErrorType::Cancelled, message.into())
    }
    /// Returns true if this error represents a cancellation.
    pub fn is_cancelled(&self) -> bool {
        self.error_type == ErrorType::Cancelled
    }
    pub fn cache_not_supported() -> Self {
        Error::new(
            ErrorType::CacheNotSupported,
            "Cache not supported".to_string(),
        )
    }
    pub fn not_supported(message: String) -> Self {
        Error::new(ErrorType::NotSupported, message)
    }
    pub fn action_not_registered(action: &ActionRequest, namespaces: &Vec<String>) -> Self {
        Error::new(
            ErrorType::ActionNotRegistered,
            format!(
                "Action '{}' not registered in namespaces {}",
                action.name,
                namespaces.iter().map(|ns| format!("'{}'", ns)).join(", ")
            ),
        )
        .with_position(&action.position)
    }
    pub fn missing_argument(i: usize, name: &str, position: &Position) -> Self {
        Error::new(
            ErrorType::ArgumentMissing,
            format!("Missing argument #{}:{}", i, name),
        )
        .with_position(position)
    }
    /// An action or resource header supplied a parameter beyond what is accepted.
    ///
    /// The dual of [`Self::missing_argument`]. `subject` names what rejected the parameter
    /// ("command 'select_columns'", "resource header"), `accepted` is how many parameters that
    /// subject consumes, and `excess_index` is the 1-based position of the first surplus
    /// parameter in the written parameter list.
    ///
    /// The position is required rather than applied afterwards with
    /// [`Self::with_position`]: pointing at the offending parameter is the purpose of this
    /// error, and an unpositioned one would be of little use to an editor or a validator.
    pub fn too_many_parameters(
        subject: &str,
        accepted: usize,
        excess_index: usize,
        excess_value: &str,
        position: &Position,
    ) -> Self {
        Error::new(
            ErrorType::TooManyParameters,
            format!(
                "Too many parameters for {subject}: accepts {accepted}, \
                 but parameter #{excess_index} '{excess_value}' was supplied"
            ),
        )
        .with_position(position)
    }
    pub fn conversion_error<W: Display, T: Display>(what: W, to: T) -> Self {
        Error::new(
            ErrorType::ConversionError,
            format!("Can't convert '{}' to {}", what, to),
        )
    }
    pub fn conversion_error_with_message<W: Display, T: Display>(
        what: W,
        to: T,
        message: &str,
    ) -> Self {
        Error::new(
            ErrorType::ConversionError,
            format!("Can't convert '{}' to {}: {}", what, to, message),
        )
    }
    pub fn conversion_error_at_position<W: Display, T: Display>(
        what: W,
        to: T,
        position: &Position,
    ) -> Self {
        Error::conversion_error(what, to).with_position(position)
    }
    pub fn key_parse_error(key: &str, err: &str, position: &Position) -> Self {
        Error::new(
            ErrorType::ParseError,
            format!("Can't parse key '{}': {}", key, err),
        )
        .with_position(position)
    }
    pub fn query_parse_error(query: &str, err: &str, position: &Position) -> Self {
        Error::new(
            ErrorType::ParseError,
            format!("Can't parse query '{}': {}", query, err),
        )
        .with_position(position)
    }
    /// A document failed to parse, with no position to report.
    ///
    /// [`Error::key_parse_error`] and [`Error::query_parse_error`] both require a [`Position`],
    /// because a key or a query is a fragment inside a larger text. A whole configuration
    /// document — YAML, JSON or TOML — has no such enclosing position: the underlying parser's
    /// message carries its own line and column, and there is nothing meaningful to attach.
    pub fn parse_error(message: String) -> Self {
        Error::new(ErrorType::ParseError, message)
    }
    pub fn general_error(message: String) -> Self {
        Error::new(ErrorType::General, message)
    }
    pub fn unexpected_error(message: String) -> Self {
        Error::new(ErrorType::UnexpectedError, message)
    }

    pub(crate) fn unknown_command_executor(
        realm: &str,
        namespace: &str,
        command_name: &str,
        action_position: &Position,
    ) -> Error {
        Error::new(
            ErrorType::UnknownCommand,
            format!(
                "Unknown command executor - realm:'{}' namespace:'{}' command:'{}'",
                realm, namespace, command_name
            ),
        )
        .with_position(action_position)
    }
    pub fn key_not_found(key: &Key) -> Self {
        Error::new(ErrorType::KeyNotFound, format!("Key not found: '{}'", key))
    }
    pub fn key_not_supported(key: &Key, store_name: &str) -> Self {
        let mut error = Error::new(
            ErrorType::KeyNotSupported,
            format!("Key '{}' not supported by store {}", key, store_name),
        );
        error.key = Some(key.encode());
        error
    }
    /// The key is not a store address: some segment is `.` or `..`.
    ///
    /// Takes no store name, unlike [`Self::key_not_supported`]: a relative key is invalid for
    /// *every* store, so naming one adds no information.
    pub fn key_not_absolute(key: &Key) -> Self {
        let mut error = Error::new(
            ErrorType::KeyNotAbsolute,
            format!(
                "Key '{}' is not absolute; a store requires a key without '.' or '..' segments",
                key
            ),
        );
        error.key = Some(key.encode());
        error
    }
    pub fn key_read_error(key: &Key, store_name: &str, message: &(impl Display + ?Sized)) -> Self {
        let mut error = Error::new(
            ErrorType::KeyReadError,
            format!(
                "Key '{}' read error by store {}: {}",
                key, store_name, message
            ),
        );
        error.key = Some(key.encode());
        error
    }
    pub fn key_write_error(key: &Key, store_name: &str, message: &(impl Display + ?Sized)) -> Self {
        let mut error = Error::new(
            ErrorType::KeyWriteError,
            format!(
                "Key '{}' write error by store {}: {}",
                key, store_name, message
            ),
        );
        error.key = Some(key.encode());
        error
    }
    pub fn execution_error(message: String) -> Self {
        Error::new(ErrorType::ExecutionError, message)
    }

    pub fn dependency_version_mismatch(dep_key: &DependencyKey, msg: impl Into<String>) -> Self {
        let store_key = Key::try_from(dep_key).ok();
        let key_str = store_key.as_ref().map(|k| k.encode());
        let mut error = Error::new(
            ErrorType::DependencyVersionMismatch,
            format!(
                "Dependency version mismatch for '{}': {}",
                dep_key.as_str(),
                msg.into()
            ),
        );
        error.query = key_str.clone();
        error.key = key_str;
        error
    }

    pub fn dependency_cycle(dep_key: &DependencyKey) -> Self {
        let store_key = Key::try_from(dep_key).ok();
        let key_str = store_key.as_ref().map(|k| k.encode());
        let mut error = Error::new(
            ErrorType::DependencyCycle,
            format!("Dependency cycle detected involving '{}'", dep_key.as_str()),
        );
        error.query = key_str.clone();
        error.key = key_str;
        error
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = if let Some(ref command_key) = self.command_key {
            let name = if command_key.name.is_empty() {
                "unnamed"
            } else {
                &command_key.name
            };

            if !command_key.realm.is_empty() || !command_key.namespace.is_empty() {
                format!(
                    "Command '{}' ({}) failed: {}",
                    name, command_key, self.message
                )
            } else {
                format!("Command '{}' failed: {}", name, self.message)
            }
        } else {
            self.message.clone()
        };

        if self.position.is_unknown() {
            write!(f, "{}", message)
        } else {
            write!(f, "{} at {}", message, self.position)
        }
    }
}

impl error::Error for Error {
    fn description(&self) -> &str {
        &self.message
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_with_command_key_simple() {
        let key = CommandKey::new("", "", "filter");
        let err = Error::general_error("Column not found".to_string()).with_command_key(&key);

        assert_eq!(err.error_type, ErrorType::General);
        assert_eq!(err.command_key, Some(key));
        assert_eq!(err.to_string(), "Command 'filter' failed: Column not found");
    }

    #[test]
    fn test_with_command_key_with_namespace() {
        let key = CommandKey::new("", "polars", "select");
        let err = Error::general_error("Invalid column".to_string()).with_command_key(&key);

        assert_eq!(err.command_key, Some(key.clone()));
        let display_str = err.to_string();
        assert!(display_str.contains("Command 'select'"));
        assert!(display_str.contains("-polars-select"));
        assert!(display_str.contains("failed: Invalid column"));
    }

    #[test]
    fn test_with_command_key_preserves_error_type() {
        let key = CommandKey::new("", "", "parse");
        let source_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let err = Error::from_error(ErrorType::ExecutionError, source_err).with_command_key(&key);

        assert_eq!(err.error_type, ErrorType::ExecutionError);
        assert!(err.to_string().contains("Command 'parse' failed:"));
        assert!(err.to_string().contains("file missing"));
    }

    #[test]
    fn test_with_command_key_unnamed() {
        let key = CommandKey::new("", "", "");
        let err = Error::general_error("Something went wrong".to_string()).with_command_key(&key);

        let display_str = err.to_string();
        assert!(display_str.contains("Command 'unnamed' failed:"));
        assert!(display_str.contains("Something went wrong"));
    }

    #[test]
    fn test_with_command_key_and_position() {
        let key = CommandKey::new("", "", "test");
        let pos = Position::new(0, 2, 5); // line 2, column 5
        let err = Error::general_error("Test error".to_string())
            .with_command_key(&key)
            .with_position(&pos);

        let display_str = err.to_string();
        assert!(display_str.contains("Command 'test' failed: Test error"));
        assert!(display_str.contains("at line 2, position 5"));
    }

    #[test]
    fn test_error_without_command_key() {
        let err = Error::general_error("No command context".to_string());

        assert_eq!(err.command_key, None);
        assert_eq!(err.to_string(), "No command context");
    }

    /// T1 - the constructor carries every fact needed to point at the surplus parameter.
    #[test]
    fn too_many_parameters_constructor() {
        let pos = Position::new(21, 1, 22);
        let err = Error::too_many_parameters("command 'select_columns'", 1, 2, "price", &pos);

        assert_eq!(err.error_type, ErrorType::TooManyParameters);
        // The position is the feature: it is what an editor highlights.
        assert_eq!(err.position, pos);

        assert!(err.message.contains("select_columns"));
        assert!(err.message.contains("accepts 1"));
        assert!(err.message.contains("#2"));
        assert!(err.message.contains("price"));
    }

    /// The point of boxing the payload: `Error` costs one pointer, so every `Result<T, Error>`
    /// in the workspace carries a word rather than the 176 bytes the inline fields occupied.
    ///
    /// 128 bytes is clippy's `result_large_err` threshold — the lint that flagged 715 signatures
    /// before this change. Asserting against it keeps the fix from silently regressing when a
    /// field is added to [`ErrorPayload`], which is exactly the mistake the box is there to
    /// absorb: growing the payload is now free at the call sites.
    #[test]
    fn error_is_one_pointer_wide() {
        assert_eq!(
            std::mem::size_of::<Error>(),
            std::mem::size_of::<*const ()>()
        );
        assert!(
            std::mem::size_of::<Result<(), Error>>() <= 128,
            "Result<(), Error> is {} bytes, at or over clippy's result_large_err threshold",
            std::mem::size_of::<Result<(), Error>>()
        );
    }

    /// Boxing is an implementation detail, not a wire-format change. `Metadata::error_data`
    /// persists an `Error`, so a stored metadata document written before this change must still
    /// deserialize, and one written after must still be a flat object with the same keys.
    #[test]
    fn serialized_form_is_a_flat_object_with_unchanged_keys() {
        let key = crate::query::Key::try_from("data/report.txt").expect("key parses");
        let err = Error::key_not_supported(&key, "memory");

        let json: serde_json::Value = serde_json::to_value(&err).expect("serializes");
        assert_eq!(
            json,
            serde_json::json!({
                "error_type": "KeyNotSupported",
                "message": "Key 'data/report.txt' not supported by store memory",
                "position": {"offset": 0, "line": 0, "column": 0},
                "query": serde_json::Value::Null,
                "key": "data/report.txt",
            }),
            "the boxed payload must serialize transparently, not as a nested or tuple value"
        );

        let restored: Error = serde_json::from_value(json).expect("round-trips");
        assert_eq!(restored, err);
    }

    /// `command_key` was and remains `#[serde(skip)]`: it survives in memory but not on the wire.
    #[test]
    fn command_key_is_still_skipped_by_serialization() {
        let err = Error::general_error("boom".to_string())
            .with_command_key(&CommandKey::new("", "polars", "select"));

        let json = serde_json::to_string(&err).expect("serializes");
        assert!(!json.contains("command_key"), "got {json}");

        let restored: Error = serde_json::from_str(&json).expect("round-trips");
        assert_eq!(restored.command_key, None);
        assert_eq!(restored.message, "boom");
    }

    /// The payload is reachable through `Deref`/`DerefMut`, so the fields read and assign exactly
    /// as they did when they were inline on `Error`. Callers across the workspace do both —
    /// `liquers-web` appends a stack trace by assigning to `err.message` — and none of them had
    /// to change.
    #[test]
    fn payload_fields_are_read_and_assigned_directly() {
        let mut err = Error::general_error("original".to_string());

        // Read.
        assert_eq!(err.message, "original");
        assert_eq!(err.error_type, ErrorType::General);
        assert!(err.position.is_unknown());
        assert_eq!(err.query, None);

        // Assign.
        err.message = format!("{}\nwith a frame", err.message);
        err.position = Position::new(3, 1, 4);
        err.key = Some("data/x.csv".to_string());

        assert_eq!(err.message, "original\nwith a frame");
        assert_eq!(err.position, Position::new(3, 1, 4));
        assert_eq!(err.key.as_deref(), Some("data/x.csv"));
        // Display still reaches the payload through the inherent impl, not the deref.
        // Line 1 is elided by `Position`'s own Display, hence just the column here.
        assert_eq!(err.to_string(), "original\nwith a frame at position 4");
    }

    /// `into_payload` is the one addition to the public surface: it hands back the unboxed fields
    /// for callers that want to move out of an error rather than clone through a reference.
    #[test]
    fn into_payload_moves_the_fields_out() {
        let err = Error::not_supported("no".to_string());
        let payload = err.into_payload();

        assert_eq!(payload.error_type, ErrorType::NotSupported);
        assert_eq!(payload.message, "no");

        // And the round trip back into an Error is lossless.
        let rebuilt: Error = Error::from(payload);
        assert_eq!(rebuilt, Error::not_supported("no".to_string()));
    }
}
