//! The `ns-mem` commands. Each takes what it acts on as state: a document from `-R/<key>`
//! (`read`), or a key from `-R-key/<path>` (all others), and the memory's root as its last
//! argument, `root: Key = query "-R-key/."`.

use indexmap::IndexMap;
use liquers_core::context::{Context, Environment};
use liquers_core::error::Error;
use liquers_core::query::Key;
use liquers_core::state::State;
use liquers_core::value::ValueInterface;

use crate::search::non_negative;
use crate::value::{ExtValueInterface, Value};

use super::check::check_view;
use super::front_matter::{compose, parse_version, FrontMatterDoc};
use super::layout::{join_path, read_bytes, relative_path, MemoryLayout};
use super::read::{
    describe_text, list_view, overview_text, read_part, search_memories, similar_view, version_of,
};
use super::write::{commit, edit_ops, today, WriteOp, WriteRequest};

/// The key a key state carries (`-R-key/<path>`).
fn key_state(state: &State<Value>, command: &str) -> Result<Key, Error> {
    let value = state.data_unchecked();
    match value.identifier().as_ref() {
        "Key" => value.try_into_key(),
        other => Err(Error::general_error(format!(
            "{command} acts on a key: start the query with -R-key/<path>, not a value of type \
             {other}"
        ))),
    }
}

async fn layout_of<E: Environment<Value = Value>>(
    root: &Key,
    context: &Context<E>,
) -> Result<MemoryLayout, Error> {
    MemoryLayout::load(&context.get_envref(), root).await
}

fn json(value: serde_json::Value) -> Result<Value, Error> {
    Value::try_from_json_value(&value)
}

/// A `fields` argument: a JSON object (an MCP override), or YAML/JSON mapping text; empty: none.
fn fields_of(fields: &Value) -> Result<IndexMap<String, serde_json::Value>, Error> {
    let value = fields.try_into_json_value()?;
    let value = match value {
        serde_json::Value::String(text) if text.trim().is_empty() => return Ok(IndexMap::new()),
        serde_json::Value::String(text) => serde_yaml::from_str::<serde_json::Value>(&text)
            .map_err(|e| Error::general_error(format!("fields is not a mapping: {e}")))?,
        serde_json::Value::Null => return Ok(IndexMap::new()),
        other => other,
    };
    match value {
        serde_json::Value::Object(map) => Ok(map.into_iter().collect()),
        serde_json::Value::Null
        | serde_json::Value::Bool(_)
        | serde_json::Value::Number(_)
        | serde_json::Value::String(_)
        | serde_json::Value::Array(_) => {
            Err(Error::general_error("fields must be a mapping of field names to values".to_string()))
        }
    }
}

fn shown_note<E: Environment<Value = Value>>(
    context: &Context<E>,
    shown: usize,
    total: usize,
) -> Result<(), Error> {
    if shown < total {
        context.info(&format!("shown {shown} of {total}; raise limit to see more"))
    } else {
        context.info(&format!("shown {shown} of {total}"))
    }
}

/// `-mem-overview`.
pub async fn overview<E: Environment<Value = Value>>(
    _state: State<Value>,
    log_lines: i64,
    root: Key,
    context: Context<E>,
) -> Result<Value, Error> {
    let log_lines = non_negative(log_lines, "log_lines")?;
    let layout = layout_of(&root, &context).await?;
    Ok(Value::from(overview_text(&layout, &root, log_lines, &context).await?))
}

/// `-mem-describe`.
pub async fn describe<E: Environment<Value = Value>>(
    _state: State<Value>,
    collection: String,
    root: Key,
    context: Context<E>,
) -> Result<Value, Error> {
    let layout = layout_of(&root, &context).await?;
    Ok(Value::from(describe_text(&layout, &root, &collection, &context).await?))
}

