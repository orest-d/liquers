# Phase 2: Solution and Architecture - Structured Error Context

## Overview

The original fix, changing `Error::with_key` from `query` to `key`, is necessary but not a
complete contract. A `Key` converts losslessly to a pure-key `Query`, so the important distinction
is semantic role and nesting, not syntax. The architecture must preserve an ordered diagnostic
path while retaining the existing flat fields during compatibility migration.

This phase compares an ordered list of structured context frames with maps, flat lists, recursive
causes, and markup. Ordered frames currently explain the verified occurrences best, but neither that
model nor its shape is approved. Frame vocabulary, propagation order, legacy-field projection, and
public binding surface remain blocking decisions; readiness is `phase2-blocked`.

## Verified Existing Semantics

- `From<&Key> for Query` and `From<Key> for Query` create one headerless resource segment
  (`query.rs:1683-1700`), preserving the key exactly. `Query::is_key` and `Query::key` recognize a
  single resource segment with no header or a trivial header (`query.rs:2515-2533`). Storing key
  text in a query slot is parseable and recoverable as a pure key, but loses its semantic role.
- `ErrorPayload` has one each of `query`, `key`, `position`, and `command_key`; every builder
  overwrites its slot (`error.rs:143-160`). `command_key` is not serialized.
- Store error constructors currently interpolate the store name into their message rather than
  carrying it as data. That is sufficient for the current error surface, but a structured context
  must be able to retain a store reference (for example, the stable store name) alongside the
  accessed resource key when it becomes relevant to consumers.
- `Error::key_not_found` does not populate `key` (`error.rs:304-306`). The repository has 44
  source-tree calls across core, native stores, and web stores that inherit this omission.
- `dependency_version_mismatch` and `dependency_cycle` convert a pure dependency key and copy it
  into both flat fields (`error.rs:356-380`), without representing dependency role or non-key query
  dependencies.
- `Recipe::to_plan_for_key` calls `self.to_plan(cmr)?` before attaching the registered recipe key
  (`recipes.rs:322-333`). A planning error may therefore contain the recipe query but not its
  distinct recipe/asset key. `AssetManager::recipe_opt` and asset evaluation propagate this path
  with `?` (`assets.rs:2193-2284`, `3336-3344`).
- Asset failure finalization persists the propagated error in metadata `error_data`; recipe preview
  and lookup paths that fail before owner enrichment can therefore expose a durable error without
  the recipe/asset key, not merely a transient incomplete diagnostic.
- `DefaultRecipeProvider::get_recipes` emits YAML and CWD errors at a known catalog/directory key,
  but only the CWD path adds a key (`recipes.rs:618-632`). The YAML error loses the
  `<directory>/recipes.yaml` resource identity.
- Link materialization either overwrites `query` or overwrites `position`, depending on link kind
  (`interpreter.rs:364-445`). Resource steps pass store errors through unchanged
  (`interpreter.rs:455-488`). These are real multi-query/key propagation sites, not hypothetical.
- Action execution preserves an inner `command_key` but overwrites the position with the outer
  action position (`interpreter.rs:538-567`). `LogEntry::from_error` then copies only one query and
  one position and ignores key/command context (`metadata.rs:511-526`).
- `liquers-web::LiquersError` exposes only one query and key (`liquers-web/src/error.rs:98-107`);
  the Python core `Error` wrapper exposes only position (`liquers-py/src/error.rs:101-121`).

`TryFrom<Query> for Key` is not the purity test: it accepts a resource query and returns the resource
key while disregarding a nontrivial header (`query.rs:1703-1713`). Use `Query::key()` (or
`is_key()` followed by `header_key()`) when purity is required.

## Occurrence Classes and Required Rule

