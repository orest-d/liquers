//! `-mem-check`: the schema rules over every memory under a folder, the rules that span
//! documents (duplicate ids, links, supersession, the write log), and the named built-in rules a
//! collection lists in `checks`.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use liquers_core::context::{Context, Environment};
use liquers_core::error::Error;
use liquers_core::query::Key;

use crate::records::{
    FieldRole, FieldSchema, FieldType, FieldValue, RecordBatchMut, RecordSchema, RecordView,
    RecordViewMut,
};
use crate::value::Value;

use super::front_matter::{value_items, FrontMatterDoc};
use super::layout::{join_path, read_bytes, relative_path, CollectionSchema, MemoryLayout};
use super::named_rules::{self, DesignRow};
use super::read::{log_entries, members, version_of, walk, Entry};
use super::rules::{document_violations, is_date, Severity, Violation, Vocabularies};

/// Rules of `docs_index.py --check` that `check` does not implement, with the reason.
pub const UNSUPPORTED: [&str; 4] = [
    "index.csv is stale (generated files belong to agent-memory-derived)",
    "index.md is stale (generated files belong to agent-memory-derived)",
    "dead links to files outside the memory's folders (source code) are not checked",
    "ids are unique per collection, not across collections",
];

/// The named built-in rules a collection may list in `checks`.
pub const NAMED_RULES: [&str; 7] = [
    "reviewed",
    "history",
    "dead_links",
    "lifecycle",
    "readiness",
    "compact_form",
    "scenarios",
];

/// One finding, with the key it is about (relative to the root).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Finding {
    pub key: String,
    pub violation: Violation,
}

/// What a check found.
#[derive(Clone, Debug, Default)]
pub struct CheckReport {
    pub findings: Vec<Finding>,
    pub errors: usize,
    pub warnings: usize,
    pub unsupported: Vec<String>,
}

