//! The reading side of the memory: listing, searching, orientation and reading one document.
//! Tables are record views; documents and orientation are Markdown text.

use std::collections::BTreeMap;
use std::sync::Arc;

use liquers_core::context::{Context, Environment};
use liquers_core::error::Error;
use liquers_core::metadata::{AssetInfo, Status, Version};
use liquers_core::query::{Key, Position};

use crate::records::search::{search_view, Node, Predicate, SearchExpr, SearchOptions, SearchReport};
use crate::records::{
    FieldRole, FieldSchema, FieldType, FieldValue, KeyRole, RecordBatchMut, RecordSchema,
    RecordView, RecordViewMut,
};
use crate::search::catalog::infos;
use crate::search::md_split::{blank_front_matter, find_section, sections};
use crate::search::{extension_of, read_text};
use crate::value::Value;

use super::front_matter::{value_text, FrontMatterDoc};
use super::layout::{
    join_path, read_bytes, relative_path, CollectionSchema, MemoryLayout, TierSource,
    TierSources,
};
use super::tiers;
use super::write::log_key;

/// Names never listed: the schema, the log, sidecars and hidden files.
pub(crate) fn is_hidden(name: &str) -> bool {
    name.starts_with('_') || name.starts_with('.')
}

fn text(value: &str) -> FieldValue {
    FieldValue::Text(Arc::from(value))
}

fn optional_text(value: &str) -> FieldValue {
    if value.is_empty() { FieldValue::Null } else { text(value) }
}

/// One listed entry, with its document text when it is a stored Markdown file.
#[derive(Clone, Debug)]
pub(crate) struct Entry {
    pub key: Key,
    pub info: AssetInfo,
    pub text: Option<String>,
}

impl Entry {
    pub fn doc(&self) -> Option<FrontMatterDoc> {
        self.text.as_deref().and_then(|t| FrontMatterDoc::parse(t).ok())
    }

    pub fn field(&self, name: &str) -> String {
        self.doc().map(|d| d.get_text(name)).unwrap_or_default()
    }

    pub fn is_superseded(&self) -> bool {
        !self.field("superseded_by").trim().is_empty()
    }
}

fn is_markdown_name(key: &Key) -> bool {
    key.filename()
        .and_then(|n| extension_of(&n.name).map(|e| e.eq_ignore_ascii_case("md")))
        .unwrap_or(false)
}

/// Keys under `folder`, breadth first, `depth` levels deep (0: every level). Hidden names are
/// skipped, and nothing is evaluated: a recipe-only key is listed with status `Recipe`.
pub(crate) async fn walk<E: Environment<Value = Value>>(
    folder: &Key,
    depth: usize,
    context: &Context<E>,
) -> Result<Vec<Key>, Error> {
    let store = context.get_envref().get_async_store();
    let mut keys = Vec::new();
    let mut pending = vec![(folder.clone(), 1usize)];
    while let Some((dir, level)) = pending.pop() {
        let mut names = context.record_listing(&dir).await?;
        names.sort();
        for name in names.into_iter().rev() {
            if is_hidden(&name) {
                continue;
            }
            let key = dir.join(&name);
            if (depth == 0 || level < depth) && store.is_dir(&key).await? {
                pending.push((key.clone(), level + 1));
            }
            keys.push(key);
        }
    }
    keys.sort_by_key(|k| k.encode());
    Ok(keys)
}

/// The entries of `keys`, each Markdown document with its text.
pub(crate) async fn entries<E: Environment<Value = Value>>(
    keys: Vec<Key>,
    context: &Context<E>,
) -> Result<Vec<Entry>, Error> {
    let mut out = Vec::new();
    for (key, info, _) in infos(keys, context).await? {
        let text = if !info.is_dir && is_markdown_name(&key) {
            read_text(&key, &info, 0, context).await
        } else {
            None
        };
        out.push(Entry { key, info, text });
    }
    Ok(out)
}

