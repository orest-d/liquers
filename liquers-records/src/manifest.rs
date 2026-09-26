//! The manifest file shape (`ManifestSpec`, `ManifestKind`, `ChunkTemplate`), chunk naming
//! (`ChunkNaming`), template query rendering (`ChunkTemplate::query_at` / `offset_at`), and the
//! load-time validation checks a manifest source runs before it can be used.
//!
//! The conversion into a `ManifestSource` itself — which calls the validation helpers below —
//! belongs to Step 4.2, once that type exists.
//!
//! See `specs/design/record-streams/phase2-architecture.md`, §"The types" and §"Construction
//! helpers, options and the provider chain", and `specs/design/record-streams/manifest-format.md`,
//! §4, §4a and §8.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use liquers_core::error::Error;
use liquers_core::expiration::Expires;
use liquers_core::parse::parse_query;
use liquers_core::query::{Key, Query};
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChunkTemplate {
    pub query: String,
    #[serde(default)]
    pub first_offset: u64,
    pub step: u64,
    pub batch_size: u64,
}

impl ChunkTemplate {
    /// The offset chunk `index` (a **global** index — see `manifest-format.md` §4a) starts at:
    /// `first_offset + step × index`. Saturates instead of overflowing on a pathological template
    /// or a very large index: a wrong (very large) offset is a validation problem for the caller
    /// to notice, not a panic in library code.
    pub fn offset_at(&self, index: u64) -> u64 {
        self.first_offset
            .saturating_add(self.step.saturating_mul(index))
    }

    /// The query for chunk `index`: `<query>-<offset>-<batch_size>`, with `filename` appended as a
    /// trailing filename segment when the chunk is keyed (`ChunkNaming::key`'s result, when this
    /// template chunk has one).
    ///
    /// Built as text and parsed back with `liquers_core::parse::parse_query`, so a malformed
    /// `query` (or, in principle, `filename`) is reported here — at manifest-load time — rather
    /// than surfacing later as an opaque evaluation failure.
    pub fn query_at(&self, index: u64, filename: Option<&str>) -> Result<Query, Error> {
        let offset = self.offset_at(index);
        let mut text = format!("{}-{}-{}", self.query, offset, self.batch_size);
        if let Some(name) = filename {
            text.push('/');
            text.push_str(name);
        }
        parse_query(&text)
    }
}

/// A template chunk's generated key: `<prefix>_{n:04}.<extension>` inside `folder` — the
/// manifest's own folder, and its filename stem (`manifest-format.md` §2). Explicit chunks are
/// never named this way; they are keyed by their own query's filename, exactly as a `recipes.yaml`
/// entry is (Phase 2 §"A. Chunk keys").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkNaming {
    pub folder: Key,
    /// The manifest's own filename stem, e.g. `"daily"` from `daily.manifest.yaml`.
    pub prefix: String,
    /// The stored format of generated chunks, and their filename extension. Not defaulted here —
    /// `ManifestSpec::extension`'s `csv` default is applied where a manifest's naming is derived,
    /// not by this struct.
    pub extension: String,
}

impl ChunkNaming {
    /// Chunk `n`'s key: `<folder>/<prefix>_{n:04}.<extension>`. `_{n:04}` pads to at least four
    /// digits (fixed for now, per `phase2-architecture.md` §A); past 9 999 the name is still
    /// correct but no longer sorts as text.
    pub fn key(&self, n: u64) -> Key {
        self.folder
            .join(format!("{}_{n:04}.{}", self.prefix, self.extension))
    }

    /// The inverse of `key`: `Some(n)` when `name` is exactly this naming's chunk `n`. The match is
    /// canonical — `name` must be exactly what `key(n)` would produce, so `daily_00042.csv` (an
    /// extra digit `key` would never emit for chunk 42) reads as `None`, not `Some(42)`.
    pub fn index_of(&self, name: &str) -> Option<u64> {
        let digits = name
            .strip_prefix(self.prefix.as_str())?
            .strip_prefix('_')?
            .strip_suffix(&format!(".{}", self.extension))?;
        if digits.len() < 4 || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let n: u64 = digits.parse().ok()?;
        if format!("{n:04}") == digits {
            Some(n)
        } else {
            None
        }
    }
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
    /// Read leniently: a manifest is written by hand, so a version that is not a whole number
    /// (`"1.5"`, `v2`) is kept as `None` — the latest — rather than refusing the file.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient_version"
    )]
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
/// `version` accepts any scalar; only a whole number that fits `u32` is kept.
fn lenient_version<'de, D>(deserializer: D) -> Result<Option<u32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(match value {
        serde_json::Value::Number(n) => n.as_u64().and_then(|v| u32::try_from(v).ok()),
        serde_json::Value::String(text) => text.trim().parse::<u32>().ok(),
        serde_json::Value::Null
        | serde_json::Value::Bool(_)
        | serde_json::Value::Array(_)
        | serde_json::Value::Object(_) => None,
    })
}

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