| Occurrence class | Example | Required behaviour |
|---|---|---|
| Wrong flat assignment | `Error::with_key` | A simple keyed error projects to `key`, not only `query`. |
| Constructor omits known key | `Error::key_not_found` | Typed keyed constructors always record the key context. |
| Store access provenance | keyed store constructor / resource access | A resource-access frame can carry a store reference, such as its stable store name, separately from the resource key. |
| Keyed boundary lacks owner identity | recipe planning/evaluation | Add an asset/recipe-key frame even when an inner query or resource key exists. |
| Nested query/link/resource failure | link materialization and `GetResource` | Retain inner failure plus the outer query, argument/action position, and owner frame. |
| Dependency identity | mismatch/cycle constructors | Record dependency role and whether the dependency is a key, directory, recipe, command, or query. |
| Durable asset error | failure finalization / recipe preview metadata | Ensure `error_data` carries the owner key as well as any distinct inner context. |
| Downstream flattening | metadata log, Axum response, and language bindings | Preserve structured contexts; define deliberate legacy projections. |

“Add the key if it is not set” is sufficient only for the flat compatibility field. In the
structured model, a keyed asset boundary adds its role-bearing frame unless the same role and key
are already present. It must not suppress an outer recipe key merely because an inner resource key
already exists.

## Data Structures

The provisional direction is an additive field on the already boxed payload:

```rust
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ErrorContextFrame {
    pub role: ErrorContextRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub store: Option<String>,
    #[serde(default, skip_serializing_if = "Position::is_unknown")]
    pub position: Position,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_key: Option<CommandKey>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorContextRole {
    AssetEvaluation,
    RecipeEvaluation,
    QueryEvaluation,
    LinkEvaluation,
    ResourceAccess,
    ActionExecution,
    Dependency,
}
```

`ErrorPayload` would add `contexts: Vec<ErrorContextFrame>` with `#[serde(default,
skip_serializing_if = "Vec::is_empty")]`. Frames group a position/action with the query in which it
is meaningful. An ungrouped list of keys and queries cannot express that association.

This is a **candidate**, not an approved signature. Open questions include whether `role` should be
a closed enum or forward-compatible string/newtype; whether recipe and asset identities need
separate roles; whether a store reference is always a name or needs a stronger identity; whether
dependency variants belong in this frame; and whether `label` should encode argument names or be
replaced by typed operation data.

## Trait Implementations

No new trait implementation is currently required. The candidate types would use derived serde,
debug, clone, and equality traits only; their exact derives remain contingent on the selected public
model. Existing `Display` and error conversion behaviour stays on `Error` and is addressed under
Error Handling rather than delegated to a new trait object or source-error hierarchy.

## Ordering, Enrichment and Legacy Projection

Recommended invariant: the cause is created first, and each propagation boundary appends an outer
frame, so storage order is innermost to outermost. Rendering reverses it to read “while evaluating
recipe …, query …, link … failed …”. Adjacent identical frames may be deduplicated by a helper;
different roles carrying the same pure-key text are not duplicates.

## Function Signatures

Candidate helpers, all synchronous and consuming like the current builders:

```rust
pub fn with_context(self, frame: ErrorContextFrame) -> Self;
pub fn with_asset_key(self, key: &Key) -> Self;
pub fn with_recipe_key(self, key: &Key) -> Self;
pub fn with_query_context(self, role: ErrorContextRole, query: &Query) -> Self;
pub fn with_store_context(self, store_name: &str) -> Self;
```

The flat `query`, `key`, `position`, and `command_key` fields remain during migration. A likely
projection is the innermost applicable context, preserving the immediate cause for old consumers,
while the context vector retains outer recipe/asset identity. However, choosing innermost versus
outermost is observable in serde, metadata, web, and Python and is a blocker. Whether existing
`with_key`/`with_query` keep overwrite semantics, become first-write-wins, or delegate to frames is
also unresolved.

## Alternatives