/// Every finding for the memories under `folder`.
pub async fn check_memory<E: Environment<Value = Value>>(
    layout: &MemoryLayout,
    root: &Key,
    folder: &Key,
    context: &Context<E>,
) -> Result<CheckReport, Error> {
    let envref = context.get_envref();
    let mut all: BTreeMap<String, Vec<Entry>> = BTreeMap::new();
    for collection in &layout.collections {
        all.insert(collection.name.clone(), members(root, collection, context).await?);
    }
    let log = log_versions(&envref, root, layout).await?;
    let in_scope = link_scope(layout, root)?;
    let mut findings = Vec::new();
    let mut unsupported: Vec<String> = UNSUPPORTED.iter().map(|s| s.to_string()).collect();
    for collection in &layout.collections {
        let docs = &all[&collection.name];
        let vocabularies = Vocabularies::load(&envref, root, collection).await?;
        for name in &collection.checks {
            if !NAMED_RULES.contains(&name.as_str()) {
                let note = format!("{name} (collection {})", collection.name);
                if !unsupported.contains(&note) {
                    unsupported.push(note);
                }
            }
        }
        let mut ids: BTreeMap<String, String> = BTreeMap::new();
        for doc in docs.iter().filter(|d| d.key.has_key_prefix(folder)) {
            let rel = relative_path(root, &doc.key);
            // Findings about other files (a design's phase files), added after this document's.
            let mut others: Vec<Finding> = Vec::new();
            let mut push = |violation: Violation| findings.push(Finding { key: rel.clone(), violation });
            let Some(text) = doc.text.as_deref() else {
                // A recipe-only key has no document to check; a stored one that is not text does.
                if super::read::is_stored(doc.info.status) {
                    push(Violation::error("readable", "not readable as UTF-8 text".to_string()));
                }
                continue;
            };
            for violation in document_violations(collection, &vocabularies, &doc.key, text) {
                push(violation);
            }
            let fm = FrontMatterDoc::parse(text).ok();
            let field = |name: &str| fm.as_ref().map(|d| d.get_text(name)).unwrap_or_default();
            let id = {
                let given = field("id");
                if given.is_empty() { collection.id_of(&doc.key) } else { given }
            };
            if let Some(first) = ids.get(&id) {
                push(Violation::error("duplicate_id", format!("duplicate id {id} (also {first})")));
            } else {
                ids.insert(id.clone(), rel.clone());
            }
            for (name, rule) in &collection.fields {
                let Some(target) = &rule.link else { continue };
                let values = value_items(fm.as_ref().and_then(|d| d.get(name)).as_ref());
                let targets = all.get(target).map(Vec::as_slice).unwrap_or(&[]);
                let target_schema = layout.collections.iter().find(|c| &c.name == target);
                for value in values {
                    match target_schema.and_then(|t| find(targets, t, root, &value)) {
                        None => push(Violation::error(
                            "link",
                            format!("{name} '{value}' does not exist"),
                        )),
                        Some(linked) => {
                            if let Some(back) = &rule.reciprocal {
                                let back_values = value_items(linked.doc().and_then(|d| d.get(back)).as_ref());
                                if !back_values.iter().any(|v| *v == id || *v == rel) {
                                    push(Violation::error(
                                        "reciprocal",
                                        format!("{name} '{value}' does not list this memory in {back}"),
                                    ));
                                }
                            }
                        }
                    }
                }
            }
            let superseded_by = field("superseded_by");
            if !superseded_by.is_empty() && !exists_any(&all, layout, root, &superseded_by) {
                push(Violation::error(
                    "superseded_by",
                    format!("superseded_by '{superseded_by}' does not exist"),
                ));
            }
            let supersedes = field("supersedes");
            if !supersedes.is_empty() {
                match find_any(&all, layout, root, &supersedes) {
                    None => push(Violation::error(
                        "supersede_reciprocal",
                        format!("supersedes '{supersedes}', which does not exist"),
                    )),
                    Some(old) => {
                        let back = old.field("superseded_by");
                        if back != rel && back != id {
                            push(Violation::error(
                                "supersede_reciprocal",
                                format!(
                                    "supersedes '{supersedes}', but its superseded_by is '{back}': a \
                                     partial supersede"
                                ),
                            ));
                        }
                    }
                }
            }
            if let Some(logged) = log.get(&rel) {
                let current = version_of(text).to_string();
                if !current.starts_with(logged.as_str()) {
                    push(Violation::warning(
                        "log_behind",
                        format!(
                            "changed since its last logged write (log {logged}…, now {}…): edited \
                             outside the service, or a log line is missing",
                            &current[..8]
                        ),
                    ));
                }
            }
            for name in &collection.checks {
                match name.as_str() {
                    "reviewed" => {
                        let stamp = collection.history.as_ref().map(|h| h.stamp.as_str()).unwrap_or("reviewed");
                        if field(stamp).trim().is_empty() {
                            push(Violation::error("reviewed", format!("no {stamp}: date (§9.2)")));
                        }
                    }
                    "history" => {
                        let (section, stamp) = match &collection.history {
                            Some(h) => (h.section.as_str(), h.stamp.as_str()),
                            None => ("History", "reviewed"),
                        };
                        let heading = format!("## {section}");
                        match text.split_once(heading.as_str()) {
                            None => push(Violation::error("history", format!("no {heading} section (§9.5)"))),
                            Some((_, after)) => {
                                let date = field(stamp);
                                if !date.is_empty() && !after.contains(date.as_str()) {
                                    push(Violation::error(
                                        "history",
                                        format!("{section} has no row dated {date} (§9.5)"),
                                    ));
                                }
                            }
                        }
                    }
                    "dead_links" => {
                        for target in dead_links(&envref, &doc.key, text, &in_scope).await? {
                            push(Violation::error("dead_links", format!("dead link {target} (§8.4)")));
                        }
                        if collection.entry.is_some() {
                            for (other, target) in folder_dead_links(&doc.key, &in_scope, context).await? {
                                others.push(Finding {
                                    key: relative_path(root, &other),
                                    violation: Violation::error("dead_links", format!("dead link {target} (§8.4)")),
                                });
                            }
                        }
                    }
                    "lifecycle" => {
                        for violation in named_rules::lifecycle(doc) {
                            push(violation);
                        }
                    }
                    "compact_form" => {
                        for violation in named_rules::compact_form(doc) {
                            push(violation);
                        }
                    }
                    "scenarios" => {
                        if fm.as_ref().is_some_and(|d| d.has_front_matter()) {
                            let phase1 = design_phase(&envref, doc, 1).await?;
                            let phase3 = design_phase(&envref, doc, 3).await?;
                            for violation in named_rules::scenarios(doc, phase1.as_deref(), phase3.as_deref()) {
                                push(violation);
                            }
                        }
                    }
                    _ => {}
                }
            }
            findings.extend(others);
        }
        if collection.checks.iter().any(|c| c == "readiness") {
            let rows: Vec<DesignRow<'_>> = docs
                .iter()
                .filter(|d| d.key.has_key_prefix(folder) && d.text.is_some())
                .map(|d| DesignRow {
                    rel: relative_path(root, &d.key),
                    slug: collection.id_of(&d.key),
                    entry: d,
                })
                .collect();
            let issue = |id: &str| find_any(&all, layout, root, id).cloned();
            for (rel, violation) in named_rules::readiness(&rows, &issue) {
                findings.push(Finding { key: rel, violation });
            }
        }
    }
    findings.sort();
    let errors = findings.iter().filter(|f| f.violation.severity == Severity::Error).count();
    let warnings = findings.len() - errors;
    Ok(CheckReport { findings, errors, warnings, unsupported })
}

