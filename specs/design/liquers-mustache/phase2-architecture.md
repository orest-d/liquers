# Phase 2: Solution & Architecture — liquers-mustache

## Overview

A new crate, `liquers-mustache`, depends on `liquers-core` only. It contains:
- a template lexer and parser (nom, with `nom_locate` spans that start at the template's origin);
- a **shadow query parser** that produces a `QueryTemplate` and is held to `parse_query` by AC-14;
- an async renderer over a `TemplateHost` trait;
- a highlighter.

`liquers-lib` (feature `templates`, on by default) adds the `ns-mustache` commands, `ContextHost`,
and the `DataNode` adapters for its value types.

**Departures from Phase 1.** Phase 1 put one module into core. This design adds three more core
items, all additive:
1. `ValueInterface::data_node`, with a default, so the renderer can view any `E::Value` without
   knowing the value type.
2. `Context::apply_ad_hoc`, because an override needs a `Recipe` with arguments, and
   `Context::apply` takes only a `Query`.
3. Deprecation of `simple_template` (`evaluate_simple_template` and `parse::SimpleTemplate`).

AC-7 and AC-16 route through `apply_ad_hoc`, which wraps `AssetManager::apply_for`, rather than
through `AssetManager::apply` directly. `liquers-py` implements `ValueInterface`
(`liquers-py/src/value.rs`), and it inherits the default with no change.

**Rejected alternatives:**
- **A Mustache crate (`ramhorns`, `mustache`) with query tags added.** Its lexer closes
  `{{#$ …{{area}}…}}` at the inner `}}`, its positions are not ours, and its `serde` data model
  converts whole values (AC-15).
- **A `Placeholder` variant in `StyledQueryToken`.** It breaks exhaustive matches outside core.
- **An `ExtValue::Template` for cached ASTs.** It needs a `TypeInfo` and serialization for little
  gain, since the rendered result is cached anyway. Deferred.
- **A blanket `impl<T: DataNode> AsyncDataNode`.** See `data-node-interface.md`.

## Known-Issue Preflight

None of these blocks the design or needs fixing first.

| Issue | Priority | Impact | Action |
|---|---|---|---|
| `NO-QUERY-AWARE-TEMPLATE-LANGUAGE`, `SIMPLE-TEMPLATE-HAS-NO-COMMAND` | P2 | Sources | Closed by this design |
| `RECIPE-OVERRIDE-ONLY-LAST-ACTION` | P3 | A non-scalar placeholder works only in the last action | Designed around: an error at the placeholder (AC-16) |
| `STORE-HANDLER-FIELD-ACCESS-HARDCODED` | P3 | First other consumer of `DataNode` | Kept out of scope. The `MetadataRecord` impl here is what it will use |
| `COMMAND-CANNOT-BE-RUN-WITH-A-RESTRICTED-CONTEXT` | P2 | Untrusted templates run any query, and `{{$$ }}` can inject one | Documented as unsafe |
| `QUERY-AST-DISCARDS-ENTITIES` | P3 | Parity compares decoded ASTs | Parity is defined as `Query`'s `PartialEq` |
| `INLINE-MANAGER-DYNAMIC-DEPENDENCY-CHAIN-NESTS` | P3 | Partials nest dynamic dependencies | Partial depth is capped at 32 |
| `CORE-TOKIO-REMOVAL` | P3 | The new crate must not add tokio | Use `futures` and `maybe_send` only |

## Interfaces

**`liquers-core/src/data_node.rs`.** The types are as in `data-node-interface.md`, with three
changes that make the traits compile on both targets:

```rust
pub trait NodeIter<'a>: Iterator<Item = NodeRef<'a>> + MaybeSend {}          // as RecordStream does
pub trait KeyIter<'a>: Iterator<Item = Cow<'a, str>> + MaybeSend {}          // (liquers-records batch.rs)
pub trait NodeStream<'a>: Stream<Item = Result<AnyNode, Error>> + MaybeSend {}
// DataNode::items -> Box<dyn NodeIter<'_> + '_>, keys -> Box<dyn KeyIter<'_> + '_>,
// AsyncDataNode::items -> Pin<Box<dyn NodeStream<'_> + '_>>
fn as_any(&self) -> Option<&dyn std::any::Any> { None }    // a host recovering the wrapped value
```

`AsyncDataNode` and `TemplateHost` use the core attribute pair: `cfg_attr(not(wasm32), async_trait)`
and `cfg_attr(wasm32, async_trait(?Send))`. They are implemented for core `Value`, `MetadataRecord`
and `serde_json::Value`, all sync.

**`ValueInterface::data_node(value: Arc<Self>) -> AnyNode`** is sync and only wraps. By default it
is a JSON snapshot through `try_into_json_value`, or an `Opaque` node when that fails. In
`liquers-lib`, the `Value` is `CombinedValue<SimpleValue, ExtValue>` (`value/mod.rs`), so the
override lives in the `CombinedValue` impl (`value/extended.rs`). It delegates by arm: `SimpleValue`
gets a `DataNode` impl, and `ExtValue` gets a defaulted `ValueExtension::data_node`, overridden for
`RecordView` (sync), `RecordSource` (async) and `PolarsDataFrame` (sync).

**`Context::apply_ad_hoc(&self, recipe: Recipe, to: State<E::Value>) -> Result<AssetRef<E>, Error>`**
is async. It resolves `recipe`'s query exactly as `apply` does (`reject_relative_query`, then
`resolve_query_from_cwd`), leaves `recipe.cwd` alone, and calls `apply_for`. It then copies the
ad-hoc asset's `MetadataRecord::dependencies` into `add_dependency`, and copies nothing from legacy
metadata.

