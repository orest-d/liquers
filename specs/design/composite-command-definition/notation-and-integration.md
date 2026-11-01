# Notation and integration analysis — Composite command definitions

Answers the maintainer's review of 2026-11-01 (PR #238): which placeholder syntax to use, a
structured *pipeline* notation that serializes directly, how placeholders are declared, how variadic
arguments are handled, what supporting payload and injected arguments would imply, and how
pipelines combine with recipes and the web API. Each section ends with a recommendation. Nothing
here is decided until the maintainer chooses. Phase 1 lists the resulting questions (Q1, Q7–Q12)
and Phase 2 follows them once they are answered.

## 1. Placeholder syntax

> **Decided 2026-11-01:** neither. Query text gets no placeholders, and placeholders exist only in
> pipelines (§2). This section is kept for the reasoning. §5's payload sources and §7 moved to
> `WEB-API-CANNOT-RUN-PIPELINES-OR-RECIPE-ENDPOINTS`.

**How `liquers-template` placeholders work at HEAD** (`liquers-template/src/query/`,
`specs/reference/TEMPLATE_LANGUAGE.md` §2):

- `{{name}}` inside a query tag (`{{$ …}}`) is a **slot** of a `QueryTemplate`: core's query AST
  with a `Slot` wherever a token stands. It is parsed by core's own parser: each slot is replaced
  by a sentinel identifier, then the result is lifted back into slots. So the grammar is core's by
  construction, and `tests/parity.rs` checks this.
- A slot may be a whole token or **part of one** (`to_{{fmt}}`, `search-area~ncolon~{{area}}`). It
  may stand in six positions: P1 parameter, P2 namespace, P3 action name, P4 key segment, P5
  filename, P6 inside a link. Each position has its own validity rule.
- Names are **paths into the data** (`{{items.0.title}}`, `{{m["a b"]}}`), resolved from the
  Mustache context stack **at render time**.
- A non-scalar value is allowed only as a whole P1 slot of the **last action**, where it becomes a
  `Recipe::arguments` override. This design removes that limit (any action).
- `QueryTemplate` depends on `liquers-core` only. It could move into core without new
  dependencies.

| | `{{name}}` (the template syntax) | `$name` (proposed in Phase 1) |
|---|---|---|
| One placeholder language across Liquers | **yes**: a query written for a template and one written for a composite or recipe read the same | no: two spellings for the same idea |
| Positions | P1–P6, partial tokens, keys and filenames, with tested validity rules | whole parameters and the start element only, unless the grammar grows |
| Names | paths into a value (`{{cfg.region}}` reads a field of a map argument) | identifiers only |
| Implementation | exists (parser, AST, instantiation, parity tests); core would adopt the `QueryTemplate` AST and its sentinel parse, with no change to core's grammar | new parser production, and a new `ActionParameter` and `QuerySegment` variant |
| URLs | `{` and `}` must be percent-encoded (`head-%7B%7Bn%7D%7D`), so hand-written HTTP is unreadable | `$` is legal in a URL path (`head-$n`) |
| Inside a Mustache template | a placeholder meant to be bound later is **rendered by Mustache first**: it needs changed delimiters or escaping | passes through Mustache untouched, though `{{$ …}}` already uses `$` for "query tag", which reads confusingly |
| Evaluation time | looks the same as a template slot but is bound by the planner, not at render time; a reader can confuse the two | visibly different from a template slot |
| Agents and tools | well known (Mustache), and an LLM will write it correctly | as easy to write; less known |

**Both at once.** `{{name}}` is the canonical form, with the whole template feature set, and `$name`
is accepted as a **shorthand for a whole-parameter `{{name}}`** (and for a start element). The
encoder writes `{{name}}`, but writes `$name` where the query is meant for a URL
(`Query::encode_for_url`, or a flag on the encoder). Pros: one model and one AST; template users
lose nothing; URLs stay readable; a template can carry a late-bound placeholder as `$name`.
Cons: two spellings to document and to test for round-trip; `$name` is the narrower form, so a
reader must learn which positions it covers; the encoder needs a policy for which form to write.

