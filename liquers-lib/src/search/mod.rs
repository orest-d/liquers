//! `ns-search`: search over a store and over any records.
//!
//! - [`catalog`] — a folder (from the input state; the root when there is none) as records.
//! - [`commands`] — the command registry as records.
//! - [`field_extract`] — fields of JSON, YAML or Markdown front-matter files as columns.
//! - [`md_split`] — Markdown sections, or lines of any text file, as records.
//! - [`search`] — filter any records with the search syntax, optionally ranked by BM25.
//!
//! The search engine itself is `liquers_records::search`; this module is the glue that needs
//! `Value`, `Context` and the asset manager. **Nothing here evaluates an asset** except an explicit
//! key or query value handed to `search`: listings describe entries through `get_asset_info`, and
//! content is read from stored bytes only. Design: `specs/design/store-and-asset-search/`.
//!
//! **Everything read is recorded** as a dependency of the evaluation: each folder walked as
//! `-R-dir/<folder>` (`Context::record_listing`), each file listed or read as `-R/<key>`, so a
//! result expires when what it was built from changes through the asset manager
//! (`specs/design/search-read-dependencies/`).

use std::sync::Arc;

use liquers_core::assets::AssetManager;
use liquers_core::context::{Context, Environment};
use liquers_core::error::Error;
use liquers_core::media_type::file_extension_to_media_type;
use liquers_core::metadata::{AssetInfo, DependencyKey, DependencyRecord, Status, Version};
use liquers_core::query::Key;
use liquers_core::state::State;
use liquers_core::value::ValueInterface;

use crate::records::{
    to_record_source, ChunkResolver, ContextResolver, RecordSource, RecordView, ToRecordOptions,
};
use crate::value::{ExtValueInterface, Value};

pub mod catalog;
pub mod command_catalog;
pub mod extract;
pub mod md_split;
pub mod search_command;

pub use catalog::catalog;
pub use command_catalog::commands;
pub use extract::field_extract;
pub use md_split::md_split;
pub use search_command::search;

/// What [`search_input`] makes of a state.
#[derive(Debug)]
pub enum SearchInput {
    View(Arc<dyn RecordView>),
    Source(Arc<dyn RecordSource>),
}

/// A non-negative integer argument as `usize`.
pub(crate) fn non_negative(value: i64, name: &str) -> Result<usize, Error> {
    usize::try_from(value)
        .map_err(|_| Error::general_error(format!("{name} must not be negative, got {value}")))
}

/// The folder a state denotes, if any:
/// - no value and no key → the root key;
/// - a `dir` state, or an `AssetInfo` listing (`-R-dir`, `-R-sdir`) → the key in its metadata;
/// - a `Key` value, or a `Query` value that is a plain key, naming a directory → that key;
/// - anything else → `None`.
pub async fn folder_of<E: Environment<Value = Value>>(
    state: &State<Value>,
    context: &Context<E>,
) -> Result<Option<Key>, Error> {
    let value = state.data_unchecked();
    let identifier = value.identifier();
    let candidate = match identifier.as_ref() {
        "Key" => Some(value.try_into_key()?),
        "Query" => value.try_into_query()?.key(),
        "AssetInfo" => state.metadata.key()?,
        _ => {
            if value.is_none() {
                let type_identifier = state.metadata.type_identifier().unwrap_or_default();
                match state.metadata.key()? {
                    None => return Ok(Some(Key::new())),
                    Some(key) if type_identifier == "dir" => return Ok(Some(key)),
                    Some(key) => Some(key),
                }
            } else {
                None
            }
        }
    };
    match candidate {
        Some(key) => {
            if key.is_empty() || context.get_envref().get_async_store().is_dir(&key).await? {
                Ok(Some(key))
            } else {
                Ok(None)
            }
        }
        None => Ok(None),
    }
}

/// `text/*`, `application/json`, `application/x-yaml`, `application/x-toml`, `application/xml`,
/// and any `+json` or `+xml` type.
pub fn is_text_media_type(media_type: &str) -> bool {
    let base = media_type.split(';').next().unwrap_or("").trim();
    base.starts_with("text/")
        || matches!(
            base,
            "application/json" | "application/x-yaml" | "application/x-toml" | "application/xml"
        )
        || base.ends_with("+json")
        || base.ends_with("+xml")
}

/// The extension of a file name, without the dot.
pub(crate) fn extension_of(filename: &str) -> Option<&str> {
    let dot = filename.rfind('.')?;
    let extension = &filename[dot + 1..];
    if dot == 0 || extension.is_empty() {
        None
    } else {
        Some(extension)
    }
}