/// The memories of a collection.
pub(crate) async fn members<E: Environment<Value = Value>>(
    root: &Key,
    collection: &CollectionSchema,
    context: &Context<E>,
) -> Result<Vec<Entry>, Error> {
    let folder = join_path(root, &collection.folder)?;
    let store = context.get_envref().get_async_store();
    if !store.is_dir(&folder).await? {
        return Ok(Vec::new());
    }
    let depth = match (collection.recursive, &collection.entry) {
        (true, _) => 0,
        (false, Some(_)) => 2,
        (false, None) => 1,
    };
    let keys = walk(&folder, depth, context)
        .await?
        .into_iter()
        .filter(|key| collection.is_memory(root, key))
        .collect();
    entries(keys, context).await
}

/// `(l0, l1)` of a document under the bridge that applies to it.
pub(crate) fn tiers_of(layout: &MemoryLayout, root: &Key, entry: &Entry) -> (String, String) {
    let collection = layout.collection_for(root, &entry.key);
    let sources = layout.tiers_for(collection);
    let meta = |name: &str| match name {
        "title" => Some(entry.info.title.clone()),
        "description" => Some(entry.info.description.clone()),
        _ => None,
    };
    tiers::resolve(sources, &meta, entry.text.as_deref())
}

/// A folder's tiers: its README's, else none.
async fn folder_tiers<E: Environment<Value = Value>>(
    layout: &MemoryLayout,
    root: &Key,
    folder: &Key,
    context: &Context<E>,
) -> Result<(String, String), Error> {
    let readme_name = layout.readme.rsplit('/').next().unwrap_or("README.md").to_string();
    let readme = folder.join(&readme_name);
    let am = context.get_envref().get_asset_manager();
    match liquers_core::assets::AssetManager::get_asset_info(am.as_ref(), &readme).await {
        Ok(info) => {
            let text = read_text(&readme, &info, 0, context).await;
            let entry = Entry { key: readme, info, text };
            let (l0, l1) = tiers_of(layout, root, &entry);
            Ok((l0, l1))
        }
        Err(_) => Ok((String::new(), String::new())),
    }
}

fn list_schema() -> Result<Arc<RecordSchema>, Error> {
    let tier = || FieldRole::text().and_stored();
    Ok(Arc::new(RecordSchema::new(vec![
        FieldSchema::new("key", FieldType::Text).with_key(KeyRole::Id),
        FieldSchema::new("l0", FieldType::Text).with_role(tier()),
        FieldSchema::new("l1", FieldType::Text).with_role(tier()),
        FieldSchema::new("kind", FieldType::Text).with_role(FieldRole::keyword()),
        FieldSchema::new("status", FieldType::Text).with_role(FieldRole::keyword()),
        FieldSchema::new("asset_status", FieldType::Text).with_role(FieldRole::keyword()),
        FieldSchema::new("is_dir", FieldType::Bool).with_role(FieldRole::keyword()),
    ])?))
}

/// `-mem-list`: the entries under `folder` with their tiers.
pub(crate) async fn list_view<E: Environment<Value = Value>>(
    layout: &MemoryLayout,
    root: &Key,
    folder: &Key,
    depth: usize,
    include_superseded: bool,
    limit: usize,
    context: &Context<E>,
) -> Result<(Arc<dyn RecordView>, usize, usize), Error> {
    let keys = walk(folder, depth, context).await?;
    let mut rows: Vec<Vec<FieldValue>> = Vec::new();
    for entry in entries(keys, context).await? {
        if !include_superseded && entry.is_superseded() {
            continue;
        }
        let (l0, l1) = if entry.info.is_dir {
            folder_tiers(layout, root, &entry.key, context).await?
        } else {
            tiers_of(layout, root, &entry)
        };
        let kind = if entry.info.is_dir {
            "folder".to_string()
        } else {
            match layout.collection_for(root, &entry.key) {
                Some(c) if c.is_memory(root, &entry.key) => c.name.clone(),
                Some(_) | None => "file".to_string(),
            }
        };
        rows.push(vec![
            text(&relative_path(root, &entry.key)),
            optional_text(&l0),
            optional_text(&l1),
            text(&kind),
            optional_text(&entry.field("status")),
            text(&format!("{:?}", entry.info.status)),
            FieldValue::Bool(entry.info.is_dir),
        ]);
    }
    let total = rows.len();
    let shown = if limit > 0 { total.min(limit) } else { total };
    let mut batch = RecordBatchMut::with_capacity(list_schema()?, shown);
    for row in rows.into_iter().take(shown) {
        batch.append_row(&row)?;
    }
    Ok((Arc::new(batch.freeze()?) as Arc<dyn RecordView>, shown, total))
}

