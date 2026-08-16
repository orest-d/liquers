# Phase 3: Examples & Use-cases - Plan CWD Freeze

## High-Level Introduction

Phase 1 set out to collapse three independent CWD cursors into one freeze pass, and Phase 2
established that the predecessor cut is then a **policy** choice rather than a correctness one. That
conclusion is the thing these examples have to earn: if cutting and expanding are equivalent, the
claim must be mechanically checked across every query shape, not asserted once.

The scenarios progress accordingly. **Scenario 1** is the workflow the design exists for — the same
analysis recipe in two sibling folders, showing what freeze resolves and why a shared input still
lands on one cache entry. **Scenario 2** goes into the mechanisms Scenario 1 glosses: how a `-R-key/.`
default link reaches a command, and what a cut boundary looks like on a frozen plan. **Scenario 3**
is the pitfalls set, and it is not hypothetical — every entry is a failure measured at HEAD when
`disable_expand_predecessors()` was switched on.

The test plan then has three layers: freeze unit tests that pin the traversal itself, the
**equivalence suite** that discharges the Phase 2 claim, and the rejection/migration tests that cover
the `Context` surface change.

## Example Type

**Runnable.** The Phase 2 conclusion is an equivalence claim, and a conceptual snippet cannot
discharge one — the suite must execute both forms and compare. Examples are therefore written as
tests in `liquers-core/tests/`, not as `examples/*.rs` demos: there is no user-facing API here to
demonstrate, only internal behaviour to pin. **Confirm at the gate.**

## Overview Table

| # | Type | Name | What it demonstrates or checks | Drafted as |
|---|---|---|---|---|
| 1 | Example | Two folders, one shared input | Freeze resolves relative operands per folder while an absolute operand stays one cache entry | Pass 1 |
| 2 | Example | CWD as a link argument, and a cut boundary | `-R-key/.` reaching a command; what `cut_predecessor` produces on a frozen plan | Pass 2 |
| 3 | Example | Four pitfalls measured at HEAD | The failures this design removes, each with its observed symptom | Pass 3 |
| 4 | Unit | `plan::tests::freeze_*` (9) | Per-step traversal, scope rules, idempotence, serde defaults | Pass 4 |
| 5 | Unit | `query::tests::cursor_consumed_*` (2) | The migration flag that proves runtime cursors go idle | Pass 4 |
| 6 | Integration | `tests/plan_cwd_freeze.rs` — equivalence suite (12 shapes) | **Primary deliverable**: cut and expanded agree on value, flags and error | Pass 5 |
| 7 | Integration | `tests/plan_cwd_freeze.rs` — rejection (5) | Relative `evaluate`/`apply` refused; non-key and absolute queries still accepted | Pass 5 |
| 8 | Integration | `tests/recipe_cwd_resolution.rs` — rewritten helpers (3) | The removed capability re-expressed through `-R-key/.` | Pass 5 |
| 9 | Integration | `tests/plan_cwd_freeze.rs` — corner cases (5) | Serialization, concurrency, memory, cross-crate, cycle | Pass 5 |

## Example 1: Two Folders, One Shared Input

### Connection to the High-Level Design

This is the Phase 1 purpose in one picture: relative operands resolved once, at freeze, so the three
downstream passes agree — and the cache behaviour that motivated cutting in the first place.

### Scenario

An analysis recipe is stored in `proj/a` and again in `proj/b`. Each joins its own local
`input.csv` against a single large shared `data/big.csv`. The folder-local input must resolve per
folder; the shared input must not be fetched or cached twice.

### Sequence of Steps

1. The asset manager is asked for `proj/a/report.txt` and finds the recipe in `proj/a/recipes.yaml`.
2. `Recipe::to_plan` builds a source-relative plan and prepends `Step::SetCwd(proj/a)`.
3. `finalize_plan` snapshots the entry CWD and calls `Plan::freeze_cwd`, which walks the steps in
   order: `SetCwd` advances the cursor, `GetAsset(./input.csv)` becomes `GetAsset(proj/a/input.csv)`,
   and the link parameter `-R/data/big.csv` is resolved against a *clone* of the cursor and, being
   absolute, is returned unchanged.