impl ManifestSpec {
    /// Refuses when two explicit chunks would be keyed by the same filename — the first collision
    /// Phase 2 §"A. Chunk keys" refuses at load. An unkeyed chunk (its query has no filename) never
    /// collides with anything; a chunk whose query itself fails to parse is reported by
    /// `Recipe::filename`'s own error rather than silently treated as unkeyed.
    ///
    /// Key-independent: this check does not need the manifest's own key, so `ManifestSource::new`
    /// (Step 4.2) can run it before one is known.
    pub fn check_explicit_chunk_collisions(&self) -> Result<(), Error> {
        let mut seen = HashSet::new();
        for chunk in &self.chunks {
            let Some(name) = chunk.filename()? else {
                continue;
            };
            let encoded = name.encode().to_string();
            if !seen.insert(encoded.clone()) {
                return Err(Error::general_error(format!(
                    "manifest: two explicit chunks are both keyed \"{encoded}\""
                )));
            }
        }
        Ok(())
    }

    /// Refuses when an explicit chunk's filename also matches `naming`'s generated pattern — the
    /// second collision Phase 2 §"A. Chunk keys" refuses at load.
    ///
    /// Key-dependent: `naming` is the manifest's own `ChunkNaming`, which needs the manifest's key
    /// (its folder and filename stem) to build, so `ManifestSource::with_key` (Step 4.2) is where
    /// this runs, not `new`.
    pub fn check_explicit_names_against_template(&self, naming: &ChunkNaming) -> Result<(), Error> {
        for chunk in &self.chunks {
            let Some(name) = chunk.filename()? else {
                continue;
            };
            let encoded = name.encode().to_string();
            if naming.index_of(&encoded).is_some() {
                return Err(Error::general_error(format!(
                    "manifest: explicit chunk \"{encoded}\" matches the template's naming pattern"
                )));
            }
        }
        Ok(())
    }

