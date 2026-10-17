# Phase 2: Solution & Architecture — liquers-template

## Overview

A new crate, **`liquers-template`**, depends on `liquers-core` only. Its parts are:
- **Syntax-neutral modules:** the `QueryTemplate` AST, the shadow query parser held to
  `parse_query` by AC-14, instantiation, and the async renderer interface (`TemplateHost`).
- **`mustache`:** the first syntax, with its AST, lexer and parser, encoder, and renderer.

A later syntax would be a sibling module and a new `Template` variant. `liquers-lib` (feature
`templates`, on by default) adds the `ns-template` commands, `ContextHost`, and the `DataNode`
adapters for its value types.

**Departures from Phase 1.** Phase 1 put one core module in scope. This design adds four more core
items, all additive:
- `ValueInterface::data_node`, with a default, so the renderer can view any `E::Value`;
- `Context::apply_ad_hoc`, because an override needs a `Recipe` with arguments, and
  `Context::apply` takes a `Query` only;
- `ArgumentInfo::with_default_link`, for template commands (an alias of `render`);
- deprecation of `simple_template` (`SimpleTemplate`, `parse_simple_template`,
  `evaluate_simple_template`). No command uses it, so there are no templates to convert.

AC-7 and AC-16 route through `apply_ad_hoc`, which wraps `AssetManager::apply_for`. `liquers-py`
implements `ValueInterface` (`liquers-py/src/value.rs`) and inherits the default unchanged. AC-10
(highlighting) was withdrawn on 2026-10-17 and filed with its design as
`TEMPLATE-HAS-NO-HIGHLIGHTER`.

**Rejected alternatives:**
- **An existing Mustache crate with query tags added.** Its lexer closes `{{#$ …{{area}}…}}` at
  the inner `}}`, it has no positions of ours, and its `serde` data model converts whole values.
- **An `ExtValue::Template` for cached ASTs.** It would need a `TypeInfo` and serialization for
  little gain, since the rendered result is cached anyway. Deferred.
- **A blanket `impl<T: DataNode> AsyncDataNode`.** See `data-node-interface.md`.

## Known-Issue Preflight

None of these blocks the design or must be fixed first.

| Issue | Priority | Impact | Action |
|---|---|---|---|
| `NO-QUERY-AWARE-TEMPLATE-LANGUAGE`, `SIMPLE-TEMPLATE-HAS-NO-COMMAND` | P2 | Sources | Closed by this design |
| `RECIPE-OVERRIDE-ONLY-LAST-ACTION` | P3 | A non-scalar slot works only in the last action | Designed around: an error at the slot (AC-16) |
| `STORE-HANDLER-FIELD-ACCESS-HARDCODED` | P3 | First other consumer of `DataNode` | Out of scope; uses the `MetadataRecord` impl made here |
| `TEMPLATE-HAS-NO-HIGHLIGHTER` | P3 | AC-10, moved out | Every AST element keeps a `Position`, which is what it needs |
| `COMMAND-CANNOT-BE-RUN-WITH-A-RESTRICTED-CONTEXT` | P2 | Untrusted templates run any query, and `{{$$ }}` can inject one | Documented as unsafe |
| `QUERY-AST-DISCARDS-ENTITIES` | P3 | Parity compares decoded ASTs | Parity is defined as `Query`'s `PartialEq` |
| `INLINE-MANAGER-DYNAMIC-DEPENDENCY-CHAIN-NESTS` | P3 | Partials nest dynamic dependencies | Partial depth is capped at 32 |
| `CORE-TOKIO-REMOVAL` | P3 | The new crate must not add tokio | Use `futures` and `maybe_send` only |

## AST

