# Phase 2: Solution & Architecture — plan-step-state-metadata

## Overview

The interpreter stops rebuilding the state after every step. Each step produces its **next state**
itself: a fetch hands on the fetched state, a pass-through hands on its input, and an action's
result gets a copy of the asset's record corrected to describe the prefix the action completes.
The prefix query is recorded by `PlanBuilder` on each `Step::Action`, the cut declines a boundary
over a bare key read, and `Recipe` stops declaring `bin`. `value_origin_key` and `fetched_key` are
deleted.

Rejected:
- *Descriptor overlay* (compact draft): keeps the asset's record as the base for fetched values,
  so the cut form's `Evaluate(a/b)` would still not hand on `a/b`'s state.
- *Parallel `Plan::step_queries` vector*: every step edit (`cut_predecessor`'s `split_off`/`drain`,
  `Recipe::to_plan` prologue) would have to keep it in step with `steps`.
- *Rebuilding prefixes at run time* (walk `Query::predecessor`, match by step count): repeats
  planning on every evaluation.
- *Slicing the query text at `position`*: positions refer to the source text, which recipe
  prologues, promoted links, aliases and CWD freezing all change.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `CONTEXT-TITLE-LOST-ACROSS-PREDECESSOR-BOUNDARY` (design `context-title-predecessor-inheritance`) | draft / design in review | P3 | Same `Step::Evaluate` arm; changes the final asset's record, not the input state | No | No | Whichever lands second rebases; Phase 1 §Design Dependencies |
| `CORE-PLAN-POLICY-AND-DEFAULTS` | accepted | P2 | Plan-builder defaults; the bare-key-read rule is one more cut policy | No | No | Note in its body when this lands |
| `RECIPE-PLAN-ANALYSIS-RUNS-OUTSIDE-PLAN-BUILDING` | draft | P3 | `Recipe::to_plan` gains one rule (clear prefix queries under overrides) | No | No | None |
| `DATA-FORMAT-CONSTANTS-AND-TOOLING` | draft | P2 | `bin` literal removed from `Recipe`; no new literal added | No | No | None |

Namespaces involved: none changed; `pl` and `rec` are exercised by tests only.

## Interfaces

All in `liquers-core`; no trait changes, no new types, no commands.

```rust
// plan.rs — one new field on an existing variant (precedent: `origin`).
Step::Action {
    realm: String, ns: String, action_name: String,
    position: Position, parameters: ResolvedParameterValues,
    #[serde(default, skip_serializing_if = "ActionOrigin::is_direct")]
    origin: ActionOrigin,
    /// The query this action completes: the prefix of the plan's query ending with it,
    /// promoted and CWD-frozen like `Plan::predecessor`. `None` when unknown — a hand-built
    /// step, or a recipe with argument or link overrides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    query: Option<Query>,
}

// recipes.rs — changed signature: no `bin` fallback.
pub fn data_format(&self) -> Result<Option<String>, Error>;

// interpreter.rs — new, crate-private; `apply_plan` and `do_step` keep their signatures and
// become thin wrappers that return `state.value()`.
pub(crate) fn apply_plan_state<E: Environment>(
    plan: Plan, input_state: State<E::Value>, context: Context<E>, envref: EnvRef<E>,
) -> BoxFuture<'static, Result<State<E::Value>, Error>>;
pub(crate) fn do_step_state<E: Environment>(
    step: Step, input: State<E::Value>, context: Context<E>, envref: EnvRef<E>,
) -> BoxFuture<'static, Result<State<E::Value>, Error>>;

// interpreter.rs — private helper building an action's output metadata.
async fn prefix_metadata<E: Environment>(
    context: &Context<E>, query: Option<&Query>,
) -> Result<Metadata, Error>;

// metadata.rs — new flag on both records, following `payload_required`'s pattern: defaulted on
// load, skipped when false, so existing records load and serialize unchanged. Legacy JSON
// metadata reads it from an `is_applied` key, absent = false.
pub struct MetadataRecord { /* … */
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_applied: bool,
}
pub struct AssetInfo { /* … */
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_applied: bool,
}
impl Metadata { pub fn is_applied(&self) -> bool; }

// assets.rs — crate-private accessor used by `prefix_metadata`.
pub(crate) async fn recipe_declared_description(&self) -> (bool, bool); // (title, description)
```

