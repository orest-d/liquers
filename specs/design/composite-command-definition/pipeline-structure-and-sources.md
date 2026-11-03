# Pipeline structure, state sources and recipe shape

Analysis for the maintainer's review of 2026-11-03. It supports Phase 1 questions Q26–Q30. Each
section ends with a recommendation; the decisions themselves are recorded in Phase 1.

## 1. The pipeline is a struct, scoped to one realm

Today a query's segment header has a **name**, a **level** and **parameters**
(`liquers-core/src/query.rs`, `SegmentHeader`):

- A transform segment's name is its realm. Every action in the segment is looked up there.
- A resource segment's name is reserved: it is warned about and ignored
  (`RECIPES_AND_PLANS.md` §Planner instructions). It is meant to become the realm too.
- Header parameters: a resource header takes one instruction (`-R-meta/…`), and a second one is
  `TooManyParameters`. A transform header's parameters have no meaning yet.
- `level` is stored and encoded, but it is reserved and not interpreted.

The 2026-11-02 draft put `realm` on each action element. Making the realm a property of the
**pipeline** matches the query better, and prepares for multi-realm execution
(`CORE-MULTI-REALM-INTERPRETER`):

```rust
pub struct Pipeline {
    pub realm: String,                  // "" = the default realm; the segment header name
    pub header: HeaderExtras,           // level and header parameters, carried as they are
    pub rooted: bool,                   // a leading `/`
    pub elements: Vec<PipelineElement>,
    pub filename: Option<String>,       // the result's name and format: last by construction
}
```

- A change of realm is a nested pipeline used as an element:
  `PipelineElement::Pipeline(Pipeline)`. It runs on the current state, in its own realm.
- Converting from a query: consecutive segments with the same header name, resource or transform,
  add to the same pipeline. A segment with a different name opens a nested pipeline.
  `-R/a.csv/-/f/-x/g/h/-/k` becomes `{realm: "", elements: [resource a.csv, f, {realm: x,
  elements: [g, h]}, k]}`.
- Converting to a query: each run of elements becomes a segment named by its pipeline's realm. A
  resource opens a resource segment, and the next action opens a transform segment with the same
  name.
- `filename` as a field removes rule P2: the filename is last because there is nowhere else to put
  it. A nested pipeline with a filename is not representable, because a query has one filename and
  it ends the query.
- The resource header name becomes the enclosing pipeline's realm. The planner keeps ignoring it,
  with its warning, until multi-realm support gives it meaning. That planner change is out of scope.

This could be implemented now. The realm already selects the lookup realm for actions, so tests
can cover nesting, merging and splitting of segments today.

**Should the struct carry title and description?** For a pipeline command they duplicate
`CommandMetadata`'s `label` and `doc`. For a recipe they duplicate the recipe's own. For a nested
pipeline they mean nothing. Recommended: no. See §4 for the other asset declarations.

## 2. Elements that set the state: one meaning each

AC-5 says how a value becomes the current state. Keys and queries can become the state in several
ways, and today the meaning depends on how a key or query is spelled:

| Meaning | Step today | Query spelling |
|---|---|---|
| the asset's value at a key | `GetAsset` | `-R/k` |
| its binary, metadata, recipe or directory listing | `GetAssetBinary`, `…Metadata`, `…Recipe`, `…Directory` | `-R-b/k`, `-R-meta/k`, … |
| the stored resource, its metadata or listing, bypassing assets | `GetResource`, `…Metadata`, `…Directory` | `-R-stored/k`, … |
| the key itself as a value | `UseKeyValue` | `-R-key/k` |
| the working directory (not a state) | `SetCwd` | `-R-cwd/k` |
| the result of evaluating a query | `Evaluate` | only inside a link, `~X~…~E` |
| the query itself as a value | `UseQueryValue` | `q`, which quotes the query **before** it |
| a literal value | `UseValue` (new) | none |

The 2026-11-01 draft added a further inference: a `{placeholder: name}` element used the bound
value, unless the value was a link, in which case it was evaluated. That rule is exactly the
ambiguity the maintainer pointed out.