**`liquers-mustache`.** Everything is sync except rendering and `instantiate`, which may read async
nodes.

```rust
pub struct Template { pub source: Arc<str>, pub nodes: Vec<TNode> }
pub enum TNode { Text(String), Name { name: DottedName, escape: Escape, position: Position },
    Section { inverted: bool, head: SectionHead, body: Vec<TNode>, position: Position },
    Partial { target: PartialTarget, indent: String, position: Position },
    Query { tag: QueryTag, escape: Escape, position: Position } }
pub enum SectionHead { Name(DottedName), Query(QueryTag) }
pub enum PartialTarget { Key(String), Query(QueryTag) }
pub struct QueryTag { pub expr: QueryExpr, pub apply: Option<ApplyTarget>, pub position: Position }
pub enum QueryExpr { Hygienic(QueryTemplate), Classic(Vec<ClassicPiece>) }   // {{$ }} / {{$$ }}
pub enum ApplyTarget { Current, Name(DottedName) }                            // {{$. }} / {{$.a.b }}
pub enum Escape { Default, Raw }
pub struct QueryTemplate { pub segments: Vec<SegmentTemplate>, pub rooted: bool }
pub enum Piece { Literal(String), Slot(DottedName, Position) }   // "Slot": core's plan has a Placeholder
pub enum ParamTemplate { Text(Vec<Piece>, Position), Link(QueryTemplate, Position) }
impl QueryTemplate {
    pub fn try_into_query(&self) -> Option<Query>;
    pub async fn instantiate(&self, data: &ContextStack<'_>, cmr: &CommandMetadataRegistry)
        -> Result<(Query, Vec<(String, serde_json::Value)>), Error>;            // query + overrides
}
pub fn parse_template(text: &str) -> Result<Template, Error>;
pub fn validate_template(text: &str, cmr: Option<&CommandMetadataRegistry>) -> Vec<Diagnostic>;
pub fn parse_query_template(text: &str) -> Result<QueryTemplate, Error>;
pub fn highlight(t: &Template) -> Vec<TemplateToken>;   // Query(StyledQueryToken) | Slot | Tag | Text
pub struct Diagnostic { pub severity: Severity, pub message: String, pub position: Position }
pub trait TemplateHost: MaybeSend + MaybeSync {          // async-trait pair as above
    async fn evaluate(&self, request: QueryRequest) -> Result<AnyNode, Error>;
    async fn partial(&self, key: &str) -> Result<String, Error>; }
pub struct QueryRequest { pub query: Query, pub overrides: Vec<(String, serde_json::Value)>,
    pub input: Option<AnyNode>, pub position: Position }
pub struct RenderOptions { pub html_escape: bool, pub max_partial_depth: usize }
pub async fn render<H: TemplateHost>(t: &Template, root: Vec<AnyNode>, host: &H, o: &RenderOptions)
    -> Result<String, Error>;
pub struct MapHost { pub partials: BTreeMap<String, String> }   // refuses queries: spec suite, tests
```

