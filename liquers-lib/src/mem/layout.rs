//! The memory's schema: `<root>/_schema/memory.yaml`, read through the store on every call.

use indexmap::IndexMap;
use liquers_core::context::{EnvRef, Environment};
use liquers_core::error::{Error, ErrorType};
use liquers_core::parse::parse_key;
use liquers_core::query::Key;
use serde::{Deserialize, Serialize};

/// The newest `memory.yaml` format this service reads.
pub const SUPPORTED_VERSION: u32 = 1;

/// Where the schema lives, relative to the memory's root.
pub const SCHEMA_PATH: &str = "_schema/memory.yaml";

fn one() -> u32 {
    1
}
fn yes() -> bool {
    true
}
fn default_readme() -> String {
    "README.md".to_string()
}
fn default_log() -> String {
    "_log".to_string()
}
fn default_filename() -> String {
    "{id}.md".to_string()
}

/// `<root>/_schema/memory.yaml`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MemoryLayout {
    /// The schema format; a version newer than [`SUPPORTED_VERSION`] is refused.
    #[serde(default = "one")]
    pub version: u32,
    /// The memory's own README, relative to the root.
    #[serde(default = "default_readme")]
    pub readme: String,
    /// The folder of the write log, relative to the root.
    #[serde(default = "default_log")]
    pub log: String,
    /// Appended to the MCP server's instructions.
    #[serde(default)]
    pub instructions: String,
    /// The L0/L1 bridge for the whole memory.
    #[serde(default)]
    pub tiers: TierSources,
    #[serde(default)]
    pub collections: Vec<CollectionSchema>,
}

/// Where a document's L0 and L1 come from; the first non-empty source wins.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TierSources {
    #[serde(default = "default_l0")]
    pub l0: Vec<TierSource>,
    #[serde(default = "default_l1")]
    pub l1: Vec<TierSource>,
}

fn default_l0() -> Vec<TierSource> {
    vec![TierSource::Meta("title".to_string())]
}
fn default_l1() -> Vec<TierSource> {
    vec![TierSource::Meta("description".to_string())]
}

impl Default for TierSources {
    fn default() -> Self {
        TierSources { l0: default_l0(), l1: default_l1() }
    }
}

/// One source of a tier, written as `kind:arg` text.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum TierSource {
    /// `field:<name>` — a front-matter field.
    Field(String),
    /// `meta:<name>` — a field of the store's metadata record (`title`, `description`).
    Meta(String),
    /// `section:<heading>` — the first paragraph of that section.
    Section(String),
    /// `heading` — the first heading.
    Heading,
    /// `paragraph` — the first paragraph after the front-matter.
    Paragraph,
}

impl TryFrom<String> for TierSource {
    type Error = Error;
    fn try_from(text: String) -> Result<Self, Error> {
        let (kind, arg) = match text.split_once(':') {
            Some((kind, arg)) => (kind.trim(), Some(arg.trim().to_string())),
            None => (text.trim(), None),
        };
        match (kind, arg) {
            ("field", Some(arg)) if !arg.is_empty() => Ok(TierSource::Field(arg)),
            ("meta", Some(arg)) if !arg.is_empty() => Ok(TierSource::Meta(arg)),
            ("section", Some(arg)) if !arg.is_empty() => Ok(TierSource::Section(arg)),
            ("heading", None) => Ok(TierSource::Heading),
            ("paragraph", None) => Ok(TierSource::Paragraph),
            (_, _) => Err(Error::configuration_error(format!(
                "tier source '{text}' is not one of field:<name>, meta:<name>, section:<heading>, \
                 heading, paragraph"
            ))),
        }
    }
}

impl From<TierSource> for String {
    fn from(source: TierSource) -> String {
        match source {
            TierSource::Field(name) => format!("field:{name}"),
            TierSource::Meta(name) => format!("meta:{name}"),
            TierSource::Section(name) => format!("section:{name}"),
            TierSource::Heading => "heading".to_string(),
            TierSource::Paragraph => "paragraph".to_string(),
        }
    }
}

