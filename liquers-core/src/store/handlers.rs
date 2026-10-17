//! The built-in handlers of the `store_config` realm (`specs/design/store-adapters/`, Phase 2 §4
//! catalogue): `defaults`, `set`, `require`, `media_type`, `file_facts`, `content_hash` and
//! `updated`. Each is a [`CommandMetadata`] built next to its [`StepBuilder`] and registered by
//! [`register_builtin_handlers`].

use crate::command_metadata::{
    ArgumentInfo, ArgumentType, CommandMetadata, CommandMetadataRegistry, EnumArgument,
};
use crate::error::Error;
use crate::metadata::{MetadataRecord, Status, Version};
use crate::plan::ResolvedParameterValues;

use super::realm::{
    string_list_parameter, string_parameter, CompiledStep, FactsIn, Stage, StoreConfigFunctions,
};

/// The text fields `set` and `require` accept.
pub const TEXT_FIELDS: [&str; 6] = [
    "title",
    "description",
    "media_type",
    "data_format",
    "type_identifier",
    "unicode_icon",
];

/// How a step treats a field that already has a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Replace the field.
    Overwrite,
    /// Set the field only where it is empty.
    Fill,
}

impl Mode {
    /// Reads the `mode` parameter.
    pub fn from_parameters(parameters: &ResolvedParameterValues) -> Result<Mode, Error> {
        match string_parameter(parameters, "mode")?.as_str() {
            "overwrite" => Ok(Mode::Overwrite),
            "fill" => Ok(Mode::Fill),
            other => Err(Error::configuration_error(format!(
                "mode '{other}' is not one of overwrite, fill"
            ))),
        }
    }

    /// True when a field holding `current` should be given a new value.
    pub fn applies_to(&self, current_is_empty: bool) -> bool {
        match self {
            Mode::Overwrite => true,
            Mode::Fill => current_is_empty,
        }
    }
}

/// The `mode` argument: `overwrite` (default) or `fill`.
pub fn mode_argument() -> ArgumentInfo {
    ArgumentInfo::argument("mode")
        .with_type(ArgumentType::Enum(
            EnumArgument::new("mode")
                .with_alternative("overwrite")
                .with_alternative("fill"),
        ))
        .with_default("overwrite")
        .with_description("overwrite replaces the field; fill sets it only where it is empty")
}

/// Refuses a stage the entry has no meaning in.
pub fn check_stage(name: &str, stage: Stage, allowed: &[Stage]) -> Result<(), Error> {
    if allowed.contains(&stage) {
        return Ok(());
    }
    let fields: Vec<&str> = allowed.iter().map(|s| s.field()).collect();
    Err(Error::configuration_error(format!(
        "'{name}' cannot be used in {}; it belongs in {}",
        stage.field(),
        fields.join(", ")
    )))
}

/// Refuses a field name `set` and `require` do not know.
pub fn check_text_field(field: &str) -> Result<(), Error> {
    if TEXT_FIELDS.contains(&field) {
        Ok(())
    } else {
        Err(Error::configuration_error(format!(
            "field '{field}' is not one of {}",
            TEXT_FIELDS.join(", ")
        )))
    }
}

/// The current value of a text field; empty when unset.
pub fn text_field(record: &MetadataRecord, field: &str) -> String {
    match field {
        "title" => record.title.clone(),
        "description" => record.description.clone(),
        "media_type" => record.media_type.clone().unwrap_or_default(),
        "data_format" => record.data_format.clone().unwrap_or_default(),
        "type_identifier" => record.type_identifier.clone(),
        "unicode_icon" => record.unicode_icon.clone(),
        _ => String::new(),
    }
}

/// Sets a text field, respecting `mode`. Unknown fields are refused at build, so are ignored here.
pub fn set_text_field(record: &mut MetadataRecord, field: &str, value: &str, mode: Mode) {
    if !mode.applies_to(text_field(record, field).is_empty()) {
        return;
    }
    let value = value.to_string();
    match field {
        "title" => record.title = value,
        "description" => record.description = value,
        "media_type" => record.media_type = Some(value),
        "data_format" => record.data_format = Some(value),
        "type_identifier" => record.type_identifier = value,
        "unicode_icon" => record.unicode_icon = value,
        _ => {}
    }
}

