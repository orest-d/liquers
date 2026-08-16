# Phase 4: Implementation Plan - Plan CWD Freeze

## Overview

**Feature:** Freeze CWD in the plan, then cut correct predecessor evaluation boundaries.

**Architecture:** `Plan::freeze_cwd(entry)` rewrites every CWD-relative operand in execution order
with one cursor, called inside `finalize_plan` before dependency analysis. `PlanBuilder` stops
cutting and records the predecessor query instead; cutting becomes a post-freeze pass. `Context`'s
CWD accessors go crate-private and `evaluate`/`apply` reject relative queries.

**Estimated complexity:** High — the freeze traversal is the whole design, and step 8 is a breaking
API change.

**Estimated time:** 10-14 hours for a developer familiar with `liquers-core`.

**Prerequisites:** Phases 1-3 approved. Baseline on the rebased tree:
`cargo test -p liquers-core --lib` = **548 passed, 0 failed**.

### Two corrections from the rust-best-practices pass

Both were found while validating this plan and both change what Phase 2 said. Recorded here rather
than silently fixed:

1. **Phase 2's claim that `get_cwd_key` has "zero users outside `liquers-core`" was wrong.** It is
   true for other *crates*, but `liquers-core/tests/recipe_cwd_resolution.rs:31,38` calls it, and an
   integration test links the crate externally — so `pub(crate)` breaks that build. The two callers
   are the test commands `cwd` and `append_cwd`, which read the CWD from context: exactly the pattern
   this design removes. They are migrated in step 10 rather than being a reason to keep the accessor
   public, and Phase 3's migration list grows from 4 sites to 6.
2. **`Error` has no `cause` field** (`error_type`, `message`, `position`, `query`, `key`,
   `command_key`) and derives `Serialize`/`PartialEq`. Adding a recursive `cause: Option<Box<Error>>`
   would change the wire shape and works against `CORE-ERROR-PAYLOAD-SIZE`. Step 9 therefore chains
   by **message composition plus context carry-over**, using the existing
   `Error::from_error<E: Display>` (`Error` implements `Display` at `error.rs:398`), not a new field.

## Implementation Steps

Steps 1-6 are inert: freeze exists but nothing calls it, so the suite must stay green throughout.
Step 7 activates it. Step 8 is the breaking change. Those are the three checkpoints.

---

### Step 1: Record CWD consumption on the cursor

**File:** `liquers-core/src/query.rs`

**Action:** add a `consumed_cwd: bool` field to `CwdCursor`, set it in the relative branch of
`resolve_key` (`:2193-2203`), and expose `take_consumed_cwd`.

```rust
pub(crate) fn resolve_key(&mut self, key: &Key) -> Key {
    if !Self::is_relative(key) {
        return key.clone();
    }
    self.consumed_cwd = true;   // NEW
    let cwd = self.cwd.get_or_insert_with(|| { self.defaulted_to_root = true; Key::new() });
    key.to_absolute(cwd)
}

/// Whether any resolution by this cursor actually consumed the CWD. Clears the flag.
pub(crate) fn take_consumed_cwd(&mut self) -> bool;
```

**Validation:** `cargo check -p liquers-core` — compiles, no behaviour change.

**Rollback:** `git checkout liquers-core/src/query.rs`

**Agent:** haiku · rust-best-practices · knowledge: `query.rs` `CwdCursor` block. Follows the
existing `defaulted_to_root`/`take_root_fallback` pattern exactly.

---

### Step 2: Add the three `Plan` fields

**File:** `liquers-core/src/plan.rs` (`pub struct Plan`, `:1632`)

```rust
/// CWD every operand was resolved against, or `None` while still source-relative.
#[serde(default)]
pub frozen_cwd: Option<Key>,

/// Predecessor sub-query, with relative default links promoted to explicit query links.
#[serde(default)]
pub predecessor: Option<Query>,

/// Number of leading `steps` emitted for `predecessor`.
#[serde(default)]
pub predecessor_steps: usize,
```

Update `Plan::new` and every struct literal. `Key` and `Query` already satisfy the derives.

**Validation:** `cargo check -p liquers-core` and
`cargo test -p liquers-core --lib plan::tests` — existing plan tests unaffected.

**Rollback:** `git checkout liquers-core/src/plan.rs`