/// `-mem-list`.
pub async fn list<E: Environment<Value = Value>>(
    state: State<Value>,
    depth: i64,
    include_superseded: bool,
    limit: i64,
    root: Key,
    context: Context<E>,
) -> Result<Value, Error> {
    let folder = key_state(&state, "list")?;
    let depth = non_negative(depth, "depth")?;
    let limit = non_negative(limit, "limit")?;
    let layout = layout_of(&root, &context).await?;
    let (view, shown, total) =
        list_view(&layout, &root, &folder, depth, include_superseded, limit, &context).await?;
    shown_note(&context, shown, total)?;
    Ok(Value::from_record_view(view))
}

/// `-mem-search`.
#[allow(clippy::too_many_arguments)]
pub async fn search<E: Environment<Value = Value>>(
    state: State<Value>,
    text: String,
    unit: String,
    filter: String,
    limit: i64,
    include_superseded: bool,
    root: Key,
    context: Context<E>,
) -> Result<Value, Error> {
    let folder = key_state(&state, "search")?;
    let limit = non_negative(limit, "limit")?;
    let by_section = match unit.as_str() {
        "document" | "" => false,
        "section" => true,
        other => {
            return Err(Error::general_error(format!(
                "unit '{other}' is not one of document, section"
            )))
        }
    };
    let layout = layout_of(&root, &context).await?;
    let (view, report) = search_memories(
        &layout, &root, &folder, &text, by_section, &filter, limit, include_superseded, &context,
    )
    .await?;
    for warning in &report.warnings {
        context.warning(warning)?;
    }
    shown_note(&context, report.shown, report.matched)?;
    Ok(Value::from_record_view(view))
}

/// `-mem-read`.
pub async fn read<E: Environment<Value = Value>>(
    state: State<Value>,
    tier: String,
    section: String,
    lines: String,
    context: Context<E>,
) -> Result<Value, Error> {
    let value = state.data_unchecked();
    // A key state is read from the store, so a hand edit is seen even when the asset manager
    // holds an older copy of the document; a document state is read as it was evaluated.
    let (text, title, description, key) = if value.identifier().as_ref() == "Key" {
        let key = value.try_into_key()?;
        let (bytes, metadata) = context.get_envref().get_async_store().get(&key).await?;
        let text = String::from_utf8(bytes).map_err(|_| {
            Error::general_error(format!("read needs a text document; {} is not UTF-8", key.encode()))
        })?;
        (text, metadata.title().to_string(), metadata.description().to_string(), Some(key))
    } else {
        let text = value.try_into_string().map_err(|_| {
            Error::general_error(format!(
                "read needs a text document, got a value of type {}",
                value.identifier()
            ))
        })?;
        (text, state.metadata.title().to_string(), state.metadata.description().to_string(), state.metadata.key()?)
    };
    let part = read_part(&text, &title, &description, &tier, &section, &lines)?;
    if let Some(key) = key {
        context.info(&format!("key {}", key.encode()))?;
        context.info(&format!("version {}", version_of(&text)))?;
    }
    Ok(Value::from(part))
}

/// `-mem-similar`.
pub async fn similar<E: Environment<Value = Value>>(
    _state: State<Value>,
    collection: String,
    title: String,
    description: String,
    limit: i64,
    root: Key,
    context: Context<E>,
) -> Result<Value, Error> {
    let limit = non_negative(limit, "limit")?;
    let layout = layout_of(&root, &context).await?;
    let schema = layout.collection(&collection)?;
    let view = similar_view(&layout, &root, schema, &title, &description, limit, &context).await?;
    context.info(&format!("{} similar", view.len()))?;
    Ok(Value::from_record_view(view))
}