/// A handler's metadata in realm `store_config`, with its documentation.
fn handler(name: &str, label: &str, doc: &str) -> CommandMetadata {
    let mut metadata = CommandMetadata::new(name);
    metadata
        .with_namespace("")
        .with_label(label)
        .with_doc(doc)
        .with_hint("store_config", serde_json::json!("handler"));
    metadata
}

/// Registers every built-in handler.
pub fn register_builtin_handlers(
    cmr: &mut CommandMetadataRegistry,
    functions: &mut StoreConfigFunctions,
) -> Result<(), Error> {
    functions.register_step(
        cmr,
        handler(
            "defaults",
            "Defaults",
            "The record today's stores produce. on_default: a new record with updated, key, \
             filename, file size, status Source and the 'New metadata has been created' warning. \
             on_write: updated, key, filename, file size and status Source where unset.",
        ),
        build_defaults,
    )?;

    let mut set = handler(
        "set",
        "Set a field",
        "Sets one text field (title, description, media_type, data_format, type_identifier, \
         unicode_icon). A value containing '/' is written with '~I'.",
    );
    set.with_argument(ArgumentInfo::string_argument("field"))
        .with_argument(ArgumentInfo::string_argument("value"))
        .with_argument(mode_argument());
    functions.register_step(cmr, set, build_set)?;

    let mut require = handler(
        "require",
        "Require fields",
        "on_validate: refuses a record whose named text fields are empty, naming the first.",
    );
    require.with_argument(ArgumentInfo::string_argument("fields").set_multiple());
    functions.register_step(cmr, require, build_require)?;

    let mut media_type = handler(
        "media_type",
        "Media type from extension",
        "Sets media_type from the key's extension, leaving it alone for an unknown extension. \
         on_validate: refuses a media_type that disagrees with a known extension.",
    );
    media_type.with_argument(mode_argument());
    functions.register_step(cmr, media_type, build_media_type)?;

    let mut file_facts = handler(
        "file_facts",
        "Backend facts",
        "Sets file_size, created and etag from what the backend reports. on_write: file_size from \
         the bytes; created and etag are set after the commit.",
    );
    file_facts.with_argument(mode_argument());
    functions.register_step(cmr, file_facts, build_file_facts)?;

    let mut content_hash = handler(
        "content_hash",
        "Content hash",
        "Sets version to the hash of the bytes: always, off, or on_change (when size, created or \
         etag differ from the stored record, or it has no version).",
    );
    content_hash.with_argument(
        ArgumentInfo::argument("when")
            .with_type(ArgumentType::Enum(
                EnumArgument::new("when")
                    .with_alternative("on_change")
                    .with_alternative("always")
                    .with_alternative("off"),
            ))
            .with_default("on_change"),
    );
    functions.register_step(cmr, content_hash, build_content_hash)?;

    functions.register_step(
        cmr,
        handler(
            "updated",
            "Updated now",
            "on_write: sets updated to the current time. defaults already does this.",
        ),
        build_updated,
    )?;
    Ok(())
}

/// The file size the facts imply: the bytes when present, else the backend's report.
fn size_of(facts: &FactsIn<'_>) -> Option<u64> {
    match (facts.bytes, facts.facts) {
        (Some(bytes), _) => Some(bytes.len() as u64),
        (None, Some(data_facts)) => Some(data_facts.size),
        (None, None) => None,
    }
}

