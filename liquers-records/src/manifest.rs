//! The manifest file shape: `ManifestSpec`, `ManifestKind`, `ChunkTemplate`.
//!
//! Plain serde declarations only — this module has no behaviour. `ChunkNaming`, `query_at` and
//! manifest validation (name collisions among explicit chunks, the `try_from` conversion into a
//! `ManifestSource`) belong to Step 4.1, once `ManifestSource` itself exists.
//!
//! See `specs/design/record-streams/phase2-architecture.md`, §"The types" and §"Construction
//! helpers, options and the provider chain".

use std::collections::HashMap;
use std::sync::Arc;

use liquers_core::expiration::Expires;
use liquers_core::recipes::Recipe;
use serde::{Deserialize, Serialize};

use crate::schema::RecordSchema;

fn true_default() -> bool {
    true
}

/// The only kind this design defines; a folder may hold other YAML documents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ManifestKind {
    #[default]
    #[serde(rename = "record-stream")]
    RecordStream,
}

/// The generated tail of a manifest. Chunk `i` (a global index, at least the number of explicit
/// chunks) is `<query>-<offset>-<batch_size>` with `offset = first_offset + step × i`, followed by
/// the chunk's key filename when the manifest is keyed (`ChunkNaming::key(i)`).
///
/// `query_at` and `offset_at` are Step 4.1's, once the naming and template-rendering logic lands.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChunkTemplate {
    pub query: String,
    #[serde(default)]
    pub first_offset: u64,
    pub step: u64,
    pub batch_size: u64,
}

/// The manifest file. The explicit list and the template **combine**: `chunks` are the stream's
/// first chunks in order, and `template` produces everything after them, rendered at the
/// **global** chunk index. Either may be absent. See `manifest-format.md` §4a.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestSpec {
    /// The discriminator, always written. `to_record_source` (Step 4.1) recognizes a plain YAML
    /// or JSON document by it; here, absent is accepted (a bare spec) and any other value is
    /// refused by that later conversion.
    #[serde(default)]
    pub manifest: ManifestKind,
    /// Absent or unknown reads as the latest (`manifest-format.md` §8.3); written as the latest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<u32>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Shared by every **keyed** chunk: merged under each explicit chunk's own `arguments` (the
    /// chunk wins) and given as-is to every template chunk. As with per-chunk arguments, a
    /// manifest that has shared `arguments` or `links` and an unkeyed chunk is refused by
    /// `with_key` / at stream open (Step 4.1) — an unkeyed chunk is a bare query and cannot carry
    /// them.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub arguments: HashMap<String, serde_json::Value>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub links: HashMap<String, String>,
    /// Copied onto every chunk recipe by `ManifestRecipeProvider` (Step 4.1), as `recipes.yaml`
    /// fields are.
    #[serde(default)]
    pub volatile: bool,
    #[serde(default)]
    pub expires: Expires,
    /// The explicit prefix, as recipes — `recipes.yaml`'s own entry type, so a chunk carries its
    /// own `arguments`, `links`, `title`. A chunk whose query ends in a filename is **keyed** by
    /// it, exactly as a `recipes.yaml` entry is (`Recipe::filename`).
    #[serde(default)]
    pub chunks: Vec<Recipe>,
    /// The rule for chunks beyond the prefix. `None` — `chunks` is the whole stream, and
    /// `chunks()` returns `Known`. `Some` — the count is unknown and it returns `Unbounded`; a
    /// walk ends at the first chunk shorter than the template's `batch_size`.
    pub template: Option<ChunkTemplate>,
    /// The extension of template-generated chunk keys, which is also their stored format.
    /// Default `csv`.
    #[serde(default)]
    pub extension: Option<String>,
    /// Whether keyed chunks are written to the store. Default `true`.
    #[serde(default = "true_default")]
    pub stored: bool,
    /// Whether keyed chunks are registered with the asset manager for reuse. Default `true`.
    /// Both `false` means "not kept, and **not volatile**" (`manifest-format.md` §4b).
    #[serde(default = "true_default")]
    pub cached: bool,
    pub uniform_schema: Option<Arc<RecordSchema>>,
}

/// Written by hand, not derived: `stored` and `cached` default to `true`, and a derived `Default`
/// would make them `false` (the pitfall `Recipe`'s own hand-written `Default` avoids for the same
/// reason).
impl Default for ManifestSpec {
    fn default() -> Self {
        ManifestSpec {
            manifest: ManifestKind::RecordStream,
            version: None,
            title: String::new(),
            description: String::new(),
            arguments: HashMap::new(),
            links: HashMap::new(),
            volatile: false,
            expires: Expires::default(),
            chunks: Vec::new(),
            template: None,
            extension: None,
            stored: true,
            cached: true,
            uniform_schema: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_spec_default_has_stored_and_cached_true_and_record_stream_kind() {
        let spec = ManifestSpec::default();
        assert_eq!(spec.manifest, ManifestKind::RecordStream);
        assert!(spec.stored);
        assert!(spec.cached);
        assert!(spec.chunks.is_empty());
        assert!(spec.template.is_none());
        assert!(spec.uniform_schema.is_none());
    }

    #[test]
    fn manifest_spec_deserializes_a_minimal_yaml_document() {
        let yaml = "manifest: record-stream\nchunks: []\n";
        let spec: ManifestSpec = serde_yaml::from_str(yaml).expect("deserialize");
        assert_eq!(spec.manifest, ManifestKind::RecordStream);
        assert!(spec.chunks.is_empty());
        assert!(spec.stored);
        assert!(spec.cached);
        assert!(spec.template.is_none());
    }

    #[test]
    fn manifest_spec_rejects_an_unknown_manifest_kind() {
        let yaml = "manifest: something-else\nchunks: []\n";
        let result: Result<ManifestSpec, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err());
    }
}