/// `-mem-new`.
#[allow(clippy::too_many_arguments)]
pub async fn new<E: Environment<Value = Value>>(
    _state: State<Value>,
    collection: String,
    title: String,
    description: String,
    fields: Value,
    body: String,
    force: bool,
    reason: String,
    root: Key,
    context: Context<E>,
) -> Result<Value, Error> {
    let envref = context.get_envref();
    let layout = layout_of(&root, &context).await?;
    let schema = layout.collection(&collection)?;
    if !schema.writable {
        return Err(Error::general_error(format!("collection '{collection}' is not writable")));
    }
    if title.trim().is_empty() || description.trim().is_empty() {
        return Err(Error::general_error(
            "a new memory needs a title and a description: they are the L0/L1 others find it by"
                .to_string(),
        ));
    }
    if !force {
        let candidates = similar_view(&layout, &root, schema, &title, &description, 5, &context).await?;
        if candidates.len() > 0 {
            let mut rows = Vec::new();
            for row in 0..candidates.len() {
                let mut object = serde_json::Map::new();
                for (col, field) in candidates.schema().fields.iter().enumerate() {
                    object.insert(field.name.clone(), field_json(&candidates.value(row, col)?));
                }
                rows.push(serde_json::Value::Object(object));
            }
            context.info(&format!(
                "{} similar memories exist; update one of them, or pass force=true to create anyway",
                rows.len()
            ))?;
            return json(serde_json::json!({ "written": false, "candidates": rows }));
        }
    }
    let given = fields_of(&fields)?;
    let today = today();
    let id = match given.get("id").map(|v| super::front_matter::value_text(Some(v))) {
        Some(id) if !id.is_empty() => id,
        Some(_) | None => schema.derive_id(&title, &today),
    };
    if id.is_empty() {
        return Err(Error::general_error(format!("no id can be derived from the title '{title}'")));
    }
    let key = schema.key_for_id(&root, &id)?;
    // Schema fields first, in their order, then title, description and the rest.
    let mut ordered: IndexMap<String, serde_json::Value> = IndexMap::new();
    let mut given = given;
    given.insert("title".to_string(), serde_json::Value::String(title.clone()));
    given.insert("description".to_string(), serde_json::Value::String(description.clone()));
    schema.apply_defaults(&mut given);
    for (name, rule) in &schema.fields {
        let is_id = name == "id" || rule.equals.is_some();
        if is_id && schema.file.is_none() {
            ordered.insert(name.clone(), serde_json::Value::String(id.clone()));
            given.shift_remove(name);
        } else if let Some(value) = given.shift_remove(name) {
            ordered.insert(name.clone(), value);
        }
    }
    for (name, value) in given {
        ordered.insert(name, value);
    }
    let mut body = body;
    if body.trim().is_empty() {
        if let Some(template) = &schema.template {
            if let Some(bytes) = read_bytes(&envref, &join_path(&root, template)?).await? {
                let text = String::from_utf8_lossy(&bytes).into_owned();
                body = FrontMatterDoc::parse(&text).map(|d| d.body().to_string()).unwrap_or(text);
                // The template's own title heading becomes the memory's.
                if let Some(line) = body.lines().find(|l| l.starts_with("# ")).map(str::to_string) {
                    body = body.replacen(&line, &format!("# {title}"), 1);
                }
            }
        }
    }
    if body.trim().is_empty() {
        body = format!("# {title}\n");
    }
    let text = compose(&ordered, &body)?;
    let outcome = commit(&envref, &root, &layout, WriteRequest {
        key,
        op: WriteOp::Create { text },
        expected: None,
        reason,
    })
    .await?;
    context.info(&format!("version {}", outcome.version_after))?;
    json(serde_json::json!({
        "written": true,
        "key": relative_path(&root, &outcome.key),
        "version": outcome.version_after.to_string(),
    }))
}

fn field_json(value: &crate::records::FieldValue) -> serde_json::Value {
    use crate::records::FieldValue;
    match value {
        FieldValue::Text(t) => serde_json::Value::String(t.to_string()),
        FieldValue::Int(i) => serde_json::json!(i),
        FieldValue::Float(f) => serde_json::json!(f),
        FieldValue::Bool(b) => serde_json::json!(b),
        FieldValue::Null => serde_json::Value::Null,
        other => serde_json::Value::String(format!("{other:?}")),
    }
}