Async throughout: every step already runs in an async interpreter future; nothing new blocks.
`State` carries `Arc<V>` and `Arc<Metadata>`, so handing a state on is two `Arc` clones.

## Next state per step (`do_step_state`)

Enumerated over every `Step` variant, no `_ =>`:

| Step | Next state |
|---|---|
| `Evaluate(q)`, `GetAsset(k)` | `context.get_dependency_state(..)` as returned. `GetAsset` sets `key` to `k` when the record lacks it (a keyed asset loaded from a store without a key in its metadata). |
| `GetAssetBinary(k)` | bytes from the dependency state, with that state's metadata (type re-synced to bytes by `State::from_value_and_metadata`) |
| `GetResource(k)` | `store.get(k)` bytes with the **stored** metadata; legacy metadata → `prefix_metadata(None)` plus `key`, and a warning log entry |
| `Info`, `Warning`, `Error`, `SetCwd`, `Filename` | the input state, unchanged |
| `Plan(p)` | `apply_plan_state(p, input, …)` |
| `Action { query, .. }` | command result + `prefix_metadata(context, query)` |
| `GetAssetDirectory(k)`, `GetResourceDirectory(k)` | listing + `prefix_metadata(None)` with `key` = `k` (today's answer) |
| `GetAssetMetadata`, `GetResourceMetadata`, `GetAssetRecipe`, `UseQueryValue`, `UseKeyValue` | new value + `prefix_metadata(None)` |

`prefix_metadata` = `context.get_metadata()` with: `query` ← the argument, `key` ← `None`,
`filename`, `data_format`, `media_type` ← `None`, `unicode_icon` ← default, and title/description
cleared when `recipe_declared_description` says the recipe set them. Status, log, progress,
dependencies, version and command-set fields are kept (Phase 1 Decision 1).

**`is_applied`.** `AssetData::new_ext` sets `assetinfo.is_applied = !initial_state.is_none()` —
the condition its doc comment already names. The `MetadataRecord` ↔ `AssetInfo` conversions copy
the flag. `prefix_metadata` copies the asset's record, so every intermediate state of an applied
plan carries it; a fetched state carries its own asset's (false: dependencies are never applied).

`apply_plan_state` loops `state = do_step_state(step, state, …)`; the `origin_key` bookkeeping
goes. The final state is still discarded by `apply_plan`; the asset's record is untouched (AC-10).

## Recording the prefix query (`plan.rs`)

- `PlanBuilder::process_action` already receives the level's `query`. For a multi-segment level
  that is the prefix ending with the action; for a single-transform level (`plan.rs` ≈1770) the
  caller empties `segments`, so it passes the original query instead. The recorded value is
  `promote_relative_default_links(prefix, cmr)`, as for `Plan::predecessor`.
- `Plan::freeze_cwd_with` resolves each `Step::Action.query` with the same post-prologue cursor
  it uses for `predecessor` (a prefix starts where the query starts).
- `Recipe::to_plan` sets every action's `query` to `None` when `has_arguments()` (Decision 6).
- Aliases keep the query as written (alias name); `origin` already records the target.

## Cut rule (`Plan::cut_predecessor`)

After the walk-back settles `cut_at`, decline when `steps[prologue_steps..cut_at]`, ignoring
`SetCwd`, is exactly one key-read step (Decision 4), with `init_info("Predecessor boundary not
cut: the prefix only reads a key")`. Volatility, payload and dependency analysis are unchanged:
they were computed over the expanded plan.

## `bin` (`recipes.rs`, `liquers-lib/src/egui/widgets.rs`)

`Recipe::data_format` returns `self.extension()` — `None` without a filename.
`Recipe::get_asset_info` assigns it directly. A keyed asset whose recipe query has no filename now
gets its format from the key's filename (`AssetInfo::with_key` → `with_filename` seeds only an
absent format). The egui recipe widget shows the row only when `Some`.

## Integration Points

- `liquers-core/src/plan.rs`: `Step::Action` field; `process_action` (both arms); `freeze_cwd_with`;
  `cut_predecessor`; `resolve`/`clone` helpers matching the variant; tests constructing it.
- `liquers-core/src/interpreter.rs`: `apply_plan`, `do_step` → wrappers; new `apply_plan_state`,
  `do_step_state`, `prefix_metadata`; remove `value_origin_key`, `fetched_key` and
  `fetched_key_honours_the_resource_header`; `resolve_absolute_query_resource_step` carries `query`.