| Alternative | Assessment |
|---|---|
| Keep only flat `query` and `key`; recognize pure keys with `Query::is_key` | Handles the one-line bug but cannot represent recipe key plus recipe query or nested failures. Rejected as complete solution. |
| `HashMap<String, QueryOrKey>` | Names contexts but loses evaluation order, permits unstable/ad-hoc keys, and cannot naturally represent repeated nested links/actions. Rejected. |
| Flat `Vec<QueryOrKey>` | Preserves order but not the association among query, position, action, argument, and key. Rejected. |
| Ordered structured frames | Preserves order, duplicates, roles, and grouped locations. Strongest current candidate, but its public shape is a blocked decision. |
| Special markup in `message`/`message_html` | Useful as a renderer for clickable UI, but fragile for transport, testing, escaping, localization, and programmatic consumers. Rejected as authoritative storage. |
| Recursive source errors | Models causality but complicates serde, equality, size/depth limits, and host-language bridges; frames provide the required propagation path without owning arbitrary source objects. Keep only as a future cause-chain design. |

## Known-Issue Preflight

| Issue/design | Status | Priority | Effect | Blocking? |
|---|---|---|---|---|
| `CORE-ERROR-PAYLOAD-SIZE` | closed | P2 | `ErrorPayload` is boxed, so an additive vector does not widen every `Result`; preserve pointer-size and flat legacy serde tests. | no |
| `CORE-ERROR-STORE-NAME-NOT-STRUCTURED` | rejected | P2 | Store name in the message is sufficient today. If structured context is adopted, the selected frame model must carry any needed store reference (for example, its stable name), rather than reviving a competing flat payload field. | no |
| `CORE-METADATA-TRACEBACK-SUPPORT` | accepted | P2 | Traceback/cause information is complementary; do not overload context frames with opaque language stacks. | no |
| `LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT` | accepted | P3 | Demonstrates that core transport must own structured data needed after language round trips. | no |
| `WEB-LIQUERSERROR-NOT-CONSTRUCTIBLE` | accepted | P3 | Constructor work is separate, but its proposed flat surface must not freeze the incomplete model. | no |
| `ASSETS-IMPROVEMENTS` | accepted | P2 | Requires persistence warnings with complete key/query/asset context. The selected model must support those warnings and persisted `error_data` without changing its value-first persistence semantics. | no, but architecture overlap |

The blocker is not another issue at a lower priority; it is the unresolved public context contract
inside this design. No priority escalation is required.

## Integration Points

Likely Rust integration files are `liquers-core/src/error.rs`, `recipes.rs`, `assets.rs`,
`interpreter.rs`, `context.rs`, and `metadata.rs`; store constructors and keyed boundaries require
an audit in `liquers-core/src/store.rs`, `liquers-store/src/`, and `liquers-web/src/store/`.
Consumer changes span `liquers-web/src/error.rs`, `liquers-py/src/error.rs`, and
`liquers-axum/src/api_core/{error,response}.rs`; Axum currently copies the flat query/key into both
`ErrorDetail` and the top-level `ApiResponse`, so its projection and compatibility shape must be
specified rather than treated as generic serde pass-through.

Frames own encoded strings because `Error` is serialized and crosses async/language boundaries.
No borrowed lifetime, trait object, generic parameter, or lock is required. The payload is already
boxed, but vector/string growth occurs on error paths; a maximum frame count or truncation rule is
still needed to prevent pathological recursive diagnostics.

## Sync vs Async

Context construction and enrichment are synchronous because they mutate an owned error after a
fallible operation. Existing async store, asset, recipe, and interpreter boundaries continue to use
`map_err`/`?`; no new async method or runtime synchronization is proposed.

## Relevant Commands

No new commands or command namespaces are introduced. Existing action execution is an integration
point only; command registration and query syntax remain unchanged.

## Error Handling and Rendering Contract

`ErrorType` continues to describe the immediate cause. Context enrichment must never change the
type or replace the cause message. Plain `Display` should remain concise and deterministic;
structured consumers render frames themselves. If clickable text is needed, define a renderer
from frames to escaped `message_html` or UI components rather than embedding a private markup
language in `message`.

