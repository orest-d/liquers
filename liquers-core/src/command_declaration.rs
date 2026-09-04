//! An author-facing command declaration, and the pipeline that turns one into [`CommandMetadata`].
//!
//! A declaration is the runtime counterpart of the `register_command!` macro: it says how a
//! *function* becomes a *command*, where [`CommandMetadata`] describes the command itself. It is
//! deliberately **partial** — it is composed over whatever the host discovered by introspection
//! rather than read alone.
//!
//! ```text
//! 1. populate   the host inspects the callable and builds a baseline    host-specific
//! 2. enhance    the author's declaration is merged over the baseline    shared
//! 3. apply      conventions reinterpret the composed result             shared
//! 4. fill       defaults are derived for whatever is still absent       shared
//! 5. build      convert to CommandMetadata, or report what is wrong     shared
//! ```
//!
//! Stages 2-5 live here. Stage 1 is per-language and does not.
//!
//! See `specs/reference/COMMAND_DECLARATION.md` for the full specification.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[cfg(test)]
use crate::command_metadata::ArgumentGUIInfo;
use crate::command_metadata::{ArgumentInfo, CommandMetadata, DEFAULT_GUI};
use crate::error::{Error, ErrorType};

/// How the input state reaches the callable.
///
/// The **first** argument is always the state-derived argument; its *name* selects only how the
/// state is delivered. `liquers-core` records the mode and never performs it — an integration
/// reads it back and does the delivery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateDelivery {
    /// Nothing is passed — a source command. `first_command` semantics: such a command is still
    /// usable anywhere in a query, and a state reaching it is ignored rather than refused.
    None,
    /// The `State` wrapper, so the callable reaches the metadata as well as the value.
    State,
    /// The value, unwrapped to the language-native form wherever the integration's value bridge
    /// can, falling back to the `Value` wrapper only where it cannot.
    Value,
    /// The value through `ValueInterface::try_into_string`. The only mode with a failure path,
    /// and it fails at call time rather than at declaration time.
    Text,
    /// Reserved. Interpreted as [`StateDelivery::Value`] today; a future integration may give it
    /// meaning — `df` as a polars or pandas DataFrame in Python is the motivating case. This is
    /// why the enum is open: an unrecognised name is not an error, so a declaration written today
    /// keeps working when the name acquires a meaning.
    Reserved(String),
}

impl StateDelivery {
    /// Derives the mode from the first argument's name.
    pub fn from_argument_name(name: &str) -> Self {
        match name {
            "none" | "na" => StateDelivery::None,
            "state" => StateDelivery::State,
            "value" => StateDelivery::Value,
            "text" => StateDelivery::Text,
            other => StateDelivery::Reserved(other.to_string()),
        }
    }

    /// What an integration actually performs: a reserved mode behaves as [`StateDelivery::Value`].
    pub fn effective(&self) -> StateDelivery {
        match self {
            StateDelivery::None => StateDelivery::None,
            StateDelivery::State => StateDelivery::State,
            StateDelivery::Value => StateDelivery::Value,
            StateDelivery::Text => StateDelivery::Text,
            StateDelivery::Reserved(_) => StateDelivery::Value,
        }
    }

    /// The recorded spelling. `na` normalises to `none`; a reserved name is kept verbatim.
    pub fn as_str(&self) -> &str {
        match self {
            StateDelivery::None => "none",
            StateDelivery::State => "state",
            StateDelivery::Value => "value",
            StateDelivery::Text => "text",
            StateDelivery::Reserved(name) => name,
        }
    }

    /// Whether the state reaches the callable at all.
    pub fn passes_state(&self) -> bool {
        match self {
            StateDelivery::None => false,
            StateDelivery::State
            | StateDelivery::Value
            | StateDelivery::Text
            | StateDelivery::Reserved(_) => true,
        }
    }
}

/// Which conventions [`CommandDeclaration::apply_conventions`] applies. All default to on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Conventions {
    /// An argument named `context` is the execution context and is not a command argument.
    pub context: bool,
    /// The first argument is the state-derived argument.
    pub state: bool,
}

impl Default for Conventions {
    fn default() -> Self {
        Conventions {
            context: true,
            state: true,
        }
    }
}

impl Conventions {
    /// Reads the declaration's `conventions` key. `false` disables everything; an object disables
    /// the named ones. Anything else leaves the defaults in place.
    fn from_value(value: Option<&Value>) -> Self {
        let mut c = Conventions::default();
        match value {
            None => c,
            Some(Value::Bool(false)) => Conventions {
                context: false,
                state: false,
            },
            Some(Value::Bool(true)) => c,
            Some(Value::Object(map)) => {
                if let Some(Value::Bool(b)) = map.get("context") {
                    c.context = *b;
                }
                if let Some(Value::Bool(b)) = map.get("state") {
                    c.state = *b;
                }
                c
            }
            Some(_) => c,
        }
    }
}

/// What a non-fatal diagnostic is about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WarningKind {
    /// The first argument's name has no defined meaning; it is being treated as `value`.
    ReservedStateDelivery,
    /// A `context` argument preceded the state, so removing it shifted which argument became the
    /// state.
    ContextBeforeState,
    /// No introspection ran, the declaration supplied arguments, and no `state_argument` was
    /// declared — so this is a source command by omission.
    NoIntrospection,
    /// A declared key cannot reach the metadata and was dropped.
    DroppedKey,
}

/// A non-fatal diagnostic from the pipeline.
///
/// Collected rather than printed: `liquers-web` is a wasm build where nothing reads stderr, so a
/// printed warning would simply be lost, and a printed warning cannot be asserted on. The host
/// surfaces these — `console.warn`, `warnings.warn`, a log line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    pub command: String,
    pub kind: WarningKind,
    pub message: String,
}

/// A command declaration in the course of being composed.
///
/// Holds the merged document. Deserializing one yields a *document* declaration — one with no
/// introspection behind it — which is what a `commands.yaml` is.
#[derive(Debug, Clone, PartialEq)]
pub struct CommandDeclaration {
    doc: Value,
    /// Whether stage 1 ran, i.e. whether the baseline carried an `arguments` key. This is what
    /// distinguishes "a function with no parameters" from "no introspection happened", and the
    /// state-delivery rule keys on it.
    introspected: bool,
    /// Whether stage 3 has run. Kept here rather than inferred from `registration.state`, because
    /// an author may *supply* that key — the documented delivery-mode override — and a recorded
    /// mode is then indistinguishable from an authored one. Keying idempotence on the recorded
    /// value made an authored mode skip the conventions entirely.
    conventions_applied: bool,
    warnings: Vec<Warning>,
}

impl Serialize for CommandDeclaration {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.doc.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for CommandDeclaration {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(CommandDeclaration::from_document(Value::deserialize(
            deserializer,
        )?))
    }
}