4. Dependency analysis and pre-scheduling run over the already-absolute plan.
5. `apply_plan` executes; the shared dependency resolves to the same `DependencyKey` from both
   folders, so one asset serves both.

### Core Example Code

```rust
// liquers-core/tests/plan_cwd_freeze.rs
type CommandEnvironment = ImmediateEnvironment<Value>;

#[tokio::test]
async fn shared_absolute_input_is_one_asset_across_folders(
) -> Result<(), Box<dyn std::error::Error>> {
    let store = AsyncMemoryStore::new(&Key::new());
    seed(&store, "data/big.csv", "shared").await?;
    for folder in ["proj/a", "proj/b"] {
        seed(&store, &format!("{folder}/input.csv"), folder).await?;
        seed_yaml(&store, &format!("{folder}/recipes.yaml"),
            b"recipes:\n  - query: -R/./input.csv/-/join-~X~-R/data/big.csv~E/report.txt\n").await?;
    }

    let envref = env_with(store)?;
    let manager = envref.get_asset_manager();

    for folder in ["proj/a", "proj/b"] {
        let asset = manager.get(&parse_key(&format!("{folder}/report.txt"))?).await?;
        assert_eq!(asset.get().await?.try_into_string()?, format!("{folder}+shared"));
    }

    // Folder-local input resolved per folder; shared input resolved once.
    assert_eq!(join_calls(), 2, "one join per folder");
    assert_eq!(big_csv_reads(), 1, "the shared input is fetched once, not once per folder");
    Ok(())
}
```

The measured basis for the assertion — `CwdCursor::resolve_key` returns a non-relative key unchanged,
so an absolute operand normalises identically from every folder:

```
-R/./input.csv/-/analyze    cwd=proj/a -> -R/proj/a/input.csv/-/analyze
                            cwd=proj/b -> -R/proj/b/input.csv/-/analyze
-R/data/big.csv/-/analyze   cwd=proj/a -> -R/data/big.csv/-/analyze
                            cwd=proj/b -> -R/data/big.csv/-/analyze     <- same key
```

## Example 2: CWD as a Link Argument, and a Cut Boundary

### Connection to the High-Level Design

Scenario 1 left two mechanisms implicit: how a command that genuinely needs the directory gets it
once `Context::get_cwd_key` is crate-private, and what a cut actually produces. Both are the parts
of the design a future reader is most likely to get wrong.

### Scenario

A `list_siblings` command needs the directory it is evaluated in. It declares a default link
`-R-key/.` rather than reading the context. The same query is then evaluated with the boundary cut
enabled, to show the promotion that keeps the boundary query self-contained.

### Sequence of Steps

1. `list_siblings` is registered with `dir: Key = query "-R-key/."`.
2. `PlanBuilder` resolves the default into `ParameterValue::DefaultLink("dir", -R-key/.)`, and —
   because that default is *relative* — records the predecessor query with the link **promoted** to
   an explicit `~X~-R-key/.~E`.
3. `freeze_cwd` rewrites both the plan parameter and the recorded predecessor query against the
   entry CWD, giving `-R-key/proj/a`.
4. The link resolves inline (Phase 2 decision 3): its plan is the single step
   `Step::UseKeyValue(proj/a)`, so no dependency asset is created.
5. `cut_predecessor` replaces the leading steps with `Step::Evaluate(<frozen predecessor>)`, leaving
   any `Step::SetCwd` and the trailing `Step::Action` + `Step::Filename` in the parent.

### Core Example Code

