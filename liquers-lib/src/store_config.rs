//! `store_config` handlers that need `liquers-records` (`specs/design/store-adapters/`, Phase 2
//! §4 catalogue): `front_matter` projects a Markdown document's front matter into its metadata
//! record, and `front_matter_sync` writes the record's fields back into the front matter.
//!
//! They are entries of the realm `store_config`, declared and compiled like the built-ins in
//! `liquers_core::store::handlers`, and registered by [`register_lib_store_config`]. Behind the
//! `records` feature, because the front-matter parser lives in `liquers-records`.

use liquers_core::command_metadata::{ArgumentInfo, CommandMetadata, CommandMetadataRegistry};
use liquers_core::error::Error;
use liquers_core::metadata::MetadataRecord;
use liquers_core::plan::ResolvedParameterValues;
use liquers_core::query::Key;
use liquers_core::store::handlers::{
    changed_since_stored, check_stage, check_text_field, media_type_of_key, mode_argument,
    set_text_field, text_field, Mode,
};
use liquers_core::store::realm::{
    string_list_parameter, CompiledFn, CompiledStep, FactsIn, NeedsBytesFn, Stage,
    StoreConfigFunctions,
};
use liquers_records::search::{front_matter_end, parse_structure, StructFormat};

use crate::search::md_split;

/// The fields projected when a handler names none.
pub const DEFAULT_FRONT_MATTER_FIELDS: [&str; 2] = ["title", "description"];

/// Registers `front_matter` and `front_matter_sync` into a registry that already holds the
/// built-ins (every `CommandRegistry` does).
pub fn register_lib_store_config(
    cmr: &mut CommandMetadataRegistry,
    functions: &mut StoreConfigFunctions,
) -> Result<(), Error> {
    let mut front_matter = handler(
        "front_matter",
        "Front matter",
        "For a Markdown entry, sets the named text fields (default title and description) from \
         the document's YAML or TOML front matter. Other media types: no effect. on_read reads \
         the bytes only when the entry changed since its record was stored.",
    );
    front_matter
        .with_argument(mode_argument())
        .with_argument(fields_argument());
    functions.register_step(cmr, front_matter, build_front_matter)?;

    let mut front_matter_sync = handler(
        "front_matter_sync",
        "Front matter write-back",
        "on_write: in set_metadata, rewrites a Markdown entry's YAML front matter from the named \
         record fields (default title and description), turning the metadata write into a data \
         write. In set the caller's bytes are kept as they are.",
    );
    front_matter_sync.with_argument(fields_argument());
    functions.register_step(cmr, front_matter_sync, build_front_matter_sync)?;

    let mut markdown_tiers = handler(
        "markdown_tiers",
        "Markdown tiers",
        "For a Markdown entry whose record has an empty title or description, fills the title \
         from the first heading and the description from the first paragraph of the first named \
         section present, else from the first paragraph after the front matter. Never \
         overwrites; other media types: no effect. Put it after front_matter.",
    );
    markdown_tiers.with_argument(ArgumentInfo::string_argument("sections").set_multiple());
    functions.register_step(cmr, markdown_tiers, build_markdown_tiers)
}

fn handler(name: &str, label: &str, doc: &str) -> CommandMetadata {
    let mut metadata = CommandMetadata::new(name);
    metadata
        .with_namespace("")
        .with_label(label)
        .with_doc(doc)
        .with_hint("store_config", serde_json::json!("handler"));
    metadata
}

fn fields_argument() -> ArgumentInfo {
    ArgumentInfo::string_argument("fields").set_multiple()
}

/// The `fields` parameter, checked against the settable text fields; empty means the defaults.
fn fields_of(parameters: &ResolvedParameterValues) -> Result<Vec<String>, Error> {
    let mut fields = string_list_parameter(parameters, "fields")?;
    if fields.is_empty() {
        fields = DEFAULT_FRONT_MATTER_FIELDS
            .iter()
            .map(|f| f.to_string())
            .collect();
    }
    for field in &fields {
        check_text_field(field)?;
    }
    Ok(fields)
}

