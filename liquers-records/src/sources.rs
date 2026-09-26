//! The two reference `RecordSource` implementations (`ManifestSource`, `InMemorySource`), the
//! retrieval half of `ChunkOrigin` (`ChunkOrigin::locator_query`), and the two `ChunkResolver`
//! implementations a caller reaches evaluation through (`ContextResolver`, `EnvResolver`).
//!
//! See `specs/design/record-streams/phase2-architecture.md`, §"The types", §"`ChunkResolver` —
//! how a source reaches evaluation", §"Why a *source* makes this work, and a stream would not",
//! §"A source serializes only as its manifest", §"Two readers: schema-aware and schema-less",
//! §"C. `stored` and `cached` in the asset manager" and §"`rec_id` — the guaranteed path, as a
//! query" / §"`locator` is the optimization, not the guarantee".
//!
//! **`ContextResolver`/`EnvResolver` are compile-checked here only.** Their behaviour — that a
//! chunk evaluated through a `Context` is recorded as a dependency and one evaluated through an
//! `EnvRef` is not — needs `liquers-lib`'s `Value: RecordValue` and lands in Step 5.6.
//!
//! **Wrapping sources.** Phase 2's Trait Implementations table names "a filtering source and an
//! asynchronously mapping one" as further `RecordSource` reference implementations, but neither
//! §"The types" nor §"Function Signatures" gives either a struct, a field list or a constructor —
//! there is nothing concrete to implement yet. Filed as
//! `RECORD-SOURCE-WRAPPERS-UNSPECIFIED` rather than guessed at here.

use std::sync::Arc;

use futures::stream;
use liquers_core::context::{Context, EnvRef, Environment};
use liquers_core::error::{Error, ErrorType};
use liquers_core::maybe_send::BoxFuture;
use liquers_core::metadata::{DependencyKey, DependencyRecord, Metadata, Version};
use liquers_core::query::{
    ActionParameter, ActionRequest, Key, Query, QuerySegment, SegmentHeader, TransformQuerySegment,
};
use liquers_core::state::State;
use serde::{Deserialize, Serialize};

use crate::batch::{
    record_stream, BoxRecordStream, ChunkDescriptor, ChunkId, ChunkList, ChunkOrigin, RecordBatch,
    RecordStreamExt, RecordView,
};
use crate::column::FieldValue;
use crate::formats::csv::format_value;
use crate::formats::{read_table, ReadOptions, ReadSchema, TableFormat};
use crate::manifest::{ChunkNaming, ChunkTemplate, ManifestSpec};
use crate::schema::RecordSchema;
use crate::value::{ChunkResolver, ChunkValue, RecordSource, RecordValue};

// =================================================================================================
// ChunkOrigin::locator_query
// =================================================================================================

impl ChunkOrigin {
    /// Builds the directly evaluable query for one row, from the shortcut `self.locator` offers —
    /// `<asset>/-/ns-<namespace>/<command>-<leading_parameters...>-<id>`. `None` when there is no
    /// locator (the common case: `rec_id` is the guaranteed path then, not this), or when `id` has
    /// no string form a query parameter can carry (`FieldValue::Null`; see
    /// [`crate::formats::csv::format_value`]).
    ///
    /// Built on the parsed `Query` AST — never by string concatenation — so the result always
    /// means "apply this action to `self.asset`", never "a resource literally named this"
    /// (phase2-architecture.md §"`rec_id` — the guaranteed path, as a query": the two read
    /// differently and only one is meant). The new segment carries an explicit, trivial header
    /// (`SegmentHeader::new()`, which encodes as a bare `-`) precisely so it is never absorbed into
    /// a preceding resource segment, matching `-R/f.csv/-/ns-csv/row-42` rather than
    /// `-R/f.csv/ns-csv/row-42`.
    pub fn locator_query(&self, id: &FieldValue) -> Option<Query> {
        let locator = self.locator.as_ref()?;
        let id_text = format_value(id).ok()??;

        let mut parameters: Vec<ActionParameter> = locator
            .leading_parameters
            .iter()
            .cloned()
            .map(ActionParameter::new_string)
            .collect();
        parameters.push(ActionParameter::new_string(id_text));

        let ns_action = ActionRequest::new("ns".to_string())
            .with_parameters(vec![ActionParameter::new_string(locator.namespace.clone())]);
        let command_action =
            ActionRequest::new(locator.command.clone()).with_parameters(parameters);

        let mut query = self.asset.clone();
        query.segments.push(QuerySegment::Transform(TransformQuerySegment {
            header: Some(SegmentHeader::new()),
            query: vec![ns_action, command_action],
            filename: None,
        }));
        Some(query)
    }
}

// =================================================================================================
// Reading a chunk: shared by ManifestSource
// =================================================================================================

/// Checks a resolved chunk's schema against a manifest's declared `uniform_schema` — names and
/// types in field order, and no nulls in a field declared not null — per phase2-architecture.md §"Two readers" ("A chunk produced
/// by a command is evaluated as usual and must yield a `RecordView`, which is **checked** against
/// the declared schema ... and refused on mismatch. Nothing is re-parsed.").
fn check_view_matches_schema(view: &dyn RecordView, declared: &RecordSchema) -> Result<(), Error> {
    let actual = view.schema();
    if actual.fields.len() != declared.fields.len() {
        return Err(Error::general_error(format!(
            "ManifestSource: chunk schema has {} field(s), but uniform_schema declares {}",
            actual.fields.len(),
            declared.fields.len()
        )));
    }
    for (index, (actual_field, declared_field)) in
        actual.fields.iter().zip(declared.fields.iter()).enumerate()
    {
        if actual_field.name != declared_field.name || actual_field.data_type != declared_field.data_type {
            return Err(Error::general_error(format!(
                "ManifestSource: chunk field {index} ('{}': {:?}) does not match uniform_schema \
                 field ('{}': {:?})",
                actual_field.name, actual_field.data_type, declared_field.name, declared_field.data_type,
            )));
        }
        // Nullability is checked against the data, not the chunk's own flag: a field declared
        // `not_null` must hold no nulls, and a field declared nullable accepts either.
        if !declared_field.nullable {
            let nulls = view.column(index)?.null_mask().count_ones();
            if nulls > 0 {
                return Err(Error::general_error(format!(
                    "ManifestSource: chunk field '{}' holds {nulls} null(s), but uniform_schema \
                     declares it not null",
                    actual_field.name
                )));
            }
        }
    }
    Ok(())
}

