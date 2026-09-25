//! `RecordSchema` and the field-level types it is built from.
//!
//! A schema carries two independent axes for each field — a logical [`FieldType`] (what Arrow,
//! polars and GlueSQL need) and a [`FieldRole`] (what a search engine needs) — plus a structural
//! [`KeyRole`] that has nothing to do with indexing. See
//! `specs/design/record-streams/phase2-architecture.md`, §"RecordSchema — three axes, because two
//! were not enough", §"The identity field, specifically" and §"Field resolution: qualified names,
//! ambiguity is an error".

use liquers_core::error::Error;
use serde::{Deserialize, Serialize};

/// `true`, as a named function so `#[serde(default = "...")]` can call it.
fn true_default() -> bool {
    true
}

/// `name.replace("_", " ")` — the label a field gets when none is given, the same formula
/// `ArgumentInfo`'s constructors use (`command_metadata.rs`), so a field and an argument read as
/// one system. See phase2-architecture.md §"Alignment with `ArgumentInfo`".
fn default_label(name: &str) -> String {
    name.replace('_', " ")
}

/// The logical type. Maps one-to-one onto the `Column` variants (added in a later step) and onto
/// Arrow's `DataType`. Ignored by anything that only cares about indexing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FieldType {
    Bool,
    Int,
    UInt,
    Float,
    Text,
    Binary,
    Date,
    Timestamp,
    Vector,
}

/// A field's structural role in a `RecordBatch` — nothing to do with indexing.
///
/// `Id` and `Source` are each **at most one per schema**, enforced by [`RecordSchema::new`].
/// Without an explicit `Id`, the implicit `RowId` identifies rows instead (phase2-architecture.md
/// §"Every row has an implicit id").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum KeyRole {
    /// The row's identity. **At most one per schema.**
    Id,
    /// Index into `RecordBatch`'s origin dictionary. At most one.
    Source,
    /// Ordinary data.
    #[default]
    None,
}

/// Portable analyzer intent. An engine maps it to its own tokenizer; `Named` is the escape hatch
/// for one this vocabulary cannot describe.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Analyzer {
    Raw,
    Simple,
    Stemming { language: String },
    Named(String),
}

/// Vector similarity metric, as Qdrant and other vector engines require at collection creation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VectorMetric {
    Cosine,
    Dot,
    Euclidean,
}

/// A single access path a field affords to a search index. **Plural** on [`FieldRole`] — one
/// field routinely has several (a B-tree *and* a trigram index in Postgres, a `text` field with a
/// `keyword` sub-field in Elasticsearch). See phase2-architecture.md
/// §"Access paths, not data structures — and what that excludes".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum IndexKind {
    /// Not tokenized: exact term match and faceting.
    Exact,
    /// Matching *inside* a token: `LIKE '%x%'`, trigram, fuzzy.
    Substring,
    /// Tokenized. `positions` is **required for phrase queries**.
    FullText { analyzer: Analyzer, positions: bool },
    /// Ordered comparisons.
    Range,
    /// Vector similarity.
    Similarity { metric: VectorMetric },
}

/// The access paths a field **affords** to an index. An engine takes the subset it can serve and
/// reports the rest as declined; nothing here is a command a field issues to an engine.
///
/// Constructors keep the common cases to one call — [`FieldRole::text`], [`FieldRole::keyword`],
/// [`FieldRole::stored_only`], [`FieldRole::numeric`], [`FieldRole::vector`],
/// [`FieldRole::ignored`] — composing with [`FieldRole::and_stored`] and [`FieldRole::and_fast`].
/// `FieldRole::text().and_stored()` is the searchable-and-displayable title case that motivated
/// the whole design (phase2-architecture.md §"RecordSchema — three axes").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct FieldRole {
    /// How the field can be searched. Empty — not searchable at all.
    pub indexed: Vec<IndexKind>,
    /// Retrievable from the engine. Lucene `stored`, Tantivy `STORED`, Qdrant payload.
    pub stored: bool,
    /// Available for sorting, faceting and cheap scans. Lucene docValues, Tantivy `FAST`.
    pub fast: bool,
}

impl FieldRole {
    /// Full-text search with phrase support (`positions: true`), a plain (non-stemming)
    /// tokenizer. Not stored, not fast — compose with [`FieldRole::and_stored`] for the common
    /// "searchable and displayable" case.
    pub fn text() -> Self {
        FieldRole {
            indexed: vec![IndexKind::FullText {
                analyzer: Analyzer::Simple,
                positions: true,
            }],
            stored: false,
            fast: false,
        }
    }