**Principles, taken from `Query` (`liquers-core/src/query.rs`):**
- Plain structs and enums with `pub` fields, deriving `Serialize, Deserialize, Debug, Clone` (and
  `Default` where `Query`'s counterpart has it).
- **Every element carries a `Position`** (`Position::unknown()` when built by hand), set by
  `with_position`. `PartialEq`, `Eq` and `Hash` are implemented by hand and ignore positions and
  provenance, as `ResourceName` and `ActionRequest` do.
- `encode()` returns canonical text, and `Display` writes it. `parse(encode(t)) == t` holds for
  every parsed `t`, and `encode ∘ parse` is idempotent (`escape.rs`, §Properties).
- Literal text is held **decoded**, as `ActionParameter::String` holds it, and it is escaped only in
  `encode()`.

**Query templates** (`liquers-template/src/query/ast.rs`) mirror the core AST one for one. A
placeholder is a `Slot` in a `Piece` list, wherever a core node holds one token:

```rust
pub struct QueryTemplate { pub segments: Vec<QuerySegmentTemplate>, pub rooted: bool, pub source: QuerySource }
pub enum QuerySegmentTemplate { Resource(ResourceQuerySegmentTemplate), Transform(TransformQuerySegmentTemplate) }
pub struct ResourceQuerySegmentTemplate { pub header: Option<SegmentHeader>, pub key: KeyTemplate }   // core header: no slots
pub struct KeyTemplate(pub Vec<TokenTemplate>);                                                     // P4
pub struct TransformQuerySegmentTemplate { pub header: Option<SegmentHeader>,
    pub query: Vec<ActionRequestTemplate>, pub filename: Option<TokenTemplate> }                    // P5
pub struct ActionRequestTemplate { pub name: TokenTemplate, pub parameters: Vec<ActionParameterTemplate>,
    pub position: Position }                                                                        // P3, and P2 via `ns`
pub enum ActionParameterTemplate { String(Vec<Piece>, Position), Link(QueryTemplate, Position) }   // P1, P6
pub struct TokenTemplate { pub pieces: Vec<Piece>, pub position: Position }  // a name, segment or filename
pub enum Piece { Literal(String), Slot(Slot) }
pub struct Slot { pub name: Name, pub position: Position }
pub struct Name { pub parts: Vec<NamePart>, pub position: Position }   // `a.b.0[k]`; empty parts = `.`
pub enum NamePart { Field(String, Position), Index(usize, Position), Lookup(LookupKey, Position) }
pub enum LookupKey { Name(Name), Integer(i64), Text(String) }         // `[y]` `[-1]` `["a b"]`

impl From<&Query> for QueryTemplate { … }     // lifts a query: every token one Literal piece
impl QueryTemplate {
    pub fn encode(&self) -> String;
    pub fn try_into_query(&self) -> Option<Query>;            // None when any Slot is present
    pub fn slots(&self) -> impl Iterator<Item = &Slot>;
    pub async fn instantiate(&self, data: &ContextStack<'_>, cmr: &CommandMetadataRegistry)
        -> Result<Instantiated, Error>;
}
pub struct Instantiated { pub query: Query, pub overrides: Vec<(String, serde_json::Value)> }
```

Parity (AC-14) is two equalities over the corpus:
`QueryTemplate::from(&parse_query(q)?) == parse_query_template(q)?` and
`parse_query_template(q)?.try_into_query() == Some(parse_query(q)?)`, with positions compared
separately.

**Mustache templates** (`liquers-template/src/mustache/ast.rs`):

```rust
pub enum Template { Mustache(MustacheTemplate) }                     // one variant per syntax
pub struct MustacheTemplate { pub elements: Vec<Element>, pub source: QuerySource }
pub enum Element { Text(TextElement), Variable(VariableTag), Section(Section),
    Partial(PartialTag), Query(QueryTag), Comment(CommentTag), SetDelimiter(SetDelimiterTag),
    Invalid(Invalid) }                                               // a recoverable syntax error
pub struct Invalid { pub source: String, pub error: Error, pub position: Position }
pub struct TextElement { pub text: String, pub position: Position }
pub struct VariableTag { pub name: Name, pub escape: Escape, pub position: Position }      // {{x}} {{{x}}} {{&x}}
pub enum Escape { Escaped, Triple, Ampersand }                    // kept apart so encode() round-trips
pub struct Section { pub inverted: bool, pub head: SectionHead, pub elements: Vec<Element>,
    pub close: CloseTag, pub standalone: Option<Standalone>, pub position: Position }
pub enum SectionHead { Name(Name), Query(QueryTag), Invalid(Invalid) }
pub struct CloseTag { pub form: CloseForm, pub standalone: Option<Standalone>, pub position: Position }
pub enum CloseForm { Bare, Name(Name), Hygienic, Classic }        // {{/}} {{/x}} {{/$}} {{/$$}}
pub struct PartialTag { pub target: PartialTarget, pub standalone: Option<Standalone>, pub position: Position }
pub enum PartialTarget { Key(Key), Query(QueryTag), Invalid(Invalid) }   // {{> key}} / {{>$ q}} {{>$$ q}}
pub struct QueryTag { pub body: QueryBody, pub apply: Option<ApplyTarget>, pub escape: Escape,
    pub position: Position }                                     // {{$ q}} {{&$ q}} {{$. q}} …
pub enum QueryBody { Hygienic(QueryTemplate), Classic(ClassicQuery), Invalid(Invalid) }
pub struct ClassicQuery { pub pieces: Vec<ClassicPiece>, pub position: Position }
pub enum ClassicPiece { Text(String, Position), Slot(Slot) }      // expanded verbatim, then parsed
pub enum ApplyTarget { Current(Position), Name(Name) }            // {{$. q}} / {{$.a.b q}}
pub struct CommentTag { pub text: String, pub standalone: Option<Standalone>, pub position: Position }
pub struct SetDelimiterTag { pub open: String, pub close: String, pub standalone: Option<Standalone>,
    pub position: Position }
pub struct Standalone { pub indent: String, pub newline: String }  // the line around a standalone tag
```

`Standalone` keeps the whitespace that the Mustache spec removes around a standalone tag. Rendering
drops it and re-indents partials with `indent`, while `encode()` restores it. The encoder tracks
`SetDelimiterTag`s, so it writes every tag with the delimiters in force at its position. `Section`
and its `CloseTag` each have a position, so a mismatched close is reported at both.

**Names and recoverable syntax errors** are specified in [`names-and-errors.md`](./names-and-errors.md):
the name grammar with list indexing (`items.0`) and the strict lookup `x[y]`, and the `Invalid`
nodes. An `Invalid` node keeps its source text, so `encode()` still round-trips.

## Interfaces

**`liquers-core/src/data_node.rs`** is as described in `data-node-interface.md`, with the iterator
helper traits that make it compile on both targets, following `RecordStream`
(`liquers-records/src/batch.rs`):

```rust
pub trait NodeIter<'a>: Iterator<Item = NodeRef<'a>> + MaybeSend {}
pub trait KeyIter<'a>: Iterator<Item = Cow<'a, str>> + MaybeSend {}
pub trait NodeStream<'a>: Stream<Item = Result<AnyNode, Error>> + MaybeSend {}
fn as_any(&self) -> Option<&dyn std::any::Any> { None }      // on DataNode: a host recovering the value
```

It is implemented for core `Value`, `MetadataRecord` and `serde_json::Value`, all sync.
`AsyncDataNode` and `TemplateHost` use core's attribute pair: `cfg_attr(not(wasm32), async_trait)`
and `cfg_attr(wasm32, async_trait(?Send))`.

**`ValueInterface::data_node(value: Arc<Self>) -> AnyNode`** is sync. Its default is a JSON snapshot
through `try_into_json_value`, or `Opaque` when that fails. In `liquers-lib` the override is on
`CombinedValue` (`value/extended.rs`), delegating by arm:
- `SimpleValue` implements `DataNode`;
- `ExtValue` gets a defaulted `ValueExtension::data_node` (trait in `value/extended.rs`), overridden
  in `value/mod.rs` for `RecordView` and `PolarsDataFrame` (sync).
- A `RecordSource` cannot stream without a `ChunkResolver`, which only a `Context` provides
  (`ContextResolver`), so `data_node` gives it as `Opaque`. `ContextHost::node_of(value)` checks for
  a `RecordSource` first and wraps it with `liquers-records`' async adapter and a
  `ContextResolver`; every other value goes through `data_node`.

**`Context::apply_ad_hoc(&self, recipe: Recipe, to: State<E::Value>) -> Result<AssetRef<E>, Error>`**
is async. It resolves the recipe's query as `apply` does (`reject_relative_query`, then
`resolve_query_from_cwd`), leaves `recipe.cwd` alone, and calls `apply_for`. It then adds the
ad-hoc asset's `MetadataRecord::dependencies` to the parent through `add_dependency` (open
question 12, accepted). Legacy metadata contributes nothing. Plan dependencies are recorded only
for a non-volatile plan (`interpreter.rs`), so a volatile ad-hoc result contributes no records.
Volatility is not passed on: `get_dependency_state` does not pass it on either, because volatility
is fixed before evaluation (`design/volatility-system`, decision 4). A template whose queries are
volatile is rendered by a query that says so (`…/v`), or by a template command declared
`volatile` (decision D6). A failed ad-hoc evaluation is returned as its error.