/// `-mem-write`.
#[allow(clippy::too_many_arguments)]
pub async fn write<E: Environment<Value = Value>>(
    state: State<Value>,
    body: String,
    title: String,
    description: String,
    fields: Value,
    expected_version: String,
    supersedes: String,
    reason: String,
    root: Key,
    context: Context<E>,
) -> Result<Value, Error> {
    let key = key_state(&state, "write")?;
    let expected = if expected_version.trim().is_empty() { None } else { Some(parse_version(&expected_version)?) };
    let mut fields = fields_of(&fields)?;
    if !title.trim().is_empty() {
        fields.insert("title".to_string(), serde_json::Value::String(title));
    }
    if !description.trim().is_empty() {
        fields.insert("description".to_string(), serde_json::Value::String(description));
    }
    let old = if supersedes.trim().is_empty() {
        None
    } else {
        let old = join_path(&root, supersedes.trim())?;
        fields.insert("supersedes".to_string(), serde_json::Value::String(relative_path(&root, &old)));
        Some(old)
    };
    let body = (!body.is_empty()).then_some(body);
    if fields.is_empty() && body.is_none() {
        return Err(Error::general_error(
            "write needs a body, a title, a description, fields or supersedes".to_string(),
        ));
    }
    let envref = context.get_envref();
    let layout = layout_of(&root, &context).await?;
    if let Some(old) = &old {
        if read_bytes(&envref, old).await?.is_none() {
            return Err(Error::key_not_found(old));
        }
    }
    let outcome = commit(&envref, &root, &layout, WriteRequest {
        key: key.clone(),
        op: WriteOp::Write { fields, body },
        expected,
        reason: reason.clone(),
    })
    .await?;
    context.info(&format!("version {}", outcome.version_after))?;
    let mut result = outcome.to_json(&root);
    if let Some(old) = old {
        let mut mark = IndexMap::new();
        mark.insert(
            "superseded_by".to_string(),
            serde_json::Value::String(relative_path(&root, &key)),
        );
        let second = commit(&envref, &root, &layout, WriteRequest {
            key: old.clone(),
            op: WriteOp::Write { fields: mark, body: None },
            expected: None,
            reason,
        })
        .await
        .map_err(|e| {
            Error::general_error(format!(
                "'{}' now supersedes '{}', but marking the old memory failed ({}); check reports \
                 the partial supersede",
                relative_path(&root, &key),
                relative_path(&root, &old),
                e.message
            ))
        })?;
        if let serde_json::Value::Object(map) = &mut result {
            map.insert("superseded".to_string(), second.to_json(&root));
        }
    }
    json(result)
}

/// `-mem-edit`.
#[allow(clippy::too_many_arguments)]
pub async fn edit<E: Environment<Value = Value>>(
    state: State<Value>,
    old: String,
    new: String,
    field: String,
    value: String,
    section: String,
    row: String,
    expected_version: String,
    reason: String,
    root: Key,
    context: Context<E>,
) -> Result<Value, Error> {
    let ops = edit_ops(&old, &new, &field, &value, &section, &row)?;
    let key = key_state(&state, "edit")?;
    let expected = if expected_version.trim().is_empty() { None } else { Some(parse_version(&expected_version)?) };
    let layout = layout_of(&root, &context).await?;
    let outcome = commit(&context.get_envref(), &root, &layout, WriteRequest {
        key,
        op: WriteOp::Edit(ops),
        expected,
        reason,
    })
    .await?;
    context.info(&format!("version {}", outcome.version_after))?;
    json(outcome.to_json(&root))
}

/// `-mem-check`.
pub async fn check<E: Environment<Value = Value>>(
    state: State<Value>,
    root: Key,
    context: Context<E>,
) -> Result<Value, Error> {
    let folder = key_state(&state, "check")?;
    let layout = layout_of(&root, &context).await?;
    let (view, report) = check_view(&layout, &root, &folder, &context).await?;
    context.info(&format!("{} errors, {} warnings", report.errors, report.warnings))?;
    if !report.unsupported.is_empty() {
        context.info(&format!("unsupported: {}", report.unsupported.join("; ")))?;
    }
    Ok(Value::from_record_view(view))
}