**Recommended rule.** Each state element names exactly one meaning. A placeholder is never an
element itself. It is an **operand** of one of these elements, so the element, not the type of the
bound value, decides what happens:

| Element | Operand | Meaning | Step |
|---|---|---|---|
| `{value: …}` | JSON, or `{placeholder}` | the literal value | `UseValue` |
| `{resource: k, read: asset\|binary\|metadata\|recipe\|directory}` | key, or `{placeholder}` | a keyed asset | `GetAsset*` |
| `{resource: k, read: stored\|stored_metadata\|stored_directory}` | key, or `{placeholder}` | the store, bypassing assets | `GetResource*` |
| `{key_value: k}` | key, or `{placeholder}` | the key itself | `UseKeyValue` |
| `{cwd: k}` | key, or `{placeholder}` | the working directory | `SetCwd` |
| `{evaluate: …}` | query text, a pipeline, or `{placeholder}` | the result | `Evaluate`, or a nested plan (§3) |
| `{query_value: …}` | query text, a pipeline, or `{placeholder}` | the query or pipeline itself | `UseQueryValue` |
| `{quote: true}` | — | the pipeline so far, as a value | `UseQueryValue` of the prefix |

- `read` defaults to `asset`, and its values are the header instructions' long names. The short
  spellings (`b`, `meta`) are accepted when reading.
- A placeholder operand must be a declared argument of a type that fits the element: a key for
  `resource`, `key_value` and `cwd`, and a query or link for `evaluate` and `query_value`. A
  mismatch is a registration issue (AC-6).
- The old `{query: "text"}` element **spliced** query text into the pipeline. It is renamed
  `{steps: "text"}`, so that `query` is not read as evaluate or value. Splicing is how the planner
  mixes queries and pipelines (AC-16).
- Representability: `resource` and `key_value` and `cwd` → `-R-<read>/k`. `query_value` is
  representable only as the first element, as `<query>/q`. `evaluate` has no spelling outside a
  link, so it is not representable as an element.

## 3. A nested pipeline as an argument (AC-18)

A representable nested pipeline is an ordinary link: `Evaluate(Query)`. Its result is an asset
identified by that query, shared and cached like any link. A pipeline that is **not**
representable has no query, so it cannot be identified as a link. Its plan needs its own
`ParameterValue` variants:

- `ParameterPipeline(name, Pipeline, Position)` for a pipeline given in an action;
- `OverridePipeline(name, Pipeline)` for one given by an override;
- `DefaultPipeline(name, Pipeline)`, if command defaults may be pipelines later (Phase 2 decides).

`TryFrom<&ParameterValue> for Pipeline` returns the pipeline of every link-like variant: a link's
query becomes a pipeline, and a pipeline variant returns its pipeline. It refuses a value. Editors
and `liquers-validate` can then show any argument as a pipeline.

**Identity of a non-representable result (recommendation).** It is evaluated as a nested plan,
with its dependencies recorded, but it has no asset of its own. It is neither shared nor cached
separately; caching happens where its caller's result is cached. The alternative is to identify it
by a hash of its canonical JSON. That would give it an asset, but it adds a second identity scheme
next to queries, which the asset manager and the web API would both have to learn.

## 4. Asset declarations and the pipeline (AC-30)

A recipe's fields fall into three groups:

| Group | Fields | Belongs to |
|---|---|---|
| computation | query, pipeline, arguments, links, cwd | the pipeline |
| plan-shaping | filename, volatile | the pipeline: both already have a query spelling (the last token, `v`) and a plan effect (`Step::Filename`, `is_volatile`) |
| asset declarations | title, description, expires, stored, cached | ? |

Options for the third group:

- **A. They stay on the recipe** (the 2026-11-02 text). A pipeline describes a computation, and the
  recipe describes the asset stored under a key. Nested pipelines and pipeline commands never carry
  meaningless fields. `Recipe::to_plan` copies `expires` and friends onto the plan, as it does
  today.
- **B. They move into the pipeline struct**, and through it into the plan. A recipe becomes a key
  plus a pipeline, and a standalone pipeline, such as a web request or a file passed to the
  validator, can say how its result is kept. The costs: every nested pipeline and every pipeline
  command definition has fields that must be refused or ignored; `title` and `description`
  duplicate the recipe's and the command's; and caching declarations would come from two places.