    /// Exact-match term search — a category, a tag, a status. Not stored, not fast.
    pub fn keyword() -> Self {
        FieldRole {
            indexed: vec![IndexKind::Exact],
            stored: false,
            fast: false,
        }
    }

    /// Retrievable, but not searchable at all.
    pub fn stored_only() -> Self {
        FieldRole {
            indexed: Vec::new(),
            stored: true,
            fast: false,
        }
    }

    /// Ordered comparisons — the numeric analogue of a B-tree. Not stored, not fast.
    pub fn numeric() -> Self {
        FieldRole {
            indexed: vec![IndexKind::Range],
            stored: false,
            fast: false,
        }
    }

    /// Vector similarity search under the given metric. Not stored, not fast.
    pub fn vector(metric: VectorMetric) -> Self {
        FieldRole {
            indexed: vec![IndexKind::Similarity { metric }],
            stored: false,
            fast: false,
        }
    }

    /// Not searchable, not retrievable, not fast — identical to [`FieldRole::default`]. Named
    /// separately for readability at a call site (`.with_role(FieldRole::ignored())`).
    ///
    /// Because it is the default value, [`RecordSchema::new`] treats a `KeyRole::Id` field left
    /// at `ignored()` as "left at default" and **supplies** the Exact-indexed, stored role it
    /// requires, rather than rejecting it — see phase2-architecture.md §"The identity field,
    /// specifically".
    pub fn ignored() -> Self {
        FieldRole::default()
    }

    /// Adds retrievability to whatever access paths are already set.
    pub fn and_stored(mut self) -> Self {
        self.stored = true;
        self
    }

    /// Adds fast (sortable/facetable) access to whatever access paths are already set.
    pub fn and_fast(mut self) -> Self {
        self.fast = true;
        self
    }
}

/// A column of a `RecordSchema`: a name, a logical type, a structural [`KeyRole`] and a
/// [`FieldRole`] declaring what an index may do with it.
///
/// Deserializes leniently for hand-authored schemas (YAML manifests): `nullable` defaults to
/// `true`, `key` to [`KeyRole::None`], `role` to [`FieldRole::default`], and an omitted `label`
/// defaults from `name` the way [`FieldSchema::new`] does.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(from = "FieldSchemaSpec")]
pub struct FieldSchema {
    /// Unique identifier, conventionally snake_case. What a predicate and a consumer match on.
    pub name: String,
    /// Human-readable, for table headers in documents, reports and dashboards. Defaults from
    /// `name` the same way `ArgumentInfo` does — `name.replace("_", " ")`.
    pub label: String,
    /// What this field means. A data dictionary entry, a column tooltip.
    pub description: String,
    pub data_type: FieldType,
    /// Default `true` — what a hand-written schema means by leaving it out.
    pub nullable: bool,
    /// Structural role in the batch — nothing to do with indexing.
    pub key: KeyRole,
    /// What an index should do with this field. Portable *intent*; engines translate it.
    pub role: FieldRole,
}

/// The wire shape `FieldSchema` deserializes through, so an omitted `label` can be filled in from
/// `name` — a plain `#[serde(default = "...")]` cannot reach across fields, so this needs its own
/// (infallible) conversion rather than a derived `Deserialize` on `FieldSchema` itself.
#[derive(Deserialize)]
struct FieldSchemaSpec {
    name: String,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    description: String,
    data_type: FieldType,
    #[serde(default = "true_default")]
    nullable: bool,
    #[serde(default)]
    key: KeyRole,
    #[serde(default)]
    role: FieldRole,
}

impl From<FieldSchemaSpec> for FieldSchema {
    fn from(spec: FieldSchemaSpec) -> Self {
        let label = spec.label.unwrap_or_else(|| default_label(&spec.name));
        FieldSchema {
            name: spec.name,
            label,
            description: spec.description,
            data_type: spec.data_type,
            nullable: spec.nullable,
            key: spec.key,
            role: spec.role,
        }
    }
}