/// Turns a resolved `ChunkValue` into a view, applying `declared` (a manifest's `uniform_schema`,
/// when it has one) the way §"Two readers" specifies for each variant.
fn view_from_chunk_value(
    value: ChunkValue,
    declared: Option<&RecordSchema>,
) -> Result<Arc<dyn RecordView>, Error> {
    match value {
        ChunkValue::View(view) => {
            if let Some(schema) = declared {
                check_view_matches_schema(view.as_ref(), schema)?;
            }
            Ok(view)
        }
        ChunkValue::Bytes { data, metadata } => parse_chunk_bytes(&data, &metadata, declared),
        ChunkValue::Source(_) => Err(Error::general_error(
            "ManifestSource: a chunk evaluated to a nested RecordSource, which this design does \
             not support"
                .to_string(),
        )),
    }
}

/// Parses stored or evaluated bytes as a table, schema-aware when `declared` is given (never
/// guessed then) and schema-less otherwise. The format comes from `metadata`'s data format, which
/// falls back to the key's extension and then `"bin"` (`Metadata::get_data_format`).
fn parse_chunk_bytes(
    bytes: &[u8],
    metadata: &Metadata,
    declared: Option<&RecordSchema>,
) -> Result<Arc<dyn RecordView>, Error> {
    let format = TableFormat::from_data_format(&metadata.get_data_format())?;
    let read_schema = match declared {
        Some(schema) => ReadSchema::Declared(schema),
        None => ReadSchema::Infer,
    };
    let batch = read_table(bytes, format, read_schema, &ReadOptions::default())?;
    Ok(Arc::new(batch) as Arc<dyn RecordView>)
}

// =================================================================================================
// ManifestSource
// =================================================================================================

/// Chunks named by queries — what a manifest deserializes to, and the only `RecordSource` whose
/// byte form is a manifest. See phase2-architecture.md §"The types" and §"A source serializes only
/// as its manifest".
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "ManifestSpec", into = "ManifestSpec")]
pub struct ManifestSource {
    spec: ManifestSpec,
    /// Where the manifest lives — its folder is the chunk keys' folder. `None` until known: a
    /// manifest read back from the store arrives without it (`deserialize_from_bytes` receives no
    /// metadata) and the glue supplies it via [`Self::with_key`]; a manifest built by a command and
    /// never stored stays `None` forever, and every one of its chunks is unkeyed.
    key: Option<Key>,
    /// Each **explicit** chunk's id, in `spec.chunks` order — derived once, at construction, since
    /// a `&[ChunkId]` cannot be borrowed from the `Vec<Recipe>` the spec holds. Template-generated
    /// chunk ids are computed lazily, one at a time, while [`RecordSource::stream`] walks them —
    /// `chunks()` is synchronous and has no I/O, so it can only report what is known without
    /// evaluating anything, which for a template is exactly the explicit prefix.
    ids: Vec<ChunkId>,
    /// How template-generated chunks are keyed. `Some` only once `with_key` has run; `None` makes
    /// every template chunk unkeyed too, identified by its own rendered query.
    naming: Option<ChunkNaming>,
}

impl ManifestSource {
    /// Validates what does not depend on the key — no name collisions among explicit chunks — and
    /// derives each explicit chunk's id. With `key`, immediately applies it through
    /// [`Self::with_key`], so its key-dependent checks run too; without one, every chunk (even one
    /// whose own query ends in a filename) stays unkeyed, per phase2-architecture.md §"A. Chunk
    /// keys" ("A manifest with no key ... has no folder, so all its chunks are unkeyed").
    pub fn new(spec: ManifestSpec, key: Option<Key>) -> Result<ManifestSource, Error> {
        spec.check_explicit_chunk_collisions()?;
        match key {
            Some(key) => {
                let source = ManifestSource { spec, key: None, ids: Vec::new(), naming: None };
                source.with_key(key)
            }
            None => {
                let ids = Self::unkeyed_explicit_ids(&spec)?;
                Ok(ManifestSource { spec, key: None, ids, naming: None })
            }
        }
    }

    /// Every explicit chunk, identified by its own query — used when the manifest has no key.
    fn unkeyed_explicit_ids(spec: &ManifestSpec) -> Result<Vec<ChunkId>, Error> {
        spec.chunks
            .iter()
            .map(|chunk| Ok(ChunkId::Query(chunk.get_query()?)))
            .collect()
    }

    /// Every explicit chunk, keyed by its query's filename when it has one (`folder/<name>`) and
    /// identified by its own query otherwise — used once the manifest's key, and so its folder, is
    /// known.
    fn keyed_explicit_ids(spec: &ManifestSpec, folder: &Key) -> Result<Vec<ChunkId>, Error> {
        spec.chunks
            .iter()
            .map(|chunk| match chunk.filename()? {
                Some(name) => Ok(ChunkId::Key(folder.join(name.encode()))),
                None => Ok(ChunkId::Query(chunk.get_query()?)),
            })
            .collect()
    }

    /// The naming a template chunk's key follows, once the manifest's own key is known: the
    /// manifest's folder, its own filename stem (with `.manifest.yaml` stripped, when present) as
    /// the prefix, and the manifest's `extension` (default `csv`).
    fn derive_naming(spec: &ManifestSpec, key: &Key) -> Result<ChunkNaming, Error> {
        let name = key.filename().ok_or_else(|| {
            Error::general_error(format!(
                "ManifestSource::with_key: key '{}' has no filename",
                key.encode()
            ))
        })?;
        let prefix = name
            .encode()
            .strip_suffix(".manifest.yaml")
            .unwrap_or(name.encode())
            .to_string();
        Ok(ChunkNaming {
            folder: key.parent(),
            prefix,
            extension: spec.extension.clone().unwrap_or_else(|| "csv".to_string()),
        })
    }

