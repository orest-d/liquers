# Phase 5: Documentation - plan-policy

## Completion Preconditions

- [x] Implementation is finished and validated (Phase 4 Steps 1–10, all ticked)
- [x] All user comments are answered or incorporated (six discussion rounds, `DESIGN.md` Notes)
- [x] All review comments are answered or incorporated (no PR review yet)
- [x] Documentation is consistent with the implemented and tested behavior
- [x] Documentation is included in the implementation branch

## Implementation Summary

`CORE-PLAN-POLICY-AND-DEFAULTS` asked for the plan builder's three unsupported policies (`cache`,
`volatile flags`, `inline flag`) to become deliberate, stated choices. After six discussion rounds
they became:

- **`cache` → caching strategies.** `liquers-core/src/cache_strategy.rs` `CacheStrategy`
  (`none` / `result` / `all`) is a property of each asset (`AssetRef::cache_strategy`,
  `Context::cache_strategy`):
  - a keyed asset takes its recipe's `cached:`, else `assets.recipe_cache_strategy`;
  - a top-level query takes `assets.query_cache_strategy`;
  - a non-keyed dependency (a boundary, a link, a `context.evaluate`) takes its creator's.

  An existing asset is reused under every strategy. `Recipe::cached` widened from `Option<bool>`
  to `Option<CacheStrategy>`; it still reads `true`/`false` and also accepts `default`.
- **`inline flag` → `cached: false` on a command** (`CommandMetadata::cached`, the macro keyword,
  declarable from a document). The builder records the last action's flag as `Plan::uncached_by`.
  The boundary walk steps back past such a candidate, so the command runs inline:
  `a/b/c/d` → `Evaluate(a/b) c d`.
- **`volatile flags` → positional `v`.** Merged from `V-INSTRUCTION-IS-WHOLE-PLAN-NOT-POSITIONAL`:
  - `a/b/v/c` cuts `a/b`;
  - `a/b/v` is `Evaluate(a/b)`;
  - `v/…` recomputes everything;
  - only a recipe's `volatile: true` remains a whole-plan declaration.
- **`assets.cut_predecessors`**, a debugging switch: `false` gives a fully expanded plan.
- The three markers are replaced by builder documentation, and a test pins their absence.

The plan never reads a strategy or the manager's state; registration is decided where a boundary
executes (`get_dependency_asset`). `ImmediateAssetManager` gained a `get_dependency_asset` override
for that. Unregistered assets record `cached: Some(false)` and a log reason. Every default equals
the previous behaviour.

**Conformance.** The implementation matches approved Phases 1–4, with the following deviations:

- *Added:* `cached` is declarable from a command declaration document. No code was needed, since
  every metadata field is declarable.
- *Added:* a recipe stating `result`/`all` records `Some(true)` in metadata. This keeps the
  existing `stored_cached_flags` contract.
- *Changed:* the AC-8 log check became a recomputation check, because planning diagnostics never
  reach the asset log (filed below).
- *Fixed in passing:* a stale claim in `DOC_02` that `v` accepts parameters.
- *Omitted:* nothing. The in-query cache directive was deferred by the user before Phase 1 was
  approved, so it was never in scope.

## Documentation Delivered

### New Reference Documents
None. The behaviour extends existing references, listed below; a new page would duplicate
`ASSETS.md` and `DOC_08`.

### New Guide Documents
None. Two new sections were added to existing guides: `ENVIRONMENT_CONSTRUCTION_GUIDE.md`
§Choosing caching strategies and `COMMAND_DESIGN_GUIDE.md` §Outputs not worth keeping.

### Existing Documents Reviewed or Updated
Every `affects_docs` entry was updated against the implemented code, with a 2026-10-10 History row
and `reviewed:` bump:

