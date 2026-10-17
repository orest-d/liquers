//! `ns-search/catalog`: a folder as records.

use std::collections::VecDeque;
use std::sync::Arc;

use futures::StreamExt;
use liquers_core::context::{Context, Environment};
use liquers_core::error::Error;
use liquers_core::metadata::{AssetInfo, DependencyKey, DependencyRecord, Version};
use liquers_core::query::Key;
use liquers_core::state::State;
use liquers_core::value::ValueInterface;

use crate::records::{
    FieldRole, FieldSchema, FieldType, FieldValue, KeyRole, RecordBatchMut, RecordSchema,
    RecordView, RecordViewMut,
};
use crate::value::{ExtValueInterface, Value};

use super::{
    extension_of, file_version, folder_of, info_of, media_type_of, non_negative, read_text,
};

/// How many `get_asset_info` calls run at once.
const CONCURRENCY: usize = 16;

/// `ns-search/catalog`. See the command's `doc`.
pub async fn catalog<E: Environment<Value = Value>>(
    state: State<Value>,
    recursive: bool,
    content: bool,
    max_bytes: i64,
    max_entries: i64,
    context: Context<E>,
) -> Result<Value, Error> {
    let max_bytes = non_negative(max_bytes, "max_bytes")?;
    let max_entries = non_negative(max_entries, "max_entries")?;
    let folder = folder_of(&state, &context).await?.ok_or_else(|| {
        Error::conversion_error_with_message(
            state.data_unchecked().identifier(),
            "folder",
            "catalog needs a folder: use -R-key/<folder>/-/ns-search/catalog, or no input for the \
             whole store",
        )
    })?;
    let (view, stopped) =
        catalog_view(&folder, recursive, content, max_bytes, max_entries, &context).await?;
    if stopped {
        let message = format!(
            "catalog of '{folder}' stopped at max_entries = {max_entries}; more entries exist"
        );
        context.set_description(&message).await?;
        context.info(&message)?;
    }
    Ok(Value::from_record_view(view))
}

/// The keys under `folder`, breadth first, at most `max_entries` of them (0: no limit). The
/// boolean says whether the walk stopped early. Every folder walked is recorded as a dependency
/// (`Context::record_listing`), so an entry added or removed expires the result.
pub(crate) async fn walk<E: Environment<Value = Value>>(
    folder: &Key,
    recursive: bool,
    max_entries: usize,
    context: &Context<E>,
) -> Result<(Vec<Key>, bool), Error> {
    let store = context.get_envref().get_async_store();
    let mut keys = Vec::new();
    let mut pending = VecDeque::from([folder.clone()]);
    while let Some(dir) = pending.pop_front() {
        let mut names = context.record_listing(&dir).await?;
        names.sort();
        for name in names {
            if max_entries > 0 && keys.len() >= max_entries {
                return Ok((keys, true));
            }
            let key = dir.join(&name);
            if recursive && store.is_dir(&key).await? {
                pending.push_back(key.clone());
            }
            keys.push(key);
        }
    }
    Ok((keys, false))
}

/// The asset info of every key, in key order, `CONCURRENCY` at a time, with the version of each
/// file (`None` for a folder, which its listing records).
pub(crate) async fn infos<E: Environment<Value = Value>>(
    keys: Vec<Key>,
    context: &Context<E>,
) -> Result<Vec<(Key, AssetInfo, Option<Version>)>, Error> {
    let mut out: Vec<(Key, AssetInfo, Option<Version>)> =
        futures::stream::iter(keys.into_iter().map(|key| {
            let context = context.clone();
            async move {
                let info = info_of(&key, &context).await?;
                let version =
                    if info.is_dir { None } else { Some(file_version(&key, &context).await) };
                Ok::<_, Error>((key, info, version))
            }
        }))
        .buffer_unordered(CONCURRENCY)
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<Result<_, _>>()?;
    out.sort_by_key(|(key, _, _)| key.encode());
    Ok(out)
}