/// Today's `finalize_metadata` on a record: key, filename seeded from it, file size, status.
fn finalize(record: &mut MetadataRecord, facts: &FactsIn<'_>) {
    record.key = Some(facts.key.clone());
    if record.filename.is_none() {
        if let Some(name) = facts.key.filename() {
            record.set_filename(name.encode());
        }
    }
    if let Some(size) = size_of(facts) {
        record.file_size = Some(size);
    }
    match record.status {
        Status::None => record.status = Status::Source,
        Status::Directory
        | Status::Recipe
        | Status::Submitted
        | Status::Dependencies
        | Status::Processing
        | Status::Partial
        | Status::Error
        | Status::Storing
        | Status::Ready
        | Status::Expired
        | Status::Cancelled
        | Status::Source
        | Status::Override
        | Status::Volatile => {}
    }
}

fn build_defaults(
    stage: Stage,
    _parameters: &ResolvedParameterValues,
) -> Result<CompiledStep, Error> {
    check_stage("defaults", stage, &[Stage::Default, Stage::Write])?;
    let run: super::realm::CompiledFn = match stage {
        Stage::Default => Box::new(|record, facts, _out| {
            // Today's `default_metadata`: `with_key` also seeds the filename, format and icon.
            record.set_updated_now().with_key(facts.key.clone()).is_dir = false;
            finalize(record, facts);
            let operation = if facts.operation.is_empty() {
                String::new()
            } else {
                format!(" ({})", facts.operation)
            };
            record.warning(&format!("New metadata has been created.{operation}"));
            Ok(())
        }),
        Stage::Write | Stage::Validate | Stage::Read => Box::new(|record, facts, _out| {
            record.set_updated_now();
            finalize(record, facts);
            Ok(())
        }),
    };
    Ok(CompiledStep::new(run))
}

fn build_set(stage: Stage, parameters: &ResolvedParameterValues) -> Result<CompiledStep, Error> {
    check_stage("set", stage, &[Stage::Default, Stage::Write, Stage::Read])?;
    let field = string_parameter(parameters, "field")?;
    check_text_field(&field)?;
    let value = string_parameter(parameters, "value")?;
    let mode = Mode::from_parameters(parameters)?;
    Ok(CompiledStep::new(Box::new(move |record, _facts, _out| {
        set_text_field(record, &field, &value, mode);
        Ok(())
    })))
}

fn build_require(
    stage: Stage,
    parameters: &ResolvedParameterValues,
) -> Result<CompiledStep, Error> {
    check_stage("require", stage, &[Stage::Validate])?;
    let fields = string_list_parameter(parameters, "fields")?;
    if fields.is_empty() {
        return Err(Error::configuration_error(
            "'require' needs at least one field".to_string(),
        ));
    }
    for field in &fields {
        check_text_field(field)?;
    }
    Ok(CompiledStep::new(Box::new(move |record, facts, _out| {
        for field in &fields {
            if text_field(record, field).is_empty() {
                return Err(Error::general_error(format!(
                    "Metadata of '{}' is refused: field '{field}' is required",
                    facts.key
                )));
            }
        }
        Ok(())
    })))
}

/// The media type a key's extension implies, or `None` for no or an unknown extension.
pub fn media_type_of_key(key: &crate::query::Key) -> Option<String> {
    let name = key.filename()?.encode();
    let (_, extension) = name.rsplit_once('.')?;
    let extension = extension.to_ascii_lowercase();
    let media_type = crate::media_type::file_extension_to_media_type(&extension);
    if media_type == "application/octet-stream" {
        None
    } else {
        Some(media_type.to_string())
    }
}

fn build_media_type(
    stage: Stage,
    parameters: &ResolvedParameterValues,
) -> Result<CompiledStep, Error> {
    let mode = Mode::from_parameters(parameters)?;
    let run: super::realm::CompiledFn = match stage {
        Stage::Validate => Box::new(|record, facts, _out| {
            match (media_type_of_key(facts.key), record.media_type.as_deref()) {
                (Some(expected), Some(supplied)) if !supplied.is_empty() && supplied != expected => {
                    Err(Error::general_error(format!(
                        "Metadata of '{}' is refused: media type '{supplied}' disagrees with the extension, which implies '{expected}'",
                        facts.key
                    )))
                }
                (Some(_), Some(_)) | (Some(_), None) | (None, Some(_)) | (None, None) => Ok(()),
            }
        }),
        Stage::Default | Stage::Write | Stage::Read => Box::new(move |record, facts, _out| {
            if let Some(media_type) = media_type_of_key(facts.key) {
                set_text_field(record, "media_type", &media_type, mode);
            }
            Ok(())
        }),
    };
    Ok(CompiledStep::new(run))
}