| Document | Change |
|---|---|
| `reference/ENVIRONMENT_CONFIG.md` | three `assets:` keys, the origin rule, the public-service example, the accessors |
| `guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md` | §Choosing caching strategies; configuration example |
| `reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md` | dependencies use the current asset's strategy; `Context::cache_strategy` |
| `reference/api/DOC_08_RECIPES_PLANS.md` | recipe `cached`; positional `v`; `uncached_by`; recursive cutting (corrects "one cut retains one intermediate"); §What is kept; a fourth boundary condition; pitfalls |
| `reference/api/DOC_02_QUERY_LANGUAGE_REFERENCE.md` | `v` positional and rejecting parameters; stale gap struck |
| `reference/PROJECT_OVERVIEW.md` | `v` rule |
| `reference/COMMAND_DECLARATION.md`, `reference/REGISTER_COMMAND_FSD.md` | `cached` key and statement |
| `reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md`, `reference/ASSETS.md` | §When an asset is kept for reuse; registration rules |
| `guides/COMMAND_REGISTRATION_GUIDE.md`, `guides/COMMAND_DESIGN_GUIDE.md` | when and how to declare `cached: false`; aliases |
| `guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md` | new accessors; rules for a custom manager; `Recipe::cached` type |

`CLAUDE.md`'s macro DSL list gains `cached:`.

### Links and Capability Map
`specs/README.md` lists the design (generated). The guides link to `ASSETS.md` §When an asset is
kept for reuse and `DOC_08` §Predecessor boundaries, so normal use needs no design reading.

## Issues Filed

- `QUERY-CANNOT-MARK-CACHED-INTERMEDIATES` (feature, P3): the in-query `cache-on|off|this` switch,
  deferred by the user, with its cost/benefit assessment, including the asset-identity requirement.
- `PLANNING-DIAGNOSTICS-NEVER-REACH-THE-ASSET-LOG` (P2): `Metadata::update_from_plan` has no
  caller, so every `init_steps` diagnostic is lost from the evaluated asset. Found in Step 9.
- `CORE-ASSET-GC` updated with the user's cache-size-limit requirement.

Closed: `CORE-PLAN-POLICY-AND-DEFAULTS`, `V-INSTRUCTION-IS-WHOLE-PLAN-NOT-POSITIONAL`.

## Important Learning

- **Cutting is recursive.** Each boundary's own plan is cut again, so every prefix of a chain is
  an asset. This was measured with counting commands, and it corrected `DOC_08`. Any "intermediate"
  rule therefore applies at every level with no extra code.
- **A plan should not read caching policy.** Deciding reuse where `Step::Evaluate` executes
  (`get_dependency_asset`, which has the parent in hand) kept the plan a function of the query, the
  metadata and the configuration. It also made "strategy follows the origin" a one-line rule.
- **`Default`-derived types and default-`true` flags.** `CommandMetadata` and
  `AssetManagerOptions` both derive `Default`. A plain `bool` would have meant "not cached" and
  "never cut", so both fields are `Option` behind an accessor, and
  `default_options_cut_predecessors` pins the second.
- **A trait-default `get_dependency_asset` hides the parent.** `ImmediateAssetManager` had no
  override, so a dependency there was a top-level query. A custom manager must override it to
  honour strategies (`ASSET_MANAGER_IMPLEMENTATION_GUIDE.md`).

## Conformance and Remaining Work

Requested: deliberate defaults for the three markers. Approved: the strategy model, the command
flag, positional `v` and the switch. Implemented: all approved scope; nothing remains in this
design. The deferred directive and the diagnostics defect are issues.

## Validation

- `cargo test -p liquers-core --lib --tests`: all 42 suites pass, including the new
  `cache_strategy` suite (18 tests).
- `cargo test -p liquers-macro`, `cargo test -p liquers-records --all-features --lib --tests`, and
  `cargo test -p liquers-lib --lib --tests`, including `registry_export`: no regeneration needed.
- `bash scripts/check-build-matrix.sh`.
- `python3 scripts/docs_index.py --check`: 0 errors; `validate_phase.py plan-policy 1–5`.