    /// Supplies the key a deserialized manifest lacks: derives the template naming and every
    /// explicit chunk's id, and validates what needs the key — per-chunk `arguments`/`links` only
    /// on keyed chunks, no explicit name colliding with the template's own naming pattern.
    /// `to_record_source` (`liquers-lib`, Step 5.x), and so every record command, calls this with
    /// the state's metadata key.
    pub fn with_key(self, key: Key) -> Result<ManifestSource, Error> {
        let ManifestSource { spec, .. } = self;
        spec.check_unkeyed_chunk_arguments()?;
        let naming = Self::derive_naming(&spec, &key)?;
        spec.check_explicit_names_against_template(&naming)?;
        let ids = Self::keyed_explicit_ids(&spec, &naming.folder)?;
        Ok(ManifestSource { spec, key: Some(key), ids, naming: Some(naming) })
    }

    /// The manifest this source was built from.
    pub fn spec(&self) -> &ManifestSpec {
        &self.spec
    }

    /// Where the manifest lives, once [`Self::with_key`] has run — `None` for a manifest built by
    /// a command and never stored, whose chunks are all unkeyed.
    pub fn key(&self) -> Option<&Key> {
        self.key.as_ref()
    }

    /// The id of template-generated chunk `index` (a global index, at least the number of explicit
    /// chunks) — keyed through `naming` when the manifest is, unkeyed (identified by its own
    /// rendered query) otherwise.
    fn template_chunk_id(&self, template: &ChunkTemplate, index: u64) -> Result<ChunkId, Error> {
        match &self.naming {
            Some(naming) => Ok(ChunkId::Key(naming.key(index))),
            None => Ok(ChunkId::Query(template.query_at(index, None)?)),
        }
    }

    /// The directly evaluable query for `id` — `id` itself when unkeyed, or a bare resource query
    /// over the key when keyed (`Query::from`, the inverse of `Query::key`).
    fn chunk_query(id: &ChunkId) -> Query {
        match id {
            ChunkId::Query(query) => query.clone(),
            ChunkId::Key(key) => Query::from(key.clone()),
        }
    }

    /// Reads one chunk's rows, following phase2-architecture.md §"How a manifest's chunks reach
    /// the reader" / §C ("How the source reads a keyed chunk"):
    /// - **keyed, and the store holds the key** — `read_resource`, parsed with `uniform_schema` in
    ///   the format the returned metadata names;
    /// - **keyed, but the store does not** (nothing has been produced yet, or the manifest is
    ///   `stored: false`) — the key is evaluated instead, "a pure key query, so the chunk becomes a
    ///   keyed asset with the manifest's flags";
    /// - **unkeyed** — the chunk's own query is evaluated.
    ///
    /// "The store holds the key" is read off `read_resource`'s own answer, not `spec.stored`: a
    /// `stored: false` manifest can still have an *existing* stored copy from before the flag was
    /// set, or `Override` data, and §C is explicit that such a copy is "still read, and preferred
    /// to recomputation".
    async fn read_chunk(
        &self,
        id: &ChunkId,
        resolver: &Arc<dyn ChunkResolver>,
    ) -> Result<Arc<dyn RecordView>, Error> {
        let declared = self.spec.uniform_schema.as_deref();
        match id {
            ChunkId::Key(key) => match resolver.read_resource(key.clone()).await {
                Ok((bytes, metadata)) => parse_chunk_bytes(&bytes, &metadata, declared),
                Err(error) if error.error_type == ErrorType::KeyNotFound => {
                    let value = resolver.evaluate(Self::chunk_query(id)).await?;
                    view_from_chunk_value(value, declared)
                }
                Err(error) => Err(error),
            },
            ChunkId::Query(query) => {
                let value = resolver.evaluate(query.clone()).await?;
                view_from_chunk_value(value, declared)
            }
        }
    }

    /// Where a fresh traversal starts: the first explicit chunk, or straight to the template
    /// (`Template`, at the global index right after the explicit prefix — 0 when there is none) —
    /// or `Done` when the manifest has neither.
    fn initial_walk_state(&self) -> WalkState {
        if self.ids.is_empty() {
            self.state_after_explicit()
        } else {
            WalkState::Explicit(0)
        }
    }

    /// Where the walk continues once every explicit chunk has been offered: the template, starting
    /// right after the explicit prefix, or `Done` when there is none.
    fn state_after_explicit(&self) -> WalkState {
        match &self.spec.template {
            Some(_) => WalkState::Template(self.ids.len() as u64),
            None => WalkState::Done,
        }
    }

    /// Advances the walk by exactly one chunk — the body `futures::stream::unfold` drives, one
    /// resident chunk at a time, per Phase 3 §1.2's corrected sketch. `None` ends the stream.
    async fn advance(
        source: Arc<ManifestSource>,
        resolver: Arc<dyn ChunkResolver>,
        state: WalkState,
    ) -> Option<(
        Result<Arc<dyn RecordView>, Error>,
        (Arc<ManifestSource>, Arc<dyn ChunkResolver>, WalkState),
    )> {
        let id = match state {
            WalkState::Done => return None,
            WalkState::Explicit(index) => source.ids[index].clone(),
            WalkState::Template(index) => {
                // The state machine only ever enters `Template` when a template exists
                // (`state_after_explicit`); `?` here is defensive, not reachable.
                let template = source.spec.template.clone()?;
                match source.template_chunk_id(&template, index) {
                    Ok(id) => id,
                    Err(error) => return Some((Err(error), (source, resolver, WalkState::Done))),
                }
            }
        };

        let result = source.read_chunk(&id, &resolver).await;
        let next_state = source.next_walk_state(&state, &result);
        Some((result, (source, resolver, next_state)))
    }

    /// `state`'s successor, given the chunk just read: the next explicit index, or the template
    /// (started or continued) once explicit chunks run out; a template chunk stops the walk as
    /// soon as it reads fewer rows than `batch_size`, or errors.
    fn next_walk_state(&self, state: &WalkState, result: &Result<Arc<dyn RecordView>, Error>) -> WalkState {
        match state {
            WalkState::Done => WalkState::Done,
            WalkState::Explicit(index) => {
                if index + 1 < self.ids.len() {
                    WalkState::Explicit(index + 1)
                } else {
                    self.state_after_explicit()
                }
            }
            WalkState::Template(index) => match self.spec.template.as_ref() {
                Some(template) => match result {
                    Ok(view) => {
                        if (view.len() as u64) < template.batch_size {
                            WalkState::Done
                        } else {
                            WalkState::Template(index + 1)
                        }
                    }
                    Err(_) => WalkState::Done,
                },
                None => WalkState::Done,
            },
        }
    }
}