- **C. A split.** `filename` and `volatile` belong to the pipeline. `expires`, `stored` and
  `cached` go in an optional `asset` block that is allowed only on a top-level pipeline, and is
  refused on nested pipelines and command definitions. `title` and `description` stay on the
  recipe.

Recommended: **C** if a standalone pipeline should be able to say how its result is kept, which
the web follow-up will want for endpoint recipes; otherwise **A**. Both keep `title` and
`description` out of the pipeline.

## 5. The recipe's shape (AC-15)

What the recipe holds today: `query`, `arguments` and `links`, which override arguments of the
**last** action. A recipe pipeline makes `arguments` and `links` redundant, because any action can
name its arguments. The options:

1. **One struct with a runtime check.** `pipeline` is a new field. Exactly one of `query` and
   `pipeline` must be present, and `arguments` and `links` are allowed only with `query`. This
   breaks no API, but the rule lives in validation.
2. **An enum, enforced by the type.**
   `Recipe { body: RecipeBody, title, description, cwd, … }`, with
   `enum RecipeBody { Query { query, arguments, links }, Pipeline(Pipeline) }`. It is serialized
   flattened and untagged, so every existing `recipes.yaml` reads and writes byte for byte as
   before. `arguments` and `links` cannot appear with a pipeline. This breaks API: about 30
   `Recipe { … }` literals across the crates, `recipe.query` users, and `liquers-py`'s recipe
   access.
3. **Query and pipeline combined**: the query runs first, then the pipeline. This is not needed:
   a pipeline can begin with `{steps: "<query>"}` or a `resource` element, so the combination is
   already a pipeline.
4. **Pipeline only, internally.** Deserialization turns `query` + `arguments` + `links` into a
   pipeline (AC-30's conversion), and serialization writes the old form back when the pipeline
   has one. There is one representation, but writing back is lossy. A recipe saved after editing
   might change form (`{steps}` versus `query`), and every reader of `recipe.query` becomes a
   conversion.

Recommended: **2**. It enforces the either/or rule in the type, keeps every existing file
unchanged, and makes AC-30's conversion a `match`. Option 4 can follow later if the old form is
ever retired.

## 6. Pipelines as values

`Value::Query` is a first-class value: a type identifier `Query`, a `TypeInfo`, the `query.txt`
default filename, conversions in `ValueInterface` (`try_into_query`, `from_query`) and
serialization. A pipeline gets the same:

- `Value::Pipeline(Pipeline)` in core, with type identifier `Pipeline` and type name `pipeline`.
  Its formats are `json` (the default, `pipeline.json`) and `yaml`. It can also be written as
  query text (`txt`) when it is representable.
- `ValueInterface::try_into_pipeline` accepts a pipeline, a query (converted), or JSON or YAML
  text. `ValueInterface::from_pipeline` builds one. Both have **default implementations** (through
  `try_into_query` and the JSON form), so every existing `ValueInterface` implementor compiles
  unchanged. `ExtValue`, the Python and the JavaScript values delegate to the base.
- `Value::Query` converts to `Value::Pipeline` through `try_into_pipeline`. The reverse needs the
  registry (Q20), so it is not a `ValueInterface` conversion.

## 7. Errors and positions (AC-31)

Editing errors are more than token errors:

- an unknown command or namespace;
- an unknown, `injected` or duplicated argument;
- a value of the wrong type for its argument;
- a placeholder that is not declared;
- a representability failure (in `QueryEditor`).

Each one names the element, the argument and the rule.

Positions: a pipeline built from a query keeps each element's and parameter's `Position`. It is an
optional field, skipped when absent, and ignored by equality and by the canonical form. An error
then reports where in the query the problem is. `QueryEditor` keeps the query it started from and
attaches it with `Error::with_query`. After an edit, positions of changed elements are gone. To
report one, the editor encodes the current pipeline, re-parses it, and takes positions from the
fresh parse; the error then carries the re-encoded query.