```rust
fn list_siblings(state: &State<Value>, dir: Key) -> Result<Value, Error> {
    // No get_cwd_key call: the directory arrives as data and is overridable.
    Ok(Value::from(dir.encode()))
}

register_command!(cr, fn list_siblings(state, dir: Key = query "-R-key/.") -> result)?;

#[tokio::test]
async fn cwd_link_default_is_promoted_and_frozen() -> Result<(), Box<dyn std::error::Error>> {
    let mut plan = recipe_plan("list_siblings/out.txt", Some("proj/a"))?;
    plan.freeze_cwd(&parse_key("proj/a")?)?;

    // The promoted, frozen predecessor query is self-contained.
    assert_eq!(
        plan.predecessor.as_ref().map(|q| q.encode()).as_deref(),
        Some("list_siblings-~X~-R-key/proj/a~E")
    );

    // Cutting keeps the last action and the filename in the parent.
    assert!(plan.cut_predecessor()?);
    assert!(matches!(plan.steps.last(), Some(Step::Filename(_))));
    assert!(plan.steps.iter().any(|s| matches!(s, Step::Evaluate(_))));
    Ok(())
}
```

**Why promotion is needed at all**, measured at HEAD: a default link is invisible to the query text,
so without promotion the boundary query is byte-identical in every folder.

```
default  -R-key/.    plan.query.encode() = list_stuff       (same under every CWD)
explicit ~X~-R-key/.~E  -> list_stuff-~X~-R-key/proj/a~E    (correct per folder)
```

## Example 3: Four Pitfalls, Each Measured at HEAD

Every entry below is an observed failure from switching `disable_expand_predecessors()` on at HEAD
(11 failures, `cargo test -p liquers-core --lib`). They are the regression set.

### Pitfall 1 — a filename is not an action

`Query::predecessor()` splits a trailing filename off as the remainder, so the cut swallowed the real
last action. Measured, the expanded plan for `-R/./x.csv/-/analyze/result.txt` is:

```
steps: [GetAsset, Action, Filename]
```

Cut at HEAD it became `[Evaluate(-R/./x.csv/-/analyze), Filename]` — no `Step::Action`, so
`Recipe::to_plan`'s override pass failed hard with `Link input not found in last action`. Because a
recipe needs a filename to be addressable in a directory, this hit *every* stored recipe with
overrides.

**Corrected by:** the cut moving after freeze, where it operates on steps and keeps the trailing
action and filename in the parent by construction.

### Pitfall 2 — a payload does not cross an undeclared boundary

`test_evaluate_immediately` registers `word` with an injected payload parameter and no
`payload: required`. Expanded it works; cut it fails with `No payload in context for injected
parameter payload at position 1`. Verified: adding `payload: required` makes it pass with the cut
enabled.

**Corrected by:** declaring it. This is the documented "declare it, or lose it" rule, not a design
defect — but the equivalence suite must cover both declarations so the difference is visible.

### Pitfall 3 — volatility hidden behind the boundary

`Recipe::is_volatile` reads the **build-time** `plan.is_volatile`. With the predecessor cut away by
the builder, a volatile command in the prefix was invisible, so `vol_counted/vol.txt` was treated as
non-volatile, got a non-volatile keyed asset, and was fast-tracked from the store on the second
request — the command ran once instead of twice.

**Corrected by:** the builder always expanding, so volatility, payload and expiration are computed
over the full plan before anything is cut.

### Pitfall 4 — a cut hides the cause of a failure