**Agent:** haiku · rust-best-practices · knowledge: `Plan` struct and its serde contract
(`:1629-1631` documents that only `serde(default)` fields are stable).

---

### Step 3: Freeze link queries inside parameters

**File:** `liquers-core/src/plan.rs` (`impl ResolvedParameterValues`)

```rust
/// Rewrite every link query against a **clone** of `cursor`, so a link's own `-R-cwd`
/// cannot move the enclosing plan's cursor.
pub(crate) fn freeze_cwd(&mut self, cursor: &CwdCursor);
```

Match `ParameterValue` exhaustively — `DefaultLink`, `ParameterLink`, `OverrideLink`, `EnumLink`
rewrite their query; `MultipleParameters` recurses; `DefaultValue`, `ParameterValue`,
`OverrideValue`, `Placeholder`, `Injected`, `None` are no-ops. **No `_ =>` arm.**

**Validation:** `cargo check -p liquers-core`

**Rollback:** `git checkout liquers-core/src/plan.rs`

**Agent:** sonnet · rust-best-practices · knowledge: `ParameterValue` variants (`:300-330`) and
`check_parameter_for_volatile_links` (`:1191`), which is the same traversal shape.

---

### Step 4: `Plan::freeze_cwd` — the traversal

**File:** `liquers-core/src/plan.rs`

```rust
impl Plan {
    /// Resolve every CWD-relative operand against `entry`, in execution order.
    /// Idempotent. Returns the CWD in effect after the last step.
    pub fn freeze_cwd(&mut self, entry: &Key) -> Result<Key, Error>;

    /// Continue an enclosing walk; nested plans share the caller's cursor.
    pub(crate) fn freeze_cwd_with(&mut self, cursor: &mut CwdCursor) -> Result<(), Error>;
}
```

Behaviour, per the Phase 2 traversal table — the `Step` match is **exhaustive, no `_ =>` arm**:

- Read `absolute_query_resource_step_index()` **once, before rewriting**; that step resolves against
  logical root instead of `entry`.
- Key-bearing steps → `cursor.resolve_key`. `SetCwd` → `cursor.set_cwd_from` (advances and rewrites).
  `Evaluate`/`UseQueryValue` → `cursor.resolve_query_scoped`. `Action` → `parameters.freeze_cwd(&cursor)`
  with a **cloned** cursor. `Plan` → `freeze_cwd_with(cursor)`, **sharing**. `Filename`/`Info`/
  `Warning`/`Error` → no-op.
- Also rewrite `self.predecessor` against a clone of the **entry** cursor, since the predecessor is
  the leading steps.
- Set `self.frozen_cwd = Some(entry.clone())`.
- If already `Some(k)`: return `Ok` when `k == entry` (idempotent), else
  `Error::general_error(...).with_query(&self.query)`.

**Validation:**
```bash
cargo check -p liquers-core
cargo test -p liquers-core --lib freeze_        # the 9 unit tests from Phase 3, added in step 12
```

**Rollback:** `git checkout liquers-core/src/plan.rs`

**Agent:** **sonnet** · rust-best-practices · knowledge: Phase 2 traversal table, `CwdCursor`
(`query.rs:2170-2266`), `find_dependencies` (`:2072`) as the reference for cursor scope rules, and
`find_dependencies_nested_plan_propagates_cwd` / `find_dependencies_child_query_cwd_does_not_leak`,
which pin the share-vs-clone asymmetry. *Rationale:* this is the design; the scope rules are the
part most easily got wrong.

---

### Step 5: `PlanBuilder` stops cutting and records the predecessor

**File:** `liquers-core/src/plan.rs`

**Action:**
- Delete the `expand_predecessors` field (`:1064`), its initialiser (`:1089`), and the
  `expand_predecessors()` / `disable_expand_predecessors()` methods (`:1105-1112`).
- In `process_query`, replace the `else if self.expand_predecessors { … } else { push Evaluate }`
  branch (`:1571`) with unconditional `self.process_query(p)?`.
- Immediately before recursing, record `self.plan.predecessor = Some(promote_relative_default_links(p, cmr)?)`;
  immediately after, `self.plan.predecessor_steps = self.plan.steps.len()`.

```rust
/// Promote every *relative* default link in `query`'s actions to an explicit query link, so the
/// query is self-contained. An absolute default is left implicit — command metadata reproduces it,
/// so the query does not grow in the common case.
fn promote_relative_default_links(
    query: &Query,
    cmr: &CommandMetadataRegistry,
) -> Result<Query, Error>;
```