**Recommendation: both, with `{{name}}` canonical.** Move `QueryTemplate` (AST, sentinel parser,
slot positions and their validity rules) from `liquers-template` into `liquers-core`, where
`liquers-template` keeps using it. Accept `$name` as the whole-parameter shorthand. Bind
placeholders in the planner (Phase 2) instead of at render time, but with the same names, positions
and rules. Partial-token and key or filename slots take only text-form values; a list, a map, a
link or `{{state}}` is allowed only where a whole parameter stands. The composite's input is then
`{{state}}` / `$state`.

## 2. A structured notation: pipelines

The maintainer's sketch, as the proposed serde shape (JSON5):

```json5
{ pipeline: [
    { action: "from_csv" },                                  // default namespaces of the realm
    { action: "is_in", ns: "pl",
      arguments: { column: { value: "region" },
                   values: { placeholder: "regions" } } },
    { action: "sort", ns: "pl",
      arguments: { column: { placeholder: "by" }, ascending: { value: false } } },
    { action: "join", ns: "pl",                              // hypothetical command
      arguments: { right: { pipeline: [ { resource: "data/targets.csv" }, { action: "from_csv", ns: "pl" } ] },
                   on: { value: "region" } } },
    { action: "head", ns: "pl", realm: "", arguments: { n: { placeholder: "n" } } },
    { filename: "top.csv" }
] }
```


```rust
pub struct Pipeline(pub Vec<PipelineElement>);           // serde, owned
pub enum PipelineElement {                               // untagged by its key
    Action { action: String, ns: Option<String>, realm: Option<String>,
             arguments: BTreeMap<String, Source>, parameters: Vec<Source> },
    Resource { resource: String, mode: Option<String> },  // `-R-<mode>/key`
    Query { query: String },                              // embedded query text
    Start { start: Source },                              // a value, placeholder or link as the initial state
    Filename { filename: String },
}
pub enum Source {
    Value(Value), Placeholder(String), Link(String), Pipeline(Pipeline),
    State, Payload(String),                               // §5
    Multiple(Vec<Source>),                                // §4
}
```

Semantics:

- **Arguments are named.** An action's `arguments` are matched to the command's `ArgumentInfo` by
  name, so skipping a defaulted argument needs no empty slots, and order does not matter.
  `parameters` (positional) is allowed for the cases where a name is unknown, but not together with
  `arguments` for the same slot. A name that is not an argument, or an `injected` one, is a
  `ParameterError`.
- **`ns` and `realm`** select the command directly (`ns` absent: the realm's default chain, as a
  query without `ns`). This is per action, not the query's "last `ns` applies" rule, so a pipeline
  has no scoping surprises.
- **`link`** is query text. **`pipeline`** is a nested pipeline evaluated as a link. When it has a
  text form, it is identical to a link with that query (cached and shared). Otherwise it is planned
  as a nested plan in the parameter (`ParameterValue::PlanLink`, new), evaluated as an unmanaged
  dependency.
- **Relation to queries.** A pipeline with only text-form sources converts to a query and back
  losslessly (`Pipeline::to_query(cmr)`, `Query::to_pipeline(cmr)`). Both need the registry to map
  names to positions. The planner gets a second entry point, `PlanBuilder::from_pipeline`, which
  resolves each element through the same `from_action` and override machinery a query uses. A
  query is therefore the *text form* of a pipeline, and nothing about planning, caching, volatility
  or dependencies differs between them.
- **Validation.** `liquers-validate` accepts a pipeline (`--pipeline file.json`) and prints its text
  form when there is one. That is also the quickest way to see whether a pipeline stays cacheable.

**Recommendation:** adopt it as the structured form for every place in this design. Its schema is
published from the Rust types (`schemars`, as the registry export does), so agents can be given a
JSON Schema.

## 3. Declaring placeholders in the registry

A composite's placeholders **are its arguments**: each is an `ArgumentInfo` in the composite's
`CommandMetadata.arguments` (name, `label`, `description`, `argument_type`, default, `multiple`,
`gui_info`, `hints`). The order of `arguments` is the order a query passes them positionally. The
state argument is `state_argument` as for any command. Every placeholder in the body must be
declared. Inference (Phase 1, Q4) only fills an omitted `argument_type`, `multiple` or `gui_info`
from the slot, and the registry export writes the inferred values, so `command_registry.yaml` is
always complete:

