//! The one write path every writing `ns-mem` command uses: read, check the precondition, apply,
//! validate, stamp, write, log.

use indexmap::IndexMap;
use liquers_core::assets::AssetManager;
use liquers_core::context::{EnvRef, Environment};
use liquers_core::error::Error;
use liquers_core::metadata::{DependencyKey, MetadataRecord, Status, Version};
use liquers_core::query::Key;

use crate::search::md_split::find_section;

use super::front_matter::{value_text, FrontMatterDoc};
use super::layout::{join_path, read_bytes, relative_path, CollectionSchema, MemoryLayout};
use super::rules::{document_violations, Vocabularies};

/// One edit operation; [`apply_edits`] applies them in this order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditOp {
    /// Replace the one occurrence of `old` with `new` (empty `new` deletes `old`).
    Replace { old: String, new: String },
    /// Set a front-matter field; `value` is a YAML scalar or flow list.
    SetField { field: String, value: String },
    /// Append a line at the end of a section.
    AppendRow { section: String, row: String },
}

/// What a write does to the document at its key.
#[derive(Clone, Debug)]
pub enum WriteOp {
    /// A new document; refused when the key exists.
    Create { text: String },
    /// Set front-matter fields and, when given, replace the body.
    Write { fields: IndexMap<String, serde_json::Value>, body: Option<String> },
    /// Apply edit operations.
    Edit(Vec<EditOp>),
}

impl WriteOp {
    fn name(&self) -> &'static str {
        match self {
            WriteOp::Create { .. } => "create",
            WriteOp::Write { .. } => "write",
            WriteOp::Edit(_) => "edit",
        }
    }
}