Build the result as an AST (`ActionParameter::Link`), never by string concatenation —
`QUERY-BUILDER-TOOLING`'s stated workaround.

**Validation:**
```bash
cargo test -p liquers-core --lib     # expect 548 passed; the option is unused today
```

**Rollback:** `git checkout liquers-core/src/plan.rs`

**Agent:** **sonnet** · rust-best-practices · knowledge: `process_query` (`:1478-1604`),
`Query::predecessor` (`query.rs:2451`), `ResolvedParameterValues::from_action` (`:425-470`) for how a
`CommandParameterValue::Query` default becomes a `DefaultLink`.

---

### Step 6: `Plan::cut_predecessor`

**File:** `liquers-core/src/plan.rs`

```rust
/// Replace the leading `predecessor_steps` with a single `Step::Evaluate`, keeping any
/// `Step::SetCwd` among them. Requires a frozen plan. `Ok(false)` when there is nothing to cut.
pub fn cut_predecessor(&mut self) -> Result<bool, Error>;
```

- `frozen_cwd.is_none()` → `Error::general_error(...).with_query(&self.query)`.
- `predecessor.is_none() || predecessor_steps == 0` → `Ok(false)`.
- Otherwise retain the `SetCwd` steps from `steps[..predecessor_steps]`, drop the rest, insert
  `Step::Evaluate(predecessor.clone())` after them, and set `predecessor_steps` to the new count.
- `is_volatile`, `payload_required`, `expires` and `dependencies` are **not** recomputed — they were
  computed over the full expanded plan, which is the point of moving the cut after building.

**Validation:** `cargo test -p liquers-core --lib cut_predecessor`

**Rollback:** `git checkout liquers-core/src/plan.rs`

**Agent:** sonnet · rust-best-practices · knowledge: Phase 2 §Cut/No-Cut Equivalence, `Step` variants.

---

### Step 7: Call freeze from `finalize_plan`

**File:** `liquers-core/src/interpreter.rs`

```rust
pub async fn finalize_plan<E: Environment>(...) -> Result<(), Error> {
    let entry = match context.get_cwd_key() {
        Some(key) => key,
        None => {
            if context.install_logical_root_if_unset() {
                context.warning(RELATIVE_WITHOUT_CWD_WARNING)?;   // exactly once, existing path
            }
            Key::new()
        }
    };
    plan.freeze_cwd(&entry)?;                                     // NEW
    has_volatile_dependencies(envref.clone(), plan, None).await?; // entry CWD no longer needed
    has_expirable_dependencies(envref.clone(), plan).await?;
    // ... unchanged ...
}
```

Also: `apply_plan` skips `resolve_absolute_query_resource_step` when `plan.frozen_cwd.is_some()` —
freeze has already applied it.

**This is the activation step.** Every behaviour change from steps 1-6 becomes live here.

**Validation:**
```bash
cargo test -p liquers-core --lib          # expect 548 passed
cargo test -p liquers-core --test recipe_cwd_resolution
cargo test -p liquers-lib --lib --tests   # cross-crate: liquers-lib's apply_recipe inherits freeze
```

**Rollback:** `git checkout liquers-core/src/interpreter.rs` — steps 1-6 return to inert.

**Agent:** **sonnet** · rust-best-practices · knowledge: `finalize_plan` (`:36-60`), the
root-fallback contract in `schedule_plan_dependencies` (`:184-196`), `apply_plan` (`:240`).
*Rationale:* the warning-once behaviour is easy to duplicate or drop, and this step is where every
latent mistake from 1-6 surfaces.

---

### Step 8: `Context` — visibility and relative-query rejection

**File:** `liquers-core/src/context.rs`

- `get_cwd_key` (`:729`) and `set_cwd_key` (`:737`) → `pub(crate)`.
- Add and call the guard at both choke points, `:423` (`schedule_dependency_asset`, covering
  `evaluate` and `get_dependency_state`) and `:595` (`apply`):

```rust
/// Reject a query carrying a CWD-relative resource operand, recursively including link parameters.
/// Tests operand form via `CwdCursor::is_relative` (`query.rs:2179`) — **not** `!query.absolute`,
/// so a query with no key operand (`greet-Hello`) stays valid.
fn reject_relative_query(query: &Query) -> Result<(), Error>;
```