```yaml
- namespace: sales
  name: top_in
  label: Top rows of some regions
  doc: Parse a sales CSV, keep the given regions, and return the largest rows.
  arguments:
  - { name: by, label: Sort by, description: Column to sort on, argument_type: string, default: !Value amount }
  - { name: n, label: Rows, argument_type: int, default: !Value 10, gui_info: !IntegerSlider { min: 1, max: 100 } }
  - { name: regions, label: Regions, argument_type: string, multiple: true }
  definition: !Composite
    pipeline:                                   # or `body: <query text>`, never both
    - { action: from_csv, ns: pl }
    - { action: is_in, ns: pl, arguments: { column: { value: region }, values: { placeholder: regions } } }
    - { action: sort, ns: pl, arguments: { column: { placeholder: by }, ascending: { value: false } } }
    - { action: head, ns: pl, arguments: { n: { placeholder: n } } }
```

A placeholder that is not a declared argument is `Error::invalid_composite`. An argument used by no
placeholder is a warning. Two slots with conflicting types need an explicit declaration.

## 4. Variadic (`multiple`) arguments

| Case | Rule |
|---|---|
| A composite argument declared `multiple` | Must be its last parameter-consuming argument, as for any command. In a query call it takes every remaining parameter; in a pipeline, recipe or HTTP binding it is a JSON array |
| It fills a `multiple` slot of a body command | The list fills that slot element by element |
| It fills a non-`multiple` slot | `invalid_composite`, unless the slot's type is `any` (then the whole list is one value) |
| A scalar placeholder inside a `multiple` slot (`select_columns-a-{{c}}-b`) | One element |
| A list placeholder among literal elements | **Spliced** in place (`a`, …list…, `b`); in a pipeline written `{ multiple: [ {value: a}, {placeholder: cols}, {value: b} ] }`, and a list-valued placeholder inside `multiple` splices |
| A link bound to a `multiple` argument | One element per link, as today; an array result does not expand (`LINK-IN-VARIADIC-DOES-NOT-EXPAND` decides that for queries and composites alike) |
| `ns` (a variadic planner instruction) | Text-form placeholders only; each element must be an identifier |
| An empty list | The slot's default for that argument (an empty variadic list), consistent with `VARIADIC-EMPTY-ELEMENT-CONVERSION` |

## 5. Payload and injected arguments

At HEAD a payload is an opaque `Environment::Payload: PayloadType`
(`liquers-core/src/commands.rs`). A Rust type reads it through `ExtractFromPayload` /
`InjectedFromContext`, and an `injected` argument is filled that way, consuming no query parameter.
Any command that reads the payload must declare `payload: required`. That makes it volatile, never
reused, and refused under a key, since keys are a payload boundary (`PAYLOAD_GUIDE.md`).

**What works with no new mechanism.** A body command with an `injected` argument stays injected:
the composite need not mention it, and the interpreter injects it from the caller's context, as for
a direct call. The composite inherits `payload: required` from its body, through the same flag
folding an alias uses. So a composite *forwards* injection and payload for free.

**What a composite cannot do: declare its own `injected` argument.** Injection is a Rust type's
`InjectedFromContext` impl, and a metadata-only composite names no Rust type. An `injected`
composite argument is therefore `invalid_composite`.

**Payload values as data (`Source::Payload`, `{{_payload.user_id}}`).** Supporting this implies:

1. **A new trait method**, which core does not have: `PayloadType::field(&self, name: &str) ->
   Option<Value>` (default `None`), or a `data_node()` on the payload so template-style paths
   (`_payload.user.id`) read it. Each environment decides which fields it exposes.
2. **Planning.** A pipeline or body that reads the payload is `payload: required`, so it is
   volatile, never cached, never reused, and refused in a keyed recipe (`to_plan_for_key`). An
   ad-hoc web request may use it.
3. **Security.** A payload often carries the caller's identity. The `payload` source must read only
   the server-side context. It is **never bindable from a request** (an HTTP body member named
   `_payload…` is a `ParameterError`), and it never appears in a cached value's identity. Otherwise
   a client could impersonate a user, or one user could be served another user's result. This ties
   in with `design/access-policy-at-entry-points/` (principals at entry points).