**Renderer** (`liquers-template`). Rendering and `instantiate` are async; everything else is sync.

```rust
pub fn parse_template(text: &str, syntax: TemplateSyntax) -> Result<Template, Error>;
pub fn parse_query_template(text: &str) -> Result<QueryTemplate, Error>;
pub fn validate_template(text: &str, syntax: TemplateSyntax, cmr: Option<&CommandMetadataRegistry>)
    -> Vec<Diagnostic>;
pub struct Diagnostic { pub severity: Severity, pub message: String, pub position: Position }
pub trait TemplateHost: MaybeSend + MaybeSync {
    async fn evaluate(&self, request: QueryRequest) -> Result<AnyNode, Error>;
    async fn partial(&self, key: &Key) -> Result<String, Error>;
    fn command_registry(&self) -> &CommandMetadataRegistry;  // names overrides at instantiation
    fn warn(&self, error: &Error);                           // a recovered error, to the log
}
pub struct QueryRequest { pub query: Query, pub overrides: Vec<(String, serde_json::Value)>,
    pub input: Option<AnyNode>, pub position: Position }
pub enum ErrorMode { Strict, Expand(ExpandFormat), Silent }
pub enum ExpandFormat { Text, Html, Markdown }
pub struct RenderOptions { pub html_escape: bool, pub max_partial_depth: usize, pub errors: ErrorMode }
pub struct ContextStack<'a> { frames: Vec<Frame<'a>> }
pub enum FrameKind { Data, Names }                  // a `Names` frame is looked up, never `{{.}}`
impl<'a> ContextStack<'a> { pub fn new() -> Self; pub fn push(&mut self, node: AnyNode, kind: FrameKind); }
pub async fn render<H: TemplateHost>(t: &Template, stack: ContextStack<'_>, host: &H, o: &RenderOptions)
    -> Result<String, Error>;
pub struct MapHost { pub partials: BTreeMap<Key, String>, pub warnings: Mutex<Vec<Error>> }   // refuses queries
```

