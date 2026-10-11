//! `ns-search/field_extract`: document fields as columns.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use liquers_core::context::{Context, Environment};
use liquers_core::error::Error;
use liquers_core::state::State;
use liquers_core::value::ValueInterface;

use crate::records::search::{parse_structure_with_report, StructFormat};
use crate::records::{
    ChunkResolver, ContextResolver, FieldRole, FieldSchema, FieldType, FieldValue, KeyRole,
    RecordBatchMut, RecordSchema, RecordView, RecordViewMut,
};
use crate::value::{ExtValueInterface, Value};

use super::catalog::catalog_view;
use super::{folder_of, info_of, read_text, search_input, SearchInput};

const JSON: &str = "application/json";

/// The rows a document command works on: a folder's catalog (recursive, no content), or records
/// taken to be a catalog — a `key` column, and optionally `filename` and `content`.
pub(crate) async fn document_rows<E: Environment<Value = Value>>(
    state: State<Value>,
    context: &Context<E>,
) -> Result<Arc<dyn RecordView>, Error> {
    if let Some(folder) = folder_of(&state, context).await? {
        return Ok(catalog_view(&folder, true, false, 0, 0, context).await?.0);
    }
    let identifier = state.data_unchecked().identifier().to_string();
    let view = match search_input(state, context).await? {
        SearchInput::View(view) => view,
        SearchInput::Source(source) => {
            let resolver: Arc<dyn ChunkResolver> = Arc::new(ContextResolver::new(context.clone()));
            source
                .materialize(resolver, liquers_records::DEFAULT_MATERIALIZE_MAX_ROWS)
                .await? as Arc<dyn RecordView>
        }
    };
    if view.schema().index_of("key").is_none() {
        return Err(Error::conversion_error_with_message(
            identifier,
            "catalog records",
            "needs a folder, or records with a 'key' column (as ns-search/catalog makes)",
        ));
    }
    Ok(view)
}

/// One document row: its key, file name and text (from `content`, or read from the store).
pub(crate) struct Document {
    pub row: usize,
    pub key: String,
    pub filename: String,
}

pub(crate) fn documents(view: &Arc<dyn RecordView>) -> Result<Vec<Document>, Error> {
    let schema = view.schema();
    let key_col = view.column(schema.index_of("key").ok_or_else(|| {
        Error::general_error("records need a 'key' column".to_string())
    })?)?;
    let filename_col = match schema.index_of("filename") {
        Some(index) => Some(view.column(index)?),
        None => None,
    };
    let mut out = Vec::with_capacity(view.len());
    for row in 0..view.len() {
        let Some(key) = cell_text(&key_col, row) else { continue };
        let filename = filename_col
            .as_ref()
            .and_then(|col| cell_text(col, row))
            .unwrap_or_else(|| key.rsplit('/').next().unwrap_or("").to_string());
        out.push(Document { row, key, filename });
    }
    Ok(out)
}

fn cell_text(column: &crate::records::Column, row: usize) -> Option<String> {
    match column.get(row).ok()? {
        FieldValue::Text(text) => Some(text.to_string()),
        FieldValue::Null
        | FieldValue::Bool(_)
        | FieldValue::Int(_)
        | FieldValue::UInt(_)
        | FieldValue::Float(_)
        | FieldValue::Bytes(_)
        | FieldValue::Date(_)
        | FieldValue::Timestamp(_)
        | FieldValue::Vector(_) => None,
    }
}

/// The text of a document: the `content` cell when the records carry one, else the stored text.
pub(crate) async fn document_text<E: Environment<Value = Value>>(
    view: &Arc<dyn RecordView>,
    document: &Document,
    context: &Context<E>,
) -> Result<Option<String>, Error> {
    if let Some(index) = view.schema().index_of("content") {
        if let Some(text) = cell_text(&view.column(index)?, document.row) {
            return Ok(Some(text));
        }
    }
    let key = liquers_core::parse::parse_key(&document.key)?;
    let info = info_of(&key, context).await?;
    Ok(read_text(&key, &info, 0, context).await)
}

/// A field or column name usable in the search syntax: `[A-Za-z0-9_]`, not starting with a digit.
pub(crate) fn sanitize(name: &str) -> String {
    let mut out: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' })
        .collect();
    if out.chars().next().is_none_or(|c| c.is_ascii_digit()) {
        out.insert(0, '_');
    }
    out
}

fn flatten_into(prefix: &str, value: &serde_json::Value, out: &mut Vec<(String, serde_json::Value)>) {
    match value {
        serde_json::Value::Object(map) if !map.is_empty() => {
            for (key, inner) in map {
                flatten_into(&format!("{prefix}_{}", sanitize(key)), inner, out);
            }
        }
        serde_json::Value::Object(_)
        | serde_json::Value::Null
        | serde_json::Value::Bool(_)
        | serde_json::Value::Number(_)
        | serde_json::Value::String(_)
        | serde_json::Value::Array(_) => out.push((prefix.to_string(), value.clone())),
    }
}