/// The media type of an entry: the declared one, else the one its extension implies.
pub(crate) fn media_type_of(info: &AssetInfo, filename: &str) -> String {
    if info.media_type.is_empty() {
        file_extension_to_media_type(extension_of(filename).unwrap_or("")).to_string()
    } else {
        info.media_type.clone()
    }
}

/// The stored text of an entry, cut to `max_bytes` (0: no limit) at a UTF-8 boundary. `None` for
/// a directory, an entry whose data is not stored (`Ready`, `Source` and `Override` only), a
/// non-text media type, bytes that are not UTF-8, or an unreadable entry. Never evaluates.
pub(crate) async fn read_text<E: Environment<Value = Value>>(
    key: &Key,
    info: &AssetInfo,
    max_bytes: usize,
    context: &Context<E>,
) -> Option<String> {
    if info.is_dir {
        return None;
    }
    let stored = holds_stored_value(info.status);
    let filename = info.filename.clone().unwrap_or_default();
    if !stored || !is_text_media_type(&media_type_of(info, &filename)) {
        return None;
    }
    let bytes = read_recorded(key, context).await?;
    let cut = if max_bytes > 0 && bytes.len() > max_bytes { &bytes[..max_bytes] } else { &bytes[..] };
    match std::str::from_utf8(cut) {
        Ok(text) => Some(text.to_string()),
        // Cut mid-character: keep the valid prefix. Any other invalid byte means it is not text.
        Err(error) if error.error_len().is_none() => {
            std::str::from_utf8(&cut[..error.valid_up_to()]).ok().map(str::to_string)
        }
        Err(_) => None,
    }
}

/// How many times [`read_recorded`] reads a file whose version moves while it is read.
const READS: usize = 3;

/// The stored bytes of `key`, recorded as a dependency of this evaluation with the version they
/// were read at (`specs/design/search-read-dependencies/`).
///
/// A live asset holding a value is read from memory, bytes and version from one snapshot: it can
/// be `Ready` before its background write reaches the store, and the store would then give the
/// previous bytes under the new version. Otherwise the store is read, and the version is read
/// before and after the bytes; if it moved, a write landed in between, and the file is read
/// again, at most [`READS`] times. A version that never settles is recorded as unknown, so the
/// next change of the file expires the result. `None` when the file cannot be read, which is
/// still recorded.
pub(crate) async fn read_recorded<E: Environment<Value = Value>>(
    key: &Key,
    context: &Context<E>,
) -> Option<Vec<u8>> {
    if let Some(asset) = context.get_envref().get_asset_manager().lookup_key_asset(key) {
        if holds_stored_value(asset.status().await) {
            if let Ok(Some((bytes, metadata))) = asset.get_binary_any_status().await {
                let version = metadata.version().unwrap_or_else(Version::unknown);
                record_read(key, version, context).await;
                return Some(bytes.as_ref().clone());
            }
        }
    }
    let store = context.get_envref().get_async_store();
    let mut version = file_version(key, context).await;
    let mut bytes = None;
    let mut settled = false;
    for _ in 0..READS {
        bytes = store.get(key).await.ok().map(|(bytes, _)| bytes);
        let after = file_version(key, context).await;
        if after == version {
            settled = true;
            break;
        }
        version = after;
    }
    record_read(key, if settled { version } else { Version::unknown() }, context).await;
    bytes
}