/// Front-matter fields that do not become search columns: they would shadow the fixed ones.
const RESERVED: [&str; 9] = ["key", "l0", "l1", "heading", "line", "content", "score", "matched", "excerpt"];

/// `-mem-search`: documents (or sections) under `folder` ranked against `text`, restricted by
/// `filter`. Superseded memories are dropped before ranking, so the report never counts them.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn search_memories<E: Environment<Value = Value>>(
    layout: &MemoryLayout,
    root: &Key,
    folder: &Key,
    query: &str,
    by_section: bool,
    filter: &str,
    limit: usize,
    include_superseded: bool,
    context: &Context<E>,
) -> Result<(Arc<dyn RecordView>, SearchReport), Error> {
    let keys = walk(folder, 0, context).await?;
    let docs: Vec<Entry> = entries(keys, context)
        .await?
        .into_iter()
        .filter(|e| e.text.is_some() && (include_superseded || !e.is_superseded()))
        .collect();
    // Front-matter fields become keyword columns, so a filter can name them.
    let mut field_names: Vec<String> = Vec::new();
    let mut fields_of: Vec<BTreeMap<String, String>> = Vec::new();
    for doc in &docs {
        let map: BTreeMap<String, String> = doc
            .doc()
            .map(|d| d.fields_lenient().0)
            .unwrap_or_default()
            .into_iter()
            .filter(|(name, _)| !RESERVED.contains(&name.as_str()))
            .map(|(name, value)| (name, value_text(Some(&value))))
            .collect();
        for name in map.keys() {
            if !field_names.contains(name) {
                field_names.push(name.clone());
            }
        }
        fields_of.push(map);
    }
    let tier = || FieldRole::text().and_stored();
    let mut schema_fields = vec![
        FieldSchema::new("key", FieldType::Text).with_key(KeyRole::Id),
        FieldSchema::new("l0", FieldType::Text).with_role(tier()),
        FieldSchema::new("l1", FieldType::Text).with_role(tier()),
        FieldSchema::new("heading", FieldType::Text).with_role(FieldRole::keyword()),
        FieldSchema::new("line", FieldType::Int).with_role(FieldRole::numeric()),
        FieldSchema::new("content", FieldType::Text).with_role(FieldRole::text()),
    ];
    for name in &field_names {
        schema_fields.push(FieldSchema::new(name, FieldType::Text).with_role(FieldRole::keyword()));
    }
    let schema = Arc::new(RecordSchema::new(schema_fields)?);
    let mut batch = RecordBatchMut::with_capacity(schema, docs.len());
    for (doc, fields) in docs.iter().zip(fields_of.iter()) {
        let (l0, l1) = tiers_of(layout, root, doc);
        let key = relative_path(root, &doc.key);
        let full = doc.text.clone().unwrap_or_default();
        let parts: Vec<(String, usize, String)> = if by_section {
            let blanked = blank_front_matter(&full);
            let lines: Vec<&str> = full.lines().collect();
            // Each section's own text: up to the next heading of any level.
            let found = sections(&blanked);
            let starts: Vec<usize> = found.iter().map(|(_, _, start, _)| *start).collect();
            found
                .into_iter()
                .enumerate()
                .map(|(i, (heading, _, start, end))| {
                    let own_end = starts.get(i + 1).map(|next| next - 1).unwrap_or(end).min(end);
                    let body = lines[start.saturating_sub(1)..own_end.min(lines.len())].join("\n");
                    (heading.unwrap_or_default(), start, body)
                })
                .filter(|(_, _, body)| !body.trim().is_empty())
                .collect()
        } else {
            let body = FrontMatterDoc::parse(&full).map(|d| d.body().to_string()).unwrap_or(full);
            vec![(String::new(), 1, body)]
        };
        for (heading, line, body) in parts {
            let mut row = vec![
                text(&key),
                optional_text(&l0),
                optional_text(&l1),
                optional_text(&heading),
                FieldValue::Int(line as i64),
                text(&body),
            ];
            for name in &field_names {
                row.push(optional_text(fields.get(name).map(String::as_str).unwrap_or("")));
            }
            batch.append_row(&row)?;
        }
    }
    let view: Arc<dyn RecordView> = Arc::new(batch.freeze()?);
    let mut parts = Vec::new();
    if !query.trim().is_empty() {
        let (expr, error) = SearchExpr::parse_lenient(query);
        if let Some(error) = error {
            context.warning(&format!("{error}; searching the words of the text instead"))?;
        }
        parts.push(expr);
    }
    if !filter.trim().is_empty() {
        parts.push(SearchExpr::try_from(filter)?);
    }
    let expr = SearchExpr {
        node: if parts.is_empty() { Node::Always } else { Node::All(parts) },
        position: Position::unknown(),
    };
    let options = SearchOptions {
        case_sensitive: false,
        fields: vec!["l0".to_string(), "l1".to_string(), "content".to_string()],
        limit: (limit > 0).then_some(limit),
    };
    let (ranked, report) = search_view(&view, &expr as &dyn Predicate, &options, true)?;
    Ok((project(&ranked, &["key", "l0", "l1", "excerpt", "score", "matched", "heading", "line"])?, report))
}