/// A memory of `collection` named by `value`: its id, or its key relative to the root.
fn find<'a>(docs: &'a [Entry], collection: &CollectionSchema, root: &Key, value: &str) -> Option<&'a Entry> {
    docs.iter().find(|d| {
        let id = {
            let given = d.field("id");
            if given.is_empty() { collection.id_of(&d.key) } else { given }
        };
        id == value || collection.id_of(&d.key) == value || relative_path(root, &d.key) == value
    })
}

fn find_any<'a>(
    all: &'a BTreeMap<String, Vec<Entry>>,
    layout: &MemoryLayout,
    root: &Key,
    value: &str,
) -> Option<&'a Entry> {
    layout
        .collections
        .iter()
        .find_map(|c| all.get(&c.name).and_then(|docs| find(docs, c, root, value)))
}

fn exists_any(all: &BTreeMap<String, Vec<Entry>>, layout: &MemoryLayout, root: &Key, value: &str) -> bool {
    find_any(all, layout, root, value).is_some()
}

/// The version prefix of each memory's last logged write, by key relative to the root.
async fn log_versions<E: Environment>(
    envref: &liquers_core::context::EnvRef<E>,
    root: &Key,
    layout: &MemoryLayout,
) -> Result<BTreeMap<String, String>, Error> {
    let log = join_path(root, &layout.log)?;
    let store = envref.get_async_store();
    let mut out = BTreeMap::new();
    if !store.is_dir(&log).await? {
        return Ok(out);
    }
    let mut names: Vec<String> = store.listdir(&log).await?.into_iter().filter(|n| n.ends_with(".md")).collect();
    names.sort();
    for name in names {
        let Some(bytes) = read_bytes(envref, &log.join(&name)).await? else { continue };
        for line in log_entries(&String::from_utf8_lossy(&bytes)) {
            // - <time> <op> <key> <before>→<after> — reason
            let parts: Vec<&str> = line.splitn(5, ' ').collect();
            if parts.len() >= 5 {
                let key = parts[3].to_string();
                let versions = parts[4].split(' ').next().unwrap_or("");
                if let Some((_, after)) = versions.split_once('→') {
                    out.insert(key, after.to_string());
                }
            }
        }
    }
    Ok(out)
}

/// Relative links in `text` whose target does not exist in the store. Code is skipped; a link
/// to an absolute path, a URL or an anchor alone is not checked.
/// Which link targets `dead_links` checks: keys in the top-level folders of the collections (a
/// hidden top-level folder, such as `.claude`, only within the collection's own folder). A link
/// out of them, to source code for instance, points at files the memory does not mount.
fn link_scope(
    layout: &MemoryLayout,
    root: &Key,
) -> Result<impl Fn(&Key) -> bool + Send + Sync, Error> {
    let mut tops: Vec<Key> = Vec::new();
    for collection in &layout.collections {
        let folder = join_path(root, &collection.folder)?;
        let top = match folder.iter().nth(root.len()) {
            Some(first) if first.name.starts_with('.') => folder,
            Some(first) => root.join(&first.name),
            None => folder,
        };
        if !tops.contains(&top) {
            tops.push(top);
        }
    }
    Ok(move |key: &Key| tops.iter().any(|top| key.has_key_prefix(top)))
}