fn is_markdown(key: &Key) -> bool {
    media_type_of_key(key).as_deref() == Some("text/markdown")
}

fn is_markdown_record(record: &MetadataRecord, key: &Key) -> bool {
    is_markdown(key) || record.media_type.as_deref() == Some("text/markdown")
}

/// A front-matter value as a field's text: strings as they are, other scalars as JSON.
fn value_text(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Bool(_) | serde_json::Value::Number(_) => Some(value.to_string()),
        serde_json::Value::Null | serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
            None
        }
    }
}

fn build_front_matter(
    stage: Stage,
    parameters: &ResolvedParameterValues,
) -> Result<CompiledStep, Error> {
    check_stage(
        "front_matter",
        stage,
        &[Stage::Default, Stage::Write, Stage::Read],
    )?;
    let mode = Mode::from_parameters(parameters)?;
    let fields = fields_of(parameters)?;
    let run: CompiledFn = Box::new(move |record, facts, _out| {
        if !is_markdown_record(record, facts.key) {
            return Ok(());
        }
        let Some(bytes) = facts.bytes else {
            return Ok(());
        };
        let text = String::from_utf8_lossy(bytes);
        let Some(serde_json::Value::Object(map)) =
            parse_structure(&text, StructFormat::FrontMatter)
        else {
            return Ok(());
        };
        for field in &fields {
            if let Some(value) = map.get(field.as_str()).and_then(value_text) {
                set_text_field(record, field, &value, mode);
            }
        }
        Ok(())
    });
    let read_stage = stage == Stage::Read;
    let needs_bytes: NeedsBytesFn = Box::new(move |facts: &FactsIn<'_>| {
        facts.bytes.is_none()
            && is_markdown(facts.key)
            && (!read_stage || changed_since_stored(facts))
    });
    Ok(CompiledStep::with_needs_bytes(run, needs_bytes))
}

fn build_front_matter_sync(
    stage: Stage,
    parameters: &ResolvedParameterValues,
) -> Result<CompiledStep, Error> {
    check_stage("front_matter_sync", stage, &[Stage::Write])?;
    let fields = fields_of(parameters)?;
    let run: CompiledFn = Box::new(move |record, facts, out| {
        // In `set` the caller's bytes are the source of truth: rewriting them from a record whose
        // fields are still empty would erase the document's own front matter.
        if facts.operation != "set_metadata" || !is_markdown_record(record, facts.key) {
            return Ok(());
        }
        let Some(bytes) = facts.bytes else {
            return Ok(());
        };
        let text = String::from_utf8_lossy(bytes);
        match rewrite_front_matter(&text, record, &fields)? {
            Some(rewritten) => out.data = Some(rewritten.into_bytes()),
            None => {
                record.warning(&format!(
                    "front_matter_sync: '{}' has TOML front matter, which is not rewritten",
                    facts.key
                ));
            }
        }
        Ok(())
    });
    let needs_bytes: NeedsBytesFn = Box::new(|facts: &FactsIn<'_>| {
        facts.bytes.is_none() && facts.operation == "set_metadata" && is_markdown(facts.key)
    });
    Ok(CompiledStep::with_needs_bytes(run, needs_bytes))
}

fn build_markdown_tiers(
    stage: Stage,
    parameters: &ResolvedParameterValues,
) -> Result<CompiledStep, Error> {
    check_stage("markdown_tiers", stage, &[Stage::Default, Stage::Read])?;
    let sections = string_list_parameter(parameters, "sections")?;
    let run: CompiledFn = Box::new(move |record, facts, _out| {
        if !is_markdown_record(record, facts.key)
            || (!record.title.is_empty() && !record.description.is_empty())
        {
            return Ok(());
        }
        let Some(bytes) = facts.bytes else {
            return Ok(());
        };
        let text = String::from_utf8_lossy(bytes);
        if record.title.is_empty() {
            if let Some(heading) = md_split::first_heading(&text) {
                set_text_field(record, "title", &heading, Mode::Fill);
            }
        }
        if record.description.is_empty() {
            let paragraph = sections
                .iter()
                .find_map(|section| md_split::section_paragraph(&text, section))
                .or_else(|| md_split::first_paragraph(&text));
            if let Some(paragraph) = paragraph {
                set_text_field(record, "description", &paragraph, Mode::Fill);
            }
        }
        Ok(())
    });
    let read_stage = stage == Stage::Read;
    let needs_bytes: NeedsBytesFn = Box::new(move |facts: &FactsIn<'_>| {
        facts.bytes.is_none()
            && is_markdown(facts.key)
            && (!read_stage || changed_since_stored(facts))
    });
    Ok(CompiledStep::with_needs_bytes(run, needs_bytes))
}