The renderer recurses through boxed futures (`maybe_send`). Partials keep a stack of keys: a key
already on the stack is a **cycle error** (AC-12), and depth is capped at 32 for dynamic partials.

**`ContextHost<E>`** (`liquers-lib`) routes as described in `query-placeholders.md`:
- with no input and no overrides, `get_dependency_state`;
- otherwise, `apply_ad_hoc`.

An apply input becomes a `State` by downcasting through `as_any`, which clones the `Arc<V>`, or else
by `to_json` and `try_from_json_value`. That second path is the usual one for a child of a frame, and
it drops the child's metadata.

**Store folder node** (AC-15, async, `liquers-lib/src/mustache/folder.rs`). It is a map over the
child names (`AsyncStore::listdir`). A file child is read on demand through `get_dependency_state`,
and a folder child is another folder node; `is_dir` is used to tell them apart, without a read.
Keys keep the listing order.

## Integration Points

| Crate | Change |
|---|---|
| `liquers-core` | `data_node.rs` (new); `lib.rs`; `value.rs` (`data_node`, `Value` impl); `metadata.rs` (`MetadataRecord` impl); `context.rs` (`apply_ad_hoc`); `parse.rs` and `interpreter.rs` (`#[deprecated]` on `SimpleTemplate`, `parse_simple_template`, `evaluate_simple_template`; no templates exist to convert, since no command uses them) |
| `liquers-records` | A row adapter over `RecordView` (sync) and a source adapter (async) |
| `liquers-mustache` (new) | `src/{lexer,parse,ast,query_template,instantiate,render,highlight,diagnostics}.rs`; `tests/spec/` (the vendored Mustache spec, MIT); `tests/parity.rs`; added to the workspace members |
| `liquers-lib` | Feature `templates`; `src/mustache/{mod,commands,host,folder,egui}.rs` (`egui.rs`, gated on `egui`, renders `TemplateToken`s for AC-10); `value/extended.rs` and `value/mod.rs` (`data_node`); `register_mustache_commands!` and its no-op stand-in in `register_all_commands!` |
| Scripts and generated files | `check-build-matrix.sh` rows for `templates` and for `liquers-mustache` on wasm32; `specs/command_registry.yaml` regenerated |

## Error Handling

| Failure | Constructor |
|---|---|
| Template grammar (unclosed or mismatched section, bad delimiter, `{{{x}}}` in a query tag) | `Error::parse_error(msg).with_position(&pos)` |
| Malformed hygienic query | `Error::query_parse_error(text, err, &pos)`, in template coordinates |
| Malformed classic query after expansion | `Error::query_parse_error(expanded, err, &tag_pos)`, with the offset in the message |
| Value invalid in P2–P5 | `Error::conversion_error_at_position(value, "<position kind>", &pos)` |
| Missing name or null in a query | `Error::general_error(msg).with_position(&pos)` |
| Non-scalar outside a whole P1 slot of the last action | `Error::not_supported(msg).with_position(&pos)` |
| Partial cycle or depth exceeded | `Error::execution_error(msg).with_position(&pos)` |
| Async-only node given to a sync consumer | `Error::not_supported(msg)` |
| Query evaluation failure | Propagated; `with_position(&tag_pos)` only when its own position is unknown |

There is no `unwrap` or `expect` in library code, and every match on the new enums is exhaustive.

## Relevant Commands

Namespace `mustache` (Phase 1, open question 7; confirmed at this gate). Interacting namespaces:
`rec` and `search` (records as data), `pl` (frames as data).

```rust
register_command!(cr, async fn render(state, template: String = "", data: Value = "", context) -> result
    namespace: "mustache" label: "Render template"
    doc: "Render a template with query tags; the state is the root context, or the template when `template` is empty")?;
register_command!(cr, async fn validate(state, template: String = "", context) -> result
    namespace: "mustache" label: "Validate template" doc: "Positioned errors and warnings; runs nothing")?;
register_command!(cr, fn highlight(state, style: String = "html") -> result
    namespace: "mustache" label: "Highlight template" doc: "The template as styled HTML or ANSI text")?;
```