Observed in the same run: the parent's error was `Dependency asset 1001 did not produce a value
(status Error)`, while the real cause `Command 'word' failed: No payload in context for injected
parameter payload at position 1` appeared only in the sub-asset's log. `assets.rs:4446` builds the
parent error from scratch and never chains the dependency's.

**Corrected by:** chaining the cause, in scope for this design. Without it, "cut and expanded are
equivalent" is false in the way a user notices first.

## Unit Tests

### `liquers-core/src/plan.rs` — `mod tests`

| Test | Checks |
|---|---|
| `freeze_resolves_every_keyed_step` | All nine key-bearing `Step` variants become absolute. Written as an exhaustive `match` so a new variant fails to compile. |
| `freeze_applies_setcwd_in_order` | `SetCwd(a/b)` then `SetCwd(../c)` then `GetAsset(./x)` yields `a/c/x`. |
| `freeze_scopes_link_parameters` | A link containing `-R-cwd/./child` does not move the enclosing cursor — the cloned-cursor rule. |
| `freeze_shares_cursor_with_nested_plan` | `Step::Plan` propagates its final CWD to later outer steps, matching `find_dependencies_nested_plan_propagates_cwd`. |
| `freeze_absolute_query_resolves_against_root` | `/-R/./x` resolves to `/x` regardless of entry, via the single pre-read of `absolute_query_resource_step_index`. |
| `freeze_is_idempotent` | `freeze(freeze(p, k), k) == freeze(p, k)`, steps compared field-by-field. |
| `freeze_twice_under_different_cwd_errors` | `Error::general_error`, `ErrorType::General`, query attached. |
| `freeze_without_entry_cwd_warns_once` | Logical root installed, `RELATIVE_WITHOUT_CWD_WARNING` emitted exactly once — not upgraded to an error. |
| `frozen_plan_serde_defaults_on_legacy` | A plan serialized without the three new fields deserializes with `frozen_cwd: None`. |

### `liquers-core/src/query.rs` — `mod tests`

| Test | Checks |
|---|---|
| `cursor_records_consumed_cwd` | `resolve_key` sets the flag on the relative branch only. |
| `cursor_reports_no_consumption_on_absolute` | An absolute key leaves the flag clear — the basis of the migration assertion. |

## Integration Tests

### The equivalence suite — `liquers-core/tests/plan_cwd_freeze.rs`

Table-driven over query shapes. For each shape the same environment evaluates the query twice, once
with the boundary cut and once expanded, and asserts **four** things agree: the produced value,
`is_volatile`, `payload_required`, and the surfaced error (type, message and position).

| # | Shape | Query | Covers |
|---|---|---|---|
| E1 | Pure transform chain | `word/greet-Ciao` | The base case; R1 when `word` declares `payload: required` |
| E2 | Resource then action | `-R/./input.csv/-/analyze` | Relative operand freezing |
| E3 | Resource, action, filename | `-R/./x.csv/-/analyze/result.txt` | Pitfall 1 |
| E4 | CWD-setting predecessor | `-R-cwd/./sub/-R/./input.csv/-/analyze` | Pitfall from R3 |
| E5 | Absolute query | `/-R/./input.csv/-/analyze` | Root resolution across the boundary |
| E6 | Volatile predecessor | `vol_counted/vol.txt` | Pitfall 3 — recomputes both ways |
| E7 | Payload predecessor, declared | `word/greet-Ciao` | Payload crosses the boundary |
| E8 | Payload predecessor, undeclared | same, without `payload: required` | The two forms **differ**; asserted as a known, documented divergence |
| E9 | Explicit link parameter | `-R/./x.csv/-/join-~X~-R/data/big.csv~E` | Link scope under freeze |
| E10 | Recipe with link override | `use/result.txt` + link `input` | Overrides stay on the parent's last action |
| E11 | Relative default link | `list_siblings/out.txt` | Promotion (Example 2) |
| E12 | Nested plan | hand-built `Step::Plan` | Shared-cursor rule end to end |

All twelve queries were checked with `liquers-validate` before being written down: 7 ok, 0 warnings,
0 errors for the shapes expressible without a registry, the remainder validated with `--command`
overlays.

E8 is deliberately an **inequivalence** test. Phase 2 concluded the two forms differ only for
declaration defects; E8 pins that difference so the conclusion is falsifiable rather than assumed.

### Rejection — `liquers-core/tests/plan_cwd_freeze.rs`

| Test | Checks |
|---|---|
| `evaluate_rejects_relative_query` | `Error::not_supported`, position on the offending segment, message names `-R-key/.` |
| `apply_rejects_relative_query` | Same at the second choke point (`context.rs:595`) |
| `dependency_state_rejects_relative_query` | Covered via `schedule_dependency_asset` (`context.rs:423`) |
| `evaluate_accepts_query_without_key_operand` | `greet-Hello` still valid — the test is operand form, not `query.absolute` |
| `evaluate_accepts_absolute_query` | `/-R/data/x.csv` unaffected |

### Migration of the removed capability — `liquers-core/tests/recipe_cwd_resolution.rs`

The three helpers are **rewritten, not deleted**, and `context_boundary_commands_use_active_cwd` keeps
its name and its assertions:

| Was | Becomes |
|---|---|
| `via_evaluate` — `context.evaluate("-R/./hello.txt")` | takes `dir: Key = query "-R-key/."`, builds the absolute query, evaluates it |
| `via_state` — `context.get_dependency_state("-R/./hello.txt")` | same shape |
| `via_apply` — `context.apply("-R-stored/./identity")` | same shape |

Plus the unit test at `context.rs:1602` (`-R-key/./from-apply`), rewritten the same way.

### Corner cases

| Test | Concern |
|---|---|
| `frozen_plan_survives_serde_round_trip` | Serialization — a frozen plan reloads frozen, with `frozen_cwd` intact |
| `two_folders_race_on_one_boundary_query` | Concurrency — two contexts requesting the same boundary query concurrently get one asset and one evaluation |
| `large_intermediate_is_not_retained_when_expanded` | Memory — the resource trade Phase 2 identified; asserts the expanded form does not insert the intermediate into `query_assets` |
| `liquers_lib_environment_freezes_via_finalize_plan` | Cross-crate — `liquers-lib`'s `apply_recipe` inherits freeze without its own call |
| `self_referential_prefix_is_a_cycle_not_a_recursion` | A cut adds a dependency edge; cycle detection must catch it |
| `runtime_cursor_is_idle_after_freeze` | Migration (Phase 2 decision 5) — `take_consumed_cwd()` is false for every runtime resolution on a frozen plan |

## Test Plan

Run order and commands:

```bash
cargo test -p liquers-core --lib                       # freeze + cursor unit tests
cargo test -p liquers-core --test plan_cwd_freeze      # equivalence, rejection, corner cases
cargo test -p liquers-core --test recipe_cwd_resolution # migrated capability
cargo test -p liquers-lib --lib --tests                # cross-crate, the CLAUDE.md default loop
```

Baseline to preserve: `cargo test -p liquers-core --lib` is **548 passed, 0 failed** on the rebased
tree. The eleven failures this design removes are all inside that suite, so the same command is the
regression gate.

Not covered, deliberately: throughput from parallel boundary scheduling (Phase 2 decision 6 files it
separately, and it wants a benchmark rather than a test), and `liquers-py`, whose `apply_recipe` is
`todo!()`.

## Documentation and Learning Log

### Guide Candidate Workflows and Examples

Phase 1 and 2 both decided **no guide**, and Phase 3 does not overturn it: nothing here is a
repeatable developer task. One candidate did emerge and is deliberately declined — "how do I write a
command that needs its directory?" is a two-line answer (`dir: Key = query "-R-key/."`), which
belongs in `PROJECT_OVERVIEW.md` beside the rejection rule, not in a guide of its own. Reconsider
only if the boundary default is flipped, since that would change how recipe authors reason about
caching.

The executable evidence a future guide would link, rather than duplicate: Example 2's
`cwd_link_default_is_promoted_and_frozen` for the `-R-key/.` mechanism, and the rewritten
`context_boundary_commands_use_active_cwd` for the migration shape. Both already exist in the plan;
Phase 4 need not create a standalone example.

### Usage, Meaning, and Connections

Facts belonging in the planned reference updates:

- A plan stops being CWD-relative at `freeze_cwd`, called inside `finalize_plan` before dependency
  analysis. Before that point operands are source-relative; after it they are absolute and the plan
  is never re-frozen under a different CWD.
- `Context::evaluate`/`apply` require queries whose resource operands are absolute. The directory is
  obtained as a `-R-key/.` link argument, which is explicit, overridable and visible to the planner.
- Cutting a predecessor into `Step::Evaluate` is a caching policy, not a semantic one. It connects
  to recipes (overrides stay on the parent's last action), to the asset manager (a boundary query
  becomes its own cache entry) and to expiration (a boundary expires independently).
- Identity is canonical in both mechanisms: `query_assets` is keyed by the AST and `DependencyKey` by
  `encode()`, both over decoded semantics, so two spellings of one query cannot name two assets.

### Repeatable Development Guidance

- Switching a plan-builder policy on and running the suite is how this class of defect surfaces; the
  eleven HEAD failures were found that way in minutes, having been unexplained for months.
- `liquers-validate` with `--command` overlays checks a query shape before it is written into a test
  or a document, with no store and no environment.
- A scratch `#[cfg(test)] mod` that prints a table of measured behaviour, then `git checkout` of the
  file, is a cheap way to establish a fact without leaving a test behind.