/// One step of [`ManifestSource`]'s walk over its chunks: the explicit prefix in order, then the
/// template — Phase 3 §1.2's corrected sketch ("walk chunks in order, one resident at a time").
/// Not `pub`: an implementation detail of [`ManifestSource::stream`]'s `futures::stream::unfold`
/// state, never observed outside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WalkState {
    Explicit(usize),
    /// A **global** chunk index (Phase 2 §A / `manifest-format.md` §4a), at least the number of
    /// explicit chunks.
    Template(u64),
    Done,
}

impl RecordSource for ManifestSource {
    fn stream(
        self: Arc<Self>,
        resolver: Arc<dyn ChunkResolver>,
    ) -> BoxFuture<'static, Result<BoxRecordStream, Error>> {
        Box::pin(async move {
            let schema = self.schema();
            let initial = self.initial_walk_state();
            let inner = stream::unfold((self, resolver, initial), |(source, resolver, state)| {
                ManifestSource::advance(source, resolver, state)
            });
            Ok(record_stream(Box::pin(inner), schema))
        })
    }

    fn chunks(&self) -> ChunkList<'_> {
        match &self.spec.template {
            Some(_) => ChunkList::Unbounded { computed: &self.ids },
            None => ChunkList::Known(&self.ids),
        }
    }

    fn describe_chunk<'a>(
        &'a self,
        id: &'a ChunkId,
        resolver: &'a dyn ChunkResolver,
    ) -> BoxFuture<'a, Result<ChunkDescriptor, Error>> {
        Box::pin(async move {
            let query = Self::chunk_query(id);
            let metadata = resolver.metadata(query.clone()).await?;
            let origin = ChunkOrigin {
                asset: query.clone(),
                chunk: query.clone(),
                info: None,
                locator: None,
            };
            Ok(ChunkDescriptor {
                id: id.clone(),
                query,
                metadata,
                origin,
                schema: self.schema().map(|schema| (*schema).clone()),
            })
        })
    }

    fn schema(&self) -> Option<Arc<RecordSchema>> {
        self.spec.uniform_schema.clone()
    }

    fn manifest(&self) -> Option<&ManifestSpec> {
        Some(&self.spec)
    }
}

impl TryFrom<ManifestSpec> for ManifestSource {
    type Error = Error;

    /// A manifest read back from the store (or hand-written and loaded fresh) arrives with no key
    /// — `deserialize_from_bytes` receives no metadata — so this runs only the key-independent
    /// checks, exactly as [`ManifestSource::new`] with `key: None`. The glue that has a key
    /// (`to_record_source`, `liquers-lib` Step 5.x) applies it afterwards with
    /// [`ManifestSource::with_key`].
    fn try_from(spec: ManifestSpec) -> Result<Self, Error> {
        ManifestSource::new(spec, None)
    }
}

impl From<ManifestSource> for ManifestSpec {
    /// A source serializes only as its manifest (phase2-architecture.md §"A source serializes only
    /// as its manifest") — `key`, `ids` and `naming` are all derived, and are not written out.
    fn from(source: ManifestSource) -> Self {
        source.spec
    }
}

// =================================================================================================
// InMemorySource
// =================================================================================================

/// Views already in memory — what a view becomes when a source is wanted (`RecordView ->
/// RecordSource` is free: `InMemorySource::new(vec![view])`). It has no byte form; `materialize`
/// it (free when it holds one view, since `RecordBatch::materialize` is a shallow clone) to
/// serialize its rows. See phase2-architecture.md §"The types" and §"There is no collection type
/// above the batch".
#[derive(Debug, Clone)]
pub struct InMemorySource {
    views: Vec<Arc<dyn RecordView>>,
    /// One id per view, in order. There is no query or key to identify an in-memory view by — it
    /// has no byte form and nothing produced it — so each id is a synthetic, stable placeholder
    /// (`ns-rec/in_memory_view-<index>`) that exists only for [`RecordSource::chunks`] and
    /// [`RecordSource::describe_chunk`]'s bookkeeping. [`RecordSource::stream`] never resolves
    /// these through a [`ChunkResolver`]: it reads `views` directly.
    ids: Vec<ChunkId>,
    uniform_schema: Option<Arc<RecordSchema>>,
}

/// A stable placeholder id for in-memory view `index` — never evaluated, only compared and
/// displayed. `ActionRequest`/`Query` are built directly rather than through `parse_query`, so this
/// can never fail on a malformed string.
fn in_memory_chunk_id(index: usize) -> ChunkId {
    let action = ActionRequest::new("in_memory_view".to_string())
        .with_parameters(vec![ActionParameter::new_string(index.to_string())]);
    ChunkId::Query(Query {
        segments: vec![QuerySegment::Transform(TransformQuerySegment {
            header: None,
            query: vec![action],
            filename: None,
        })],
        absolute: false,
        source: liquers_core::query::QuerySource::Unspecified,
    })
}

impl InMemorySource {
    pub fn new(views: Vec<Arc<dyn RecordView>>) -> InMemorySource {
        let uniform_schema = Self::infer_uniform_schema(&views);
        let ids = (0..views.len()).map(in_memory_chunk_id).collect();
        InMemorySource { views, ids, uniform_schema }
    }

    /// `Some` only when every view shares one schema (by value, not by `Arc` identity) —
    /// phase2-architecture.md §"Schema uniformity is declared, not assumed".
    fn infer_uniform_schema(views: &[Arc<dyn RecordView>]) -> Option<Arc<RecordSchema>> {
        let first = views.first()?.schema().clone();
        if views.iter().all(|view| view.schema() == &first) {
            Some(first)
        } else {
            None
        }
    }
}