fn build_file_facts(
    stage: Stage,
    parameters: &ResolvedParameterValues,
) -> Result<CompiledStep, Error> {
    check_stage(
        "file_facts",
        stage,
        &[Stage::Default, Stage::Write, Stage::Read],
    )?;
    let mode = Mode::from_parameters(parameters)?;
    let run: super::realm::CompiledFn = match stage {
        Stage::Write => Box::new(move |record, facts, _out| {
            // `created` and `etag` are the backend's, set by the pipeline after the commit.
            if let Some(size) = size_of(facts) {
                if mode.applies_to(record.file_size.is_none()) {
                    record.file_size = Some(size);
                }
            }
            Ok(())
        }),
        Stage::Default | Stage::Read | Stage::Validate => Box::new(move |record, facts, _out| {
            if let Some(data_facts) = facts.facts {
                if mode.applies_to(record.file_size.is_none()) {
                    record.file_size = Some(data_facts.size);
                }
                if data_facts.modified.is_some() && mode.applies_to(record.created.is_none()) {
                    record.created = data_facts.modified.clone();
                }
                if data_facts.etag.is_some() && mode.applies_to(record.etag.is_none()) {
                    record.etag = data_facts.etag.clone();
                }
            }
            Ok(())
        }),
    };
    Ok(CompiledStep::new(run))
}

/// When `content_hash` hashes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum When {
    OnChange,
    Always,
    Off,
}

/// True when the facts differ from the stored record: its size, backend time or ETag, or it has
/// no version yet. Unknown facts are not a change: there is nothing to compare.
pub fn changed_since_stored(facts: &FactsIn<'_>) -> bool {
    let Some(previous) = facts.previous else {
        return true;
    };
    if previous.version.is_none() {
        return true;
    }
    let Some(data_facts) = facts.facts else {
        return false;
    };
    previous.file_size != Some(data_facts.size)
        || (data_facts.modified.is_some() && previous.created != data_facts.modified)
        || (data_facts.etag.is_some() && previous.etag != data_facts.etag)
}

fn build_content_hash(
    stage: Stage,
    parameters: &ResolvedParameterValues,
) -> Result<CompiledStep, Error> {
    check_stage(
        "content_hash",
        stage,
        &[Stage::Default, Stage::Write, Stage::Read],
    )?;
    let when = match string_parameter(parameters, "when")?.as_str() {
        "on_change" => When::OnChange,
        "always" => When::Always,
        "off" => When::Off,
        other => {
            return Err(Error::configuration_error(format!(
                "when '{other}' is not one of on_change, always, off"
            )))
        }
    };
    let wants = move |facts: &FactsIn<'_>| match when {
        When::Off => false,
        When::Always => true,
        When::OnChange => changed_since_stored(facts),
    };
    let run: super::realm::CompiledFn = Box::new(move |record, facts, _out| {
        if when == When::Off {
            return Ok(());
        }
        // Bytes are present when the operation supplied them (`set`, which always hashes) or
        // the chain asked for them because `wants` held.
        if let Some(bytes) = facts.bytes {
            record.version = Some(Version::from_bytes(bytes));
        }
        Ok(())
    });
    let needs_bytes: super::realm::NeedsBytesFn =
        Box::new(move |facts| facts.bytes.is_none() && wants(facts));
    Ok(CompiledStep::with_needs_bytes(run, needs_bytes))
}

fn build_updated(
    stage: Stage,
    _parameters: &ResolvedParameterValues,
) -> Result<CompiledStep, Error> {
    check_stage("updated", stage, &[Stage::Write])?;
    Ok(CompiledStep::new(Box::new(|record, _facts, _out| {
        record.set_updated_now();
        Ok(())
    })))
}