impl std::fmt::Display for TierSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", String::from(self.clone()))
    }
}

/// A collection: a folder of memories sharing fields, sections and rules.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CollectionSchema {
    pub name: String,
    /// Relative to the root.
    pub folder: String,
    /// The file name of a new memory; `{id}` is replaced.
    #[serde(default = "default_filename")]
    pub filename: String,
    /// A single-document collection (`profile.md`).
    #[serde(default)]
    pub file: Option<String>,
    /// Each sub-folder is one memory, this file its document (`DESIGN.md`).
    #[serde(default)]
    pub entry: Option<String>,
    /// Documents in sub-folders are memories too (`reference/api/…`).
    #[serde(default)]
    pub recursive: bool,
    #[serde(default = "yes")]
    pub writable: bool,
    #[serde(default)]
    pub id: IdRule,
    /// Key of a Markdown template, relative to the root; its body starts a new memory.
    #[serde(default)]
    pub template: Option<String>,
    /// In front-matter order.
    #[serde(default)]
    pub fields: IndexMap<String, FieldRule>,
    /// Headings every memory must have.
    #[serde(default)]
    pub sections: Vec<String>,
    #[serde(default)]
    pub history: Option<HistoryRule>,
    /// Named built-in check rules.
    #[serde(default)]
    pub checks: Vec<String>,
    /// Overrides the memory-wide bridge.
    #[serde(default)]
    pub tiers: Option<TierSources>,
    /// What the collection holds, for `describe` and `overview`.
    #[serde(default)]
    pub description: String,
}

/// A rule for one front-matter field.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FieldRule {
    #[serde(default)]
    pub required: bool,
    /// Filled when a memory is created.
    #[serde(default)]
    pub default: Option<serde_json::Value>,
    #[serde(default, rename = "enum")]
    pub values: Vec<String>,
    /// Key of a YAML list of allowed values, relative to the root.
    #[serde(default)]
    pub enum_from: Option<String>,
    #[serde(default)]
    pub list: bool,
    #[serde(default)]
    pub equals: Option<FieldEquals>,
    /// The collection a value must name a memory of.
    #[serde(default)]
    pub link: Option<String>,
    /// The field on the linked memory that must name this one back.
    #[serde(default)]
    pub reciprocal: Option<String>,
    /// Key of a YAML map `from: [to, …]`, relative to the root.
    #[serde(default)]
    pub transitions: Option<String>,
    #[serde(default, rename = "type")]
    pub field_type: FieldType,
}

/// What a field must equal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldEquals {
    /// The document's file name without its extension (or its folder name for an `entry`).
    FilenameStem,
}

/// The type of a field's value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    #[default]
    Text,
    /// `YYYY-MM-DD`.
    Date,
    /// A key relative to the root.
    Key,
}

/// The section rows are appended to and the field stamped when one is.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HistoryRule {
    pub section: String,
    pub stamp: String,
}