/// The named columns of `view`, in that order; a name it lacks is skipped.
pub(crate) fn project(view: &Arc<dyn RecordView>, names: &[&str]) -> Result<Arc<dyn RecordView>, Error> {
    let schema = view.schema();
    let picked: Vec<usize> = names.iter().filter_map(|n| schema.index_of(n)).collect();
    let fields: Vec<FieldSchema> = picked.iter().map(|&i| schema.fields[i].clone()).collect();
    let mut batch = RecordBatchMut::with_capacity(Arc::new(RecordSchema::new(fields)?), view.len());
    for row in 0..view.len() {
        let values = picked.iter().map(|&col| view.value(row, col)).collect::<Result<Vec<_>, _>>()?;
        batch.append_row(&values)?;
    }
    Ok(Arc::new(batch.freeze()?))
}

/// Words of a title worth matching: three letters or more, not a common word.
pub(crate) fn significant_words(title: &str) -> Vec<String> {
    const COMMON: [&str; 16] = [
        "the", "and", "for", "with", "not", "are", "but", "from", "that", "this", "has", "have",
        "was", "its", "into", "can",
    ];
    let mut out: Vec<String> = Vec::new();
    for word in title.split(|c: char| !c.is_alphanumeric()) {
        let word = word.to_lowercase();
        if word.chars().count() >= 3 && !COMMON.contains(&word.as_str()) && !out.contains(&word) {
            out.push(word);
        }
    }
    out
}