fn schema(content: bool) -> Result<Arc<RecordSchema>, Error> {
    let text = || FieldRole::text().and_stored();
    let mut fields = vec![
        FieldSchema::new("key", FieldType::Text).with_key(KeyRole::Id),
        FieldSchema::new("filename", FieldType::Text).with_role(text()),
        FieldSchema::new("extension", FieldType::Text).with_role(FieldRole::keyword()),
        FieldSchema::new("parent", FieldType::Text).with_role(FieldRole::keyword()),
        FieldSchema::new("is_dir", FieldType::Bool).with_role(FieldRole::keyword()),
        FieldSchema::new("status", FieldType::Text).with_role(FieldRole::keyword()),
        FieldSchema::new("type_identifier", FieldType::Text).with_role(FieldRole::keyword()),
        FieldSchema::new("media_type", FieldType::Text).with_role(FieldRole::keyword()),
        FieldSchema::new("title", FieldType::Text).with_role(text()),
        FieldSchema::new("description", FieldType::Text).with_role(text()),
        FieldSchema::new("size", FieldType::Int).with_role(FieldRole::numeric()),
        FieldSchema::new("updated", FieldType::Timestamp).with_role(FieldRole::numeric()),
    ];
    if content {
        fields.push(FieldSchema::new("content", FieldType::Text).with_role(FieldRole::text()));
    }
    Ok(Arc::new(RecordSchema::new(fields)?))
}

fn text(value: &str) -> FieldValue {
    FieldValue::Text(Arc::from(value))
}

fn optional_text(value: &str) -> FieldValue {
    if value.is_empty() { FieldValue::Null } else { text(value) }
}

/// The first `# ` heading of a Markdown body, after any front-matter.
pub(crate) fn markdown_title(body: &str) -> Option<String> {
    let body = &body[liquers_records::search::front_matter_end(body)..];
    let mut in_fence = false;
    for line in body.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if !in_fence {
            if let Some(title) = trimmed.strip_prefix("# ") {
                let title = title.trim();
                if !title.is_empty() {
                    return Some(title.to_string());
                }
            }
        }
    }
    None
}

/// The catalog of `folder` as a view, and whether `max_entries` stopped the walk.
pub(crate) async fn catalog_view<E: Environment<Value = Value>>(
    folder: &Key,
    recursive: bool,
    content: bool,
    max_bytes: usize,
    max_entries: usize,
    context: &Context<E>,
) -> Result<(Arc<dyn RecordView>, bool), Error> {
    let (keys, stopped) = walk(folder, recursive, max_entries, context).await?;
    let entries = infos(keys, context).await?;
    // Every file listed is a dependency, its text read or not: `size`, `updated`, `status` and
    // `title` come from it. Recorded before any text is read, so a file whose text is read is
    // left recorded at the version its text was read at (`read_text`).
    context
        .add_dependencies(entries.iter().filter_map(|(key, _, version)| {
            version.map(|version| DependencyRecord::new(DependencyKey::from(key), version))
        }))
        .await;
    let schema = schema(content)?;
    let mut batch = RecordBatchMut::with_capacity(schema, entries.len());
    for (key, info, _) in entries {
        let filename = info
            .filename
            .clone()
            .or_else(|| key.filename().map(|name| name.name.clone()))
            .unwrap_or_default();
        let extension = extension_of(&filename).map(str::to_string).unwrap_or_default();
        let body = if content { read_text(&key, &info, max_bytes, context).await } else { None };
        let mut title = info.title.clone();
        if title.is_empty() && extension == "md" {
            if let Some(heading) = body.as_deref().and_then(markdown_title) {
                title = heading;
            }
        }
        let updated = chrono::DateTime::parse_from_rfc3339(&info.updated)
            .map(|dt| FieldValue::Timestamp(dt.timestamp_micros()))
            .unwrap_or(FieldValue::Null);
        let mut row = vec![
            text(&key.encode()),
            text(&filename),
            optional_text(&extension),
            text(&key.parent().encode()),
            FieldValue::Bool(info.is_dir),
            text(&format!("{:?}", info.status)),
            optional_text(&info.type_identifier),
            optional_text(&media_type_of(&info, &filename)),
            optional_text(&title),
            optional_text(&info.description),
            match info.file_size.and_then(|size| i64::try_from(size).ok()) {
                Some(size) if !info.is_dir => FieldValue::Int(size),
                Some(_) | None => FieldValue::Null,
            },
            updated,
        ];
        if content {
            row.push(match body {
                Some(body) => text(&body),
                None => FieldValue::Null,
            });
        }
        batch.append_row(&row)?;
    }
    Ok((Arc::new(batch.freeze()?) as Arc<dyn RecordView>, stopped))
}