impl FieldSchema {
    /// Nullable, [`KeyRole::None`], the default role, and `label` = `name` with `_` → space.
    pub fn new(name: impl Into<String>, data_type: FieldType) -> Self {
        let name = name.into();
        let label = default_label(&name);
        FieldSchema {
            name,
            label,
            description: String::new(),
            data_type,
            nullable: true,
            key: KeyRole::None,
            role: FieldRole::default(),
        }
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    pub fn with_key(mut self, key: KeyRole) -> Self {
        self.key = key;
        self
    }

    pub fn with_role(mut self, role: FieldRole) -> Self {
        self.role = role;
        self
    }

    /// Sets `nullable` to `false`.
    pub fn not_null(mut self) -> Self {
        self.nullable = false;
        self
    }
}

/// The recognized name qualifiers a projected field may carry, per phase2-architecture.md
/// §"Field resolution: qualified names, ambiguity is an error". `meta.status` is the asset
/// lifecycle, `key.status` (say) would be something else entirely — qualification is what makes
/// a name arriving from HTTP or MCP unambiguous.
const QUALIFIERS: [&str; 3] = ["meta.", "attr.", "key."];

/// A record's columns, plus the type identity of the thing the rows describe (when there is one).
///
/// Two structural invariants are checked once, in [`RecordSchema::new`], rather than documented
/// and hoped for: at most one field declares [`KeyRole::Id`], at most one declares
/// [`KeyRole::Source`], and the `Id` field's [`FieldRole`] — needed for delete-by-term
/// reconciliation — is Exact-indexed and stored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RecordSchemaSpec", into = "RecordSchemaSpec")]
pub struct RecordSchema {
    pub fields: Vec<FieldSchema>,
    /// Liquers' own type identity for the thing the rows describe, when there is one.
    pub type_identifier: Option<String>,
    /// Positions of fields whose `role.indexed` includes `FullText` — what `text_fields()`
    /// borrows. Computed once in `new` rather than rescanned on every call.
    text_field_indices: Vec<usize>,
}

/// The wire shape `RecordSchema` (de)serializes through, so a hand-authored schema — loaded
/// straight from YAML, not built via [`RecordSchema::new`] — still has its invariants checked and
/// its field-lookup cache populated. Mirrors the `ManifestSpec`/`ManifestSource` pattern used
/// elsewhere in this crate.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct RecordSchemaSpec {
    fields: Vec<FieldSchema>,
    #[serde(default)]
    type_identifier: Option<String>,
}

impl From<RecordSchema> for RecordSchemaSpec {
    fn from(schema: RecordSchema) -> Self {
        RecordSchemaSpec {
            fields: schema.fields,
            type_identifier: schema.type_identifier,
        }
    }
}

impl TryFrom<RecordSchemaSpec> for RecordSchema {
    type Error = Error;

    fn try_from(spec: RecordSchemaSpec) -> Result<Self, Error> {
        let mut schema = RecordSchema::new(spec.fields)?;
        schema.type_identifier = spec.type_identifier;
        Ok(schema)
    }
}

impl RecordSchema {
    /// Fails when more than one field has `KeyRole::Id`, when that field's role contradicts
    /// `Exact`-indexed and stored (supplied when left at [`FieldRole::default`]), or when more
    /// than one field has `KeyRole::Source`. Checked once per schema rather than per row.
    pub fn new(fields: Vec<FieldSchema>) -> Result<Self, Error> {
        let id_positions: Vec<usize> = fields
            .iter()
            .enumerate()
            .filter(|(_, field)| field.key == KeyRole::Id)
            .map(|(index, _)| index)
            .collect();
        if id_positions.len() > 1 {
            let names: Vec<&str> = id_positions
                .iter()
                .map(|&i| fields[i].name.as_str())
                .collect();
            return Err(Error::general_error(format!(
                "RecordSchema: more than one field declares KeyRole::Id: {}",
                names.join(", ")
            )));
        }

        let source_positions: Vec<usize> = fields
            .iter()
            .enumerate()
            .filter(|(_, field)| field.key == KeyRole::Source)
            .map(|(index, _)| index)
            .collect();
        if source_positions.len() > 1 {
            let names: Vec<&str> = source_positions
                .iter()
                .map(|&i| fields[i].name.as_str())
                .collect();
            return Err(Error::general_error(format!(
                "RecordSchema: more than one field declares KeyRole::Source: {}",
                names.join(", ")
            )));
        }

        let mut fields = fields;
        if let Some(&id_index) = id_positions.first() {
            let role = fields[id_index].role.clone();
            let has_exact = role.indexed.contains(&IndexKind::Exact);
            if has_exact && role.stored {
                // Already satisfies delete-by-term reconciliation's requirement.
            } else if role == FieldRole::default() {
                // Left at default: supply the role the `Id` field requires rather than reject it.
                fields[id_index].role = FieldRole {
                    indexed: vec![IndexKind::Exact],
                    stored: true,
                    fast: role.fast,
                };
            } else {
                return Err(Error::general_error(format!(
                    "RecordSchema: field '{}' has KeyRole::Id but its role ({:?}) is neither \
                     Exact-indexed and stored, nor left at FieldRole::default() to have that \
                     role supplied — delete-by-term reconciliation requires an Exact-indexed, \
                     stored identity field",
                    fields[id_index].name, role
                )));
            }
        }

        let text_field_indices: Vec<usize> = fields
            .iter()
            .enumerate()
            .filter(|(_, field)| {
                field
                    .role
                    .indexed
                    .iter()
                    .any(|kind| matches!(kind, IndexKind::FullText { .. }))
            })
            .map(|(index, _)| index)
            .collect();

        Ok(RecordSchema {
            fields,
            type_identifier: None,
            text_field_indices,
        })
    }