`ContextStack<'a>` (`src/stack.rs`) holds frames, either `NodeRef<'a>` borrowed during sync
iteration or an owned `AnyNode`, each `Data` or `Names`. `{{.}}` and the apply forms take the top
`Data` frame; a `Names` frame (`render`'s `data`) only answers name lookups. Names resolve by the Mustache rules plus `_meta`. The renderer recurses through boxed
futures (`maybe_send`). Partials keep a stack of keys: a key already on it is a **cycle error**
(AC-12), and depth is capped at 32 for dynamic partials. `DataNode` and `AsyncDataNode` gain a
defaulted `index(i)` for list indexing (`names-and-errors.md`).

**`ContextHost<E>`** (`liquers-lib/src/template/host.rs`) holds the `Context` and the `cwd` key
(below). Before evaluating, it makes relative keys absolute with `Query::to_absolute(&cwd)`, and
implements `warn` with `Context::warning`, and routes as in `query-placeholders.md`:
- no input and no overrides go to `get_dependency_state`;
- anything else goes to `apply_ad_hoc`.

An apply input becomes a `State` through `as_any`, which clones the `Arc<V>`, or else through
`to_json` and `try_from_json_value`. The second path is the usual one for a child of a frame, and
it drops the child's metadata.

**Store folder node** (AC-15, async, `liquers-lib/src/template/folder.rs`). It is a map of child
names (`AsyncStore::listdir`), in listing order. `is_dir` tells a folder child from a file child
without reading it. A file child is read on demand through `get_dependency_state`.

## Integration Points

| Crate | Change |
|---|---|
| `liquers-core` | `data_node.rs` (new, with the `Value`, `MetadataRecord` and JSON impls); `lib.rs`; `value.rs` (`data_node`); `context.rs` (`apply_ad_hoc`); `command_metadata.rs` (`ArgumentInfo::with_default_link`); `#[deprecated]` in `parse.rs` and `interpreter.rs` |
| `liquers-records` | A row adapter over `RecordView` (sync) and a source adapter (async) |
| `liquers-template` (new) | `src/lib.rs`, `src/{name,stack,host,render,diagnostics}.rs` (syntax-neutral; `render.rs` dispatches on `Template`); `src/query/{ast,parse,instantiate}.rs`; `src/mustache/{ast,lexer,parse,encode,render}.rs`; `tests/spec/` (the vendored Mustache spec, MIT); `tests/{parity,spec,render,node}.rs`; added to the workspace `members` and `default-members` |
| `liquers-lib` | Feature `templates`; `src/template/{mod,commands,host,folder}.rs`; `value/extended.rs` and `value/mod.rs` (`data_node`); `register_template_commands!` and its no-op stand-in in `register_all_commands!` |
| Scripts and generated files | `check-build-matrix.sh` rows for `templates` and for `liquers-template` on wasm32; `specs/command_registry.yaml` regenerated |

## Error Handling

**Modes** (AC-18, `names-and-errors.md`). *Strict* fails with the first error. *Expand* renders the
error in place of its tag, as text, HTML or Markdown, and *silent* renders nothing; both log every
error as a warning. The unit of failure is one tag, or a whole section when its head fails. An
unclosed or mismatched section, an unterminated tag and a bad set-delimiter tag are never
recoverable and fail in every mode. Any other tag that does not parse is kept as an `Invalid` node,
which strict mode rejects before any query runs.

| Failure | Constructor |
|---|---|
| Template structure (unclosed or mismatched section, unterminated tag, bad delimiter): irrecoverable | `Error::parse_error(msg).with_position(&pos)` |
| Tag content (bad name, unknown sigil, `{{{x}}}` in a query tag): an `Invalid` node | `Error::parse_error(msg).with_position(&pos)` |
| Bracket lookup: not indexable, out of range, missing key, non-scalar key | `Error::general_error(msg).with_position(&part_pos)`, naming the kind and the key |
| Malformed hygienic query | `Error::query_parse_error(text, err, &pos)`, in template coordinates |
| Malformed classic query after expansion | `Error::query_parse_error(expanded, err, &tag_pos)`, with the offset in the message |
| Value invalid in P2–P5 | `Error::conversion_error_at_position(value, "<position kind>", &pos)` |
| Missing name or null in a query | `Error::general_error(msg).with_position(&pos)` |
| Non-scalar outside a whole P1 slot of the last action | `Error::not_supported(msg).with_position(&pos)` |
| Partial cycle or depth exceeded | `Error::execution_error(msg).with_position(&pos)` |
| Async-only node given to a sync consumer | `Error::not_supported(msg)` |
| `cwd` link does not yield a key | Conversion error from `TryFrom<Value> for Key`, raised before the command runs |
| Query evaluation failure | Propagated; `with_position(&tag_pos)` only when its own position is unknown |

There is no `unwrap` or `expect` in library code, and every match on the new enums is exhaustive.

## Relevant Commands

The namespace is **`template`** (decided 2026-10-17). It interacts with `rec` and `search` (records
as data) and `pl` (frames as data).

```rust
register_command!(cr, async fn render(state, template: String, data: Value = "", errors: String = "strict",
    cwd: Key = query "-R-key/.", context) -> result
    namespace: "template" label: "Render template"
    doc: "Render the template given as the first argument; the state and `data` are its data")?;
register_command!(cr, async fn expand(state, data: Value = "", errors: String = "strict",
    cwd: Key = query "-R-key/.", context) -> result
    namespace: "template" label: "Expand template"
    doc: "Render the state as a template; `data` is its data")?;
register_command!(cr, async fn validate(state, context) -> result
    namespace: "template" label: "Validate template" doc: "Positioned errors and warnings for the state as a template; runs nothing")?;
```

Two render commands (decided 2026-10-17), because the template and the data are two different
inputs and either can be the one a query produces:
- **`render`** takes the template as its first argument, normally a link
  (`render-~X~-R/templates/issues.md~E`), so the planner evaluates it and records it as a
  dependency. The state is the data. Having the template first is also what makes a **template
  command** possible: an alias of `render` whose head is the template link (below).
- **`expand`** takes the template as the state (`-R/templates/hello.md/-/ns-template/expand`), for a
  template that is itself computed or read in the query. Only `data` is its data.
- **`validate`** reads the template from the state, as `expand` does.

**How the state and `data` combine.** They are two frames of one Mustache context stack, never
merged into one value:

| Command | Bottom frame | Top frame | `{{.}}`, `{{$. q}}` |
|---|---|---|---|
| `render` | the state | `data`, when not empty | the state |
| `expand` | none | `data`, when not empty | `data` |

- A name is looked up in the top frame first, then the bottom, by the Mustache stack rule, so a
  field in `data` shadows the same field in the state and every other field of the state stays
  visible. `render-~X~…~E-~X~ns-fixture/ctx-core~/store~E` on a state with `area` and `title`
  sees `data`'s `area` and the state's `title`.
- `data` must be a map (or empty text, meaning no data): it holds **named** values, which is how
  AC-8's named arguments and a recipe's `{"area": …}` arrive. A list or scalar `data` is an error
  before rendering, since it would have no names and would hide the state from `{{.}}`.
- `{{.}}` and the apply forms refer to the state in `render`, because the state is the input the
  query produced; `data` only supplies names. Sections push further frames above both, as usual.
- An empty state (`State::new()`, as at the start of a query) is no frame.

**Template commands.** A command whose behaviour is a template is an alias of `render` with the
template as its head parameter, so it needs no Rust function:

```rust
cr.register_alias(
    CommandKey::new("", "pack", "open_issues"),
    CommandKey::new("", "template", "render"),
    vec![CommandParameterValue::Query(parse_query("-R/templates/open_issues.md")?)], // `template`
    vec![ArgumentInfo::any_argument("data").with_default(""),
         ArgumentInfo::string_argument("errors").with_default("strict"),
         ArgumentInfo::any_argument("cwd").with_default_link(parse_query("-R-key/.")?)],
)?.with_label("Open issues").with_doc("Open issues of the area in the state");
```

`ns-fixture/ctx-core~/store/ns-pack/open_issues` then renders the pack. The head is a link, so the
template is a dependency of every result, and an edit to it invalidates them; the alias's own
metadata key is a dependency too (`COMMAND_ALIASES.md` §5). The alias's arguments line up with
`render`'s after the head, as `register_alias` requires. `cwd` is `any_argument` because `Key`
maps to `ArgumentType::Any`; the conversion to `Key` happens in `render`'s executor, so it applies
through the alias too. `ArgumentInfo::with_default_link` is a
small core addition (today a link default is set by assigning `ArgumentInfo::default`). A head
parameter that is a `CommandParameterValue::Query` becomes a `DefaultLink`
(`ParameterValue::from_command_parameter_value`), which no alias uses yet, so Phase 3 tests it.

- **`errors`** is `strict`, `expand`, `expand_html`, `expand_md` or `silent`; anything else fails
  before rendering (open question 16).
- **`cwd`** is a key. Relative partial keys and relative keys in query tags resolve against it.
  It follows `specs/guides/COMMAND_REGISTRATION_GUIDE.md`, "Passing the working directory": a
  default link `-R-key/.`, which yields the working directory, so the folder is explicit,
  overridable and part of what names the result (open question 14). There is no hidden fallback to
  the template's own folder. A caller who wants it sets the working directory
  (`-R-cwd/templates/-/ns-template/render-~X~-R/./issues.md~E--strict`) or passes it
  (`-R/templates/issues.md/-/ns-template/expand--strict-templates`). `cwd` is last, so the link is
  promoted into the query only when every earlier argument is written (the guide's
  argument-order pitfall).
- **The result** is text. Its media type follows the asset's filename, `text/plain` when there is
  none. HTML escaping is on when that media type is `text/html`.
- **`validate`** checks a literal partial key's syntax, not its existence (AC-9). There is no
  separate apply command, because tags choose the mode (AC-11).

## Documentation Architecture

| Path | Kind | Change |
|---|---|---|
| `specs/reference/TEMPLATE_LANGUAGE.md` | reference, new (`core/query`, `lib/commands`) | Mustache syntax, tags, P1–P6, AST, data and `_meta`, escaping, evaluation routes, `cwd`, diagnostics, conformance |
| `specs/guides/TEMPLATE_GUIDE.md` | guide, new | Writing, validating, `render` versus `expand`, how the state and `data` combine; **template commands** (an alias of `render`, with the context pack as the worked example and its test); implementing `DataNode`; adding a syntax |
| `specs/guides/COMMAND_REGISTRATION_GUIDE.md` | update | §*Registering an alias*: a template command is an alias of `template/render`; link to the template guide |
| `specs/guides/COMMAND_DESIGN_GUIDE.md` | update | §Aliases: when a command is better written as a template |
| `specs/reference/COMMAND_ALIASES.md` | update | A head parameter may be a link; `pack/open_issues` → `template/render` as the example |
| `specs/reference/VALUE_TYPE_SYSTEM.md` | update | §`DataNode` and `ValueInterface::data_node` |
| `specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md` | update | `Context::apply_ad_hoc` and its dependency rule |
| `specs/reference/api/DOC_02_QUERY_LANGUAGE_REFERENCE.md` | update | Parametrized queries; `simple_template` deprecated |
| `specs/reference/PROJECT_OVERVIEW.md` | update | The new crate and the dependency flow |

`CLAUDE.md` and `specs/README.md` gain the crate and the links; `CLAUDE.md`'s "Adding a Command"
gains one line pointing at template commands. The proposed `affects_docs` is the nine paths in the
table.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | The new crate; about six core files (additions); a new `liquers-lib` module plus value and commands; two records adapters; the workspace `Cargo.toml`; the matrix script |
| Crates and workflows affected | core, records, template, lib; the registry export; the build matrix |
| Existing tests likely to change | `registry_export` and the registry-dependent tests, gated on four feature groups, gain `templates` |
| New validation | Mustache spec suite (AC-1), parity corpus (AC-14), encode round-trip, wasm32 build and Node tests (AC-13) |
| Compatibility | Additive API, plus deprecation warnings |
| Concurrency | `apply_ad_hoc` runs inline with no job slot, so it is deadlock-safe inside an evaluation (`ASSETS-FIX1` #17) |
| Performance | Frame rows are built one at a time (`NodeRef::Owned`), which is fine for documents. `parse_query`'s hot path is untouched |
| Security | `{{$$ }}` and untrusted templates (preflight) |
| Recovery | Turning off the `templates` feature leaves the core additions unused |
| Certainty | High for the parser, the AST and the renderer. **Medium** for the volatility of dynamically found dependencies, and for three facts Phase 3 must test: a linked `.md` arrives as text, the asset's filename is known when the command runs, and the template key is visible in the state's metadata |

## Open Questions

- **12 [decided 2026-10-17]** `apply_ad_hoc` propagates the ad-hoc asset's dependencies.
- **13 [decided 2026-10-17]** Relative partial keys resolve against a cwd passed as an argument.
- **14 [decided 2026-10-17]** `cwd` defaults to the link `-R-key/.` and is the last argument. The
  earlier claim that a link needed a new command was wrong: `-R-key/.` is the documented pattern.
- **15 [accepted 2026-10-17]** Inside `[…]`: a name resolved from the top of
  the context stack, an integer (negative from the end) or a quoted string; not a query, which a
  later `[$ q]` could add.
- **16 [accepted 2026-10-17]** The default mode is `strict`, so a broken template is
  noticed; `errors` sits before `cwd`, so a caller choosing a mode keeps `cwd` promoted.