/// The document with its YAML front matter carrying the record's `fields`, other keys kept;
/// `None` for TOML front matter. A document without front matter gets a new block.
fn rewrite_front_matter(
    text: &str,
    record: &MetadataRecord,
    fields: &[String],
) -> Result<Option<String>, Error> {
    let end = front_matter_end(text);
    if end > 0 && text.trim_start_matches('\u{feff}').starts_with("+++") {
        return Ok(None);
    }
    let mut map = match parse_structure(text, StructFormat::FrontMatter) {
        Some(serde_json::Value::Object(map)) => map,
        Some(_) | None => serde_json::Map::new(),
    };
    for field in fields {
        let value = text_field(record, field);
        if value.is_empty() {
            map.remove(field.as_str());
        } else {
            map.insert(field.clone(), serde_json::Value::String(value));
        }
    }
    let body = &text[end..];
    if map.is_empty() {
        return Ok(Some(body.to_string()));
    }
    let yaml = serde_yaml::to_string(&serde_json::Value::Object(map))
        .map_err(|e| Error::general_error(format!("front matter does not serialize: {e}")))?;
    Ok(Some(format!("---\n{yaml}---\n{body}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use liquers_core::parse::parse_key;
    use liquers_core::store::realm::StepOut;

    fn entries() -> (CommandMetadataRegistry, StoreConfigFunctions) {
        let (mut cmr, mut functions) = StoreConfigFunctions::builtin();
        register_lib_store_config(&mut cmr, &mut functions).expect("lib entries register");
        (cmr, functions)
    }

    fn run(
        spec: &str,
        stage: Stage,
        record: &mut MetadataRecord,
        facts: &FactsIn<'_>,
    ) -> Result<StepOut, Error> {
        let (cmr, functions) = entries();
        let chain = functions.compile(spec, stage, &cmr)?;
        let mut out = StepOut::default();
        chain.run(record, facts, &mut out)?;
        Ok(out)
    }

    const NOTE: &[u8] = b"---\ntitle: A note\ndescription: About things\ntags: [x]\n---\n# Body\n";

    #[test]
    fn front_matter_projects_title_and_description() -> Result<(), Error> {
        let key = parse_key("docs/a.md")?;
        let mut facts = FactsIn::new(&key);
        facts.bytes = Some(NOTE);
        let mut record = MetadataRecord::new();
        run("front_matter", Stage::Read, &mut record, &facts)?;
        assert_eq!(record.title, "A note");
        assert_eq!(record.description, "About things");
        // A non-Markdown key is left alone.
        let other = parse_key("docs/a.txt")?;
        let mut facts = FactsIn::new(&other);
        facts.bytes = Some(NOTE);
        let mut record = MetadataRecord::new();
        run("front_matter", Stage::Read, &mut record, &facts)?;
        assert_eq!(record.title, "");
        Ok(())
    }

    #[test]
    fn markdown_tiers_fills_only_empty_fields() -> Result<(), Error> {
        let key = parse_key("specs/issues/X.md")?;
        let text = b"---\nstatus: draft\n---\n# The title\n\nIntro.\n\n## Problem\n\nIt breaks.\n";
        let mut facts = FactsIn::new(&key);
        facts.bytes = Some(text);
        let mut record = MetadataRecord::new();
        run("front_matter/markdown_tiers-Problem-Purpose", Stage::Read, &mut record, &facts)?;
        assert_eq!(record.title, "The title");
        assert_eq!(record.description, "It breaks.");
        // Fields already set by front_matter are kept; without a listed section, the first
        // paragraph is used.
        let mut record = MetadataRecord::new();
        record.title = "Given".to_string();
        run("markdown_tiers-Purpose", Stage::Read, &mut record, &facts)?;
        assert_eq!(record.title, "Given");
        assert_eq!(record.description, "Intro.");
        let txt = parse_key("a.txt")?;
        let mut facts_txt = FactsIn::new(&txt);
        facts_txt.bytes = Some(text);
        let mut plain = MetadataRecord::new();
        run("markdown_tiers", Stage::Read, &mut plain, &facts_txt)?;
        assert_eq!(plain.title, "");
        Ok(())
    }

    #[test]
    fn front_matter_reads_bytes_on_read_only_when_changed() -> Result<(), Error> {
        let (cmr, functions) = entries();
        let chain = functions.compile("front_matter", Stage::Read, &cmr)?;
        let key = parse_key("docs/a.md")?;
        // No stored record: changed.
        assert!(chain.needs_bytes(&FactsIn::new(&key)));
        let mut previous = MetadataRecord::new();
        previous.version = Some(liquers_core::metadata::Version::from_bytes(NOTE));
        previous.file_size = Some(NOTE.len() as u64);
        let facts_data = liquers_core::store::DataFacts {
            size: NOTE.len() as u64,
            modified: None,
            etag: None,
        };
        let mut facts = FactsIn::new(&key);
        facts.previous = Some(&previous);
        facts.facts = Some(&facts_data);
        assert!(!chain.needs_bytes(&facts));
        Ok(())
    }

    #[test]
    fn front_matter_sync_rewrites_the_description() -> Result<(), Error> {
        let key = parse_key("docs/a.md")?;
        let mut facts = FactsIn::new(&key);
        facts.bytes = Some(NOTE);
        facts.operation = "set_metadata";
        let mut record = MetadataRecord::new();
        record.title = "A note".to_string();
        record.description = "Rewritten".to_string();
        let out = run("front_matter_sync", Stage::Write, &mut record, &facts)?;
        let data = String::from_utf8(out.data.expect("rewritten")).expect("utf-8");
        assert!(data.ends_with("---\n# Body\n"), "{data}");
        let reread = parse_structure(&data, StructFormat::FrontMatter).expect("front matter");
        assert_eq!(reread["description"], "Rewritten");
        assert_eq!(reread["title"], "A note");
        assert_eq!(reread["tags"], serde_json::json!(["x"]));
        // In `set`, the caller's bytes are kept.
        facts.operation = "set";
        let out = run("front_matter_sync", Stage::Write, &mut record, &facts)?;
        assert!(out.data.is_none());
        Ok(())
    }

    #[test]
    fn markdown_without_front_matter_reads_without_error() -> Result<(), Error> {
        let key = parse_key("docs/plain.md")?;
        let mut facts = FactsIn::new(&key);
        facts.bytes = Some(b"# Just a heading\n");
        let mut record = MetadataRecord::new();
        run("front_matter", Stage::Read, &mut record, &facts)?;
        assert_eq!(record.title, "");
        assert!(record.log.is_empty());
        Ok(())
    }

    #[test]
    fn lib_registration_makes_front_matter_nameable() -> Result<(), Error> {
        let (cmr, functions) = entries();
        functions.compile("media_type-fill/front_matter-fill-title", Stage::Read, &cmr)?;
        functions.compile("front_matter_sync", Stage::Write, &cmr)?;
        assert!(functions
            .compile("front_matter_sync", Stage::Read, &cmr)
            .is_err());
        assert!(functions
            .compile("front_matter-overwrite-children", Stage::Read, &cmr)
            .is_err());
        // Without the lib registration the name is unknown.
        let (cmr, functions) = StoreConfigFunctions::builtin();
        assert!(functions
            .compile("front_matter", Stage::Read, &cmr)
            .is_err());
        Ok(())
    }
}