    /// Refuses per-chunk or shared `arguments`/`links` on a chunk that is unkeyed — its own query
    /// has no filename, so its identity is the query alone and there is no key for an override to
    /// attach to without aliasing silently. Phase 2 §"A. Chunk keys": "Per-chunk arguments or
    /// links on an unkeyed explicit chunk are refused ... as soon as the manifest's key is known
    /// (`with_key`)". Template chunks are excluded: they share the template's arguments/links "by
    /// construction" and are never checked here.
    ///
    /// Key-dependent **in effect**, not in the check itself (it never reads a `ChunkNaming`): a
    /// manifest that never receives a key has no keyed chunk for anything to alias, so the rule
    /// only bites once `ManifestSource::with_key` (Step 4.2) runs it — never from `new`. A manifest
    /// built by a command and kept keyless (§"A": "all its chunks are unkeyed") is therefore free
    /// to carry per-chunk arguments that simply never apply to anything.
    pub fn check_unkeyed_chunk_arguments(&self) -> Result<(), Error> {
        for chunk in &self.chunks {
            let is_keyed = chunk.filename()?.is_some();
            if !is_keyed && (!chunk.arguments.is_empty() || !chunk.links.is_empty()) {
                return Err(Error::general_error(format!(
                    "manifest: chunk \"{}\" is unkeyed (its query has no filename) but declares \
                     per-chunk arguments or links",
                    chunk.query
                )));
            }
        }
        let any_unkeyed = self
            .chunks
            .iter()
            .map(Recipe::filename)
            .collect::<Result<Vec<_>, Error>>()?
            .iter()
            .any(Option::is_none);
        if any_unkeyed && (!self.arguments.is_empty() || !self.links.is_empty()) {
            return Err(Error::general_error(
                "manifest: shared arguments/links require every explicit chunk to be keyed"
                    .to_string(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Used by the `ManifestSource` tests deferred from Step 4.1 (Phase 3 §2.7 / §9), which land
    // here once `ManifestSource` exists (Step 4.2's `sources.rs`). `RecordSource` brings
    // `chunks()` into scope — `ManifestSource` implements it, but a trait method needs its trait
    // imported to be callable.
    use crate::value::RecordSource;
    use crate::{ChunkId, ChunkList, ManifestSource};
    use liquers_core::parse::parse_key;

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

    // --- Phase 3 §2.7 (`liquers-records/src/manifest.rs`) ---
    //
    // Eight of the section's twelve tests: the `ChunkNaming` / `ChunkTemplate` / `ManifestSpec`
    // ones. The other four (`manifest_source_new_refuses_colliding_explicit_chunk_names`,
    // `manifest_source_with_key_refuses_per_chunk_arguments_on_an_unkeyed_chunk`,
    // `manifest_source_with_key_refuses_explicit_name_matching_template_pattern`,
    // `manifest_source_without_key_keeps_chunks_identified_by_query`) construct a `ManifestSource`
    // and land in Step 4.2, once that type exists.

    #[test]
    fn chunk_naming_key_formats_with_padding() {
        let naming = ChunkNaming {
            folder: Key::new(),
            prefix: "daily".to_string(),
            extension: "csv".to_string(),
        };
        assert_eq!(naming.key(42).to_string(), "daily_0042.csv");
        assert_eq!(naming.key(10_000).to_string(), "daily_10000.csv");
    }

    #[test]
    fn chunk_naming_index_of_returns_chunk_number() {
        let naming = ChunkNaming {
            folder: Key::new(),
            prefix: "daily".to_string(),
            extension: "csv".to_string(),
        };
        assert_eq!(naming.index_of("daily_0042.csv"), Some(42));
        assert_eq!(naming.index_of("daily_0000.csv"), Some(0));
        assert_eq!(naming.index_of("other.csv"), None);
    }

    #[test]
    fn chunk_template_offset_at_advances_by_step() {
        let template = ChunkTemplate {
            query: "sql_query".to_string(),
            first_offset: 100,
            step: 50,
            batch_size: 25,
        };
        assert_eq!(template.offset_at(0), 100);
        assert_eq!(template.offset_at(1), 150);
        assert_eq!(template.offset_at(5), 350);
    }

    #[test]
    fn chunk_template_query_at_appends_offset_and_batch_size() -> Result<(), Error> {
        let template = ChunkTemplate {
            query: "sql_query".to_string(),
            first_offset: 0,
            step: 10,
            batch_size: 5,
        };
        let query = template.query_at(0, None)?;
        let encoded = query.encode();
        assert!(encoded.contains("sql_query"));
        assert!(encoded.contains("0") && encoded.contains("5"));
        Ok(())
    }

    #[test]
    fn chunk_template_query_at_with_filename_appends_it() -> Result<(), Error> {
        let template = ChunkTemplate {
            query: "sql_query".to_string(),
            first_offset: 0,
            step: 10,
            batch_size: 5,
        };
        let query = template.query_at(0, Some("chunk.csv"))?;
        assert!(query.encode().contains("chunk.csv"));
        Ok(())
    }

    #[test]
    fn manifest_spec_yaml_defaults_stored_cached_extension() {
        let yaml = "chunks:\n  - query: select 1\n";
        let spec: ManifestSpec = serde_yaml::from_str(yaml).expect("deserialize");
        assert!(spec.stored);
        assert!(spec.cached);
        // `extension` is `Option<String>` with a plain `#[serde(default)]`, so an absent field
        // reads as `None`; the `csv` default is applied where the naming is derived
        // (`ChunkNaming`, in `with_key`), not by serde.
        assert_eq!(spec.extension, None);
    }

    #[test]
    fn manifest_spec_explicit_stored_false_is_honored() {
        let yaml = "chunks: []\nstored: false\n";
        let spec: ManifestSpec = serde_yaml::from_str(yaml).expect("deserialize");
        assert!(!spec.stored);
    }

    #[test]
    fn manifest_spec_deserialize_ignores_unmodeled_envelope_fields() {
        // `manifest` and `version` are modelled fields; an unrecognized version — even one that
        // is not a whole number — reads as the latest, and an unmodelled field is tolerated.
        let yaml = "manifest: record-stream\nversion: \"1.5\"\nextra: unmodeled\nchunks: []\n";
        let spec: ManifestSpec = serde_yaml::from_str(yaml).expect("deserialize");
        assert_eq!(spec.version, None);
        let spec: ManifestSpec =
            serde_yaml::from_str("version: 15\nchunks: []\n").expect("deserialize");
        assert_eq!(spec.version, Some(15));
    }

    // --- Phase 3 §9 (`liquers-records/tests/manifest_validation.rs`) ---
    //
    // Four of the section's five tests, moved here (rather than to a separate integration test
    // file) because they exercise only `ChunkTemplate` / `ChunkNaming` / `ManifestSpec`, all of
    // which already live in this module. The fifth,
    // `explicit_chunk_name_collisions_are_refused_at_load`, constructs a `ManifestSource` and
    // lands in Step 4.2 alongside the other four deferred above.

    #[test]
    fn a_chunk_query_plans_without_a_registry() -> Result<(), Box<dyn std::error::Error>> {
        // `liquers-validate --no-registry` is the CLAUDE.md-prescribed way to check a query parses
        // and plans without opening a store or a command registry — exactly what a manifest author
        // should run on `template.query` before committing a manifest. This test exercises the
        // same parse the tool would, at the level `liquers-records` can reach without a registry
        // dependency. `Query` has no `FromStr`; `parse_query` is the parser entry point.
        let query = liquers_core::parse::parse_query("ns-sql/sql_query-0-1000")?;
        assert!(!query.encode().is_empty());
        Ok(())
    }

    #[test]
    fn chunk_template_argument_names_are_positional_not_named(
    ) -> Result<(), Box<dyn std::error::Error>> {
        // `ChunkTemplate::query_at` appends `offset` and `batch_size` positionally
        // (`<query>-<offset>-<batch_size>`), so the template's `query` must name a command whose
        // first two parameters accept them in that order — checked here as a documented
        // constraint, since `liquers-records` cannot itself see the target command's
        // `ArgumentInfo`.
        let template = ChunkTemplate {
            query: "ns-sql/sql_query".to_string(),
            first_offset: 0,
            step: 1000,
            batch_size: 1000,
        };
        let query = template.query_at(0, None)?;
        let encoded = query.encode();
        let offset_pos = encoded.find("-0-").expect("offset before batch_size");
        let batch_size_pos = encoded.find("-1000").expect("batch_size present");
        assert!(offset_pos < batch_size_pos);
        Ok(())
    }

    #[test]
    fn an_unrecognized_manifest_version_is_accepted_as_the_latest_shape() {
        // ManifestSpec has no `version` field of its own (§2.7); an unfamiliar `version:` in the
        // envelope is simply ignored rather than rejected, which is "defaults to latest" in effect
        // — there is exactly one shape today, so every version string reads the same spec.
        let yaml = "manifest: record-stream\nversion: 999\nchunks: []\n";
        assert!(serde_yaml::from_str::<ManifestSpec>(yaml).is_ok());
    }

    #[test]
    fn templated_naming_matches_the_documented_pattern() {
        let naming = ChunkNaming {
            folder: Key::new(),
            prefix: "daily".to_string(),
            extension: "csv".to_string(),
        };
        // `<prefix>_{n:04}.<extension>` (phase2-architecture.md §A)
        assert_eq!(naming.key(7).to_string(), "daily_0007.csv");
        assert_eq!(naming.key(123_456).to_string(), "daily_123456.csv"); // past 9999: correct, no longer sortable as text
    }

    // --- New helpers this step adds: `ManifestSpec` validation ---

    #[test]
    fn check_explicit_chunk_collisions_accepts_distinct_names() {
        let spec = ManifestSpec {
            chunks: vec![
                Recipe {
                    query: "ns-sql/sql_query-0-1000/a.csv".to_string(),
                    ..Default::default()
                },
                Recipe {
                    query: "ns-sql/sql_query-1000-1000/b.csv".to_string(),
                    ..Default::default()
                },
            ],
            ..ManifestSpec::default()
        };
        assert!(spec.check_explicit_chunk_collisions().is_ok());
    }

    #[test]
    fn check_explicit_chunk_collisions_refuses_a_shared_filename() {
        let spec = ManifestSpec {
            chunks: vec![
                Recipe {
                    query: "ns-sql/sql_query-0-1000/data.csv".to_string(),
                    ..Default::default()
                },
                Recipe {
                    query: "ns-sql/sql_query-1000-1000/data.csv".to_string(),
                    ..Default::default()
                },
            ],
            ..ManifestSpec::default()
        };
        assert!(spec.check_explicit_chunk_collisions().is_err());
    }

    #[test]
    fn check_explicit_chunk_collisions_ignores_unkeyed_chunks() {
        let spec = ManifestSpec {
            chunks: vec![
                Recipe {
                    query: "ns-sql/sql_query-0-1000".to_string(),
                    ..Default::default()
                },
                Recipe {
                    query: "ns-sql/sql_query-1000-1000".to_string(),
                    ..Default::default()
                },
            ],
            ..ManifestSpec::default()
        };
        assert!(spec.check_explicit_chunk_collisions().is_ok());
    }

    #[test]
    fn check_explicit_names_against_template_refuses_a_matching_name() {
        let naming = ChunkNaming {
            folder: Key::new(),
            prefix: "daily".to_string(),
            extension: "csv".to_string(),
        };
        let spec = ManifestSpec {
            chunks: vec![Recipe {
                query: "ns-sql/sql_query-0-1/daily_0042.csv".to_string(),
                ..Default::default()
            }],
            ..ManifestSpec::default()
        };
        assert!(spec.check_explicit_names_against_template(&naming).is_err());
    }

    #[test]
    fn check_unkeyed_chunk_arguments_refuses_arguments_on_a_filename_less_chunk() {
        let mut arguments = HashMap::new();
        arguments.insert("param".to_string(), serde_json::json!("value"));
        let spec = ManifestSpec {
            chunks: vec![Recipe {
                query: "ns-sql/sql_query-0-1000".to_string(), // no filename: unkeyed
                arguments,
                ..Default::default()
            }],
            ..ManifestSpec::default()
        };
        assert!(spec.check_unkeyed_chunk_arguments().is_err());
    }

    #[test]
    fn check_unkeyed_chunk_arguments_accepts_arguments_on_a_keyed_chunk() {
        let mut arguments = HashMap::new();
        arguments.insert("param".to_string(), serde_json::json!("value"));
        let spec = ManifestSpec {
            chunks: vec![Recipe {
                query: "ns-sql/sql_query-0-1000/data.csv".to_string(),
                arguments,
                ..Default::default()
            }],
            ..ManifestSpec::default()
        };
        assert!(spec.check_unkeyed_chunk_arguments().is_ok());
    }

    #[test]
    fn check_unkeyed_chunk_arguments_refuses_shared_arguments_with_an_unkeyed_chunk() {
        let mut arguments = HashMap::new();
        arguments.insert("shared".to_string(), serde_json::json!(1));
        let spec = ManifestSpec {
            arguments,
            chunks: vec![Recipe {
                query: "ns-sql/sql_query-0-1000".to_string(),
                ..Default::default()
            }],
            ..ManifestSpec::default()
        };
        assert!(spec.check_unkeyed_chunk_arguments().is_err());
    }

    #[test]
    fn check_explicit_names_against_template_accepts_a_non_matching_name() {
        let naming = ChunkNaming {
            folder: Key::new(),
            prefix: "daily".to_string(),
            extension: "csv".to_string(),
        };
        let spec = ManifestSpec {
            chunks: vec![Recipe {
                query: "ns-sql/sql_query-0-1/orders_eu.csv".to_string(),
                ..Default::default()
            }],
            ..ManifestSpec::default()
        };
        assert!(spec.check_explicit_names_against_template(&naming).is_ok());
    }

    // --- Deferred from Step 4.1: the five `ManifestSource` tests (Phase 3 §2.7 ×4, §9 ×1) ---
    //
    // These need `ManifestSource`, which lands in Step 4.2's `sources.rs`.

    #[test]
    fn manifest_source_new_refuses_colliding_explicit_chunk_names() {
        let spec = ManifestSpec {
            chunks: vec![
                Recipe {
                    query: "ns-sql/sql_query-0-1000/data.csv".to_string(),
                    ..Default::default()
                },
                Recipe {
                    query: "ns-sql/sql_query-1000-1000/data.csv".to_string(),
                    ..Default::default()
                },
            ],
            template: None,
            extension: Some("csv".to_string()),
            stored: true,
            cached: true,
            uniform_schema: None,
            ..ManifestSpec::default()
        };
        assert!(ManifestSource::new(spec, None).is_err());
    }

    #[test]
    fn manifest_source_with_key_refuses_per_chunk_arguments_on_an_unkeyed_chunk() {
        let mut arguments = HashMap::new();
        arguments.insert("param".to_string(), serde_json::json!("value"));
        let spec = ManifestSpec {
            chunks: vec![Recipe {
                query: "ns-sql/sql_query-0-1000".to_string(), // no filename: unkeyed
                arguments,
                ..Default::default()
            }],
            template: None,
            extension: Some("csv".to_string()),
            stored: true,
            cached: true,
            uniform_schema: None,
            ..ManifestSpec::default()
        };
        let source =
            ManifestSource::new(spec, None).expect("new validates only key-independent rules");
        assert!(source.with_key(Key::new()).is_err());
    }

    #[test]
    fn manifest_source_with_key_refuses_explicit_name_matching_template_pattern() {
        let spec = ManifestSpec {
            chunks: vec![Recipe {
                query: "ns-sql/sql_query-0-1/daily_0042.csv".to_string(),
                ..Default::default()
            }],
            template: Some(ChunkTemplate {
                query: "ns-sql/sql_query".to_string(),
                first_offset: 0,
                step: 1,
                batch_size: 1000,
            }),
            extension: Some("csv".to_string()),
            stored: true,
            cached: true,
            uniform_schema: None,
            ..ManifestSpec::default()
        };
        let source = ManifestSource::new(spec, None).expect("new");
        // The manifest's own filename becomes the template's prefix ("daily"), which is what
        // makes "daily_0042.csv" collide with the pattern once the key is known.
        let key = parse_key("data/sales/daily.manifest.yaml").expect("key");
        assert!(source.with_key(key).is_err());
    }

    #[test]
    fn manifest_source_without_key_keeps_chunks_identified_by_query() {
        // No key supplied (a manifest built by a command, never stored): a chunk with no
        // filename in its query stays unkeyed — identified by the query itself, never by a key
        // it was never given.
        //
        // Phase 3 §2.7's draft used `query: select 1`, which is not valid Liquers query syntax
        // (a bare space is not a separator the grammar accepts) and made `ManifestSource::new`
        // fail before `chunks()` could even be reached; corrected to a query shaped like the
        // rest of this file's fixtures.
        let yaml = "chunks:\n  - query: ns-sql/sql_query-0-1000\nstored: true\n";
        let spec: ManifestSpec = serde_yaml::from_str(yaml).expect("deserialize");
        let source = ManifestSource::new(spec, None).expect("new");
        match source.chunks() {
            ChunkList::Known(ids) => assert!(matches!(ids[0], ChunkId::Query(_))),
            ChunkList::Unbounded { .. } => panic!("expected Known: this manifest has no template"),
        }
    }

    // --- Phase 3 §9 (`liquers-records/tests/manifest_validation.rs`) — the fifth deferred test ---

    #[test]
    fn explicit_chunk_name_collisions_are_refused_at_load() {
        let spec = ManifestSpec {
            chunks: vec![
                Recipe {
                    query: "ns-sql/sql_query-0-1000/data.csv".to_string(),
                    ..Default::default()
                },
                Recipe {
                    query: "ns-sql/sql_query-1000-1000/data.csv".to_string(),
                    ..Default::default()
                },
            ],
            template: None,
            extension: Some("csv".to_string()),
            stored: true,
            cached: true,
            uniform_schema: None,
            ..ManifestSpec::default()
        };
        assert!(ManifestSource::new(spec, None).is_err());
    }

    // --- "Tests this plan adds": `manifest_spec_serializes_its_discriminator` ---

    #[test]
    fn manifest_spec_serializes_its_discriminator() -> Result<(), Box<dyn std::error::Error>> {
        let spec = ManifestSpec {
            chunks: vec![Recipe {
                query: "ns-sql/sql_query-0-1000".to_string(),
                ..Default::default()
            }],
            ..ManifestSpec::default()
        };
        let source = ManifestSource::new(spec, None)?;
        let yaml = serde_yaml::to_string(&source)?;
        assert!(yaml.starts_with("manifest: record-stream"));
        // The discriminator round-trips through `to_record_source`'s recognition: a plain
        // `serde_yaml::Value` sees the same `manifest: record-stream` field this crate wrote.
        let doc: serde_yaml::Value = serde_yaml::from_str(&yaml)?;
        assert_eq!(
            doc.get("manifest").and_then(|v| v.as_str()),
            Some("record-stream")
        );
        Ok(())
    }
}