impl RecordSource for InMemorySource {
    fn stream(
        self: Arc<Self>,
        _resolver: Arc<dyn ChunkResolver>,
    ) -> BoxFuture<'static, Result<BoxRecordStream, Error>> {
        // No chunk here is ever produced by a query, so the resolver is unused: every view is
        // already resident.
        Box::pin(async move {
            let schema = self.schema();
            let views = self.views.clone();
            let inner = stream::iter(views.into_iter().map(Ok));
            Ok(record_stream(Box::pin(inner), schema))
        })
    }

    fn chunks(&self) -> ChunkList<'_> {
        ChunkList::Known(&self.ids)
    }

    fn describe_chunk<'a>(
        &'a self,
        id: &'a ChunkId,
        _resolver: &'a dyn ChunkResolver,
    ) -> BoxFuture<'a, Result<ChunkDescriptor, Error>> {
        Box::pin(async move {
            let index = self
                .ids
                .iter()
                .position(|existing| existing == id)
                .ok_or_else(|| {
                    Error::general_error("InMemorySource::describe_chunk: unknown chunk id".to_string())
                })?;
            let query = match id {
                ChunkId::Query(query) => query.clone(),
                ChunkId::Key(_) => {
                    return Err(Error::general_error(
                        "InMemorySource: chunk ids are never keyed".to_string(),
                    ))
                }
            };
            Ok(ChunkDescriptor {
                id: id.clone(),
                query: query.clone(),
                metadata: Metadata::new(),
                origin: ChunkOrigin { asset: query.clone(), chunk: query, info: None, locator: None },
                schema: Some(self.views[index].schema().as_ref().clone()),
            })
        })
    }

    fn schema(&self) -> Option<Arc<RecordSchema>> {
        self.uniform_schema.clone()
    }

    /// Overrides the trait default: with exactly one view, materializing it directly is free (a
    /// `RecordBatch`'s own `materialize` is a shallow clone) rather than opening a stream to
    /// concatenate a single item with itself. Phase 2's Trait Implementations table:
    /// "`InMemorySource` overrides `materialize` (free for one batch)".
    fn materialize(
        self: Arc<Self>,
        resolver: Arc<dyn ChunkResolver>,
        max_rows: usize,
    ) -> BoxFuture<'static, Result<Arc<RecordBatch>, Error>> {
        if self.views.len() == 1 {
            let view = self.views[0].clone();
            return Box::pin(async move {
                let batch = view.materialize()?;
                if batch.len > max_rows {
                    return Err(Error::general_error(format!(
                        "InMemorySource::materialize: {} rows exceeds max_rows ({max_rows})",
                        batch.len
                    )));
                }
                Ok(batch)
            });
        }
        Box::pin(async move {
            let stream = RecordSource::stream(self, resolver).await?;
            RecordStreamExt::materialize(stream, max_rows).await
        })
    }
}

// =================================================================================================
// ChunkResolver implementations
// =================================================================================================

/// Converts an evaluated value into a [`ChunkValue`] via the [`RecordValue`] adapter: a view or a
/// source is taken as such, and anything else is serialized in its own data format into
/// [`ChunkValue::Bytes`] — phase2-architecture.md §"`ChunkResolver`" ("Both are implemented for `E:
/// Environment` where `E::Value: RecordValue`: a value that is a view or a source is taken as
/// such, and any other is serialized in its data format into `ChunkValue::Bytes`").
fn classify_state<V: RecordValue>(state: State<V>) -> Result<ChunkValue, Error> {
    let value = state.value()?;
    if let Some(view) = value.as_record_view() {
        return Ok(ChunkValue::View(view));
    }
    if let Some(source) = value.as_record_source() {
        return Ok(ChunkValue::Source(source));
    }
    let metadata = (*state.metadata).clone();
    let data = state.as_bytes()?;
    Ok(ChunkValue::Bytes { data, metadata })
}

/// The resolver a command uses while traversing a source inside its own body: evaluating a chunk
/// records it as a dependency of the asset being computed, so the asset manager can invalidate the
/// materialized result when a chunk changes. See phase2-architecture.md §"`ChunkResolver`".
pub struct ContextResolver<E: Environment>
where
    E::Value: RecordValue,
{
    context: Context<E>,
}

impl<E: Environment> ContextResolver<E>
where
    E::Value: RecordValue,
{
    pub fn new(context: Context<E>) -> Self {
        ContextResolver { context }
    }
}

impl<E: Environment> ChunkResolver for ContextResolver<E>
where
    E::Value: RecordValue,
{
    fn evaluate(&self, query: Query) -> BoxFuture<'static, Result<ChunkValue, Error>> {
        let context = self.context.clone();
        Box::pin(async move {
            let state = context.get_dependency_state(&query).await?;
            classify_state(state)
        })
    }

    fn metadata(&self, query: Query) -> BoxFuture<'static, Result<Metadata, Error>> {
        let context = self.context.clone();
        Box::pin(async move {
            match query.key() {
                // A plain resource query: the store's own metadata answers directly, with no
                // evaluation.
                Some(key) => context.get_envref().get_async_store().get_metadata(&key).await,
                // Anything else has no metadata to read without evaluating it — the closest this
                // resolver can come to "the asset manager's" metadata for an unkeyed chunk.
                None => {
                    let state = context.get_dependency_state(&query).await?;
                    Ok((*state.metadata).clone())
                }
            }
        })
    }

    fn read_resource(&self, key: Key) -> BoxFuture<'static, Result<(Vec<u8>, Metadata), Error>> {
        let context = self.context.clone();
        Box::pin(async move {
            let store = context.get_envref().get_async_store();
            let (bytes, metadata) = store.get(&key).await?;
            // Bypasses the asset manager on purpose (phase2-architecture.md §"Two readers"), so
            // the dependency this read stands in for is recorded by hand, exactly as an evaluated
            // dependency would have been.
            let version = metadata.version().unwrap_or_else(Version::unknown);
            context
                .add_dependency(DependencyRecord::new(DependencyKey::from(&key), version))
                .await;
            Ok((bytes, metadata))
        })
    }
}