/// Whether an asset in `status` holds a value whose bytes `read_text` may use: the same statuses
/// it accepts from an `AssetInfo`.
fn holds_stored_value(status: Status) -> bool {
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

/// The version the asset manager holds for `key`, read from metadata: it never evaluates. A
/// store error answers `Version::unknown()`, which only makes invalidation more eager.
pub(crate) async fn file_version<E: Environment<Value = Value>>(
    key: &Key,
    context: &Context<E>,
) -> Version {
    context
        .get_envref()
        .get_asset_manager()
        .version(key)
        .await
        .unwrap_or_else(|_| Version::unknown())
}

/// Records `-R/<key>` at `version` on this evaluation: a later write or removal of `key` through
/// the asset manager expires the result.
pub(crate) async fn record_read<E: Environment<Value = Value>>(
    key: &Key,
    version: Version,
    context: &Context<E>,
) {
    context.add_dependency(DependencyRecord::new(DependencyKey::from(key), version)).await;
}

/// The asset info of `key`, through the asset manager — which never starts the asset.
pub(crate) async fn info_of<E: Environment<Value = Value>>(
    key: &Key,
    context: &Context<E>,
) -> Result<AssetInfo, Error> {
    context.get_envref().get_asset_manager().get_asset_info(key).await
}

/// The records a state denotes:
/// - a folder (see [`folder_of`]) → its catalog, recursive and without content;
/// - a `Key` or `Query` value → evaluated, and the result converted again;
/// - a view → itself; a source → itself;
/// - anything else → `to_record_source` (tables in their declared format, JSON, manifests); a
///   non-manifest result is materialized into a view.
pub async fn search_input<E: Environment<Value = Value>>(
    state: State<Value>,
    context: &Context<E>,
) -> Result<SearchInput, Error> {
    if let Some(folder) = folder_of(&state, context).await? {
        let view = catalog::catalog_view(&folder, true, false, 0, 0, context).await?.0;
        return Ok(SearchInput::View(view));
    }
    let value = state.data_unchecked();
    let identifier = value.identifier();
    match identifier.as_ref() {
        "RecordView" => Ok(SearchInput::View(value.as_record_view()?)),
        "RecordSource" => Ok(SearchInput::Source(value.as_record_source()?)),
        "Key" | "Query" => {
            let query = match identifier.as_ref() {
                "Key" => liquers_core::query::Query::from(value.try_into_key()?),
                _ => value.try_into_query()?,
            };
            let evaluated = context.get_dependency_state(&query).await?;
            Box::pin(search_input(evaluated, context)).await
        }
        _ => {
            let source = to_record_source(value, &state.metadata, &ToRecordOptions::default(), context)
                .await?;
            if source.manifest().is_some() {
                return Ok(SearchInput::Source(source));
            }
            let resolver: Arc<dyn ChunkResolver> = Arc::new(ContextResolver::new(context.clone()));
            let batch = source
                .materialize(resolver, liquers_records::DEFAULT_MATERIALIZE_MAX_ROWS)
                .await?;
            Ok(SearchInput::View(batch as Arc<dyn RecordView>))
        }
    }
}

/// Register the `ns-search` commands. The caller must define `type CommandEnvironment = ...`.
#[macro_export]
macro_rules! register_search_commands {
    ($cr:expr) => {{
        use liquers_macro::register_command;
        use $crate::search::{catalog, commands, field_extract, md_split, search};

        register_command!($cr,
            async fn catalog(state, recursive: bool = true, content: bool = false,
                max_bytes: i64 = 262144, max_entries: i64 = 100000, context) -> result
            namespace: "search"
            label: "Catalog"
            doc: "A folder (from the input; the whole store without one) as records: key, filename, extension, parent, is_dir, status, type_identifier, media_type, title, description, size, updated, and with content=true the stored text. Never evaluates anything"
        )?;

        register_command!($cr,
            async fn commands(namespace: String = "", context) -> result
            namespace: "search"
            label: "Commands"
            doc: "The command registry (or one namespace) as records: id, query, realm, namespace, name, module, title, description (the signature), doc, volatile, output_filename"
        )?;

        register_command!($cr,
            async fn search(state, query: String = "", rank: bool = true, case_sensitive: bool = false,
                limit: i64 = 50, fields: Vec<String> multiple, context) -> result
            namespace: "search"
            label: "Search"
            doc: "Filter records with the search syntax (terms, \"phrases\", -negation, a | b, ( ), field:value, field:>v, /regex/, field:( )); rank=true sorts by BM25 and adds score, matched and excerpt; limit=0 means no limit; trailing arguments name the fields free text is matched in"
        )?;

        register_command!($cr,
            async fn field_extract(state, extension: String = "md" (enum: ["md", "json", "yaml", "yml"]),
                prefix: String = "", flatten: bool = false, fields: Vec<String> multiple, context) -> result
            namespace: "search"
            label: "Extract fields"
            doc: "Files with the extension (md: front-matter; json; yaml/yml) under a folder, or rows of a catalog, with their fields as columns; files without fields are dropped"
        )?;

        register_command!($cr,
            async fn md_split(state, unit: String = "section" (enum: ["section", "line"]), context) -> result
            namespace: "search"
            label: "Split Markdown"
            doc: "Markdown sections (unit=section, .md files) or lines of any text file (unit=line) as records with key, filename, n, line, end_line, heading, level and text"
        )?;

        Ok::<(), liquers_core::error::Error>(())
    }};
}