`Error::not_supported`, `.with_query(query)`, `.with_position(&segment.position)`, message naming
`-R-key/.` as the replacement — in the style of the payload message at `interpreter.rs:260`.

**Breaking change.** Fails the build of `liquers-core/tests/recipe_cwd_resolution.rs` until step 10.

**Validation:** `cargo check -p liquers-core --tests` — expected to fail on that file only; step 10
resolves it. Then `cargo test -p liquers-core --lib`.

**Rollback:** `git checkout liquers-core/src/context.rs`

**Agent:** **sonnet** · rust-best-practices · knowledge: `Context` accessors, both choke points,
`CONTEXT-APPLY-BARE-KEY-ILL-DEFINED` (the rejection is on operand form, not state consumption —
do not reintroduce the rejected objection).

---

### Step 9: Chain a dependency's error into the parent

**File:** `liquers-core/src/assets.rs` (`:4446`)

Replace the from-scratch construction with one that preserves the cause:

```rust
// Before: Error::general_error(format!("Dependency asset {} did not produce a value (status {:?})", ...))
// After:  compose the cause's message, and carry its query/position/command_key when the
//         parent has none. No new Error field — see the Phase 4 overview.
let cause = dependency.error().await;   // the dependency's stored Error, if any
let e = match cause {
    Some(cause) => Error::from_error(ErrorType::General, &cause)
        .with_query_opt(cause.query.as_deref())
        .with_position(&cause.position),
    None => Error::general_error(format!(
        "Dependency asset {} did not produce a value (status {:?})", dependency.id(), status)),
};
```

**Validation:** `cargo test -p liquers-core --test plan_cwd_freeze dependency_error_chains_cause`

**Rollback:** `git checkout liquers-core/src/assets.rs`

**Agent:** sonnet · rust-best-practices · knowledge: `Error` shape (`error.rs:41-52`), `Display`
impl (`:398`), `AssetRef` error accessors. *Rationale:* must not add a recursive field; must not use
`Error::new`.

---

### Step 10: Migrate the six call sites

**Files:** `liquers-core/tests/recipe_cwd_resolution.rs`, `liquers-core/src/context.rs`,
`liquers-core/src/interpreter.rs`

| Site | Was | Becomes |
|---|---|---|
| `recipe_cwd_resolution.rs:29` `cwd` | `context.get_cwd_key()` | `dir: Key = query "-R-key/."` |
| `recipe_cwd_resolution.rs:36` `append_cwd` | `context.get_cwd_key()` | same |
| `recipe_cwd_resolution.rs:51` `via_evaluate` | `context.evaluate("-R/./hello.txt")` | builds an absolute query from `dir` |
| `recipe_cwd_resolution.rs:62` `via_state` | `context.get_dependency_state("-R/./hello.txt")` | same |
| `recipe_cwd_resolution.rs:75` `via_apply` | `context.apply("-R-stored/./identity")` | same |
| `context.rs:1602` | `apply("-R-key/./from-apply")` | same |

`context_boundary_commands_use_active_cwd` keeps its name and assertions — the capability is
re-expressed, not removed. Separately, add `payload: required` to the `word` command in
`interpreter::tests::test_evaluate_immediately` (pitfall 2; verified sufficient).

The inline `cwd`/`append_cwd` in `interpreter::tests::resolver_scopes_nested_links` need **no**
change: they are inside the crate, so `pub(crate)` reaches them, and their relative parts are query
operands that freeze resolves.

**Validation:** `cargo test -p liquers-core --tests` — all green, no test deleted.

**Rollback:** `git checkout` the three files.

**Agent:** sonnet · rust-best-practices, liquers-unittest · knowledge: Phase 3 §Migration,
`register_command!` default-query syntax (`= query "..."`).

---

### Step 11: The equivalence suite and rejection tests

**File:** `liquers-core/tests/plan_cwd_freeze.rs` (new)

Implements Phase 3's tables: 12 equivalence shapes (E1-E12, E8 asserting **inequivalence**), 5
rejection tests, 6 corner cases. Table-driven; each shape evaluated cut and expanded, comparing
value, `is_volatile`, `payload_required` and surfaced error.