### Corrections and Unexpected Learning

Assumptions corrected during the design, worth preserving in Phase 5:

1. **The issue's premise was wrong twice.** Nothing panics, and the failure is not undiagnosed — it
   is the documented `payload: required` rule. The title says "crashes"; it does not.
2. **The boundary was a symptom.** The design started as a cut fix and became a freeze design once
   three independent CWD cursors turned out to be the actual problem.
3. **Rejecting relative `evaluate`/`apply` was proposed, withdrawn, then reinstated** — withdrawn
   because freeze makes execution deterministic without it, reinstated because nothing identifies
   CWD-dynamic commands, so tolerating them forces a CWD into every boundary query.
4. **`encode` round-tripping was broken and then fixed mid-design.** Measured as failing for the
   protocol mnemonics, then re-measured as passing after rebasing onto `parameter-entity-escaping`.
   A constraint recorded as load-bearing reverted to ordinary good practice inside one phase — a
   reminder to re-measure a preflight finding rather than carry it forward.
5. **The prior design already anticipated this pass.** `plan-relative-resolution` §"Future Plan
   Normalization and Optimization" blessed rewriting operands and blocked only *removing* `SetCwd`.
   Reading it saved re-deriving the constraint and settled Phase 2 decision 4.

The accumulated information does **not** overturn the Phase 1 `neither` decision on a guide; it is
reference material about current behaviour, which is where it is going.