/// Registers the `ns-mem` commands. The caller must define `type CommandEnvironment = ...`.
#[macro_export]
macro_rules! register_mem_commands {
    ($cr:expr) => {{
        use liquers_macro::register_command;
        use $crate::mem::commands::*;
        #[allow(unused_imports)]
        use $crate::value::Value;
        #[allow(unused_imports)]
        use liquers_core::query::Key;

        register_command!($cr,
            async fn overview(state, log_lines: i64 = 10, root: Key = query "-R-key/.", context) -> result
            namespace: "mem"
            label: "Memory overview"
            doc: "The memory's README tiers, the schema's instructions, its collections with counts, and the latest log lines"
            filename: "overview.md"
            volatile: true
        )?;
        register_command!($cr,
            async fn describe(state, collection: String = "", root: Key = query "-R-key/.", context) -> result
            namespace: "mem"
            label: "Describe collections"
            doc: "Collections, fields, vocabularies, templates; which are writable"
            filename: "describe.md"
            volatile: true
        )?;
        register_command!($cr,
            async fn list(state, depth: i64 = 1, include_superseded: bool = false, limit: i64 = 100,
                root: Key = query "-R-key/.", context) -> result
            namespace: "mem"
            label: "List memories"
            doc: "Entries under the folder (state: -R-key/<folder>) with their L0/L1, kind and status; superseded memories hidden"
            filename: "list.md"
            volatile: true
        )?;
        register_command!($cr,
            async fn search(state, text: String = "",
                unit: String = "document" (enum: ["document", "section"]),
                filter: String = "", limit: i64 = 10, include_superseded: bool = false,
                root: Key = query "-R-key/.", context) -> result
            namespace: "mem"
            label: "Search memories"
            doc: "Documents (or sections) under the folder ranked against text; filter is a search expression over front-matter fields, e.g. -status:closed*"
            filename: "search.md"
            volatile: true
        )?;
        register_command!($cr,
            async fn read(state, tier: String = "full" (enum: ["l0", "l1", "full"]),
                section: String = "", lines: String = "", context) -> result
            namespace: "mem"
            label: "Read memory"
            doc: "A document (state: -R-key/<key>, read fresh from the store, or -R/<key>): its L0, L1, a section by heading, a line range (N-M) or all of it; notes carry its key and version"
            filename: "read.md"
            volatile: true
        )?;
        register_command!($cr,
            async fn similar(state, collection: String, title: String, description: String = "",
                limit: i64 = 5, root: Key = query "-R-key/.", context) -> result
            namespace: "mem"
            label: "Similar memories"
            doc: "Memories of a collection sharing at least half of the title's significant words"
            filename: "similar.md"
            volatile: true
        )?;
        register_command!($cr,
            async fn new(state, collection: String, title: String, description: String,
                fields: Value = "", body: String = "", force: bool = false, reason: String = "",
                root: Key = query "-R-key/.", context) -> result
            namespace: "mem"
            label: "New memory"
            doc: "Create a memory in a collection; returns similar memories instead unless force is set"
            filename: "new.json"
            volatile: true
        )?;
        register_command!($cr,
            async fn write(state, body: String = "", title: String = "", description: String = "",
                fields: Value = "", expected_version: String = "", supersedes: String = "",
                reason: String = "", root: Key = query "-R-key/.", context) -> result
            namespace: "mem"
            label: "Write memory"
            doc: "Replace a memory's body and set fields (state: -R-key/<key>); supersedes marks an older memory"
            filename: "write.json"
            volatile: true
        )?;
        register_command!($cr,
            async fn edit(state, old: String = "", new: String = "", field: String = "",
                value: String = "", section: String = "", row: String = "",
                expected_version: String = "", reason: String = "",
                root: Key = query "-R-key/.", context) -> result
            namespace: "mem"
            label: "Edit memory"
            doc: "Apply every given pair to one memory (state: -R-key/<key>) in one write: old/new replaces text, field/value sets a field, section/row appends a row"
            filename: "edit.json"
            volatile: true
        )?;
        register_command!($cr,
            async fn check(state, root: Key = query "-R-key/.", context) -> result
            namespace: "mem"
            label: "Check memory"
            doc: "Schema and consistency findings for the memories under the folder (state: -R-key/<folder>)"
            filename: "check.md"
            volatile: true
        )?;
        Ok::<(), liquers_core::error::Error>(())
    }};
}