- `liquers-core/src/recipes.rs`: `data_format`, `get_asset_info`, `to_plan` override rule (its
  `Step::Action` test patterns use `..` and are unaffected).
- `liquers-core/src/assets.rs`: `AssetRef::recipe_declared_description`; `AssetData::new_ext` sets
  `is_applied`.
- `liquers-core/src/metadata.rs`: `is_applied` on `MetadataRecord` and `AssetInfo`, both
  conversions, `Metadata::is_applied`, legacy read.
- `liquers-lib/src/egui/widgets.rs`: `Recipe::data_format` caller.
- `liquers-lib/src/records/commands.rs` ≈575: comment naming `value_origin_key`.
- Tests constructing `Step::Action` literally: `liquers-core/tests/command_alias.rs`,
  `validate_integration.rs`, `liquers-lib/tests/plan_namespace_resolution.rs` (patterns with
  `..` are unaffected).

## Error Handling

No new failure modes. `GetResource` with legacy stored metadata degrades to a warning, as
`GetResourceMetadata` does today. `context.get_metadata()` keeps its `unexpected_error` for a
legacy asset record. `query.encode()` / freezing errors propagate as today via `?`.

## Relevant Commands

None added or changed; `specs/command_registry.yaml` is unaffected (plans are not exported).
Exercised: `pl` (`slice`, `head` alias), `rec` (`materialize`, `to_record`).

## Documentation Architecture

| Document | Kind | Change |
|---|---|---|
| `specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md` | reference | New §Metadata ownership during evaluation (Phase 1 Background + reference rule); rewrite §Context lifetime step-state paragraph |
| `specs/reference/api/DOC_08_RECIPES_PLANS.md` | reference | §Plan fields: `Step::Action.query`; §execution: next-state table; cut exception for a bare key read; recipe `data_format` |
| `specs/reference/VALUE_TYPE_SYSTEM.md` | reference | §Seeding: a query without a filename seeds nothing (check only; add a History row if touched) |
| `specs/guides/COMMAND_REGISTRATION_GUIDE.md` | guide | Read the input's description from `state.metadata`; write the asset's through `Context` |
| `specs/README.md` | map | Capability line status update at Phase 5 |

`affects_docs` as in `DESIGN.md`.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `plan.rs`, `interpreter.rs`, `recipes.rs`, `assets.rs` (core); `egui/widgets.rs` (lib); 3 test files with literal `Step::Action` |
| Crates and workflows affected | `liquers-core`, `liquers-lib`; `liquers-validate` output gains `query` on action steps |
| Existing tests likely to change | Tests asserting `bin` on unnamed queries (`recipes.rs`, `assets.rs` unit tests, `records_end_to_end.rs`, `resolver_dependency_recording.rs` workarounds can be removed); tests asserting the cut plan shape of `-R/<key>/-/ns-x/action` (`plan.rs`, `plan_cwd_freeze.rs`, `predecessor`-cut tests); `fetched_key_honours_the_resource_header` deleted |
| New validation | Phase 3 tests per AC; full `cargo test -p liquers-lib --lib --tests` and `-p liquers-core`; `scripts/check-build-matrix.sh` (egui caller) |
| Compatibility | Serialized plans: new optional field, old plans load. HTTP/web: an unnamed query's media type now follows the value's default instead of `application/octet-stream` — intended (AC-8), visible to clients. Commands reading `state.metadata.status/log` of their input now see the input's, not the running asset's |
| Data / concurrency / performance / security | No stored-format change. Fewer metadata copies per step (pass-through and fetch steps no longer call `get_metadata`). One fewer asset per bare-key-read query |
| Recovery | Revert the commit; the plan field is optional, so plans serialized meanwhile still load |
| Log timing | Log entries reach the asset through the service channel, so the copy `prefix_metadata` takes may lag the last entries a command wrote. AC-6 is asserted on the final asset's log, or after the channel drains; the input state's log is informational (Decision 5) |
| Certainty and open questions | High for the interpreter and `bin`; medium for the cut rule's interaction with recipe prologues (covered by `plan_cwd_freeze` tests). Phase 1 Decision 7 resolved (`is_applied`) |