/// `-mem-similar`: memories of a collection that share at least half of the title's
/// significant words, ranked by BM25 over their tiers.
pub(crate) async fn similar_view<E: Environment<Value = Value>>(
    layout: &MemoryLayout,
    root: &Key,
    collection: &CollectionSchema,
    title: &str,
    description: &str,
    limit: usize,
    context: &Context<E>,
) -> Result<Arc<dyn RecordView>, Error> {
    let words = significant_words(title);
    let tier = || FieldRole::text().and_stored();
    let schema = Arc::new(RecordSchema::new(vec![
        FieldSchema::new("key", FieldType::Text).with_key(KeyRole::Id),
        FieldSchema::new("l0", FieldType::Text).with_role(tier()),
        FieldSchema::new("l1", FieldType::Text).with_role(tier()),
        FieldSchema::new("status", FieldType::Text).with_role(FieldRole::keyword()),
    ])?);
    let docs = members(root, collection, context).await?;
    let mut batch = RecordBatchMut::with_capacity(schema.clone(), docs.len());
    for doc in &docs {
        let (l0, l1) = tiers_of(layout, root, doc);
        let haystack = format!("{l0} {l1}").to_lowercase();
        let shared = words.iter().filter(|w| haystack.contains(w.as_str())).count();
        if words.is_empty() || shared * 2 < words.len() {
            continue;
        }
        batch.append_row(&[
            text(&relative_path(root, &doc.key)),
            optional_text(&l0),
            optional_text(&l1),
            optional_text(&doc.field("status")),
        ])?;
    }
    let view: Arc<dyn RecordView> = Arc::new(batch.freeze()?);
    let mut terms: Vec<SearchExpr> = words
        .iter()
        .chain(significant_words(description).iter())
        .map(|w| SearchExpr { node: Node::Term(w.clone()), position: Position::unknown() })
        .collect();
    terms.dedup();
    let expr = SearchExpr {
        node: if terms.is_empty() { Node::Always } else { Node::Any(terms) },
        position: Position::unknown(),
    };
    let options = SearchOptions {
        case_sensitive: false,
        fields: vec!["l0".to_string(), "l1".to_string()],
        limit: (limit > 0).then_some(limit),
    };
    let (ranked, _) = search_view(&view, &expr as &dyn Predicate, &options, true)?;
    project(&ranked, &["key", "l0", "l1", "status", "score"])
}

/// `-mem-overview`: the README's tiers, the collections with their counts and the latest log.
pub(crate) async fn overview_text<E: Environment<Value = Value>>(
    layout: &MemoryLayout,
    root: &Key,
    log_lines: usize,
    context: &Context<E>,
) -> Result<String, Error> {
    let envref = context.get_envref();
    let mut out = String::new();
    let readme = join_path(root, &layout.readme)?;
    let (l0, l1) = match read_bytes(&envref, &readme).await? {
        Some(bytes) => {
            let text = String::from_utf8_lossy(&bytes).into_owned();
            let readme_tiers = TierSources {
                l0: vec![TierSource::Field("l0".into()), TierSource::Field("title".into()), TierSource::Heading],
                l1: vec![TierSource::Field("l1".into()), TierSource::Field("description".into()), TierSource::Paragraph],
            };
            tiers::resolve(&readme_tiers, &|_| None, Some(&text))
        }
        None => (String::new(), String::new()),
    };
    out.push_str(&format!("# {}\n\n", if l0.is_empty() { "Memory" } else { l0.as_str() }));
    if !l1.is_empty() {
        out.push_str(&format!("{l1}\n\n"));
    }
    // The schema's own guidance; the MCP server's instructions carry the overview.
    if !layout.instructions.trim().is_empty() {
        out.push_str(&format!("{}\n\n", layout.instructions.trim()));
    }
    out.push_str("## Collections\n\n| collection | folder | count | writable | description |\n|---|---|---|---|---|\n");
    for collection in &layout.collections {
        let count = members(root, collection, context).await?.len();
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            collection.name,
            cell(&collection.folder),
            count,
            if collection.writable { "yes" } else { "no" },
            cell(&collection.description)
        ));
    }
    if log_lines > 0 {
        out.push_str("\n## Latest writes\n\n");
        let lines = latest_log_lines(&envref, root, layout, log_lines).await?;
        if lines.is_empty() {
            out.push_str("No writes logged yet.\n");
        }
        for line in lines {
            out.push_str(&line);
            out.push('\n');
        }
    }
    Ok(out)
}

/// The last `n` lines of the log, newest month first, oldest line first.
pub(crate) async fn latest_log_lines<E: Environment>(
    envref: &liquers_core::context::EnvRef<E>,
    root: &Key,
    layout: &MemoryLayout,
    n: usize,
) -> Result<Vec<String>, Error> {
    let log = join_path(root, &layout.log)?;
    let store = envref.get_async_store();
    if !store.is_dir(&log).await? {
        // This month's file may still be the only one.
        let key = log_key(root, layout)?;
        return Ok(match read_bytes(envref, &key).await? {
            Some(bytes) => tail(&String::from_utf8_lossy(&bytes), n),
            None => Vec::new(),
        });
    }
    let mut names: Vec<String> = store.listdir(&log).await?.into_iter().filter(|n| n.ends_with(".md")).collect();
    names.sort();
    let mut lines: Vec<String> = Vec::new();
    for name in names.iter().rev() {
        if lines.len() >= n {
            break;
        }
        if let Some(bytes) = read_bytes(envref, &log.join(name)).await? {
            let mut month: Vec<String> = log_entries(&String::from_utf8_lossy(&bytes));
            month.append(&mut lines);
            lines = month;
        }
    }
    let skip = lines.len().saturating_sub(n);
    Ok(lines.into_iter().skip(skip).collect())
}