impl CommandDeclaration {
    /// Stage 1's result. An `arguments` key means introspection ran and reported the callable's
    /// parameters — including an empty list, which means a function with no parameters.
    pub fn from_introspection(baseline: Value) -> Self {
        let introspected = baseline.get("arguments").is_some();
        CommandDeclaration {
            doc: baseline,
            introspected,
            conventions_applied: false,
            warnings: Vec::new(),
        }
    }

    /// A declaration with no introspection behind it — a document. The state-delivery rule does
    /// not apply, so a declared first argument is an ordinary command argument.
    pub fn from_document(document: Value) -> Self {
        CommandDeclaration {
            doc: document,
            introspected: false,
            conventions_applied: false,
            warnings: Vec::new(),
        }
    }

    /// The composed document.
    pub fn as_value(&self) -> &Value {
        &self.doc
    }

    /// Registration hints — how to register and call the function. Declaration-only: they never
    /// reach the metadata. Returns a null value when none were declared.
    pub fn registration(&self) -> &Value {
        static NULL: Value = Value::Null;
        self.doc.get("registration").unwrap_or(&NULL)
    }

    /// Diagnostics accumulated so far. De-duplicated, so re-running a stage does not multiply them.
    pub fn warnings(&self) -> &[Warning] {
        &self.warnings
    }

    fn command_name(&self) -> String {
        self.doc
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    }

    fn warn(&mut self, kind: WarningKind, message: String) {
        let warning = Warning {
            command: self.command_name(),
            kind,
            message,
        };
        if !self.warnings.contains(&warning) {
            self.warnings.push(warning);
        }
    }
}

// --- stage 2: the merge ------------------------------------------------------------------------

fn parameter_error(message: String) -> Error {
    Error::from_error(ErrorType::ParameterError, message)
}

/// The name an argument entry carries, or an error naming where it was missing.
fn argument_name(entry: &Value, command: &str, position: usize) -> Result<String, Error> {
    entry
        .get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| {
            parameter_error(format!(
                "command {command:?}: argument {position} must have a string `name`"
            ))
        })
}

/// Merges a declaration object over a base object, key by key.
///
/// Objects recurse; scalars and arrays replace; an absent key leaves the base untouched, which is
/// the distinction the whole design rests on. `null` is an ordinary value, never a deletion.
fn merge_object(base: &mut Value, declaration: &Value, command: &str) -> Result<(), Error> {
    let declaration_map = match declaration {
        Value::Object(map) => map,
        Value::Null => return Ok(()),
        other => {
            return Err(parameter_error(format!(
                "command {command:?}: a declaration must be an object, found {}",
                type_name_of(other)
            )))
        }
    };

    if !base.is_object() {
        *base = Value::Object(serde_json::Map::new());
    }

    for (key, declared) in declaration_map.iter() {
        if key == "arguments" {
            merge_arguments(base, declared, command)?;
            continue;
        }
        let base_map = match base.as_object_mut() {
            Some(map) => map,
            None => {
                return Err(parameter_error(format!(
                    "command {command:?}: internal error, base is not an object"
                )))
            }
        };
        // Matching an `Option<&mut serde_json::Value>`, an external type: the catch-all is
        // "absent, or not two objects", which is the replace case.
        match base_map.get_mut(key) {
            Some(existing) if existing.is_object() && declared.is_object() => {
                merge_object(existing, declared, command)?;
            }
            _ => {
                base_map.insert(key.clone(), declared.clone());
            }
        }
    }
    Ok(())
}

/// Merges declared arguments **by name, never by position**.
///
/// An entry naming a discovered argument augments it, field by field, leaving what it omits alone —
/// this is the case the design exists for. An entry naming an unknown argument is rejected, because
/// Liquers binds query parameters positionally and a typo would silently misbind. The exception:
/// when the base carries no `arguments` key at all, no introspection ran and the declaration
/// establishes the list.
fn merge_arguments(base: &mut Value, declared: &Value, command: &str) -> Result<(), Error> {
    let declared_list = match declared {
        Value::Array(list) => list,
        other => {
            return Err(parameter_error(format!(
                "command {command:?}: `arguments` must be an array, found {}",
                type_name_of(other)
            )))
        }
    };

    let base_map = match base.as_object_mut() {
        Some(map) => map,
        None => {
            return Err(parameter_error(format!(
                "command {command:?}: internal error, base is not an object"
            )))
        }
    };

    // No `arguments` key at all: discovery did not run, so the declaration establishes the list.
    let existing = match base_map.get_mut("arguments") {
        None => {
            base_map.insert("arguments".to_string(), declared.clone());
            return Ok(());
        }
        Some(value) => value,
    };

    let existing_list = match existing.as_array_mut() {
        Some(list) => list,
        None => {
            return Err(parameter_error(format!(
                "command {command:?}: discovered `arguments` is not an array"
            )))
        }
    };

    for (position, entry) in declared_list.iter().enumerate() {
        let name = argument_name(entry, command, position)?;
        let target = existing_list.iter_mut().find(|candidate| {
            candidate.get("name").and_then(|v| v.as_str()) == Some(name.as_str())
        });
        match target {
            Some(existing_entry) => merge_object(existing_entry, entry, command)?,
            None => {
                let known: Vec<&str> = existing_list
                    .iter()
                    .filter_map(|candidate| candidate.get("name").and_then(|v| v.as_str()))
                    .collect();
                return Err(parameter_error(format!(
                    "command {command:?}: declared argument {name:?} does not exist; \
                     the callable declares {known:?}"
                )));
            }
        }
    }
    Ok(())
}