Open rendering decisions: which frames appear in `Display`, how repeated frames collapse, how
positions refer to a specific query, and whether links target an asset key, a query console, or
source text. These must be resolved before Phase 3 can assert exact messages.

## Risk Assessment

| Assessment | Record |
|---|---|
| Likely files | 6 core error/evaluation files, store call-site audit, web/Python/Axum consumers, tests, and planned docs. |
| Affected workflows/crates | Store access, keyed recipe evaluation, nested link/resource evaluation, dependency errors, metadata logs, `liquers-core`, stores, web, py, and axum. |
| Existing-test impact | Flat serde snapshots and binding tests may change; pure-key evaluation must remain unchanged. |
| New validation | Constructor audit, frame ordering/dedup, recipe-key-versus-query, nested link/resource/action, serde old/new round trips, metadata projection, and web/Python transport. |
| Compatibility/data | Additive vector can read old payloads; legacy projection and output shape are unresolved and externally visible. No historical migration should be required. |
| Concurrency/performance/security | Enrichment is local and sync; recursive paths need a frame/depth bound; HTML/link rendering must escape content and avoid unsafe schemes. |
| Recovery | Keep flat fields and gate new consumer use; context producers can be reverted independently if the additive schema remains ignored. |
| Certainty | High that the flat model is insufficient; insufficient to implement until projection, roles, order/dedup, limits, and renderer exposure are chosen. |

## Documentation Architecture

| Path | Kind/audience/area | Planned change and links | `affects_docs` |
|---|---|---|---|
| `specs/reference/ERROR_CONTEXT.md` | new reference; internal/integrators; `core/error`, `core/query`, `core/assets` | Authoritative schema, ordering, roles, projections, serde and rendering rules; link code and tests. | yes |
| `specs/reference/PROJECT_OVERVIEW.md` | existing reference; internal; add `core/error` area | Replace “Query/Key context preserved” with the selected model and link `ERROR_CONTEXT.md`. | yes |
| `specs/reference/ASSETS.md` | existing reference; internal; `core/assets` | State how keyed asset/recipe failures add owner context without replacing nested resource context. | yes |
| `specs/reference/ASSET_LIFECYCLE.md` | existing reference; internal; `core/assets` | Update evaluation/failure-finalization paths to show when owner context is attached and what structured error is persisted in metadata `error_data`. | yes |
| `specs/guides/LANGUAGE_INTEGRATION_GUIDE.md` | existing guide; integrators; `core/error`, `web`, `py` | Extend ERROR mapping from single flat values to the chosen context representation and compatibility fields. | yes |
| `specs/reference/WEB_API_SPECIFICATION.md` | existing reference; API consumers; `axum`, `web` | Specify the selected `ErrorDetail` and duplicated top-level `ApiResponse` context/projection shape, whether structured contexts are exposed or intentionally omitted, and compatibility expectations. | yes |
| `specs/README.md` | capability map | Link the new error-context reference under core concepts when implemented. | yes |

The proposed authoritative `affects_docs` set is all six reference/guide rows above, including the
new reference; `specs/README.md` is a required capability-map link update, not an implementation
contract reference. Phase 5 must review each affected document against implemented behaviour,
update its `reviewed:` date and matching `## History` row, and collect evidence from recipe,
resource/link, persistence-warning, Axum, web, and Python diagnostics.

## Continuation Blockers

Phase 3 cannot define correct expected values, and Phase 4 cannot name stable signatures, until the
following are decided:

1. Choose ordered structured frames or another authoritative model.
2. Choose the frame role vocabulary/granularity, including the representation and stability of a
   store reference, and whether role is a closed enum.
3. Choose storage order, deduplication, and a maximum frame/depth policy.
4. Define which frame projects to each legacy flat field and whether old builders overwrite,
   preserve-first, or append context.
5. Define metadata, web, Python, and HTTP exposure plus the plain/HTML rendering boundary.

Until those decisions are made, the safe local repair is known but intentionally not isolated as
the whole implementation: doing so would leave the broader contract ambiguous.