/// The log's list items.
pub(crate) fn log_entries(text: &str) -> Vec<String> {
    text.lines().filter(|l| l.starts_with("- ")).map(str::to_string).collect()
}

fn tail(text: &str, n: usize) -> Vec<String> {
    let lines = log_entries(text);
    let skip = lines.len().saturating_sub(n);
    lines.into_iter().skip(skip).collect()
}

/// A table cell: pipes escaped, newlines flattened.
fn cell(text: &str) -> String {
    text.replace('|', "\\|").replace('\n', " ")
}

/// `-mem-describe`: what a collection holds and what a write must satisfy.
pub(crate) async fn describe_text<E: Environment<Value = Value>>(
    layout: &MemoryLayout,
    root: &Key,
    only: &str,
    context: &Context<E>,
) -> Result<String, Error> {
    let envref = context.get_envref();
    let collections: Vec<&CollectionSchema> = if only.is_empty() {
        layout.collections.iter().collect()
    } else {
        vec![layout.collection(only)?]
    };
    let mut out = String::from("# Collections\n");
    for c in collections {
        out.push_str(&format!("\n## {}\n\n", c.name));
        if !c.description.is_empty() {
            out.push_str(&format!("{}\n\n", c.description));
        }
        let place = match (&c.file, &c.entry) {
            (Some(file), _) => format!("one document, `{}/{file}`", c.folder),
            (None, Some(entry)) => format!("one folder per memory under `{}/`, document `<id>/{entry}`", c.folder),
            (None, None) => format!("`{}/{}`", c.folder, c.filename),
        };
        out.push_str(&format!("- Location: {place}\n"));
        out.push_str(&format!("- Writable: {}\n", if c.writable { "yes" } else { "no" }));
        out.push_str(&format!("- Id: from {:?}, {:?}{}\n", c.id.from, c.id.case,
            c.id.max_len.map(|m| format!(", at most {m} characters")).unwrap_or_default()));
        if let Some(template) = &c.template {
            out.push_str(&format!("- Template: `{template}`\n"));
        }
        if !c.sections.is_empty() {
            out.push_str(&format!("- Required sections: {}\n", c.sections.join(", ")));
        }
        if let Some(history) = &c.history {
            out.push_str(&format!(
                "- History: rows appended to `{}` stamp `{}`\n",
                history.section, history.stamp
            ));
        }
        let sources = layout.tiers_for(Some(c));
        let show = |list: &[TierSource]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>().join(", ");
        out.push_str(&format!("- L0 from: {}; L1 from: {}\n", show(&sources.l0), show(&sources.l1)));
        if !c.fields.is_empty() {
            let vocabularies = super::rules::Vocabularies::load(&envref, root, c).await?;
            out.push_str("\n| field | required | values | default | list | type |\n|---|---|---|---|---|---|\n");
            for (name, rule) in &c.fields {
                let mut values = vocabularies.allowed(c, name).join(", ");
                if let Some(link) = &rule.link {
                    values = format!("a memory of `{link}`");
                }
                if rule.transitions.is_some() {
                    values.push_str(" (transitions limited)");
                }
                out.push_str(&format!(
                    "| {name} | {} | {} | {} | {} | {:?} |\n",
                    if rule.required { "yes" } else { "" },
                    cell(&values),
                    rule.default.as_ref().map(|d| value_text(Some(d))).unwrap_or_default(),
                    if rule.list { "yes" } else { "" },
                    rule.field_type
                ));
            }
        }
    }
    Ok(out)
}