fn lookup<'v>(value: &'v serde_json::Value, path: &str) -> Option<&'v serde_json::Value> {
    path.split('.').try_fold(value, |current, segment| current.get(segment))
}

/// The column type that holds every non-null value of a column.
fn unify(values: &[Option<serde_json::Value>]) -> (FieldType, bool) {
    let present: Vec<&serde_json::Value> =
        values.iter().flatten().filter(|v| !v.is_null()).collect();
    let structured = present.iter().any(|v| v.is_array() || v.is_object());
    if structured {
        return (FieldType::Text, true);
    }
    // An integer above i64::MAX is exact only as UInt or text, never as a float.
    let beyond_i64 = |v: &&serde_json::Value| v.is_u64() && !v.is_i64();
    if !present.is_empty() && present.iter().all(|v| v.is_i64()) {
        (FieldType::Int, false)
    } else if !present.is_empty() && present.iter().all(|v| v.is_u64()) {
        (FieldType::UInt, false)
    } else if !present.is_empty()
        && present.iter().all(|v| v.is_number())
        && !present.iter().any(beyond_i64)
    {
        (FieldType::Float, false)
    } else if !present.is_empty() && present.iter().all(|v| v.is_boolean()) {
        (FieldType::Bool, false)
    } else {
        (FieldType::Text, false)
    }
}

fn cell(value: Option<&serde_json::Value>, data_type: FieldType) -> FieldValue {
    let Some(value) = value.filter(|v| !v.is_null()) else { return FieldValue::Null };
    match data_type {
        FieldType::Int => value.as_i64().map(FieldValue::Int).unwrap_or(FieldValue::Null),
        FieldType::UInt => value.as_u64().map(FieldValue::UInt).unwrap_or(FieldValue::Null),
        FieldType::Float => value.as_f64().map(FieldValue::Float).unwrap_or(FieldValue::Null),
        FieldType::Bool => value.as_bool().map(FieldValue::Bool).unwrap_or(FieldValue::Null),
        FieldType::Text
        | FieldType::Binary
        | FieldType::Date
        | FieldType::Timestamp
        | FieldType::Vector => FieldValue::Text(Arc::from(
            match value {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            }
            .as_str(),
        )),
    }
}