fn type_name_of(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

// --- stage 4: derived defaults -----------------------------------------------------------------

/// Derives a readable label from an identifier.
///
/// `snake_case` and `camelCase` are both broken into words and the first is capitalised, so an
/// author who names a function idiomatically in their own language gets a readable label without
/// writing one. A run of capitals is kept as one word, so an acronym survives.
///
/// | Name | Label |
/// |---|---|
/// | `to_text` | `To text` |
/// | `toText` | `To text` |
/// | `toHTML` | `To HTML` |
/// | `parseHTTPResponse` | `Parse HTTP response` |
pub fn derive_label(name: &str) -> String {
    let mut words: Vec<String> = Vec::new();
    for part in name.split(['_', '-', ' ']) {
        if part.is_empty() {
            continue;
        }
        words.extend(split_camel_case(part));
    }
    if words.is_empty() {
        return String::new();
    }
    let rendered: Vec<String> = words
        .iter()
        .map(|word| {
            if is_all_caps(word) {
                word.clone()
            } else {
                word.to_lowercase()
            }
        })
        .collect();
    let mut label = rendered.join(" ");
    if let Some(first) = label.chars().next() {
        let upper: String = first.to_uppercase().collect();
        label = format!("{}{}", upper, &label[first.len_utf8()..]);
    }
    label
}

fn is_all_caps(word: &str) -> bool {
    word.chars().any(|c| c.is_alphabetic()) && word.chars().all(|c| !c.is_lowercase())
}

/// Splits one identifier fragment at lower-to-upper boundaries, and before the last capital of a
/// capital run that is followed by a lowercase letter — so `HTTPResponse` gives `HTTP`, `Response`.
fn split_camel_case(part: &str) -> Vec<String> {
    let chars: Vec<char> = part.chars().collect();
    let mut words: Vec<String> = Vec::new();
    let mut current = String::new();
    for (i, c) in chars.iter().enumerate() {
        if i > 0 && c.is_uppercase() {
            let previous = chars[i - 1];
            let next_is_lower = chars.get(i + 1).map(|n| n.is_lowercase()).unwrap_or(false);
            if previous.is_lowercase() || previous.is_numeric() || next_is_lower {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
            }
        }
        current.push(*c);
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

// --- stages 3-5 --------------------------------------------------------------------------------

impl CommandDeclaration {
    /// Stage 2. Merges `declaration` over what is already here.
    ///
    /// May be called more than once; composition is associative, so layered declarations compose.
    pub fn enhance(&mut self, declaration: &Value) -> Result<(), Error> {
        let command = self.command_name();
        merge_object(&mut self.doc, declaration, &command)
    }

    /// Stage 3. Applies conventions, moving recognised parameters out of `arguments`.
    ///
    /// Runs **after** the merge, so an author declaring metadata for a recognised name gets it
    /// matched by name rather than rejected as unknown. Structural conventions run before delivery
    /// ones, or a leading `context` would become the state. Idempotent, warnings included.
    pub fn apply_conventions(&mut self) -> Result<(), Error> {
        if self.conventions_applied {
            return Ok(());
        }
        self.conventions_applied = true;

        let conventions = Conventions::from_value(self.doc.get("conventions"));
        if conventions.context {
            self.take_context_argument()?;
        }
        if conventions.state {
            self.apply_state_delivery()?;
        }
        Ok(())
    }

    /// Removes an argument named `context` and records where it was. Returns its former position.
    fn take_context_argument(&mut self) -> Result<Option<usize>, Error> {
        let position = match self.doc.get("arguments").and_then(|v| v.as_array()) {
            Some(list) => list
                .iter()
                .position(|a| a.get("name").and_then(|v| v.as_str()) == Some("context")),
            None => None,
        };
        let position = match position {
            Some(p) => p,
            None => return Ok(None),
        };
        if let Some(list) = self.doc.get_mut("arguments").and_then(|v| v.as_array_mut()) {
            list.remove(position);
        }
        self.set_registration("context", Value::from(position));
        if position == 0 {
            self.warn(
                WarningKind::ContextBeforeState,
                "a `context` argument preceded the state, so the state is taken from the \
                 argument that followed it"
                    .to_string(),
            );
        }
        Ok(Some(position))
    }

    /// The first argument is always the state-derived argument; its name selects the delivery mode.
    fn apply_state_delivery(&mut self) -> Result<(), Error> {
        // An explicitly declared state argument is never touched by the convention.
        if self.doc.get("state_argument").map(|v| !v.is_null()) == Some(true) {
            return Ok(());
        }

        // An authored mode: the documented override, which wins over anything derived from a name.
        let declared_mode = self
            .registration()
            .get("state")
            .and_then(|v| v.as_str())
            .map(StateDelivery::from_argument_name);

        if !self.introspected {
            // A document's `arguments` are the command's *public* arguments, not a function's
            // parameters, so none of them is ever consumed here. But a document may still say
            // that its command takes a state, and declaring the mode is the documented way to do
            // it (`reference/COMMAND_DECLARATION.md` §3.2.3).
            match declared_mode {
                Some(mode) => {
                    self.warn_if_reserved(&mode);
                    if mode.passes_state() {
                        self.set_state_argument(ArgumentInfo::any_argument("state"));
                    }
                    self.set_registration("state", Value::from(mode.as_str()));
                }
                None => {
                    let declared_arguments = self
                        .doc
                        .get("arguments")
                        .and_then(|v| v.as_array())
                        .map(|l| !l.is_empty())
                        .unwrap_or(false);
                    if declared_arguments {
                        self.warn(
                            WarningKind::NoIntrospection,
                            "no introspection ran and no state was declared, so this is a source \
                             command; declare `state_argument`, or `registration.state`, if it \
                             should transform a state"
                                .to_string(),
                        );
                    }
                }
            }
            return Ok(());
        }

        let first_name = match self
            .doc
            .get("arguments")
            .and_then(|v| v.as_array())
            .and_then(|l| l.first())
            .and_then(|a| a.get("name"))
            .and_then(|v| v.as_str())
        {
            Some(name) => name.to_string(),
            None => return Ok(()),
        };

        let mode = declared_mode.unwrap_or_else(|| StateDelivery::from_argument_name(&first_name));

        self.warn_if_reserved(&mode);

        // The first argument is consumed either way: as the state, or as the `none` marker.
        // External `Option`; the catch-all is "no arguments to take", which is a no-op.
        let first = match self.doc.get_mut("arguments").and_then(|v| v.as_array_mut()) {
            Some(list) if !list.is_empty() => list.remove(0),
            _ => return Ok(()),
        };

        if mode.passes_state() {
            let mut state_argument = first;
            if !state_argument.is_object() {
                state_argument = Value::Object(serde_json::Map::new());
            }
            if let Some(map) = self.doc.as_object_mut() {
                map.insert("state_argument".to_string(), state_argument);
            }
        }
        self.set_registration("state", Value::from(mode.as_str()));
        Ok(())
    }

    /// A delivery mode with no defined meaning is not an error — it means `value` until something
    /// gives it one — but the author should learn that nothing yet delivers what the name suggests.
    fn warn_if_reserved(&mut self, mode: &StateDelivery) {
        if let StateDelivery::Reserved(name) = mode {
            let name = name.clone();
            self.warn(
                WarningKind::ReservedStateDelivery,
                format!(
                    "the state delivery mode {name:?} has no defined meaning; it is treated as \
                     `value`"
                ),
            );
        }
    }

    fn set_state_argument(&mut self, argument: ArgumentInfo) {
        if let Ok(value) = serde_json::to_value(argument) {
            if let Some(map) = self.doc.as_object_mut() {
                map.insert("state_argument".to_string(), value);
            }
        }
    }

    fn set_registration(&mut self, key: &str, value: Value) {
        let map = match self.doc.as_object_mut() {
            Some(map) => map,
            None => return,
        };
        let entry = map
            .entry("registration".to_string())
            .or_insert_with(|| Value::Object(serde_json::Map::new()));
        if !entry.is_object() {
            *entry = Value::Object(serde_json::Map::new());
        }
        if let Some(registration) = entry.as_object_mut() {
            registration.insert(key.to_string(), value);
        }
    }

    /// Stage 4. Fills what a declaration may omit. Idempotent; never overwrites a declared value.
    pub fn fill_defaults(&mut self) {
        let name = self.command_name();
        let map = match self.doc.as_object_mut() {
            Some(map) => map,
            None => return,
        };
        let label_empty = map
            .get("label")
            .map(|v| v.as_str().unwrap_or("").is_empty())
            .unwrap_or(true);
        if label_empty && !name.is_empty() {
            map.insert("label".to_string(), Value::from(derive_label(&name)));
        }
        for key in ["state_argument", "arguments"] {
            // External `serde_json::Value`; the catch-all is "absent or not a shape that holds
            // arguments", which needs no defaulting.
            match map.get_mut(key) {
                Some(Value::Array(list)) => {
                    for argument in list.iter_mut() {
                        fill_argument_defaults(argument);
                    }
                }
                Some(argument) if argument.is_object() => fill_argument_defaults(argument),
                _ => {}
            }
        }
    }

    /// Stage 5. Converts to metadata and validates, or reports what is wrong.
    ///
    /// `registration` and `conventions` are declaration-only and do not reach the metadata; nor,
    /// today, does a command-level `hints`, which warns instead.
    pub fn build(&self) -> Result<CommandMetadata, Error> {
        let command = self.command_name();
        let metadata: CommandMetadata = CommandMetadata::deserialize(&self.doc).map_err(|e| {
            parameter_error(if command.is_empty() {
                format!("command declaration: {e}")
            } else {
                format!("command {command:?}: {e}")
            })
        })?;
        validate(&metadata)?;
        Ok(metadata)
    }

    /// Stages 3-5 in order. The normal entry point once a declaration has been enhanced.
    pub fn finish(&mut self) -> Result<CommandMetadata, Error> {
        self.apply_conventions()?;
        if self.doc.get("hints").is_some() {
            self.warn(
                WarningKind::DroppedKey,
                "`hints` at command level is dropped: CommandMetadata has no command-level hints \
                 field, so the key cannot reach the metadata"
                    .to_string(),
            );
        }
        self.fill_defaults();
        self.build()
    }
}

fn fill_argument_defaults(argument: &mut Value) {
    let name = argument
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let map = match argument.as_object_mut() {
        Some(map) => map,
        None => return,
    };
    let label_empty = map
        .get("label")
        .map(|v| v.as_str().unwrap_or("").is_empty())
        .unwrap_or(true);
    if label_empty && !name.is_empty() {
        map.insert("label".to_string(), Value::from(derive_label(&name)));
    }
    // `ArgumentGUIInfo`'s `Default` is `None`, but `ArgumentInfo::any_argument` — the constructor
    // every other registration path uses — sets `TextField(40)`. Matching the constructor keeps
    // `metadata_version` stable for commands registered either way.
    if map.get("gui_info").is_none() {
        if let Ok(default_gui) = serde_json::to_value(DEFAULT_GUI.clone()) {
            map.insert("gui_info".to_string(), default_gui);
        }
    }
}

/// Reports what is wrong with built metadata, naming the command and, where there is one, the
/// argument. Global-enum references are deliberately not resolved: that needs a
/// `CommandMetadataRegistry` and happens at registry insertion and plan building.
fn validate(metadata: &CommandMetadata) -> Result<(), Error> {
    if metadata.name.is_empty() {
        return Err(parameter_error(
            "a command declaration must have a non-empty `name`".to_string(),
        ));
    }
    let command = &metadata.name;
    let last = metadata.arguments.len().saturating_sub(1);
    for (position, argument) in metadata.arguments.iter().enumerate() {
        if argument.name.is_empty() {
            return Err(parameter_error(format!(
                "command {command:?}: argument {position} must have a non-empty `name`"
            )));
        }
        if argument.multiple && position != last {
            return Err(parameter_error(format!(
                "command {command:?}: the `multiple` argument {:?} must be the last argument, \
                 but {:?} follows it",
                argument.name,
                metadata.arguments[position + 1].name
            )));
        }
    }
    Ok(())
}

/// Convenience for a host that has both halves in hand.
pub fn build_command_metadata(
    baseline: Value,
    declaration: &Value,
) -> Result<(CommandMetadata, Vec<Warning>), Error> {
    let mut command = CommandDeclaration::from_introspection(baseline);
    command.enhance(declaration)?;
    let metadata = command.finish()?;
    Ok((metadata, command.warnings().to_vec()))
}

/// Unused import guard: `ArgumentInfo` is referenced by the documentation above and by tests.
#[allow(dead_code)]
fn _argument_info_is_used(a: ArgumentInfo) -> String {
    a.name
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command_metadata::{ArgumentType, CommandKey, CommandParameterValue};
    use serde_json::json;

    fn try_build(doc: Value) -> Result<CommandMetadata, Error> {
        CommandDeclaration::from_document(doc).finish()
    }
    fn build(doc: Value) -> CommandMetadata {
        try_build(doc).expect("declaration should build")
    }
    fn finish(declaration: &mut CommandDeclaration) -> CommandMetadata {
        declaration.finish().expect("declaration should build")
    }
    fn baseline() -> Value {
        json!({
            "name": "repeat",
            "arguments": [
                { "name": "count", "argument_type": "int", "default": { "Value": 2 } }
            ]
        })
    }
    fn kinds(declaration: &CommandDeclaration) -> Vec<WarningKind> {
        declaration
            .warnings()
            .iter()
            .map(|w| w.kind.clone())
            .collect()
    }

    // --- stage 2: the merge laws ---------------------------------------------------------------

    #[test]
    fn merge01_empty_declaration_is_identity() {
        let mut d = CommandDeclaration::from_introspection(baseline());
        d.enhance(&json!({})).unwrap();
        assert_eq!(d.as_value(), &baseline());
    }

    #[test]
    fn merge02_enhance_is_idempotent() {
        let declaration = json!({ "label": "Repeat text" });
        let mut once = CommandDeclaration::from_introspection(baseline());
        once.enhance(&declaration).unwrap();
        let mut twice = CommandDeclaration::from_introspection(baseline());
        twice.enhance(&declaration).unwrap();
        twice.enhance(&declaration).unwrap();
        assert_eq!(once.as_value(), twice.as_value());
    }

    #[test]
    fn merge03_declared_scalar_overrides_discovered() {
        let mut d = CommandDeclaration::from_introspection(json!({"name":"x","doc":"discovered"}));
        d.enhance(&json!({ "doc": "declared" })).unwrap();
        assert_eq!(d.as_value()["doc"], json!("declared"));
    }

    #[test]
    fn merge04_omitted_field_leaves_discovered_value() {
        let mut d = CommandDeclaration::from_introspection(json!({"name":"x","doc":"discovered"}));
        d.enhance(&json!({ "label": "X" })).unwrap();
        assert_eq!(d.as_value()["doc"], json!("discovered"));
    }

    /// The case the design exists for: type and default survive an entry that mentions neither.
    #[test]
    fn merge05_argument_entry_augments_by_name() {
        let mut d = CommandDeclaration::from_introspection(baseline());
        d.enhance(&json!({ "arguments": [{ "name": "count", "label": "Count" }] }))
            .unwrap();
        let argument = &d.as_value()["arguments"][0];
        assert_eq!(argument["label"], json!("Count"));
        assert_eq!(argument["argument_type"], json!("int"));
        assert_eq!(argument["default"], json!({ "Value": 2 }));
    }

    #[test]
    fn merge06_unknown_argument_name_is_rejected() {
        let mut d = CommandDeclaration::from_introspection(baseline());
        let error = d
            .enhance(&json!({ "arguments": [{ "name": "cnt" }] }))
            .unwrap_err();
        let message = format!("{error}");
        assert!(message.contains("cnt"), "names the offender: {message}");
        assert!(message.contains("repeat"), "names the command: {message}");
        assert!(message.contains("count"), "lists what exists: {message}");
    }

    /// The plain-document host: no baseline to check against, so the declaration establishes it.
    #[test]
    fn merge07_no_arguments_key_lets_the_declaration_establish_the_list() {
        let mut d = CommandDeclaration::from_introspection(json!({ "name": "repeat" }));
        d.enhance(&json!({ "arguments": [{ "name": "count", "argument_type": "int" }] }))
            .unwrap();
        assert_eq!(d.as_value()["arguments"][0]["name"], json!("count"));
    }

    /// An empty list means "introspected, no parameters" — different from "not introspected".
    #[test]
    fn merge08_empty_arguments_list_still_rejects() {
        let mut d = CommandDeclaration::from_introspection(json!({"name":"f","arguments":[]}));
        assert!(d
            .enhance(&json!({ "arguments": [{ "name": "count" }] }))
            .is_err());
    }

    #[test]
    fn merge09_null_sets_rather_than_deletes() {
        let mut d = CommandDeclaration::from_introspection(json!({"name":"x","filename":"a.txt"}));
        d.enhance(&json!({ "filename": null })).unwrap();
        assert!(d.as_value().get("filename").is_some(), "the key is present");
        assert_eq!(d.as_value()["filename"], Value::Null);
    }

    #[test]
    fn merge10_declaration_cannot_reorder_arguments() {
        let base = json!({"name":"f","arguments":[{"name":"a"},{"name":"b"}]});
        let mut d = CommandDeclaration::from_introspection(base);
        d.enhance(&json!({ "arguments": [
            { "name": "b", "label": "Bee" },
            { "name": "a", "label": "Ay"  }
        ]}))
        .unwrap();
        let arguments = d.as_value()["arguments"].as_array().unwrap();
        assert_eq!(arguments[0]["name"], json!("a"));
        assert_eq!(arguments[0]["label"], json!("Ay"));
        assert_eq!(arguments[1]["name"], json!("b"));
    }

    #[test]
    fn merge11_composition_is_associative() {
        let (first, second) = (json!({ "doc": "one" }), json!({ "label": "Two" }));
        let mut layered = CommandDeclaration::from_introspection(baseline());
        layered.enhance(&first).unwrap();
        layered.enhance(&second).unwrap();
        let mut combined = CommandDeclaration::from_introspection(baseline());
        combined
            .enhance(&json!({ "doc": "one", "label": "Two" }))
            .unwrap();
        assert_eq!(layered.as_value(), combined.as_value());
    }

    #[test]
    fn merge12_nested_maps_merge_and_other_arrays_replace() {
        let base = json!({"name":"f",
                          "registration":{"js":{"state":"text","variadic":"spread"}},
                          "next":["a","b"]});
        let mut d = CommandDeclaration::from_introspection(base);
        d.enhance(&json!({ "registration": { "js": { "state": "value" } }, "next": ["c"] }))
            .unwrap();
        assert_eq!(d.as_value()["registration"]["js"]["state"], json!("value"));
        assert_eq!(
            d.as_value()["registration"]["js"]["variadic"],
            json!("spread"),
            "the sibling survives"
        );
        assert_eq!(
            d.as_value()["next"],
            json!(["c"]),
            "a non-argument array replaces"
        );
    }

    // --- stage 4: derived defaults -------------------------------------------------------------

    #[test]
    fn def01_label_derivation() {
        for (name, want) in [
            ("to_text", "To text"),
            ("toText", "To text"),
            ("toHTML", "To HTML"),
            ("parseHTTPResponse", "Parse HTTP response"),
            ("x", "X"),
        ] {
            assert_eq!(derive_label(name), want, "deriving from {name:?}");
        }
    }

    #[test]
    fn def02_derivation_never_overwrites() {
        let mut d = CommandDeclaration::from_document(json!({"name":"to_text"}));
        d.enhance(&json!({ "label": "Textify" })).unwrap();
        d.fill_defaults();
        assert_eq!(d.as_value()["label"], json!("Textify"));
    }

    #[test]
    fn def03_fill_defaults_is_idempotent() {
        let mut once = CommandDeclaration::from_document(json!({"name":"to_text"}));
        once.fill_defaults();
        let mut twice = CommandDeclaration::from_document(json!({"name":"to_text"}));
        twice.fill_defaults();
        twice.fill_defaults();
        assert_eq!(once.as_value(), twice.as_value());
    }

    /// `ArgumentGUIInfo`'s `Default` is `None`, but `ArgumentInfo::any_argument` sets
    /// `TextField(40)`. Getting this wrong silently re-versions every command with declared
    /// arguments registered through the other path.
    #[test]
    fn def04_argument_gui_info_defaults_to_text_field_40() {
        let m = build(json!({ "name": "f", "arguments": [{ "name": "a" }] }));
        assert_eq!(m.arguments[0].gui_info, ArgumentGUIInfo::TextField(40));
    }

    #[test]
    fn def05_scalar_defaults_match_from_key() {
        let m = build(json!({ "name": "greet" }));
        let k = CommandMetadata::from_key(CommandKey::new("", "", "greet"));
        assert_eq!(m.cache, k.cache);
        assert_eq!(m.volatile, k.volatile);
        assert_eq!(m.expires, k.expires);
        assert_eq!(m.definition, k.definition);
        assert_eq!(m.payload_required, k.payload_required);
    }

    /// Order is normative. Deriving before merging would make a derived label look "present".
    #[test]
    fn def06_derive_runs_after_merge() {
        let mut d = CommandDeclaration::from_document(json!({ "name": "to_text" }));
        d.fill_defaults();
        d.enhance(&json!({ "label": "Textify" })).unwrap();
        assert_eq!(d.as_value()["label"], json!("Textify"));
    }

    // --- stage 5: build and validation ---------------------------------------------------------

    #[test]
    fn build01_minimal_declaration_builds() {
        let m = build(json!({ "name": "greet" }));
        assert_eq!(m.name, "greet");
        assert_eq!(m.label, "Greet");
        assert!(m.arguments.is_empty());
    }

    #[test]
    fn build02_type_is_accepted_for_argument_type() {
        let m = build(json!({ "name": "f", "arguments": [{ "name": "a", "type": "int" }] }));
        assert_eq!(m.arguments[0].argument_type, ArgumentType::Integer);
    }

    #[test]
    fn build03_argument_type_aliases() {
        for (spelling, want) in [
            ("str", ArgumentType::String),
            ("text", ArgumentType::String),
            ("integer", ArgumentType::Integer),
            ("number", ArgumentType::Float),
            ("boolean", ArgumentType::Boolean),
        ] {
            let m = build(json!({"name":"f","arguments":[{"name":"a","type":spelling}]}));
            assert_eq!(m.arguments[0].argument_type, want, "spelling {spelling:?}");
        }
    }

    #[test]
    fn build04_command_parameter_value_shapes() {
        let cases = [
            (json!(2), CommandParameterValue::Value(json!(2))),
            (json!("hello"), CommandParameterValue::Value(json!("hello"))),
            (json!(true), CommandParameterValue::Value(json!(true))),
            (json!(null), CommandParameterValue::Value(json!(null))),
            (json!("None"), CommandParameterValue::None),
            (
                json!({ "Value": 2 }),
                CommandParameterValue::Value(json!(2)),
            ),
        ];
        for (input, want) in cases {
            let m = build(json!({"name":"f","arguments":[{"name":"a","default":input}]}));
            assert_eq!(m.arguments[0].default, want);
        }
    }

    /// The documented trap: the bare string "None" is the absent-default marker.
    #[test]
    fn build05_none_string_needs_the_tagged_form() {
        let bare = build(json!({"name":"f","arguments":[{"name":"a","default":"None"}]}));
        assert_eq!(bare.arguments[0].default, CommandParameterValue::None);
        let tagged =
            build(json!({"name":"f","arguments":[{"name":"a","default":{"Value":"None"}}]}));
        assert_eq!(
            tagged.arguments[0].default,
            CommandParameterValue::Value(json!("None"))
        );
    }

    #[test]
    fn val01_empty_name_is_refused() {
        assert!(try_build(json!({ "name": "" })).is_err());
        assert!(try_build(json!({})).is_err());
    }

    #[test]
    fn val02_multiple_argument_must_be_last() {
        let error = try_build(json!({"name":"f","arguments":[
            {"name":"xs","multiple":true},{"name":"y"}]}))
        .unwrap_err();
        let message = format!("{error}");
        assert!(message.contains("xs"), "{message}");
        assert!(message.contains('y'), "{message}");
    }

    #[test]
    fn val04_unknown_argument_type_names_the_command() {
        let error =
            try_build(json!({"name":"f","arguments":[{"name":"a","type":"zzz"}]})).unwrap_err();
        let message = format!("{error}");
        assert!(message.contains('f'), "names the command: {message}");
        assert!(
            message.contains("zzz"),
            "names the offending type: {message}"
        );
    }

    // --- stage 3: conventions -----------------------------------------------------------------

    /// A context parameter is not a command argument. `register_command!` gives it no argument
    /// slot; a dynamic host needs this rule to reach the same place.
    #[test]
    fn conv01_context_leaves_arguments_and_lands_in_registration() {
        let mut d = CommandDeclaration::from_introspection(json!({"name":"f","arguments":[
            { "name": "value" }, { "name": "count" }, { "name": "context" }]}));
        d.apply_conventions().unwrap();
        let m = finish(&mut d);
        assert_eq!(
            m.arguments.len(),
            1,
            "`value` became the state, `context` is gone"
        );
        assert_eq!(m.arguments[0].name, "count");
        assert_eq!(d.registration()["context"], json!(2));
    }

    /// The first argument is *always* the state-derived argument; its name selects only the
    /// delivery mode. `df` here is the state, delivered as `Reserved("df")`, which is `value`.
    #[test]
    fn conv02_the_first_argument_is_always_the_state() {
        for (first, want_mode) in [
            ("state", "state"),
            ("value", "value"),
            ("text", "text"),
            ("df", "df"),
        ] {
            let mut d = CommandDeclaration::from_introspection(
                json!({"name":"f","arguments":[{ "name": first }, { "name": "count" }]}),
            );
            d.apply_conventions().unwrap();
            let m = finish(&mut d);
            assert!(
                m.state_argument.is_some(),
                "first argument {first:?} is the state"
            );
            assert_eq!(m.arguments.len(), 1, "only `count` remains ({first:?})");
            assert_eq!(d.registration()["state"], json!(want_mode));
        }
    }

    /// Position still matters: only the *first* argument is the state.
    #[test]
    fn conv03_a_non_leading_state_name_is_an_ordinary_argument() {
        let mut d = CommandDeclaration::from_introspection(json!({"name":"f","arguments":[
            { "name": "value" }, { "name": "state" }, { "name": "text" }]}));
        d.apply_conventions().unwrap();
        let m = finish(&mut d);
        assert_eq!(m.arguments.len(), 2);
        assert_eq!(m.arguments[0].name, "state");
        assert_eq!(m.arguments[1].name, "text");
    }

    #[test]
    fn conv04_a_convention_can_be_disabled_by_name() {
        let mut d = CommandDeclaration::from_introspection(json!({"name":"f","arguments":[
            { "name": "value" }, { "name": "context" }]}));
        d.enhance(&json!({ "conventions": { "context": false } }))
            .unwrap();
        d.apply_conventions().unwrap();
        let m = finish(&mut d);
        assert_eq!(
            m.arguments.len(),
            1,
            "a genuine `context` argument survives"
        );
        assert_eq!(m.arguments[0].name, "context");
        assert!(
            m.state_argument.is_some(),
            "the delivery rule still applied to `value`"
        );
    }

    #[test]
    fn conv05_all_conventions_can_be_disabled() {
        let mut d = CommandDeclaration::from_introspection(json!({"name":"f","arguments":[
            { "name": "state" }, { "name": "context" }]}));
        d.enhance(&json!({ "conventions": false })).unwrap();
        d.apply_conventions().unwrap();
        let m = finish(&mut d);
        assert_eq!(m.arguments.len(), 2);
        assert!(m.state_argument.is_none());
    }

    /// Why conventions run *after* the merge: an author declaring metadata for a recognised name
    /// gets it matched by name, not rejected as unknown.
    #[test]
    fn conv06_declared_entry_for_a_recognised_name_merges_before_it_is_lifted() {
        let mut d = CommandDeclaration::from_introspection(json!({"name":"f","arguments":[
            { "name": "state" }, { "name": "count" }]}));
        d.enhance(&json!({ "arguments": [{ "name": "state", "label": "Input" }] }))
            .expect("must not be rejected as an unknown argument");
        d.apply_conventions().unwrap();
        let m = finish(&mut d);
        assert_eq!(m.state_argument.as_ref().unwrap().label, "Input");
    }

    #[test]
    fn conv07_conventions_are_idempotent() {
        let base = json!({"name":"f","arguments":[{ "name": "state" }, { "name": "context" }]});
        let mut once = CommandDeclaration::from_introspection(base.clone());
        once.apply_conventions().unwrap();
        let mut twice = CommandDeclaration::from_introspection(base);
        twice.apply_conventions().unwrap();
        twice.apply_conventions().unwrap();
        assert_eq!(once.as_value(), twice.as_value());
        assert_eq!(once.warnings(), twice.warnings(), "warnings too");
    }

    /// Core records the mode and never performs it; every integration reads the same values.
    #[test]
    fn conv08_each_delivery_mode_is_recorded_distinctly() {
        for name in ["none", "na", "state", "value", "text"] {
            let mut d = CommandDeclaration::from_introspection(
                json!({"name":"f","arguments":[{ "name": name }]}),
            );
            d.apply_conventions().unwrap();
            let want = if name == "na" { "none" } else { name };
            assert_eq!(d.registration()["state"], json!(want), "name {name:?}");
        }
    }

    /// `first_command` semantics: no state argument at all, and the marker is not an argument
    /// either. Matches `liquer`'s decorator, where has_state_argument=False dispatches f(*argv).
    #[test]
    fn conv09_none_gives_a_source_command() {
        for name in ["none", "na"] {
            let mut d = CommandDeclaration::from_introspection(
                json!({"name":"f","arguments":[{ "name": name }, { "name": "count" }]}),
            );
            d.apply_conventions().unwrap();
            let m = finish(&mut d);
            assert!(m.state_argument.is_none(), "{name:?} is a source command");
            assert_eq!(
                m.arguments.len(),
                1,
                "the marker is not an argument ({name:?})"
            );
            assert_eq!(m.arguments[0].name, "count");
        }
    }

    /// The escape hatch: declaring it explicitly beats the naming rule.
    #[test]
    fn conv10_explicit_state_argument_is_left_alone() {
        let mut d = CommandDeclaration::from_introspection(
            json!({"name":"f","arguments":[{ "name": "df" }]}),
        );
        d.enhance(&json!({ "state_argument": { "name": "df" } }))
            .unwrap();
        d.apply_conventions().unwrap();
        let m = finish(&mut d);
        assert_eq!(m.state_argument.as_ref().unwrap().name, "df");
    }

    /// The extension point: an unrecognised name is not an error, it means `value` until something
    /// gives it meaning, so a declaration written today survives `df` acquiring one.
    #[test]
    fn conv11_reserved_name_behaves_as_value() {
        assert_eq!(
            StateDelivery::from_argument_name("df"),
            StateDelivery::Reserved("df".to_string())
        );
        assert_eq!(
            StateDelivery::from_argument_name("df").effective(),
            StateDelivery::Value
        );
        assert_eq!(StateDelivery::from_argument_name("na"), StateDelivery::None);
    }

    /// How a host offers its own `first_command` affordance without depending on a name.
    #[test]
    fn conv12_a_declared_mode_wins_over_the_derived_one() {
        let mut d = CommandDeclaration::from_introspection(json!({"name":"f","arguments":[
            { "name": "value" }, { "name": "count" }]}));
        d.enhance(&json!({ "registration": { "state": "none" } }))
            .unwrap();
        d.apply_conventions().unwrap();
        let m = finish(&mut d);
        assert!(
            m.state_argument.is_none(),
            "declared `none` beats derived `value`"
        );
        // Asserted because the earlier version of this test checked only `state_argument`, which
        // for `none` is absent either way — so it passed even when the conventions had not run at
        // all. The first argument must still be consumed as the state marker.
        assert_eq!(m.arguments.len(), 1, "the first argument is still consumed");
        assert_eq!(m.arguments[0].name, "count");
    }

    /// An authored mode must be *applied*, not merely recorded. The guard that makes
    /// `apply_conventions` idempotent keyed on `registration.state` being present, which an author
    /// supplying the documented override sets before the first call — so the conventions were
    /// skipped entirely and the callable's first parameter stayed a public query argument.
    #[test]
    fn conv15_an_authored_mode_still_runs_the_conventions() {
        let mut d = CommandDeclaration::from_introspection(json!({"name":"f","arguments":[
            { "name": "df" }, { "name": "count" }, { "name": "context" }]}));
        d.enhance(&json!({ "registration": { "state": "value" } }))
            .unwrap();
        d.apply_conventions().unwrap();
        let m = finish(&mut d);
        assert!(
            m.state_argument.is_some(),
            "the authored mode creates the state argument"
        );
        assert_eq!(
            m.arguments.len(),
            1,
            "the first parameter is consumed, `context` removed"
        );
        assert_eq!(m.arguments[0].name, "count");
        assert_eq!(d.registration()["state"], json!("value"));
        assert_eq!(d.registration()["context"], json!(2));
    }

    /// A document declaring a delivery mode is stating that its command takes a state. Its
    /// `arguments` are the command's *public* arguments, so none of them is consumed — which is
    /// exactly why an explicit mode is the documented way for a document host to say this
    /// (`reference/COMMAND_DECLARATION.md` §3.2.3).
    #[test]
    fn conv16_a_document_can_declare_its_state_delivery() {
        let mut d = CommandDeclaration::from_document(json!({ "name": "f" }));
        d.enhance(&json!({ "arguments": [{ "name": "count" }],
                           "registration": { "state": "value" } }))
            .unwrap();
        d.apply_conventions().unwrap();
        let m = finish(&mut d);
        assert!(m.state_argument.is_some(), "the declared mode is honoured");
        assert_eq!(m.arguments.len(), 1, "no public argument is consumed");
        assert_eq!(m.arguments[0].name, "count");
        assert!(
            !kinds(&d).contains(&WarningKind::NoIntrospection),
            "the state is stated, so there is nothing to warn about"
        );
    }

    /// The same, for `none`: a document may declare itself a source command explicitly.
    #[test]
    fn conv17_a_document_can_declare_itself_a_source_command() {
        let mut d = CommandDeclaration::from_document(json!({ "name": "f" }));
        d.enhance(&json!({ "arguments": [{ "name": "count" }],
                           "registration": { "state": "none" } }))
            .unwrap();
        d.apply_conventions().unwrap();
        let m = finish(&mut d);
        assert!(m.state_argument.is_none());
        assert_eq!(m.arguments.len(), 1, "no public argument is consumed");
        assert!(!kinds(&d).contains(&WarningKind::NoIntrospection));
    }

    /// Structural before delivery, or `def f(context, x)` would make the context the state.
    #[test]
    fn conv13_leading_context_is_removed_before_the_delivery_rule() {
        let mut d = CommandDeclaration::from_introspection(json!({"name":"f","arguments":[
            { "name": "context" }, { "name": "value" }, { "name": "count" }]}));
        d.apply_conventions().unwrap();
        let m = finish(&mut d);
        assert_eq!(d.registration()["context"], json!(0));
        assert_eq!(
            d.registration()["state"],
            json!("value"),
            "`value`, not `context`"
        );
        assert_eq!(m.arguments.len(), 1);
    }

    /// The rule interprets a *function's parameters*. A document declaring public arguments where
    /// none were discovered must not lose its first one to the state.
    #[test]
    fn conv14_no_introspection_means_no_delivery_rule() {
        let mut d = CommandDeclaration::from_document(json!({ "name": "f" }));
        d.enhance(&json!({ "arguments": [{ "name": "count" }] }))
            .unwrap();
        d.apply_conventions().unwrap();
        let m = finish(&mut d);
        assert!(m.state_argument.is_none());
        assert_eq!(m.arguments.len(), 1);
        assert_eq!(m.arguments[0].name, "count");
    }

    // --- warnings -----------------------------------------------------------------------------

    #[test]
    fn warn01_a_reserved_delivery_name_warns_and_still_means_value() {
        let mut d = CommandDeclaration::from_introspection(json!({"name":"f","arguments":[
            { "name": "df" }, { "name": "count" }]}));
        d.apply_conventions().unwrap();
        assert!(kinds(&d).contains(&WarningKind::ReservedStateDelivery));
        assert_eq!(d.registration()["state"], json!("df"), "recorded verbatim");
        let w = &d.warnings()[0];
        assert!(w.message.contains("df"), "{}", w.message);
        assert_eq!(w.command, "f");
    }

    /// The surprise: removing a leading context shifts which argument becomes the state.
    #[test]
    fn warn02_a_leading_context_warns_that_it_shifted_the_state() {
        let mut d = CommandDeclaration::from_introspection(json!({"name":"f","arguments":[
            { "name": "context" }, { "name": "count" }]}));
        d.apply_conventions().unwrap();
        assert!(kinds(&d).contains(&WarningKind::ContextBeforeState));
        assert_eq!(
            d.registration()["state"],
            json!("count"),
            "`count` became the state"
        );
    }

    /// Scoped to the unstated case, so a document host that declares `state_argument` stays quiet
    /// — otherwise every command in a commands.yaml would warn.
    #[test]
    fn warn03_no_introspection_warns_only_when_the_state_is_unstated() {
        let mut noisy = CommandDeclaration::from_document(json!({ "name": "f" }));
        noisy
            .enhance(&json!({ "arguments": [{ "name": "count" }] }))
            .unwrap();
        noisy.apply_conventions().unwrap();
        assert!(kinds(&noisy).contains(&WarningKind::NoIntrospection));

        let mut quiet = CommandDeclaration::from_document(json!({ "name": "f" }));
        quiet
            .enhance(&json!({ "arguments": [{ "name": "count" }],
                              "state_argument": { "name": "state" } }))
            .unwrap();
        quiet.apply_conventions().unwrap();
        assert!(
            !kinds(&quiet).contains(&WarningKind::NoIntrospection),
            "declared, so no warning"
        );
    }

    #[test]
    fn warn04_a_dropped_command_level_hints_key_warns() {
        let mut d = CommandDeclaration::from_document(json!({"name":"f"}));
        d.enhance(&json!({ "hints": { "category": "text" } }))
            .unwrap();
        let m = finish(&mut d);
        assert!(kinds(&d).contains(&WarningKind::DroppedKey));
        assert_eq!(serde_json::to_value(&m).unwrap().get("hints"), None);
    }

    #[test]
    fn warn05_warnings_are_deduplicated() {
        let mut d = CommandDeclaration::from_introspection(
            json!({"name":"f","arguments":[{ "name": "df" }]}),
        );
        d.apply_conventions().unwrap();
        d.apply_conventions().unwrap();
        assert_eq!(
            d.warnings()
                .iter()
                .filter(|w| w.kind == WarningKind::ReservedStateDelivery)
                .count(),
            1
        );
    }

    /// Every warned-about case has a legitimate use, so failing would block correct declarations
    /// in order to catch incorrect ones.
    #[test]
    fn warn06_a_warning_is_never_fatal() {
        let mut d = CommandDeclaration::from_introspection(json!({"name":"f","arguments":[
            { "name": "context" }, { "name": "df" }]}));
        d.apply_conventions().unwrap();
        assert!(!d.warnings().is_empty());
        assert!(d.build().is_ok(), "warnings do not fail the build");
    }

    // --- hints, of two kinds -------------------------------------------------------------------

    #[test]
    fn hint01_registration_hints_merge_like_any_map() {
        let mut d = CommandDeclaration::from_introspection(json!({"name":"f"}));
        d.enhance(&json!({ "registration": { "python": { "state": "text" } } }))
            .unwrap();
        d.enhance(&json!({ "registration": { "python": { "variadic": "spread" } } }))
            .unwrap();
        assert_eq!(d.registration()["python"]["state"], json!("text"));
        assert_eq!(d.registration()["python"]["variadic"], json!("spread"));
    }

    /// Registration hints are declaration-only: readable from the declaration, absent from
    /// the metadata.
    #[test]
    fn hint02_build_drops_registration() {
        let mut d = CommandDeclaration::from_document(json!({"name":"f"}));
        d.enhance(&json!({ "registration": { "python": { "state": "text" } } }))
            .unwrap();
        let m = finish(&mut d);
        assert_eq!(serde_json::to_value(&m).unwrap().get("registration"), None);
        assert_eq!(
            d.registration()["python"]["state"],
            json!("text"),
            "still readable"
        );
    }

    #[test]
    fn hint03_unknown_registration_key_is_carried_not_rejected() {
        let mut d = CommandDeclaration::from_document(json!({"name":"f"}));
        d.enhance(&json!({ "registration": { "javascript": { "stat": "text" } } }))
            .unwrap();
        assert_eq!(d.registration()["javascript"]["stat"], json!("text"));
        assert!(d.build().is_ok());
    }

    /// The other kind: a usage hint is ordinary metadata and must reach the built command.
    #[test]
    fn hint04_usage_hint_on_an_argument_reaches_the_metadata() {
        let m = build(json!({ "name": "f", "arguments": [
            { "name": "a", "hints": { "placeholder": "how many times" } }
        ]}));
        assert_eq!(m.arguments[0].hints["placeholder"], json!("how many times"));
    }

    /// Resolving one needs a registry, so it stays where it happens today.
    #[test]
    fn val05_global_enum_reference_is_not_resolved_and_does_not_fail() {
        let m = build(json!({"name":"f","arguments":[
            {"name":"a","argument_type":{"GlobalEnum":"colours"}}]}));
        assert!(matches!(
            m.arguments[0].argument_type,
            ArgumentType::GlobalEnum(_)
        ));
    }
}