Environments per the `liquers-unittest` table: `ImmediateEnvironment<Value>` for plan/CWD,
`SimpleEnvironment<Value>` for recipes and assets, `SimpleEnvironmentWithPayload<Value, String>` for
E7/E8. `type CommandEnvironment` alias before any `register_command!`.

**Validation:** `cargo test -p liquers-core --test plan_cwd_freeze`

**Rollback:** `rm liquers-core/tests/plan_cwd_freeze.rs`

**Agent:** **sonnet** · rust-best-practices, liquers-unittest · knowledge: Phase 3 in full,
`recipe_cwd_resolution.rs` as the house style for CWD integration tests.

---

### Step 12: Unit tests

**Files:** `liquers-core/src/plan.rs` (`mod tests`), `liquers-core/src/query.rs` (`mod tests`)

The 9 `freeze_*` tests and 2 `cursor_consumed_*` tests from Phase 3, including
`runtime_cursor_is_idle_after_freeze` — the Phase 2 decision-5 migration assertion.

**Validation:** `cargo test -p liquers-core --lib`

**Rollback:** `git checkout` both files.

**Agent:** haiku · liquers-unittest · knowledge: Phase 3 unit-test tables, existing `mod tests`
conventions in each file.

---

### Step 13: Remove the dead marker

**File:** `liquers-core/src/recipes.rs:217`

Delete the commented `disable_expand_predecessors()` line and its TODO. The option no longer exists.

**Validation:** `cargo test -p liquers-core --lib`

**Rollback:** `git checkout liquers-core/src/recipes.rs`

**Agent:** haiku · — · knowledge: none beyond the line itself.

---

## Testing Plan

### Unit tests
**After steps 4, 6, 12.** `cargo test -p liquers-core --lib`
Expected: 548 existing + 11 new pass; no regressions.

### Integration tests
**After steps 10, 11.**
```bash
cargo test -p liquers-core --test plan_cwd_freeze
cargo test -p liquers-core --test recipe_cwd_resolution
```
Expected: equivalence suite green with E8 asserting the documented divergence; no test deleted.

### Cross-crate
**After step 7** (freeze activation) and again at the end.
```bash
cargo test -p liquers-lib --lib --tests   # the CLAUDE.md default loop
```
Expected: `liquers-lib`'s `apply_recipe` inherits freeze through `finalize_plan` with no change.

### Manual validation
```bash
cargo run -p liquers-core --features cli --bin liquers-validate -- --detail summary -- \
  '-R/./input.csv/-/analyze' '-R/data/big.csv/-/analyze' '-R-key/.'
# Expected: 3 ok, 0 warnings, 0 errors — planning still works with the builder change.

cargo test -p liquers-lib --test registry_export
# Expected: pass. No register_command! signature changed, so the registry is unaffected.
```

**Success criteria:** the eleven HEAD failures are gone; baseline 548 still passes; the equivalence
suite is green; no test was deleted to achieve it.

## Task Splitting (Agent Assignments)

| Step | Model | Skills | Rationale |
|---|---|---|---|
| 1 | haiku | rust-best-practices | Mirrors the existing `defaulted_to_root` pattern |
| 2 | haiku | rust-best-practices | Struct fields with serde defaults |
| 3 | sonnet | rust-best-practices | Exhaustive `ParameterValue` match; clone-vs-share matters |
| 4 | **sonnet** | rust-best-practices | The design itself; scope rules are the failure mode |
| 5 | **sonnet** | rust-best-practices | Builder surgery plus AST construction |
| 6 | sonnet | rust-best-practices | Step-list rewrite preserving computed flags |
| 7 | **sonnet** | rust-best-practices | Activation; warning-once contract is fragile |
| 8 | **sonnet** | rust-best-practices | Breaking API change with a rejected-issue precedent to respect |
| 9 | sonnet | rust-best-practices | Error composition without a new field |
| 10 | sonnet | rust-best-practices, liquers-unittest | Behaviour-preserving rewrite of six sites |
| 11 | **sonnet** | rust-best-practices, liquers-unittest | Largest test artifact; the primary deliverable |
| 12 | haiku | liquers-unittest | Follows Phase 3 tables |
| 13 | haiku | — | Delete one comment |

No step needs opus: the architectural reasoning is settled in Phases 1-3, and each step is local to
one or two files.

## Rollback Plan