- **`template`** is normally a link (`render-~X~-R/templates/issues.md~E`). The planner then
  evaluates it and records it as a dependency.
- **`data`** (`ArgumentType::Any`) holds named data above the state, such as `{"area": …}` as a
  recipe argument. This is how AC-8's named arguments arrive. Empty text means no data.
- **The result** is text. Its media type follows the asset's filename, `text/plain` when there is
  none. HTML escaping is on when that media type is `text/html`.
- **`highlight`** is a third command beyond AC-11, serving AC-10. **`validate`** reports an unknown
  partial only when the partial is a literal key; it checks the key's syntax, not its existence,
  since nothing is read (AC-9). The `apply` command of Phase 1, question 7 is dropped, because tags
  choose the mode (AC-11).

## Documentation Architecture

| Path | Kind | Change |
|---|---|---|
| `specs/reference/TEMPLATE_LANGUAGE.md` | reference, new (`core/query`, `lib/commands`) | Grammar, tags, P1–P6, data and `_meta`, escaping, evaluation routes, diagnostics, conformance |
| `specs/guides/TEMPLATE_GUIDE.md` | guide, new | Writing, validating and rendering templates; implementing `DataNode`; a context pack as the example |
| `specs/reference/VALUE_TYPE_SYSTEM.md` | update | §`DataNode` and `ValueInterface::data_node` |
| `specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md` | update | `Context::apply_ad_hoc` and its dependency rule |
| `specs/reference/api/DOC_02_QUERY_LANGUAGE_REFERENCE.md` | update | Parametrized queries; `simple_template` deprecated |
| `specs/reference/PROJECT_OVERVIEW.md` | update | The new crate and the dependency flow |

`CLAUDE.md` and `specs/README.md` also gain the crate and the links. The proposed `affects_docs`
is: `specs/reference/TEMPLATE_LANGUAGE.md`, `specs/guides/TEMPLATE_GUIDE.md`,
`specs/reference/VALUE_TYPE_SYSTEM.md`, `specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md`,
`specs/reference/api/DOC_02_QUERY_LANGUAGE_REFERENCE.md` and `specs/reference/PROJECT_OVERVIEW.md`.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | The new crate; about six core files (additions); `liquers-lib` value, commands and a new module; two records adapters; the workspace `Cargo.toml`; the matrix script |
| Crates and workflows affected | core, records, mustache, lib; the registry export; the build matrix |
| Existing tests likely to change | `registry_export` and the registry-dependent tests, gated on four feature groups, gain `templates` |
| New validation | Mustache spec suite (AC-1), parity corpus (AC-14), wasm32 build and Node tests (AC-13) |
| Compatibility | Additive API, apart from the deprecation warnings |
| Concurrency | `apply_ad_hoc` runs inline and takes no job slot, so it is deadlock-safe inside an evaluation (`ASSETS-FIX1` #17) |
| Performance | Frame rows are built per row (`NodeRef::Owned`), which is fine for documents. The shadow parser is not on the hot path of `parse_query` |
| Security | `{{$$ }}` and untrusted templates (preflight) |
| Recovery | The `templates` feature can be turned off; the core additions are unused without it |
| Certainty | High for the parser and renderer. **Medium** for dependency propagation (open question 12), volatility of dynamically found dependencies, and three facts Phase 3 must test: a linked `.md` arrives as text, the asset's filename is known when the command runs, and the cwd that relative partial keys resolve against (open question 13) |

## Open Questions

- **7 [proposed]** Namespace `mustache`. *Recommended: confirm.* Phase 1 questions 2, 5, 10 and 11
  are taken as approved with Phase 1.
- **12 [proposed]** Should `apply_ad_hoc` propagate the ad-hoc asset's dependencies to the parent?
  *Recommended: yes.* Without it, a template using `{{$. q}}` or an override is never recomputed when
  the data behind `q` changes.
- **13 [detail]** What do relative partial keys resolve against? A template passed as a link has no
  key of its own. *Recommended:* the command's cwd in the MVP, with absolute keys in the guide's
  examples. Resolving against the template's own folder is a follow-up.