/// `-mem-read`: a tier, a section, a line range or the whole document.
pub(crate) fn read_part(
    text: &str,
    title: &str,
    description: &str,
    tier: &str,
    section: &str,
    lines: &str,
) -> Result<String, Error> {
    let read_tiers = TierSources {
        l0: vec![TierSource::Meta("title".into()), TierSource::Field("title".into()), TierSource::Heading],
        l1: vec![
            TierSource::Meta("description".into()),
            TierSource::Field("description".into()),
            TierSource::Paragraph,
        ],
    };
    let meta = |name: &str| match name {
        "title" => Some(title.to_string()),
        "description" => Some(description.to_string()),
        _ => None,
    };
    if !section.is_empty() {
        let (_, start, end) = find_section(text, section)
            .ok_or_else(|| Error::general_error(format!("no section '{section}' in the document")))?;
        return Ok(line_range(text, start, end));
    }
    if !lines.is_empty() {
        let (start, end) = parse_lines(lines)?;
        return Ok(line_range(text, start, end));
    }
    match tier {
        "l0" => Ok(tiers::resolve(&read_tiers, &meta, Some(text)).0),
        "l1" => {
            let (l0, l1) = tiers::resolve(&read_tiers, &meta, Some(text));
            Ok(format!("{l0}\n\n{l1}"))
        }
        "full" | "" => Ok(text.to_string()),
        other => Err(Error::general_error(format!("tier '{other}' is not one of l0, l1, full"))),
    }
}

fn line_range(text: &str, start: usize, end: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let start = start.max(1);
    let end = end.min(lines.len());
    if start > end {
        return String::new();
    }
    let mut out = lines[start - 1..end].join("\n");
    out.push('\n');
    out
}

/// `12`, `12-40` or `12-` (to the end), 1-based and inclusive.
fn parse_lines(lines: &str) -> Result<(usize, usize), Error> {
    let bad = || Error::general_error(format!("lines '{lines}' is not N, N-M or N-"));
    let (a, b) = match lines.split_once('-') {
        Some((a, b)) => (a.trim(), Some(b.trim())),
        None => (lines.trim(), None),
    };
    let start: usize = a.parse().map_err(|_| bad())?;
    let end = match b {
        None => start,
        Some("") => usize::MAX,
        Some(b) => b.parse().map_err(|_| bad())?,
    };
    if start == 0 || end < start {
        return Err(bad());
    }
    Ok((start, end))
}

/// The version a reader reports and a writer checks: the content hash of the text.
pub fn version_of(text: &str) -> Version {
    Version::from_content(text.as_bytes())
}

/// Whether an asset in `status` is a stored value (not a recipe nor a directory).
pub(crate) fn is_stored(status: Status) -> bool {
    match status {
        Status::Ready | Status::Source | Status::Override => true,
        Status::None
        | Status::Directory
        | Status::Recipe
        | Status::Submitted
        | Status::Dependencies
        | Status::Processing
        | Status::Partial
        | Status::Storing
        | Status::Expired
        | Status::Error
        | Status::Cancelled
        | Status::Volatile => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "---\ntitle: T\n---\n# Head\n\nIntro.\n\n## Problem\n\nBroken.\nStill.\n\n## Fix\n\nDo it.\n";

    #[test]
    fn read_part_selects_tiers_sections_and_lines() -> Result<(), Error> {
        assert_eq!(read_part(DOC, "", "", "l0", "", "")?, "T");
        assert_eq!(read_part(DOC, "", "", "l1", "", "")?, "T\n\nIntro.");
        assert_eq!(read_part(DOC, "", "", "full", "Problem", "")?, "## Problem\n\nBroken.\nStill.\n\n");
        assert_eq!(read_part(DOC, "", "", "full", "", "4-4")?, "# Head\n");
        assert!(read_part(DOC, "", "", "full", "", "x").is_err());
        assert!(read_part(DOC, "", "", "full", "Nope", "").is_err());
        Ok(())
    }

    #[test]
    fn significant_words_drop_short_and_common_words() {
        assert_eq!(significant_words("The store has no write precondition"), vec!["store", "write", "precondition"]);
    }
}