### Per-step
Each step lists its own `git checkout`. Steps 1-6 are inert, so rolling any of them back cannot
change behaviour. Step 7 is the single switch: reverting `interpreter.rs` returns the whole feature
to dormant while leaving the new code in place.

### Checkpoints
- **After step 6** — new code present, nothing calls it, suite green. Safe to pause or ship as a
  no-op refactor.
- **After step 7** — freeze live, `Context` unchanged. Safe to pause; the boundary cut is still off
  by default.
- **After step 10** — breaking change complete and migrated. The natural review point.

### Full rollback
```bash
git checkout main -- liquers-core/src liquers-core/tests
rm -f liquers-core/tests/plan_cwd_freeze.rs
```
New files: `liquers-core/tests/plan_cwd_freeze.rs`.
Modified: `plan.rs`, `query.rs`, `interpreter.rs`, `context.rs`, `assets.rs`, `recipes.rs`,
`tests/recipe_cwd_resolution.rs`. No `Cargo.toml` change — this design adds no dependency.

### Partial completion
Stop at any checkpoint above. Do not stop between steps 8 and 10: `liquers-core/tests` does not
compile in that window.

## Out of Scope

Deferred deliberately, to be filed in Phase 5:
- Removing the now-idle runtime cursors (Phase 2 decision 5 lands the assertion; removal follows on
  evidence).
- Non-keyed `Step::Evaluate` pre-scheduling (Phase 2 decision 6).
- Flipping the boundary default — `CORE-PLAN-POLICY-AND-DEFAULTS` owns it, and Phase 2 recorded that
  the memory-versus-recomputation trade is per-query, which argues against one global default.
- Documentation: Phase 5, per the approved documentation architecture.

## Phase 5 Entry Criteria

- [ ] Implementation is finished and validated — all 13 steps done, the eleven HEAD failures gone,
      baseline 548 still passing, equivalence suite green with no test deleted
- [ ] All user comments are answered or incorporated — three remain open at the Phase 4 gate: the
      example-type choice (runnable tests), the cost of the rejection decision (six migrated sites),
      and whether any downstream command passes a relative query that this workspace cannot see
- [ ] All review comments are answered or incorporated
- [ ] Documentation can be verified against the implemented and tested behavior — the three
      reference updates in the Phase 2 documentation architecture, each checked against the code as
      merged rather than against this plan
- [ ] Phase 5 documentation will be included in the implementation PR when practical
- [ ] Documentation and learning evidence has been collected — the Phase 3 Learning Log is already
      populated with five corrected assumptions; add anything found during steps 1-13, especially
      whether `runtime_cursor_is_idle_after_freeze` actually holds

Issues to file in Phase 5 for the deferred work listed under **Out of Scope**: removal of the idle
runtime cursors, non-keyed `Step::Evaluate` pre-scheduling, and the per-query nature of the
boundary-default trade (as a note on `CORE-PLAN-POLICY-AND-DEFAULTS`, which already exists).

## Review Record

Host does not permit spawning agents, so the four conformity passes and the final holistic pass ran
sequentially, per this skill's host-compatibility clause.

**Reviewer 1 — Phase 1 conformity.** Steps map onto the Phase 1 scope list; nothing added. Scope
item 3 (recipe overrides never enter a query) needs no step — it holds by construction, which step 6
relies on rather than enforces.

**Reviewer 2 — Phase 2 conformity.** Signatures match. Two Phase 2 statements corrected in the
overview above (accessor users, `Error` chaining mechanism); both were Phase 2 errors, not step
errors.

**Reviewer 3 — Phase 3 conformity.** All Phase 3 tests are assigned: unit tests to step 12,
equivalence/rejection/corner to step 11, migration to step 10. Phase 3's migration list was
incomplete at 4 sites; step 10 covers 6.

**Reviewer 4 — codebase compatibility.** Line references re-verified post-rebase. `liquers-lib`
needs no change (its `apply_recipe` calls `finalize_plan`); `liquers-py`'s is `todo!()`. No
`register_command!` signature changes, so `specs/command_registry.yaml` does not need regenerating —
step 13's manual check confirms via `registry_export`.

**Final holistic pass.** One ordering hazard found and encoded: steps 8-10 leave
`liquers-core/tests` uncompilable, so they must land together; the rollback plan now says so. One
risk accepted: step 4 is large and has no smaller safe decomposition — the traversal must handle
every `Step` variant at once or the match is non-exhaustive and will not compile.