/// How a new memory's id is derived.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct IdRule {
    #[serde(default)]
    pub from: IdSource,
    #[serde(default)]
    pub case: IdCase,
    #[serde(default)]
    pub max_len: Option<usize>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdSource {
    #[default]
    Title,
    Date,
    DateTitle,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdCase {
    #[default]
    ScreamingKebab,
    Kebab,
    AsIs,
}

/// `rel`, a `/`-separated path relative to `root`, as a key under it. Empty: `root` itself.
pub fn join_path(root: &Key, rel: &str) -> Result<Key, Error> {
    let rel = rel.trim_matches('/');
    if rel.is_empty() || rel == "." {
        return Ok(root.clone());
    }
    let parsed = parse_key(rel)?;
    if parsed.is_relative() {
        return Err(Error::general_error(format!(
            "path '{rel}' must not contain '.' or '..'"
        )));
    }
    Ok(parsed.iter().fold(root.clone(), |key, name| key.join(&name.name)))
}

/// The names of `key` below `root`, or `None` when `key` is not under it.
pub fn relative_names(root: &Key, key: &Key) -> Option<Vec<String>> {
    if !key.has_key_prefix(root) {
        return None;
    }
    Some(key.iter().skip(root.len()).map(|name| name.name.clone()).collect())
}

/// `key` relative to `root`, as text.
pub fn relative_path(root: &Key, key: &Key) -> String {
    match relative_names(root, key) {
        Some(names) => names.join("/"),
        None => key.encode(),
    }
}

impl MemoryLayout {
    /// Parses `memory.yaml`, refusing a format newer than this service reads.
    pub fn parse(text: &str) -> Result<Self, Error> {
        #[derive(Deserialize)]
        struct VersionOnly {
            #[serde(default = "one")]
            version: u32,
        }
        let version = serde_yaml::from_str::<Option<VersionOnly>>(text)
            .map_err(|e| Error::configuration_error(format!("{SCHEMA_PATH}: {e}")))?
            .map(|v| v.version)
            .unwrap_or(1);
        if version > SUPPORTED_VERSION {
            return Err(Error::configuration_error(format!(
                "{SCHEMA_PATH} has version {version}, but this service reads version \
                 {SUPPORTED_VERSION} at most; upgrade the service"
            )));
        }
        serde_yaml::from_str::<Option<MemoryLayout>>(text)
            .map_err(|e| Error::configuration_error(format!("{SCHEMA_PATH}: {e}")))?
            .ok_or_else(|| Error::configuration_error(format!("{SCHEMA_PATH} is empty")))
    }

    /// Reads `<root>/_schema/memory.yaml` through the store.
    pub async fn load<E: Environment>(envref: &EnvRef<E>, root: &Key) -> Result<Self, Error> {
        let key = join_path(root, SCHEMA_PATH)?;
        let bytes = read_bytes(envref, &key).await?.ok_or_else(|| {
            Error::configuration_error(format!(
                "no memory schema at '{}': a memory root holds {SCHEMA_PATH}",
                key.encode()
            ))
        })?;
        let text = String::from_utf8(bytes)
            .map_err(|_| Error::configuration_error(format!("'{}' is not UTF-8", key.encode())))?;
        Self::parse(&text)
    }

    /// The collection whose folder is the longest prefix of `key`.
    pub fn collection_for(&self, root: &Key, key: &Key) -> Option<&CollectionSchema> {
        let names = relative_names(root, key)?;
        self.collections
            .iter()
            .filter_map(|collection| {
                let folder = folder_names(&collection.folder);
                let matches = names.len() > folder.len() && names[..folder.len()] == folder[..];
                let matches = matches
                    && match &collection.file {
                        Some(file) => names.len() == folder.len() + 1 && &names[folder.len()] == file,
                        None => true,
                    };
                matches.then_some((folder.len(), collection))
            })
            .max_by_key(|(len, _)| *len)
            .map(|(_, collection)| collection)
    }

    /// The collection named `name`.
    pub fn collection(&self, name: &str) -> Result<&CollectionSchema, Error> {
        self.collections.iter().find(|c| c.name == name).ok_or_else(|| {
            let names: Vec<&str> = self.collections.iter().map(|c| c.name.as_str()).collect();
            Error::general_error(format!(
                "no collection '{name}'; the collections are: {}",
                names.join(", ")
            ))
        })
    }

    /// The tier sources of a collection, else the memory's.
    pub fn tiers_for<'a>(&'a self, collection: Option<&'a CollectionSchema>) -> &'a TierSources {
        collection.and_then(|c| c.tiers.as_ref()).unwrap_or(&self.tiers)
    }
}

fn folder_names(folder: &str) -> Vec<String> {
    folder
        .split('/')
        .filter(|name| !name.is_empty() && *name != ".")
        .map(str::to_string)
        .collect()
}

impl CollectionSchema {
    /// Fills every field that has a `default` and no value yet.
    pub fn apply_defaults(&self, fields: &mut IndexMap<String, serde_json::Value>) {
        for (name, rule) in &self.fields {
            if let Some(default) = &rule.default {
                let missing = match fields.get(name) {
                    None | Some(serde_json::Value::Null) => true,
                    Some(serde_json::Value::String(s)) => s.is_empty(),
                    Some(_) => false,
                };
                if missing {
                    fields.insert(name.clone(), default.clone());
                }
            }
        }
    }

    /// The key a memory with this id is written at.
    pub fn key_for_id(&self, root: &Key, id: &str) -> Result<Key, Error> {
        check_id(id)?;
        let folder = join_path(root, &self.folder)?;
        if let Some(file) = &self.file {
            return Ok(folder.join(file));
        }
        let name = self.filename.replace("{id}", id);
        match &self.entry {
            Some(entry) => Ok(folder.join(id).join(entry)),
            None => Ok(folder.join(name)),
        }
    }

    /// The id a memory at `key` has: its file stem, or its folder name for an `entry`.
    pub fn id_of(&self, key: &Key) -> String {
        match &self.entry {
            Some(_) => key.parent().filename().map(|n| n.name.clone()).unwrap_or_default(),
            None => {
                let name = key.filename().map(|n| n.name.clone()).unwrap_or_default();
                match name.rfind('.') {
                    Some(dot) if dot > 0 => name[..dot].to_string(),
                    Some(_) | None => name,
                }
            }
        }
    }

    /// Whether `key` is a memory of this collection rather than another file in its folder.
    pub fn is_memory(&self, root: &Key, key: &Key) -> bool {
        let Some(names) = relative_names(root, key) else { return false };
        let folder = folder_names(&self.folder);
        if names.len() <= folder.len() || names[..folder.len()] != folder[..] {
            return false;
        }
        let rest = &names[folder.len()..];
        match (&self.file, &self.entry) {
            (Some(file), _) => rest.len() == 1 && &rest[0] == file,
            (None, Some(entry)) => rest.len() == 2 && &rest[1] == entry,
            (None, None) => {
                let last = &rest[rest.len() - 1];
                (rest.len() == 1 || self.recursive) && last.ends_with(".md") && !is_readme(last)
            }
        }
    }

    /// Derives an id from a title and a date (`YYYY-MM-DD`).
    pub fn derive_id(&self, title: &str, date: &str) -> String {
        let base = match self.id.from {
            IdSource::Title => title.to_string(),
            IdSource::Date => date.to_string(),
            IdSource::DateTitle => format!("{date} {title}"),
        };
        let mut id = match self.id.case {
            IdCase::AsIs => base.trim().to_string(),
            IdCase::ScreamingKebab => kebab(&base).to_uppercase(),
            IdCase::Kebab => kebab(&base),
        };
        if let Some(max) = self.id.max_len {
            if id.chars().count() > max {
                id = id.chars().take(max).collect();
                while id.ends_with('-') {
                    id.pop();
                }
            }
        }
        id
    }
}

/// Refuses an id that would not be one key element: empty, `.` or `..`, or containing a path
/// separator, which a file store would read as a folder (and could leave the collection by).
pub fn check_id(id: &str) -> Result<(), Error> {
    if id.is_empty() || id == "." || id == ".." || id.contains(['/', '\\']) {
        return Err(Error::general_error(format!(
            "id {id:?} is not a valid file name: it must be non-empty, not . or .., and contain no / or \\"
        )));
    }
    Ok(())
}

/// Lower-case ASCII words joined by `-`; other characters are transliterated where they have an
/// obvious ASCII letter, and dropped otherwise (`RESOURCE-NAME-ASCII-ONLY`).
fn kebab(text: &str) -> String {
    let mut out = String::new();
    let mut gap = false;
    for c in text.chars() {
        let c = ascii_of(c);
        match c {
            Some(c) if c.is_ascii_alphanumeric() => {
                if gap && !out.is_empty() {
                    out.push('-');
                }
                gap = false;
                out.push(c.to_ascii_lowercase());
            }
            Some(_) | None => gap = true,
        }
    }
    out
}

fn ascii_of(c: char) -> Option<char> {
    if c.is_ascii() {
        return Some(c);
    }
    let mapped = match c {
        'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' | 'Á' | 'À' | 'Â' | 'Ä' | 'Ã' | 'Å' => 'a',
        'č' | 'ç' | 'Č' | 'Ç' => 'c',
        'ď' | 'Ď' => 'd',
        'é' | 'è' | 'ê' | 'ë' | 'ě' | 'É' | 'È' | 'Ê' | 'Ë' | 'Ě' => 'e',
        'í' | 'ì' | 'î' | 'ï' | 'Í' | 'Ì' | 'Î' | 'Ï' => 'i',
        'ľ' | 'ĺ' | 'Ľ' | 'Ĺ' => 'l',
        'ň' | 'ñ' | 'Ň' | 'Ñ' => 'n',
        'ó' | 'ò' | 'ô' | 'ö' | 'õ' | 'Ó' | 'Ò' | 'Ô' | 'Ö' | 'Õ' => 'o',
        'ř' | 'ŕ' | 'Ř' | 'Ŕ' => 'r',
        'š' | 'Š' => 's',
        'ť' | 'Ť' => 't',
        'ú' | 'ù' | 'û' | 'ü' | 'ů' | 'Ú' | 'Ù' | 'Û' | 'Ü' | 'Ů' => 'u',
        'ý' | 'Ý' => 'y',
        'ž' | 'Ž' => 'z',
        _ => return None,
    };
    Some(mapped)
}

/// Whether a file name is a folder's README.
pub fn is_readme(name: &str) -> bool {
    name.eq_ignore_ascii_case("README.md")
}

/// The stored bytes of `key`, or `None` when it does not exist. Read from the store directly,
/// never from a live asset: the schema and the write path need what is on disk now.
pub(crate) async fn read_bytes<E: Environment>(
    envref: &EnvRef<E>,
    key: &Key,
) -> Result<Option<Vec<u8>>, Error> {
    let store = envref.get_async_store();
    match store.get_bytes(key).await {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.error_type == ErrorType::KeyNotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// A YAML file under `root` read as `T`; used for `enum_from` lists and `transitions` maps.
pub(crate) async fn load_yaml<E: Environment, T: serde::de::DeserializeOwned>(
    envref: &EnvRef<E>,
    root: &Key,
    rel: &str,
) -> Result<T, Error> {
    let key = join_path(root, rel)?;
    let bytes = read_bytes(envref, &key)
        .await?
        .ok_or_else(|| Error::configuration_error(format!("'{}' does not exist", key.encode())))?;
    serde_yaml::from_slice(&bytes)
        .map_err(|e| Error::configuration_error(format!("'{}': {e}", key.encode())))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAYOUT: &str = r#"
collections:
  - name: issues
    folder: specs/issues
    fields:
      title: { required: true }
      status: { default: draft, enum: [draft, accepted, closed] }
  - name: profile
    folder: user/memories
    file: profile.md
  - name: designs
    folder: specs/design
    entry: DESIGN.md
"#;

    #[test]
    fn field_default_fills_status_draft() -> Result<(), Error> {
        let layout = MemoryLayout::parse(LAYOUT)?;
        let issues = layout.collection("issues")?;
        let mut fields = IndexMap::new();
        fields.insert("title".to_string(), serde_json::json!("X"));
        issues.apply_defaults(&mut fields);
        assert_eq!(fields.get("status"), Some(&serde_json::json!("draft")));
        let mut given = IndexMap::new();
        given.insert("status".to_string(), serde_json::json!("accepted"));
        issues.apply_defaults(&mut given);
        assert_eq!(given.get("status"), Some(&serde_json::json!("accepted")));
        Ok(())
    }

    #[test]
    fn schema_version_newer_than_supported_is_refused() -> Result<(), Error> {
        let error = MemoryLayout::parse("version: 2\ncollections: []\n").err().ok_or_else(|| {
            Error::general_error("a newer version was accepted".to_string())
        })?;
        assert_eq!(error.error_type, ErrorType::ConfigurationError);
        assert!(error.message.contains("version 2") && error.message.contains("version 1"));
        assert_eq!(MemoryLayout::parse("collections: []\n")?.version, 1);
        Ok(())
    }

    #[test]
    fn tier_sources_read_kind_and_argument() -> Result<(), Error> {
        let tiers: TierSources =
            serde_yaml::from_str("l0: [field:l0, heading]\nl1: ['section:Problem', paragraph]")
                .map_err(|e| Error::general_error(e.to_string()))?;
        assert_eq!(tiers.l0, vec![TierSource::Field("l0".into()), TierSource::Heading]);
        assert_eq!(
            tiers.l1,
            vec![TierSource::Section("Problem".into()), TierSource::Paragraph]
        );
        assert!(serde_yaml::from_str::<TierSources>("l0: [bogus]").is_err());
        Ok(())
    }

    #[test]
    fn collection_for_takes_the_longest_folder() -> Result<(), Error> {
        let layout = MemoryLayout::parse(LAYOUT)?;
        let root = Key::new();
        let issue = parse_key("specs/issues/X.md")?;
        assert_eq!(layout.collection_for(&root, &issue).map(|c| c.name.as_str()), Some("issues"));
        let profile = parse_key("user/memories/profile.md")?;
        assert_eq!(layout.collection_for(&root, &profile).map(|c| c.name.as_str()), Some("profile"));
        assert!(layout.collection_for(&root, &parse_key("user/memories/other.md")?).is_none());
        let design = layout.collection("designs")?;
        let key = design.key_for_id(&root, "x")?;
        assert_eq!(key.encode(), "specs/design/x/DESIGN.md");
        assert_eq!(design.id_of(&key), "x");
        assert!(design.is_memory(&root, &key));
        Ok(())
    }

    #[test]
    fn ids_are_derived_in_the_declared_case() -> Result<(), Error> {
        let layout = MemoryLayout::parse(LAYOUT)?;
        let issues = layout.collection("issues")?.clone();
        assert_eq!(issues.derive_id("Store write: no précondition!", ""), "STORE-WRITE-NO-PRECONDITION");
        let mut events = issues.clone();
        events.id = IdRule { from: IdSource::DateTitle, case: IdCase::Kebab, max_len: Some(20) };
        assert_eq!(events.derive_id("Met Alice at the café", "2026-10-19"), "2026-10-19-met-alice");
        let mut as_is = issues.clone();
        as_is.id = IdRule { from: IdSource::Title, case: IdCase::AsIs, max_len: Some(1) };
        assert_eq!(as_is.derive_id("été", ""), "é");
        Ok(())
    }

    #[test]
    fn ids_that_are_not_one_key_element_are_refused() -> Result<(), Error> {
        let layout = MemoryLayout::parse(LAYOUT)?;
        let issues = layout.collection("issues")?;
        let designs = layout.collection("designs")?;
        for id in ["../../outside", "a/b", "a\\b", "..", ".", ""] {
            assert!(issues.key_for_id(&Key::new(), id).is_err(), "{id:?}");
            assert!(designs.key_for_id(&Key::new(), id).is_err(), "{id:?}");
        }
        assert_eq!(issues.key_for_id(&Key::new(), "X")?.encode(), "specs/issues/X.md");
        Ok(())
    }
}