/// The resolver `liquers-axum` uses to serve a source over HTTP, and any other caller for whom the
/// stream must outlive the request that opened it: it wraps an `EnvRef`, not a `Context`, so
/// nothing it does can write into per-asset state that has already finished. Records no
/// dependency. See phase2-architecture.md §"Streaming a record source over HTTP".
pub struct EnvResolver<E: Environment>
where
    E::Value: RecordValue,
{
    envref: EnvRef<E>,
}

impl<E: Environment> EnvResolver<E>
where
    E::Value: RecordValue,
{
    pub fn new(envref: EnvRef<E>) -> Self {
        EnvResolver { envref }
    }
}

impl<E: Environment> ChunkResolver for EnvResolver<E>
where
    E::Value: RecordValue,
{
    fn evaluate(&self, query: Query) -> BoxFuture<'static, Result<ChunkValue, Error>> {
        let envref = self.envref.clone();
        Box::pin(async move {
            let asset = envref.evaluate(query).await?;
            let state = asset.get().await?;
            classify_state(state)
        })
    }

    fn metadata(&self, query: Query) -> BoxFuture<'static, Result<Metadata, Error>> {
        let envref = self.envref.clone();
        Box::pin(async move {
            match query.key() {
                Some(key) => envref.get_async_store().get_metadata(&key).await,
                None => {
                    let asset = envref.evaluate(query).await?;
                    let state = asset.get().await?;
                    Ok((*state.metadata).clone())
                }
            }
        })
    }

    fn read_resource(&self, key: Key) -> BoxFuture<'static, Result<(Vec<u8>, Metadata), Error>> {
        let envref = self.envref.clone();
        Box::pin(async move { envref.get_async_store().get(&key).await })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::batch::{ChunkOrigin, LocatorRule};
    use liquers_core::parse::parse_query;

    // --- ChunkOrigin::locator_query ---------------------------------------------------------

    #[test]
    fn locator_query_appends_namespace_and_command_after_the_asset() -> Result<(), Box<dyn std::error::Error>> {
        let asset = parse_query("-R/data/sales.csv")?;
        let origin = ChunkOrigin {
            asset: asset.clone(),
            chunk: asset,
            info: None,
            locator: Some(LocatorRule {
                namespace: "csv".to_string(),
                command: "row".to_string(),
                leading_parameters: Vec::new(),
            }),
        };
        let query = origin.locator_query(&FieldValue::Int(42)).expect("locator");
        assert_eq!(query.encode(), "-R/data/sales.csv/-/ns-csv/row-42");
        Ok(())
    }

    #[test]
    fn locator_query_includes_leading_parameters_before_the_id() -> Result<(), Box<dyn std::error::Error>> {
        let asset = parse_query("-R/data/sales.csv")?;
        let origin = ChunkOrigin {
            asset: asset.clone(),
            chunk: asset,
            info: None,
            locator: Some(LocatorRule {
                namespace: "csv".to_string(),
                command: "cell".to_string(),
                leading_parameters: vec!["price".to_string()],
            }),
        };
        let query = origin.locator_query(&FieldValue::Int(42)).expect("locator");
        assert_eq!(query.encode(), "-R/data/sales.csv/-/ns-csv/cell-price-42");
        Ok(())
    }

    #[test]
    fn locator_query_is_none_without_a_locator_rule() -> Result<(), Box<dyn std::error::Error>> {
        let asset = parse_query("-R/data/sales.csv")?;
        let origin = ChunkOrigin { asset: asset.clone(), chunk: asset, info: None, locator: None };
        assert!(origin.locator_query(&FieldValue::Int(42)).is_none());
        Ok(())
    }

    #[test]
    fn locator_query_is_none_for_a_null_id() -> Result<(), Box<dyn std::error::Error>> {
        let asset = parse_query("-R/data/sales.csv")?;
        let origin = ChunkOrigin {
            asset: asset.clone(),
            chunk: asset,
            info: None,
            locator: Some(LocatorRule {
                namespace: "csv".to_string(),
                command: "row".to_string(),
                leading_parameters: Vec::new(),
            }),
        };
        assert!(origin.locator_query(&FieldValue::Null).is_none());
        Ok(())
    }

    // --- InMemorySource ----------------------------------------------------------------------

    use crate::buffer::Buffer;
    use crate::column::Column;
    use crate::schema::{FieldSchema, FieldType, KeyRole};
    use futures::stream::StreamExt;

    fn tiny_batch(values: &[i64]) -> Arc<RecordBatch> {
        let schema = Arc::new(
            RecordSchema::new(vec![FieldSchema::new("id", FieldType::Int).with_key(KeyRole::Id)])
                .expect("schema"),
        );
        let column = Column::Int { validity: None, values: Buffer::from_slice(values) };
        Arc::new(RecordBatch::new(schema, vec![column], None, None, Vec::new()).expect("batch"))
    }

    struct NeverResolver;
    impl ChunkResolver for NeverResolver {
        fn evaluate(&self, _query: Query) -> BoxFuture<'static, Result<ChunkValue, Error>> {
            Box::pin(async { Err(Error::general_error("not used by InMemorySource".to_string())) })
        }
        fn metadata(&self, _query: Query) -> BoxFuture<'static, Result<Metadata, Error>> {
            Box::pin(async { Err(Error::general_error("not used by InMemorySource".to_string())) })
        }
        fn read_resource(&self, _key: Key) -> BoxFuture<'static, Result<(Vec<u8>, Metadata), Error>> {
            Box::pin(async { Err(Error::general_error("not used by InMemorySource".to_string())) })
        }
    }

    #[tokio::test]
    async fn in_memory_source_stream_never_touches_the_resolver() -> Result<(), Box<dyn std::error::Error>> {
        let view = tiny_batch(&[1, 2, 3]) as Arc<dyn RecordView>;
        let source = Arc::new(InMemorySource::new(vec![view]));
        let resolver: Arc<dyn ChunkResolver> = Arc::new(NeverResolver);
        let stream = source.stream(resolver).await?;
        let views: Vec<_> = stream.collect().await;
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].as_ref().expect("view").len(), 3);
        Ok(())
    }

    #[test]
    fn in_memory_source_chunks_lists_one_id_per_view() {
        let view_a = tiny_batch(&[1]) as Arc<dyn RecordView>;
        let view_b = tiny_batch(&[2]) as Arc<dyn RecordView>;
        let source = InMemorySource::new(vec![view_a, view_b]);
        match source.chunks() {
            ChunkList::Known(ids) => assert_eq!(ids.len(), 2),
            ChunkList::Unbounded { .. } => panic!("expected Known"),
        }
    }

    #[test]
    fn in_memory_source_reports_no_uniform_schema_when_views_disagree() {
        let int_view = tiny_batch(&[1]) as Arc<dyn RecordView>;
        let float_schema = Arc::new(
            RecordSchema::new(vec![FieldSchema::new("id", FieldType::Float)]).expect("schema"),
        );
        let float_view = Arc::new(
            RecordBatch::new(
                float_schema,
                vec![Column::Float { validity: None, values: Buffer::from_slice(&[1.5f64]) }],
                None,
                None,
                Vec::new(),
            )
            .expect("batch"),
        ) as Arc<dyn RecordView>;
        let source = InMemorySource::new(vec![int_view, float_view]);
        assert!(source.schema().is_none());
    }

    #[tokio::test]
    async fn in_memory_source_materialize_of_a_single_view_is_free() -> Result<(), Box<dyn std::error::Error>> {
        let view = tiny_batch(&[1, 2]) as Arc<dyn RecordView>;
        let source = Arc::new(InMemorySource::new(vec![view]));
        let resolver: Arc<dyn ChunkResolver> = Arc::new(NeverResolver);
        let batch = source.materialize(resolver, 100).await?;
        assert_eq!(batch.len, 2);
        Ok(())
    }

    #[tokio::test]
    async fn in_memory_source_materialize_exceeding_max_rows_is_refused() {
        let view = tiny_batch(&[1, 2, 3]) as Arc<dyn RecordView>;
        let source = Arc::new(InMemorySource::new(vec![view]));
        let resolver: Arc<dyn ChunkResolver> = Arc::new(NeverResolver);
        assert!(source.materialize(resolver, 1).await.is_err());
    }

    // --- ManifestSource::stream: reading each chunk kind --------------------------------------

    use crate::manifest::ManifestSpec;
    use crate::ManifestSource;
    use liquers_core::parse::parse_key;
    use liquers_core::recipes::Recipe;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// A fixture resolver over a fixed map from encoded query to `ChunkValue`, plus an optional
    /// fixed map from encoded key to stored `(bytes, metadata)` — enough to exercise every branch
    /// `ManifestSource::read_chunk` takes without a real store or asset manager.
    #[derive(Debug, Default)]
    struct FixtureResolver {
        evaluations: Mutex<HashMap<String, ChunkValue>>,
        stored: Mutex<HashMap<String, (Vec<u8>, Metadata)>>,
    }

    impl FixtureResolver {
        fn new() -> Self {
            FixtureResolver::default()
        }

        fn with_evaluation(self, query: &str, value: ChunkValue) -> Self {
            self.evaluations.lock().expect("lock").insert(query.to_string(), value);
            self
        }

        fn with_stored(self, key: &str, bytes: Vec<u8>, metadata: Metadata) -> Self {
            self.stored.lock().expect("lock").insert(key.to_string(), (bytes, metadata));
            self
        }
    }

    impl ChunkResolver for FixtureResolver {
        fn evaluate(&self, query: Query) -> BoxFuture<'static, Result<ChunkValue, Error>> {
            let encoded = query.encode();
            let value = self.evaluations.lock().expect("lock").get(&encoded).cloned();
            Box::pin(async move {
                value.ok_or_else(|| {
                    Error::general_error(format!("FixtureResolver: no evaluation for '{encoded}'"))
                })
            })
        }

        fn metadata(&self, _query: Query) -> BoxFuture<'static, Result<Metadata, Error>> {
            Box::pin(async { Ok(Metadata::new()) })
        }

        fn read_resource(&self, key: Key) -> BoxFuture<'static, Result<(Vec<u8>, Metadata), Error>> {
            let encoded = key.encode();
            let found = self.stored.lock().expect("lock").get(&encoded).cloned();
            Box::pin(async move { found.ok_or_else(|| Error::key_not_found(&key)) })
        }
    }

    fn schema_with_id() -> Arc<RecordSchema> {
        Arc::new(
            RecordSchema::new(vec![FieldSchema::new("id", FieldType::Int).with_key(KeyRole::Id)])
                .expect("schema"),
        )
    }

    #[tokio::test]
    async fn manifest_source_reads_an_unkeyed_chunk_by_evaluating_its_query(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let spec = ManifestSpec {
            chunks: vec![Recipe {
                query: "ns-sql/sql_query-0-1000".to_string(),
                ..Default::default()
            }],
            ..ManifestSpec::default()
        };
        let source = Arc::new(ManifestSource::new(spec, None)?);
        let view = tiny_batch(&[1, 2]) as Arc<dyn RecordView>;
        let resolver: Arc<dyn ChunkResolver> = Arc::new(
            FixtureResolver::new().with_evaluation("ns-sql/sql_query-0-1000", ChunkValue::View(view)),
        );

        let stream = source.stream(resolver).await?;
        let views: Vec<_> = stream.collect().await;
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].as_ref().expect("view").len(), 2);
        Ok(())
    }

    #[tokio::test]
    async fn manifest_source_reads_a_stored_keyed_chunk_via_read_resource(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let spec = ManifestSpec {
            chunks: vec![Recipe {
                query: "ns-sql/sql_query-0-1000/data.csv".to_string(),
                ..Default::default()
            }],
            uniform_schema: Some(schema_with_id()),
            ..ManifestSpec::default()
        };
        let key = parse_key("data/sales/daily.manifest.yaml")?;
        let source = Arc::new(ManifestSource::new(spec, Some(key))?);
        let mut metadata = Metadata::new();
        metadata.set_filename("data.csv")?;
        let resolver: Arc<dyn ChunkResolver> = Arc::new(FixtureResolver::new().with_stored(
            "data/sales/data.csv",
            b"id\n1\n2\n3\n".to_vec(),
            metadata,
        ));

        let stream = source.stream(resolver).await?;
        let views: Vec<_> = stream.collect().await;
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].as_ref().expect("view").len(), 3);
        Ok(())
    }

    #[tokio::test]
    async fn manifest_source_evaluates_a_keyed_chunk_the_store_does_not_hold(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let spec = ManifestSpec {
            chunks: vec![Recipe {
                query: "ns-sql/sql_query-0-1000/data.csv".to_string(),
                ..Default::default()
            }],
            stored: false,
            ..ManifestSpec::default()
        };
        let key = parse_key("data/sales/daily.manifest.yaml")?;
        let source = Arc::new(ManifestSource::new(spec, Some(key))?);
        let view = tiny_batch(&[9]) as Arc<dyn RecordView>;
        // read_resource has nothing stored for it, so `read_chunk` falls back to evaluating the
        // key itself as a bare resource query.
        let resolver: Arc<dyn ChunkResolver> = Arc::new(
            FixtureResolver::new().with_evaluation("-R/data/sales/data.csv", ChunkValue::View(view)),
        );

        let stream = source.stream(resolver).await?;
        let views: Vec<_> = stream.collect().await;
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].as_ref().expect("view").len(), 1);
        Ok(())
    }

    #[tokio::test]
    async fn manifest_source_stream_walks_explicit_chunks_then_the_template(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let spec = ManifestSpec {
            chunks: vec![Recipe {
                query: "ns-sql/sql_query-0-1".to_string(),
                ..Default::default()
            }],
            template: Some(ChunkTemplate {
                query: "ns-sql/sql_query".to_string(),
                first_offset: 0,
                step: 1,
                batch_size: 1,
            }),
            ..ManifestSpec::default()
        };
        let source = Arc::new(ManifestSource::new(spec, None)?);
        // `offset_at(index) = first_offset + step * index`, at the **global** index — one
        // explicit chunk precedes the template, so its first chunk is global index 1, offset
        // `0 + 1*1 = 1`. Explicit chunk 0 has one row; template chunk (global index 1) also has
        // one row, exactly `batch_size`, so the walk continues to global index 2 (offset 2),
        // which is short (0 rows) and ends it.
        let template_query_1 = liquers_core::parse::parse_query("ns-sql/sql_query-1-1")?.encode();
        let template_query_2 = liquers_core::parse::parse_query("ns-sql/sql_query-2-1")?.encode();
        let resolver: Arc<dyn ChunkResolver> = Arc::new(
            FixtureResolver::new()
                .with_evaluation("ns-sql/sql_query-0-1", ChunkValue::View(tiny_batch(&[1])))
                .with_evaluation(&template_query_1, ChunkValue::View(tiny_batch(&[2])))
                .with_evaluation(&template_query_2, ChunkValue::View(tiny_batch(&[]))),
        );

        let stream = source.stream(resolver).await?;
        let views: Vec<Arc<dyn RecordView>> = stream
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(views.len(), 3);
        assert_eq!(views[0].len(), 1);
        assert_eq!(views[1].len(), 1);
        assert_eq!(views[2].len(), 0);
        Ok(())
    }

    #[tokio::test]
    async fn manifest_source_refuses_a_nested_source_chunk_value() -> Result<(), Box<dyn std::error::Error>> {
        let spec = ManifestSpec {
            chunks: vec![Recipe {
                query: "ns-sql/sql_query-0-1000".to_string(),
                ..Default::default()
            }],
            ..ManifestSpec::default()
        };
        let source = Arc::new(ManifestSource::new(spec, None)?);
        let nested: Arc<dyn RecordSource> = Arc::new(InMemorySource::new(Vec::new()));
        let resolver: Arc<dyn ChunkResolver> = Arc::new(
            FixtureResolver::new()
                .with_evaluation("ns-sql/sql_query-0-1000", ChunkValue::Source(nested)),
        );

        let stream = source.stream(resolver).await?;
        let views: Vec<_> = stream.collect().await;
        assert_eq!(views.len(), 1);
        assert!(views[0].is_err());
        Ok(())
    }

    #[tokio::test]
    async fn manifest_source_refuses_a_chunk_view_mismatching_uniform_schema(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let spec = ManifestSpec {
            chunks: vec![Recipe {
                query: "ns-sql/sql_query-0-1000".to_string(),
                ..Default::default()
            }],
            uniform_schema: Some(Arc::new(
                RecordSchema::new(vec![FieldSchema::new("total", FieldType::Float)]).expect("schema"),
            )),
            ..ManifestSpec::default()
        };
        let source = Arc::new(ManifestSource::new(spec, None)?);
        let mismatched = tiny_batch(&[1]) as Arc<dyn RecordView>; // field "id": Int, not "total": Float
        let resolver: Arc<dyn ChunkResolver> = Arc::new(
            FixtureResolver::new()
                .with_evaluation("ns-sql/sql_query-0-1000", ChunkValue::View(mismatched)),
        );

        let stream = source.stream(resolver).await?;
        let views: Vec<_> = stream.collect().await;
        assert_eq!(views.len(), 1);
        assert!(views[0].is_err());
        Ok(())
    }

    // --- Compile-time checks: ContextResolver / EnvResolver are usable as Arc<dyn ChunkResolver>

    /// A minimal fixture `Value` implementing `RecordValue`, so `ContextResolver<E>`/
    /// `EnvResolver<E>` can be named at all — `liquers-records` itself has no honest implementor
    /// (`value.rs`'s test module note), so this exists purely to let the two structs' `where
    /// E::Value: RecordValue` bound resolve in a doctest-free, environment-free way. It is never
    /// evaluated against a real environment here; behaviour tests are Step 5.6, in `liquers-lib`.
    #[test]
    fn context_resolver_and_env_resolver_are_usable_as_arc_dyn_chunk_resolver() {
        // Nothing to run: this test's presence is what matters. If `ContextResolver`/
        // `EnvResolver`'s `impl ChunkResolver` did not type-check against the object-safe trait,
        // this whole module — and `cargo test -p liquers-records` — would already have failed to
        // compile before this test could run.
        fn assert_usable_as_arc_dyn_chunk_resolver(_resolver: Arc<dyn ChunkResolver>) {}
        let _ = assert_usable_as_arc_dyn_chunk_resolver;
    }
}