/// `ns-search/field_extract`. See the command's `doc`.
pub async fn field_extract<E: Environment<Value = Value>>(
    state: State<Value>,
    extension: String,
    prefix: String,
    flatten: bool,
    fields: Vec<String>,
    context: Context<E>,
) -> Result<Value, Error> {
    let format = match extension.as_str() {
        "md" => StructFormat::FrontMatter,
        "json" => StructFormat::Json,
        "yaml" | "yml" => StructFormat::Yaml,
        other => {
            return Err(Error::general_error(format!(
                "field_extract: extension must be md, json, yaml or yml, not '{other}'"
            )))
        }
    };
    let suffix = format!(".{extension}");
    let view = document_rows(state, &context).await?;
    let mut kept: Vec<(usize, Vec<(String, serde_json::Value)>)> = Vec::new();
    let (mut without_fields, mut skipped_keys) = (0usize, 0usize);
    let mut collided: BTreeSet<String> = BTreeSet::new();
    for document in documents(&view)? {
        if !document.filename.ends_with(&suffix) {
            continue;
        }
        let Some(text) = document_text(&view, &document, &context).await? else {
            without_fields += 1;
            continue;
        };
        let (parsed, skipped) = parse_structure_with_report(&text, format);
        skipped_keys += skipped;
        let Some(value) = parsed.filter(|v| v.as_object().is_some_and(|m| !m.is_empty())) else {
            without_fields += 1;
            continue;
        };
        let mut row_fields = Vec::new();
        if fields.is_empty() {
            if let Some(map) = value.as_object() {
                for (key, inner) in map {
                    let name = sanitize(key);
                    if flatten {
                        flatten_into(&name, inner, &mut row_fields);
                    } else {
                        row_fields.push((name, inner.clone()));
                    }
                }
            }
        } else {
            for path in &fields {
                if let Some(inner) = lookup(&value, path) {
                    let name = sanitize(&path.replace('.', "_"));
                    if flatten {
                        flatten_into(&name, inner, &mut row_fields);
                    } else {
                        row_fields.push((name, inner.clone()));
                    }
                }
            }
        }
        if row_fields.is_empty() {
            // None of the requested fields is in this document.
            without_fields += 1;
            continue;
        }
        // Two fields can sanitize to one name (`a-b` and `a_b`, or `a.b` flattened and `a_b`):
        // the first, in key order, is kept and the collision is reported.
        let mut seen: BTreeSet<String> = BTreeSet::new();
        row_fields.retain(|(name, _)| {
            let first = seen.insert(name.clone());
            if !first {
                collided.insert(name.clone());
            }
            first
        });
        kept.push((document.row, row_fields));
    }
    if !collided.is_empty() {
        context.warning(&format!(
            "field_extract: fields with the same sanitized name; the first is kept for {}",
            collided.into_iter().collect::<Vec<_>>().join(", ")
        ))?;
    }
    if without_fields > 0 || skipped_keys > 0 {
        context.info(&format!(
            "field_extract: {without_fields} file(s) without fields dropped; {skipped_keys} \
             non-scalar YAML key(s) skipped"
        ))?;
    }

    // Columns in order of first appearance, with the prefix applied.
    let prefix = if prefix.is_empty() { String::new() } else { sanitize(&prefix) };
    let mut names: Vec<String> = Vec::new();
    for (_, row_fields) in &kept {
        for (name, _) in row_fields {
            let full = format!("{prefix}{name}");
            if !names.contains(&full) {
                names.push(full);
            }
        }
    }
    let lookup_rows: Vec<BTreeMap<String, serde_json::Value>> = kept
        .iter()
        .map(|(_, row_fields)| {
            row_fields.iter().map(|(n, v)| (format!("{prefix}{n}"), v.clone())).collect()
        })
        .collect();

    let input = view.schema();
    let id_name = input.id_field().map(|i| input.fields[i].name.clone());
    let mut extracted: Vec<(String, FieldType, bool)> = Vec::new();
    let mut replaced: Vec<String> = Vec::new();
    for name in &names {
        if Some(name) == id_name.as_ref() {
            context.warning(&format!(
                "field_extract: field '{name}' has the name of the Id column and is skipped"
            ))?;
            continue;
        }
        let values: Vec<Option<serde_json::Value>> =
            lookup_rows.iter().map(|row| row.get(name).cloned()).collect();
        let (data_type, json) = unify(&values);
        if input.index_of(name).is_some() {
            replaced.push(name.clone());
        }
        extracted.push((name.clone(), data_type, json));
    }
    if !replaced.is_empty() {
        context.info(&format!("field_extract: replaced columns {}", replaced.join(", ")))?;
    }

    let kept_input: Vec<usize> = (0..input.fields.len())
        .filter(|&i| !replaced.contains(&input.fields[i].name))
        .collect();
    let mut schema_fields: Vec<FieldSchema> =
        kept_input.iter().map(|&i| input.fields[i].clone()).collect();
    for (name, data_type, json) in &extracted {
        let role = input
            .index_of(name)
            .map(|i| input.fields[i].role.clone())
            .unwrap_or_else(FieldRole::keyword);
        let mut field = FieldSchema::new(name, *data_type).with_role(role);
        if *json {
            field = field.with_media_type(JSON);
        }
        if input.index_of(name).is_some_and(|i| input.fields[i].key != KeyRole::None) {
            field = field.with_key(KeyRole::None);
        }
        schema_fields.push(field);
    }
    let schema = Arc::new(RecordSchema::new(schema_fields)?);
    let input_columns: Vec<_> =
        kept_input.iter().map(|&i| view.column(i)).collect::<Result<_, _>>()?;
    let mut batch = RecordBatchMut::with_capacity(schema, kept.len());
    for ((row, _), values) in kept.iter().zip(lookup_rows.iter()) {
        let mut out = Vec::with_capacity(input_columns.len() + extracted.len());
        for column in &input_columns {
            out.push(column.get(*row)?);
        }
        for (name, data_type, json) in &extracted {
            let value = values.get(name);
            out.push(if *json {
                match value.filter(|v| !v.is_null()) {
                    Some(v) => FieldValue::Text(Arc::from(v.to_string().as_str())),
                    None => FieldValue::Null,
                }
            } else {
                cell(value, *data_type)
            });
        }
        batch.append_row(&out)?;
    }
    let result: Arc<dyn RecordView> = Arc::new(batch.freeze()?);
    Ok(Value::from_record_view(result))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_makes_addressable_names() {
        assert_eq!(sanitize("2fa"), "_2fa");
        assert_eq!(sanitize("a-b c"), "a_b_c");
        assert_eq!(sanitize("ok_1"), "ok_1");
    }

    #[test]
    fn unify_picks_the_narrowest_type() {
        let v = |j: &str| Some(serde_json::from_str::<serde_json::Value>(j).expect("json"));
        assert_eq!(unify(&[v("1"), v("2"), None]), (FieldType::Int, false));
        assert_eq!(unify(&[v("1"), v("2.5")]), (FieldType::Float, false));
        assert_eq!(unify(&[v("true")]), (FieldType::Bool, false));
        assert_eq!(unify(&[v("1"), v("\"x\"")]), (FieldType::Text, false));
        assert_eq!(unify(&[v("[1]")]), (FieldType::Text, true));
        let big = "18446744073709551615";
        assert_eq!(unify(&[v(big), v("1")]), (FieldType::UInt, false));
        assert_eq!(unify(&[v(big), v("-1")]), (FieldType::Text, false), "no exact numeric type");
        assert_eq!(unify(&[v(big), v("2.5")]), (FieldType::Text, false), "a float would round it");
        assert_eq!(cell(Some(&serde_json::json!(u64::MAX)), FieldType::UInt), FieldValue::UInt(u64::MAX));
    }
}