4. **Bindings.** For JavaScript and Python the payload is a host object. A field read needs the
   bridge to expose fields, so it is later work there.

**Recommendation:** in this design, forward injection and payload requirements (free). Make
`injected` composite arguments an error. Specify `Source::Payload` and the `field` trait method,
but implement them as a follow-up step after the web API part, gated on the access-policy design,
because their value is the authenticated agent endpoint of §7.

## 6. Recipes: pipeline before or after the query?

A recipe would gain an optional `pipeline` next to `query`.

| Option | Reads as | Pros | Cons |
|---|---|---|---|
| A: `pipeline` **instead of** `query` | the pipeline is the whole recipe | simplest; one form per recipe | a resource and simple steps must be spelled structurally; no readable text part |
| B: `query` **then** `pipeline` | the query's result is the pipeline's input | the text part (`-R/data/sales.csv/-/ns-pl/from_csv`) keeps a query identity, so it is a cacheable, shared predecessor boundary; the structured tail carries the complex values; resources and keys stay readable | the query's filename would sit mid-plan, so the filename moves to the pipeline's end; the query cannot end with `q` |
| C: `pipeline` **then** `query` | the pipeline produces the source, the query transforms it | fits a long structured *source* argument (an SQL script, a request body), which is the classic recipe override case (`PROJECT_OVERVIEW.md` §Recipes) | the query must be a pure transform (no resource segment, no leading `/`); the cacheable prefix is the pipeline, which caches only if it is text-form; two halves to read in an unusual order |

**Recommendation: B, plus an embedded `{ query: "…" }` element**, which covers C without a second
ordering rule:

```yaml
- query: -R/data/sales.csv/-/ns-pl/from_csv           # text prefix: cached and shared
  pipeline:
  - { action: is_in, ns: pl, arguments: { column: { value: region }, values: { value: [EU, US] } } }
  - { query: sort-amount-f/head-10 }                  # text fragment inside the pipeline
  - { filename: eu.csv }
- pipeline:                                           # option C as a pipeline alone
  - { action: sql, ns: db, arguments: { script: { value: "SELECT … long text …" } } }
  - { query: ns-pl/head-10 }
  - { filename: recent.csv }
```

When `pipeline` is present, the recipe's asset name comes from its trailing `filename` element, and
the query must have no filename. `arguments` and `links` bind the placeholders of both parts. A name
that is no placeholder keeps today's meaning (the last action), which with a pipeline is the
pipeline's last action.

## 7. Web API

1. **A recipe as the request.** New routes `POST q/recipe/data|entry|submit` (Assets API) and a
   WebSocket `subscribe` with `recipe` take a recipe document (`query` and/or `pipeline`,
   `arguments`, `links`) as the JSON body. This is the JSON API representation of complex ad-hoc
   queries that `PROJECT_OVERVIEW.md` §Recipes item 4 already promises. It is unmanaged unless it
   has a text form (AC-14). The existing `POST q/…` body stays an override object, so no current
   client breaks.
2. **Endpoint recipes.** A `recipes.yaml` entry with declared `parameters` (`ArgumentInfo`, as §3)
   is a named, typed **endpoint**: `GET recipes/endpoints` lists each with its argument schema (JSON
   Schema from the `ArgumentInfo`s), and `POST recipes/call/{*key}` binds a JSON object of arguments
   to it. For agents, this is a tool catalogue: `liquers-mcp` can serve each endpoint as an MCP tool,
   as it serves commands now, and an agent can propose new ones into a staging area
   (`design/agent-command-proposal/`). An entry that also names a `command: <ns>/<name>` is
   registered as a composite in the registry (§3). This answers Phase 1's Q3, *how composites are
   loaded without Rust*: from recipes in the store.
3. **Placeholders in the path** (`GET q/data/…/head-$n?n=10`) remain, with AC-12–AC-14.
4. **Payload** (§5): the HTTP layer may build the payload (for example the authenticated
   principal), and endpoint recipes read it through `payload` sources, never from the request
   body.

**Recommendation:** items 1–3 in this design; item 2's MCP serving, and item 4, as follow-up steps
once the access policy exists. An endpoint that executes arbitrary pipelines is no more powerful
than today's `q/` routes, which already evaluate arbitrary queries. Access control is the same
problem in both cases, not a new one.