#[derive(Clone, Debug)]
pub struct WriteRequest {
    pub key: Key,
    pub op: WriteOp,
    /// The version the caller read; the write is refused when the document changed since.
    pub expected: Option<Version>,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WriteOutcome {
    pub key: Key,
    pub version_before: Option<Version>,
    pub version_after: Version,
}

impl WriteOutcome {
    /// `{key, version_before, version_after}`, the key relative to `root`.
    pub fn to_json(&self, root: &Key) -> serde_json::Value {
        serde_json::json!({
            "key": relative_path(root, &self.key),
            "version_before": self.version_before.map(|v| v.to_string()),
            "version_after": self.version_after.to_string(),
        })
    }
}

/// The edit operations given as flat argument pairs. `old` with an empty `new` deletes `old`; any
/// other half-given pair, or no pair at all, is refused, before anything is read.
pub fn edit_ops(
    old: &str,
    new: &str,
    field: &str,
    value: &str,
    section: &str,
    row: &str,
) -> Result<Vec<EditOp>, Error> {
    let missing = |given: &str, absent: &str| {
        Error::general_error(format!("'{given}' is given without '{absent}'; give both or neither"))
    };
    let mut ops = Vec::new();
    match (old.is_empty(), new.is_empty()) {
        (false, _) => ops.push(EditOp::Replace { old: old.to_string(), new: new.to_string() }),
        (true, false) => return Err(missing("new", "old")),
        (true, true) => {}
    }
    match (field.is_empty(), value.is_empty()) {
        (false, false) => ops.push(EditOp::SetField { field: field.to_string(), value: value.to_string() }),
        (false, true) => return Err(missing("field", "value")),
        (true, false) => return Err(missing("value", "field")),
        (true, true) => {}
    }
    match (section.is_empty(), row.is_empty()) {
        (false, false) => ops.push(EditOp::AppendRow { section: section.to_string(), row: row.to_string() }),
        (false, true) => return Err(missing("section", "row")),
        (true, false) => return Err(missing("row", "section")),
        (true, true) => {}
    }
    if ops.is_empty() {
        return Err(Error::general_error(
            "edit needs at least one pair: old/new, field/value or section/row".to_string(),
        ));
    }
    Ok(ops)
}

/// A field value given as text: a YAML scalar or flow list (`[a, b]`).
pub fn parse_field_value(value: &str) -> serde_json::Value {
    serde_yaml::from_str::<serde_json::Value>(&format!("k: {value}"))
        .ok()
        .and_then(|v| v.get("k").cloned())
        .filter(|v| !v.is_object())
        .unwrap_or_else(|| serde_json::Value::String(value.to_string()))
}

/// `YYYY-MM-DD` of today, in UTC.
pub fn today() -> String {
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}

/// Applies `ops` to `text` in the fixed order replace, set field, append row. All or nothing:
/// any failure leaves nothing applied.
pub fn apply_edits(
    text: &str,
    ops: &[EditOp],
    collection: Option<&CollectionSchema>,
    today: &str,
) -> Result<String, Error> {
    let mut text = text.to_string();
    let order = |op: &EditOp| match op {
        EditOp::Replace { .. } => 0,
        EditOp::SetField { .. } => 1,
        EditOp::AppendRow { .. } => 2,
    };
    let mut ops: Vec<&EditOp> = ops.iter().collect();
    ops.sort_by_key(|op| order(op));
    for op in ops {
        match op {
            EditOp::Replace { old, new } => {
                let count = text.matches(old.as_str()).count();
                if count != 1 {
                    return Err(Error::general_error(format!(
                        "old text found {count} times; it must occur exactly once (nothing was \
                         written)"
                    )));
                }
                text = text.replacen(old.as_str(), new, 1);
            }
            EditOp::SetField { field, value } => {
                let mut doc = FrontMatterDoc::parse(&text)?;
                doc.set(field, &parse_field_value(value))?;
                text = doc.render();
            }
            EditOp::AppendRow { section, row } => {
                text = append_row(&text, section, row)?;
                if let Some(history) = collection.and_then(|c| c.history.as_ref()) {
                    if history.section.eq_ignore_ascii_case(section.trim()) {
                        let mut doc = FrontMatterDoc::parse(&text)?;
                        doc.set(&history.stamp, &serde_json::Value::String(today.to_string()))?;
                        text = doc.render();
                    }
                }
            }
        }
    }
    Ok(text)
}

/// `text` with `row` added after the last non-blank line of the section `section`.
fn append_row(text: &str, section: &str, row: &str) -> Result<String, Error> {
    if row.contains('\n') {
        return Err(Error::general_error("row must be a single line".to_string()));
    }
    let (_, start, end) = find_section(text, section)
        .ok_or_else(|| Error::general_error(format!("no section '{section}' in the document")))?;
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let mut last = start - 1;
    for i in (start - 1)..end.min(lines.len()) {
        if !lines[i].trim().is_empty() {
            last = i;
        }
    }
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut out = String::with_capacity(text.len() + row.len() + 2);
    for (i, line) in lines.iter().enumerate() {
        out.push_str(line);
        if i == last {
            if !line.ends_with('\n') {
                out.push_str(newline);
            }
            out.push_str(row);
            out.push_str(newline);
        }
    }
    Ok(out)
}

fn record_for(key: &Key, text: &str) -> MetadataRecord {
    let doc = FrontMatterDoc::parse(text).ok();
    let field = |name: &str| doc.as_ref().map(|d| d.get_text(name)).unwrap_or_default();
    let mut record = MetadataRecord {
        type_identifier: "Text".to_string(),
        type_name: "text".to_string(),
        data_format: Some("md".to_string()),
        media_type: Some("text/markdown".to_string()),
        title: field("title"),
        description: field("description"),
        ..Default::default()
    };
    record.filename = key.filename().map(|n| n.name.clone());
    record.status = Status::Source;
    record
}

/// Writes one memory: the steps of `specs/design/agent-memory-mvp/phase2-architecture.md`
/// §`commit`. When the data write succeeds and the log append fails, the data write stands and
/// the error says the log line is missing.
pub async fn commit<E: Environment>(
    envref: &EnvRef<E>,
    root: &Key,
    layout: &MemoryLayout,
    request: WriteRequest,
) -> Result<WriteOutcome, Error> {
    let key = request.key.clone();
    let rel = relative_path(root, &key);
    let collection = layout.collection_for(root, &key).ok_or_else(|| {
        Error::general_error(format!("'{rel}' is outside every collection of the memory"))
    })?;
    if !collection.writable {
        return Err(Error::general_error(format!(
            "collection '{}' is not writable",
            collection.name
        )));
    }
    // 1. Read.
    let current = match read_bytes(envref, &key).await? {
        Some(bytes) => Some(String::from_utf8(bytes).map_err(|_| {
            Error::general_error(format!("'{rel}' is not UTF-8 text"))
        })?),
        None => None,
    };
    let version_before = current.as_ref().map(|t| Version::from_content(t.as_bytes()));
    // 2. Precondition.
    if let Some(expected) = request.expected {
        if version_before != Some(expected) {
            let found = version_before.map(|v| v.to_string()).unwrap_or_else(|| "no document".to_string());
            return Err(Error::dependency_version_mismatch(
                &DependencyKey::from(&key),
                format!("expected version {expected}, found {found}; read it again"),
            ));
        }
    }
    let today = today();
    let mut text = match (&request.op, &current) {
        (WriteOp::Create { .. }, Some(_)) => {
            return Err(Error::general_error(format!("'{rel}' already exists")));
        }
        (WriteOp::Create { text }, None) => text.clone(),
        (WriteOp::Write { .. } | WriteOp::Edit(_), None) => return Err(Error::key_not_found(&key)),
        (WriteOp::Write { fields, body }, Some(current)) => {
            let mut doc = FrontMatterDoc::parse(current)?;
            for (name, value) in fields {
                doc.set(name, value)?;
            }
            if let Some(body) = body {
                doc.set_body(&if body.starts_with('\n') { body.clone() } else { format!("\n{body}") });
            }
            doc.render()
        }
        (WriteOp::Edit(ops), Some(current)) => apply_edits(current, ops, Some(collection), &today)?,
    };
    // 3. Validate: a write must not add a violation, nor make a transition the table forbids.
    let vocabularies = Vocabularies::load(envref, root, collection).await?;
    let before = current
        .as_deref()
        .map(|t| document_violations(collection, &vocabularies, &key, t))
        .unwrap_or_default();
    let new_doc = FrontMatterDoc::parse(&text)?;
    if let WriteOp::Create { .. } = request.op {
        for field in ["title", "description"] {
            if new_doc.get_text(field).trim().is_empty() {
                return Err(Error::general_error(format!(
                    "a new memory needs a {field}: it is the L0/L1 others find it by"
                )));
            }
        }
    }
    if let Some(current) = &current {
        let old_doc = FrontMatterDoc::parse(current)?;
        for name in collection.fields.keys() {
            vocabularies.check_transition(
                collection,
                name,
                &old_doc.get_text(name),
                &new_doc.get_text(name),
            )?;
        }
    }
    let added: Vec<String> = document_violations(collection, &vocabularies, &key, &text)
        .into_iter()
        .filter(|v| v.severity == super::rules::Severity::Error && !before.contains(v))
        .map(|v| v.message)
        .collect();
    if !added.is_empty() {
        return Err(Error::general_error(format!(
            "'{rel}' would break the schema of collection '{}': {}",
            collection.name,
            added.join("; ")
        )));
    }
    // 4. Stamp.
    let mut doc = new_doc;
    if let WriteOp::Create { .. } = request.op {
        if value_text(doc.get("created").as_ref()).is_empty() {
            doc.set("created", &serde_json::Value::String(today.clone()))?;
        }
    }
    doc.set("updated", &serde_json::Value::String(today.clone()))?;
    text = doc.render();
    // 5. Write.
    envref
        .get_asset_manager()
        .set_binary(&key, text.as_bytes(), record_for(&key, &text))
        .await?;
    let version_after = Version::from_content(text.as_bytes());
    let outcome = WriteOutcome { key: key.clone(), version_before, version_after };
    // 6. Log.
    let line = log_line(request.op.name(), &rel, version_before, version_after, &request.reason);
    append_log(envref, root, layout, &line).await.map_err(|e| {
        Error::general_error(format!(
            "'{rel}' was written (version {version_after}), but its log line is missing: {}",
            e.message
        ))
    })?;
    Ok(outcome)
}

/// `- 2026-10-17T15:20:58Z write specs/issues/X.md 1a2b3c4d→5e6f7a8b — reason`.
pub fn log_line(op: &str, rel: &str, before: Option<Version>, after: Version, reason: &str) -> String {
    let short = |v: Version| v.to_string()[..8].to_string();
    let before = before.map(short).unwrap_or_else(|| "∅".to_string());
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
    let reason = reason.replace('\n', " ");
    let mut line = format!("- {now} {op} {rel} {before}→{}", short(after));
    if !reason.trim().is_empty() {
        line.push_str(&format!(" — {}", reason.trim()));
    }
    line
}

/// The log file a line written now goes to: `<root>/<log>/YYYY-MM.md`.
pub fn log_key(root: &Key, layout: &MemoryLayout) -> Result<Key, Error> {
    let month = chrono::Utc::now().format("%Y-%m").to_string();
    Ok(join_path(root, &layout.log)?.join(format!("{month}.md")))
}

/// Appends one line to this month's log, without schema validation.
pub async fn append_log<E: Environment>(
    envref: &EnvRef<E>,
    root: &Key,
    layout: &MemoryLayout,
    line: &str,
) -> Result<(), Error> {
    let key = log_key(root, layout)?;
    let mut text = match read_bytes(envref, &key).await? {
        Some(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        None => String::new(),
    };
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(line);
    text.push('\n');
    let mut record = record_for(&key, "");
    record.title = format!("Memory log {}", key.filename().map(|n| n.name.trim_end_matches(".md").to_string()).unwrap_or_default());
    envref.get_asset_manager().set_binary(&key, text.as_bytes(), record).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment::DefaultEnvironment;
    use crate::value::Value;
    use liquers_core::error::ErrorType;
    use liquers_core::metadata::Metadata;
    use liquers_core::parse::parse_key;
    use liquers_core::store::{AsyncMemoryStore, AsyncStore, AsyncStoreRouter, ReadOnlyStore};
    use std::sync::Arc;

    const SCHEMA: &str = "collections:\n  - name: issues\n    folder: issues\n    history: { section: History, stamp: reviewed }\n    fields:\n      id: { required: true, equals: filename_stem }\n      title: { required: true }\n      status: { enum: [draft, in_progress, closed] }\n";
    const DOC: &str = "---\nid: X\ntitle: An issue\ndescription: What breaks\nstatus: draft\n---\n# X\n\nOld text.\n\n## History\n\n| Date | Change |\n|---|---|\n| 2026-01-01 | Filed |\n\n## After\n";

    async fn put(store: &dyn AsyncStore, key: &str, text: &str) -> Result<(), Error> {
        let mut metadata = Metadata::new();
        metadata.set_status(Status::Source)?;
        store.set(&parse_key(key)?, text.as_bytes(), &metadata).await
    }

    async fn env(read_only_log: bool) -> Result<EnvRef<DefaultEnvironment<Value>>, Error> {
        let main = AsyncMemoryStore::new(&Key::new());
        put(&main, "_schema/memory.yaml", SCHEMA).await?;
        put(&main, "issues/X.md", DOC).await?;
        let mut env = DefaultEnvironment::<Value>::new();
        if read_only_log {
            let mut router = AsyncStoreRouter::new();
            let log = AsyncMemoryStore::new(&parse_key("_log")?);
            router.add_store(Box::new(ReadOnlyStore::new(Arc::new(log))));
            router.add_store(Box::new(main));
            env.with_async_store(Box::new(router));
        } else {
            env.with_async_store(Box::new(main));
        }
        Ok(env.to_ref())
    }

    async fn text_of(envref: &EnvRef<DefaultEnvironment<Value>>, key: &str) -> Result<String, Error> {
        Ok(String::from_utf8_lossy(&envref.get_async_store().get_bytes(&parse_key(key)?).await?).into_owned())
    }

    async fn log_lines(envref: &EnvRef<DefaultEnvironment<Value>>, layout: &MemoryLayout) -> Result<usize, Error> {
        let key = log_key(&Key::new(), layout)?;
        Ok(match read_bytes(envref, &key).await? {
            Some(bytes) => String::from_utf8_lossy(&bytes).lines().count(),
            None => 0,
        })
    }

    #[tokio::test]
    async fn edit_applies_every_pair_once() -> Result<(), Error> {
        let envref = env(false).await?;
        let layout = MemoryLayout::parse(SCHEMA)?;
        let key = parse_key("issues/X.md")?;
        let before = Version::from_content(DOC.as_bytes());
        let ops = edit_ops("Old text.", "New text.", "status", "in_progress", "History", "| 2026-10-19 | Edited |")?;
        let outcome = commit(&envref, &Key::new(), &layout, WriteRequest {
            key: key.clone(),
            op: WriteOp::Edit(ops),
            expected: Some(before),
            reason: "test".to_string(),
        })
        .await?;
        let text = text_of(&envref, "issues/X.md").await?;
        assert_eq!(outcome.version_before, Some(before));
        assert_eq!(outcome.version_after, Version::from_content(text.as_bytes()));
        assert_eq!(text.matches("New text.").count(), 1);
        assert!(text.contains("status: in_progress\n"));
        assert!(text.contains("| 2026-01-01 | Filed |\n| 2026-10-19 | Edited |\n\n## After"), "{text}");
        assert!(text.contains(&format!("reviewed: {}\n", today())));
        assert!(text.contains(&format!("updated: {}\n", today())));
        assert_eq!(log_lines(&envref, &layout).await?, 1);
        Ok(())
    }

    #[test]
    fn half_given_pair_is_refused() {
        for (args, missing) in [
            (("", "", "status", "", "", ""), "value"),
            (("", "x", "", "", "", ""), "old"),
            (("", "", "", "", "History", ""), "row"),
            (("", "", "", "", "", ""), "at least one pair"),
        ] {
            let (o, n, f, v, s, r) = args;
            let error = edit_ops(o, n, f, v, s, r).err();
            assert!(error.is_some_and(|e| e.message.contains(missing)), "{missing}");
        }
        assert_eq!(edit_ops("gone", "", "", "", "", "").ok().map(|ops| ops.len()), Some(1));
    }

    #[tokio::test]
    async fn failed_replace_writes_nothing() -> Result<(), Error> {
        let envref = env(false).await?;
        let layout = MemoryLayout::parse(SCHEMA)?;
        let ops = edit_ops("X", "Y", "status", "closed", "", "")?;
        let result = commit(&envref, &Key::new(), &layout, WriteRequest {
            key: parse_key("issues/X.md")?,
            op: WriteOp::Edit(ops),
            expected: None,
            reason: String::new(),
        })
        .await;
        assert!(result.err().is_some_and(|e| e.message.contains("found") && e.message.contains("times")));
        assert_eq!(text_of(&envref, "issues/X.md").await?, DOC);
        assert_eq!(log_lines(&envref, &layout).await?, 0);
        Ok(())
    }

    #[tokio::test]
    async fn log_failure_keeps_the_write() -> Result<(), Error> {
        let envref = env(true).await?;
        let layout = MemoryLayout::parse(SCHEMA)?;
        let ops = edit_ops("", "", "status", "closed", "", "")?;
        let error = commit(&envref, &Key::new(), &layout, WriteRequest {
            key: parse_key("issues/X.md")?,
            op: WriteOp::Edit(ops),
            expected: None,
            reason: String::new(),
        })
        .await
        .err()
        .ok_or_else(|| Error::general_error("the log write into a read-only mount succeeded".to_string()))?;
        assert!(error.message.contains("log line is missing"), "{}", error.message);
        assert!(text_of(&envref, "issues/X.md").await?.contains("status: closed"));
        Ok(())
    }

    #[tokio::test]
    async fn malformed_expected_version_is_a_parse_error() -> Result<(), Error> {
        let error = super::super::front_matter::parse_version("3f9c").err();
        assert!(error.is_some_and(|e| e.error_type == ErrorType::ParseError));
        Ok(())
    }

    #[tokio::test]
    async fn stale_version_and_schema_breaks_are_refused() -> Result<(), Error> {
        let envref = env(false).await?;
        let layout = MemoryLayout::parse(SCHEMA)?;
        let key = parse_key("issues/X.md")?;
        let stale = commit(&envref, &Key::new(), &layout, WriteRequest {
            key: key.clone(),
            op: WriteOp::Edit(edit_ops("", "", "status", "closed", "", "")?),
            expected: Some(Version::from_content(b"other")),
            reason: String::new(),
        })
        .await;
        assert!(stale.err().is_some_and(|e| e.error_type == ErrorType::DependencyVersionMismatch));
        let bad = commit(&envref, &Key::new(), &layout, WriteRequest {
            key,
            op: WriteOp::Edit(edit_ops("", "", "status", "open", "", "")?),
            expected: None,
            reason: String::new(),
        })
        .await;
        assert!(bad.err().is_some_and(|e| e.message.contains("'open' is not one of")));
        assert_eq!(text_of(&envref, "issues/X.md").await?, DOC);
        Ok(())
    }
}