async fn dead_links<E: Environment>(
    envref: &liquers_core::context::EnvRef<E>,
    key: &Key,
    text: &str,
    in_scope: &(dyn Fn(&Key) -> bool + Send + Sync),
) -> Result<Vec<String>, Error> {
    let store = envref.get_async_store();
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    let mut in_fence = false;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        let line = strip_inline_code(line);
        let mut rest = line.as_str();
        while let Some(i) = rest.find("](") {
            let after = &rest[i + 2..];
            let Some(close) = after.find(')') else { break };
            let raw = &after[..close];
            rest = &after[close..];
            if raw.starts_with("http://") || raw.starts_with("https://") {
                continue;
            }
            let target = raw.split('#').next().unwrap_or("");
            if target.is_empty() || target.starts_with('/') || target.contains("://") || target.starts_with("mailto:") {
                continue;
            }
            if !seen.insert(raw.to_string()) {
                continue;
            }
            let mut resolved = key.parent();
            let mut outside = false;
            for part in target.split('/') {
                match part {
                    "" | "." => {}
                    ".." => {
                        if resolved.is_empty() {
                            outside = true;
                        }
                        resolved = resolved.parent();
                    }
                    name => resolved = resolved.join(name),
                }
            }
            if !outside && !in_scope(&resolved) {
                continue;
            }
            if outside || !(store.contains(&resolved).await? || store.is_dir(&resolved).await?) {
                out.push(raw.to_string());
            }
        }
    }
    Ok(out)
}

/// Dead links in the other Markdown files of an `entry` memory's folder (a design's phase files).
async fn folder_dead_links<E: Environment<Value = Value>>(
    entry: &Key,
    in_scope: &(dyn Fn(&Key) -> bool + Send + Sync),
    context: &Context<E>,
) -> Result<Vec<(Key, String)>, Error> {
    let envref = context.get_envref();
    let mut out = Vec::new();
    for key in walk(&entry.parent(), 0, context).await? {
        let is_md = key.filename().is_some_and(|n| n.name.ends_with(".md"));
        if key == *entry || !is_md {
            continue;
        }
        let Some(bytes) = read_bytes(&envref, &key).await? else { continue };
        let Ok(text) = String::from_utf8(bytes) else { continue };
        for target in dead_links(&envref, &key, &text, in_scope).await? {
            out.push((key.clone(), target));
        }
    }
    Ok(out)
}

/// The text of one phase of a design: its `## Phase N` section in a compact design, or every
/// `phase<N>-*.md` file beside its DESIGN.md, joined.
async fn design_phase<E: Environment>(
    envref: &liquers_core::context::EnvRef<E>,
    doc: &Entry,
    phase: usize,
) -> Result<Option<String>, Error> {
    if doc.field("form") == "compact" {
        return Ok(doc.text.as_deref().and_then(|t| named_rules::compact_phase(t, phase)));
    }
    let folder = doc.key.parent();
    let prefix = format!("phase{phase}-");
    let mut names: Vec<String> = envref
        .get_async_store()
        .listdir(&folder)
        .await?
        .into_iter()
        .filter(|n| n.starts_with(&prefix) && n.ends_with(".md"))
        .collect();
    names.sort_by_key(|n| (n.to_lowercase(), n.clone()));
    if names.is_empty() {
        return Ok(None);
    }
    let mut parts = Vec::new();
    for name in names {
        if let Some(bytes) = read_bytes(envref, &folder.join(&name)).await? {
            parts.push(String::from_utf8_lossy(&bytes).into_owned());
        }
    }
    Ok(Some(parts.join("\n")))
}

fn strip_inline_code(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut in_code = false;
    for c in line.chars() {
        if c == '`' {
            in_code = !in_code;
            out.push(' ');
        } else if in_code {
            out.push(' ');
        } else {
            out.push(c);
        }
    }
    out
}

/// The report as a record view `rule, key, severity, message`.
pub async fn check_view<E: Environment<Value = Value>>(
    layout: &MemoryLayout,
    root: &Key,
    folder: &Key,
    context: &Context<E>,
) -> Result<(Arc<dyn RecordView>, CheckReport), Error> {
    let report = check_memory(layout, root, folder, context).await?;
    let schema = Arc::new(RecordSchema::new(vec![
        FieldSchema::new("rule", FieldType::Text).with_role(FieldRole::keyword()),
        FieldSchema::new("key", FieldType::Text).with_role(FieldRole::keyword()),
        FieldSchema::new("severity", FieldType::Text).with_role(FieldRole::keyword()),
        FieldSchema::new("message", FieldType::Text).with_role(FieldRole::text().and_stored()),
    ])?);
    let mut batch = RecordBatchMut::with_capacity(schema, report.findings.len());
    let text = |s: &str| FieldValue::Text(Arc::from(s));
    for finding in &report.findings {
        batch.append_row(&[
            text(&finding.violation.rule),
            text(&finding.key),
            text(finding.violation.severity.as_str()),
            text(&finding.violation.message),
        ])?;
    }
    Ok((Arc::new(batch.freeze()?) as Arc<dyn RecordView>, report))
}