    /// Index into `RecordBatch`'s origin dictionary, when a field declares `KeyRole::Source`.
    pub fn source_field(&self) -> Option<usize> {
        self.fields
            .iter()
            .position(|field| field.key == KeyRole::Source)
    }

    /// Columns whose `indexed` includes `FullText` — what an unqualified text query matches.
    pub fn text_fields(&self) -> &[usize] {
        &self.text_field_indices
    }

    /// Exact-name lookup. Does not expand a bare name against the `meta.`/`attr.`/`key.`
    /// qualifiers — see [`RecordSchema::resolve_field`] for that.
    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.fields.iter().position(|field| field.name == name)
    }

    /// Resolves a field reference the way a consumer's parser expands an unqualified name
    /// (phase2-architecture.md §"Field resolution: qualified names, ambiguity is an error"). An
    /// exact match on `name` wins outright — the common case, since most fields carry no
    /// qualifier. Failing that, `name` is tried under each recognized qualifier (`meta.`,
    /// `attr.`, `key.`); more than one candidate is an error naming every one of them, because an
    /// unqualified name arriving from HTTP or MCP must resolve to exactly one field or be
    /// rejected — never guessed.
    pub fn resolve_field(&self, name: &str) -> Result<Option<usize>, Error> {
        if let Some(index) = self.index_of(name) {
            return Ok(Some(index));
        }
        let candidates: Vec<usize> = QUALIFIERS
            .iter()
            .filter_map(|qualifier| self.index_of(&format!("{qualifier}{name}")))
            .collect();
        match candidates.len() {
            0 => Ok(None),
            1 => Ok(Some(candidates[0])),
            _ => {
                let names: Vec<&str> = candidates
                    .iter()
                    .map(|&index| self.fields[index].name.as_str())
                    .collect();
                Err(Error::general_error(format!(
                    "RecordSchema: field '{}' is ambiguous among {}",
                    name,
                    names.join(", ")
                )))
            }
        }
    }

    /// Fields whose `KeyRole` is neither `Id` nor `Source` — what scalar reading counts.
    pub fn payload_fields(&self) -> Vec<usize> {
        self.fields
            .iter()
            .enumerate()
            .filter(|(_, field)| match field.key {
                KeyRole::Id => false,
                KeyRole::Source => false,
                KeyRole::None => true,
            })
            .map(|(index, _)| index)
            .collect()
    }

    /// The declared `Id` field, when there is one.
    pub fn id_field(&self) -> Option<usize> {
        self.fields
            .iter()
            .position(|field| field.key == KeyRole::Id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_new_single_id_field_succeeds() {
        let fields = vec![
            FieldSchema::new("id", FieldType::Text).with_key(KeyRole::Id),
            FieldSchema::new("name", FieldType::Text),
        ];
        let schema = RecordSchema::new(fields).expect("single id field");
        assert_eq!(schema.id_field(), Some(0));
    }

    #[test]
    fn schema_new_multiple_id_fields_refused() {
        let fields = vec![
            FieldSchema::new("id1", FieldType::Text).with_key(KeyRole::Id),
            FieldSchema::new("id2", FieldType::Text).with_key(KeyRole::Id),
        ];
        assert!(RecordSchema::new(fields).is_err());
    }

    #[test]
    fn schema_new_id_without_exact_index_is_supplied_not_rejected() {
        // The Id field's implied role (Exact-indexed, stored) is supplied by `new` when left at
        // the default — only a *contradicting* role is rejected.
        let fields = vec![FieldSchema::new("id", FieldType::Text).with_key(KeyRole::Id)];
        let schema = RecordSchema::new(fields).expect("default role is supplied, not rejected");
        assert_eq!(schema.id_field(), Some(0));
    }

    #[test]
    fn schema_new_id_with_contradicting_role_refused() {
        // CORRECTED from Phase 3 §2.1: the original test used `FieldRole::ignored()`, but
        // `ignored()` is defined to equal `FieldRole::default()` (phase2-architecture.md §"The
        // identity field, specifically" — "supplied when left default"), so `RecordSchema::new`
        // auto-supplies the Id role for it instead of rejecting it; `is_err()` would fail against
        // the real API. `FieldRole::numeric()` is a genuinely non-default role (Range-indexed)
        // that still lacks Exact-indexing, so it is a real contradiction rather than an
        // unconfigured default.
        let fields = vec![FieldSchema::new("id", FieldType::Text)
            .with_key(KeyRole::Id)
            .with_role(FieldRole::numeric())]; // Range-indexed, not stored — contradicts Id
        assert!(RecordSchema::new(fields).is_err());
    }

    #[test]
    fn schema_new_single_source_field_succeeds() {
        let fields = vec![
            FieldSchema::new("source_idx", FieldType::UInt).with_key(KeyRole::Source),
            FieldSchema::new("data", FieldType::Text),
        ];
        let schema = RecordSchema::new(fields).expect("single source field");
        assert_eq!(schema.source_field(), Some(0));
    }

    #[test]
    fn schema_new_multiple_source_fields_refused() {
        let fields = vec![
            FieldSchema::new("src1", FieldType::UInt).with_key(KeyRole::Source),
            FieldSchema::new("src2", FieldType::UInt).with_key(KeyRole::Source),
        ];
        assert!(RecordSchema::new(fields).is_err());
    }

    #[test]
    fn payload_fields_excludes_id_and_source() {
        let fields = vec![
            FieldSchema::new("id", FieldType::Text).with_key(KeyRole::Id),
            FieldSchema::new("value", FieldType::Int),
            FieldSchema::new("source_idx", FieldType::UInt).with_key(KeyRole::Source),
        ];
        let schema = RecordSchema::new(fields).expect("schema");
        assert_eq!(schema.payload_fields(), vec![1]);
    }

    #[test]
    fn id_field_returns_none_when_not_declared() {
        let fields = vec![
            FieldSchema::new("value", FieldType::Int),
            FieldSchema::new("name", FieldType::Text),
        ];
        let schema = RecordSchema::new(fields).expect("schema");
        assert_eq!(schema.id_field(), None);
    }

    #[test]
    fn index_of_finds_field_by_name() {
        let fields = vec![
            FieldSchema::new("id", FieldType::Text),
            FieldSchema::new("amount", FieldType::Float),
            FieldSchema::new("date", FieldType::Date),
        ];
        let schema = RecordSchema::new(fields).expect("schema");
        assert_eq!(schema.index_of("id"), Some(0));
        assert_eq!(schema.index_of("amount"), Some(1));
        assert_eq!(schema.index_of("date"), Some(2));
        assert_eq!(schema.index_of("missing"), None);
    }

    #[test]
    fn text_fields_returns_fulltext_indexed_columns_only() {
        let fields = vec![
            FieldSchema::new("title", FieldType::Text).with_role(FieldRole {
                indexed: vec![IndexKind::FullText {
                    analyzer: Analyzer::Simple,
                    positions: true,
                }],
                stored: true,
                fast: false,
            }),
            FieldSchema::new("tags", FieldType::Text).with_role(FieldRole {
                indexed: vec![IndexKind::Exact],
                stored: true,
                fast: false,
            }),
            FieldSchema::new("value", FieldType::Int),
        ];
        let schema = RecordSchema::new(fields).expect("schema");
        assert_eq!(schema.text_fields(), &[0]);
    }

    #[test]
    fn field_schema_new_defaults() {
        let field = FieldSchema::new("user_name", FieldType::Text);
        assert_eq!(field.name, "user_name");
        assert_eq!(field.label, "user name");
        assert_eq!(field.data_type, FieldType::Text);
        assert!(field.nullable);
        assert_eq!(field.key, KeyRole::None);
    }

    #[test]
    fn yaml_deserialization_omitted_fields_use_defaults() {
        let yaml = "name: status\ndata_type: Text\n";
        let field: FieldSchema = serde_yaml::from_str(yaml).expect("deserialize");
        assert_eq!(field.name, "status");
        assert_eq!(field.label, "status");
        assert!(field.nullable);
        assert_eq!(field.key, KeyRole::None);
    }

    #[test]
    fn yaml_deserialization_explicit_nullable_false() {
        let yaml = "name: id\ndata_type: Text\nnullable: false\n";
        let field: FieldSchema = serde_yaml::from_str(yaml).expect("deserialize");
        assert!(!field.nullable);
    }
}