## Review Record

Host does not permit spawning agents, so the three Phase 3 review passes ran sequentially against
the same briefs, per this skill's host-compatibility clause.

**Reviewer 1 — Phase 1 conformity.** Scenarios map to the Phase 1 purpose; no scope added. One gap
closed: Phase 1 promised the equivalence claim but named no artifact, so the suite is now the
document's primary deliverable rather than an appendix.

**Reviewer 2 — Phase 2 conformity.** Signatures used in the examples match Phase 2
(`freeze_cwd(&Key) -> Result<Key, Error>`, `cut_predecessor() -> Result<bool, Error>`,
`Plan::{frozen_cwd, predecessor, predecessor_steps}`). Two corrections applied: an earlier draft had
`freeze_cwd` returning `()`, and had the link default resolving as a dependency asset rather than
inline (Phase 2 decision 3).

**Reviewer 3 — codebase and query validation.** All queries passed `liquers-validate` (7 ok, 0
warnings, 0 errors; unregistered commands supplied via `--command`). No query contains a space or
newline. Every `-R/` query is exercised in an environment with a store. Environments follow the
`liquers-unittest` table: `ImmediateEnvironment<Value>` for plan/CWD, `SimpleEnvironment<Value>` for
recipes and assets, `SimpleEnvironmentWithPayload<Value, String>` for E7/E8. Test file placement
follows the convention — inline `mod tests` for `plan.rs`/`query.rs`, `tests/` for the end-to-end
suites.
