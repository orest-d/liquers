# Phase 4: Implementation Plan - Payload Inheritance in Nested Evaluation

## Overview

**Feature:** Payload inheritance in nested evaluation (resolves `ISSUES.md`:
PAYLOAD-NESTED-EVALUATION-INHERITANCE).

**Architecture:** A `PayloadRequirement` enum declared on `CommandMetadata`, propagated to
`Plan::payload_required` by `PlanBuilder` and an async `RequiresPayload` trait, driving a routing
switch in `Context::schedule_dependency_asset` that forwards the parent's payload to the existing
`AssetRef::run_immediately` path.

**Estimated complexity:** Medium-High — the individual changes are mechanical (most mirror an
existing `volatile` counterpart), but they span seven files across three crates and one step changes
observable evaluation semantics.

**Estimated time:** 8–12 hours for a developer familiar with `liquers-core`.

**Prerequisites:** Phases 1–3 approved; no open questions remain.

**Sequencing principle:** every step ends at a compiling tree. Steps 1–7 are additive and change no
behavior; **Step 8 is the behavioral cut-over**; Steps 9–13 complete and verify it.

## Implementation Steps

---

### Step 1: `PayloadRequirement` enum and `CommandMetadata` field

**File:** `liquers-core/src/command_metadata.rs`

**Action:**
- Add the `PayloadRequirement` enum with `join`, `is_required`, `is_none` (Phase 2 signatures).
- Add `CommandMetadata::payload_required` with `#[serde(default)]` +
  `skip_serializing_if = "PayloadRequirement::is_none"`.
- Update the constructors at `:828` and `:859` that set `volatile: false` explicitly.
- Add unit tests **U1** (join over all four combinations, default, predicates) and the
  `CommandMetadata` half of **U2** (absent field defaults; `None` not emitted; `Required` round-trips).

**Note:** any struct literal without `..Default::default()` will fail to compile until updated. That
is intended — the compiler enumerates every construction site.

**Validation:**
```bash
cargo check -p liquers-core
cargo test -p liquers-core --lib command_metadata
```

**Rollback:** `git revert` this commit; nothing depends on it yet.

**Agent:** haiku · skills: rust-best-practices, liquers-unittest · knowledge: `phase2-architecture.md`
(Data Structures), `command_metadata.rs:760-870`.
*Rationale:* self-contained enum plus a field, fully specified; no judgment required.

---

### Step 2: Diagnostic surface in `metadata.rs`

**File:** `liquers-core/src/metadata.rs`

**Action:**
- `AssetInfo::payload_required` with `#[serde(default)]`, matching the legacy-support treatment of
  `is_volatile` at `:654-656`.
- `MetadataRecord::payload_required` field + `payload_required()` / `set_payload_required()`,
  mirroring `:1246-1248` and `:1261-1264`.
- `Metadata::payload_required()` mirroring `:2085-2101`, including legacy-JSON extraction defaulting
  to `None`.
- Carry the field through `to_asset_info` (`:992`), `From<AssetInfo>` (`:746`), and the legacy-JSON
  load (`:1348-1350`).
- Unit tests **U6**, including the assertion that `status == Status::Volatile` still reports
  `payload_required() == None`.

**Critical detail:** unlike `is_volatile()`, there is **no `Status` disjunction** —
`payload_required()` is a plain field read. Do not add a `Status::PayloadRequired` variant.

**Validation:**
```bash
cargo check -p liquers-core
cargo test -p liquers-core --lib metadata
```

**Agent:** haiku · skills: rust-best-practices, liquers-unittest · knowledge:
`phase2-architecture.md` (Diagnostic surface), `metadata.rs:640-760, 810-1000, 1240-1360, 2080-2250`.
*Rationale:* five mirrored sites, each with an exact counterpart to copy.

---

### Step 3: `Plan`, `PlanBuilder`, splitting, and metadata wiring

**File:** `liquers-core/src/plan.rs`

**Action:**
- `Plan::payload_required` with `#[serde(default)]` (deliberately unlike `is_volatile` — required for
  stored plans). Update `Plan::new()` at `:1390-1398`.
- `PlanBuilder::payload_required` field plus `mark_payload_required`, `action_payload_requirement`,
  `check_parameter_for_payload_links` — each mirroring its `volatile` counterpart at `:923-938`
  and `:975-995`.
- `build()` copies builder → plan (`:1009-1010`).
- **Plan splitting** (`:1599`, `:1607`) copies `payload_required` to *both* halves.
- `to_metadata_record` (`:1456`) and `update_metadata_record` (`:1496`) carry the field.
- Unit tests **U3**, **U4**, **U7**, and the `Plan` half of **U2**.

**Highest-risk omission in the whole plan:** the two plan-splitting lines. A miss here silently
drops the requirement on one half and is invisible until a split plan is evaluated. **U4 exists
specifically to catch this** — write it before the production change.

**`init_steps` reasoning:** `mark_payload_required` must keep `mark_volatile`'s transition guard
(`if !required { … init_info(reason) }`) so a plan emits exactly one message, and none at all when no
command requires a payload. Message names the trigger.

**Validation:**
```bash
cargo check -p liquers-core
cargo test -p liquers-core --lib plan
```

**Agent:** sonnet · skills: rust-best-practices, liquers-unittest · knowledge:
`phase2-architecture.md`, `plan.rs:880-1010, 1350-1500, 1590-1615`.
*Rationale:* the splitting and metadata-wiring sites need judgment about where the field must be
threaded; a mechanical pass is what would miss them.

---

### Step 4: `payload: required` in `register_command!`

**File:** `liquers-macro/src/registration.rs`

**Action:**
- Parse a `"payload"` arm at `:773`, taking a **bare ident** (`required` / `none`), not a `LitBool`.
  Unknown idents must produce a `syn::Error`.
- Add `CommandSignatureStatement::PayloadRequired(bool)` and the builder field/plumbing mirroring
  `volatile` at `:1043, 1646, 1663, 1687`.
- Codegen mirroring `:1225-1229` emitting **both** lines:
  ```rust
  cm.payload_required = liquers_core::command_metadata::PayloadRequirement::Required;
  cm.volatile = true;
  ```
- Unit tests **U5**.

**Do not** add a compile-time dependency on `liquers-core`; emit the fully-qualified path in the
generated tokens, as `volatile` does.

**Validation:**
```bash
cargo check -p liquers-macro && cargo test -p liquers-core --lib
```

**Agent:** sonnet · skills: rust-best-practices · knowledge: `phase2-architecture.md` (macro section),
`registration.rs:760-790, 1030-1290, 1630-1700`.
*Rationale:* proc-macro parsing and codegen; errors here surface as confusing downstream compile
failures rather than local ones.

---

### Step 5: `RequiresPayload` trait

**File:** `liquers-core/src/interpreter.rs`

**Action:** add `pub(crate) trait RequiresPayload<E: Environment>` and six impls mirroring
`IsVolatile` (`:402-510`): `ParameterValue` (with `Box::pin` for recursion, as at `:409`),
`ResolvedParameterValues`, `Plan` (cached field), `Recipe`, `Query`, `Step`.

**`Step` match — the deliberate divergence from `IsVolatile`:** all four keyed arms
(`GetAsset`, `GetAssetBinary`, `GetAssetMetadata`, `GetAssetRecipe`) return
`PayloadRequirement::None` **unconditionally**. Keys are a payload boundary; do **not** consult the
asset manager as `IsVolatile` does at `:477-482`. No default match arm.

**Validation:**
```bash
cargo check -p liquers-core
```

**Agent:** sonnet · skills: rust-best-practices · knowledge: `phase2-architecture.md`
(RequiresPayload), `interpreter.rs:396-510`.
*Rationale:* the keyed-boundary divergence is the single most likely thing to be "corrected" back
into a manager call by someone pattern-matching on `IsVolatile`.

---

### Step 6: `ImmediateEnvironmentWithPayload<V, P>`

**File:** `liquers-core/src/context.rs`

**Action:** add the environment mirroring `SimpleEnvironmentWithPayload` (`:956-1040`) with
`type AssetManager = ImmediateAssetManager<Self>` and no spawning in `init_with_envref` (follow
`ImmediateEnvironment` at `:846`).

Without this, integration test **I8** cannot be written — no existing environment pairs a payload
with the inline manager, and `liquers-lib::DefaultEnvironment` selects its manager at compile time.

**Validation:**
```bash
cargo check -p liquers-core
cargo check -p liquers-core --target wasm32-unknown-unknown
```

**Agent:** haiku · skills: rust-best-practices · knowledge: `context.rs:840-1045`.
*Rationale:* direct structural copy of two existing environments.

---

### Step 7: `AssetManager::get_dependency_asset_with_payload`

**File:** `liquers-core/src/assets.rs`

**Action:**
- Add the trait method with a **default implementation** that ignores the payload and delegates to
  `get_dependency_asset` (`:2683-2690`), so no implementor breaks.
- Override in `DefaultAssetManager` (`:3864`): reuse the existing resolution — stale-terminal
  eviction, `poll_state` short-circuit, store fast-track — and replace only the final queue
  submission with `asset.run_immediately(payload)`.
- Override in `ImmediateAssetManager` (**new** — it inherits the default today): resolve as
  `get_asset` does (`:5187-5230`), then call `run_immediately_inline(payload)` instead of
  `run_inline()`. `apply_immediately` at `:5240-5250` is the shape to copy.

Still additive: nothing calls the new method until Step 8.

**Validation:**
```bash
cargo check -p liquers-core
cargo test -p liquers-lib --lib --tests
# Expected: unchanged behavior, all existing tests pass
```

**Agent:** sonnet · skills: rust-best-practices · knowledge: `phase2-architecture.md` (AssetManager),
`assets.rs:2630-2714, 3860-3930, 5186-5260, 1660-1730`.
*Rationale:* the two overrides differ structurally and must each preserve their manager's existing
resolution semantics.

---

### Step 8: Routing switch, cycle guard, and `apply` — **the behavioral cut-over**

**File:** `liquers-core/src/context.rs`

**Action:**
- Add the active-query set to `Context`, shared across clones like `pending_dependencies` (`:328`).
- In `schedule_dependency_asset` (`:369-425`), compute
  `query.requires_payload(envref).await?` and switch:
  - `None` → today's path, **byte-for-byte unchanged**.
  - `Required` → error if `self.payload.is_none()`; push/check the active-query set
    (`Error::dependency_cycle` on re-entry); **skip** the edge registration (`:398-409`) and
    `add_dependent_asset` (`:415-420`); **keep** `self.add_dependency` (`:421-422`); call
    `get_dependency_asset_with_payload`.
- `Context::apply` (`:471-474`) switches to `apply_immediately` with the inherited payload when
  required.
- Update the module rustdoc at `:76-80` and the per-method docs at `:450`, `:459-460`, `:469-470`,
  which currently state that nested evaluation does not inherit.

**This is the only step that changes observable behavior.** Commit it alone.

**Validation:**
```bash
cargo test -p liquers-lib --lib --tests
cargo test -p liquers-core --test injection
# Expected: test_payload_not_inherited_in_nested_evaluation NOW FAILS.
# That failure is the success signal; Step 12 replaces it.
```

**Rollback:** revert this commit alone — Steps 1–7 are inert without it.

**Agent:** sonnet · skills: rust-best-practices · knowledge: all phase docs,
`context.rs:306-475`, `dependencies.rs:405-500`, `assets.rs:1840-1860`.
*Rationale:* the highest-judgment step; the skip/keep split across the four registration actions is
the core of the design.

---

### Step 9: Keyed-recipe rejection

**File:** `liquers-core/src/plan.rs` (and `recipes.rs` if recipe→plan validation lives there)

**Action:** when a plan is built **for a key** and comes out `Required`, return
`Error::general_error` naming the key. Keys are global; payloads are per-evaluation.

**Validation:**
```bash
cargo test -p liquers-lib --lib --tests
```

**Agent:** sonnet · skills: rust-best-practices · knowledge: `phase2-architecture.md` (keyed
boundary), `plan.rs`, `recipes.rs:340-380`.
*Rationale:* requires locating the correct choke point so keyed and non-keyed plan building are not
conflated.

---

### Step 10: `liquers-py` getter parity

**File:** `liquers-py/src/command_metadata.rs`

**Action:** add a `payload_required` getter returning a `String`, mirroring `volatile()` at `:362`.

**Validation:** `cargo check -p liquers-py`

**Agent:** haiku · skills: rust-best-practices · knowledge: `liquers-py/src/command_metadata.rs:350-375`.

---

### Step 11: Migration — annotate payload-using commands

**Files:** `liquers-lib/src/ui/commands.rs`, `liquers-core/tests/injection.rs`

**Action:** add `payload: required` to every command that reads a payload.

**Detection:** `grep -rn "get_payload_clone\|ExtractFromPayload" liquers-lib/src liquers-core/tests`
then read each hit. Neither signal is compiler-visible, so this is a manual pass — an omission is
silent (works at top level, fails only nested), which is exactly the hazard Phase 1 D2 accepted.

**Validation:**
```bash
cargo test -p liquers-lib --lib --tests
```

**Agent:** sonnet · skills: rust-best-practices · knowledge: `phase3-examples.md` (Migration Audit),
`liquers-lib/src/ui/commands.rs`, `liquers-lib/src/ui/payload.rs`.
*Rationale:* judgment per call site about whether payload is genuinely read; a mechanical pass
over-annotates and forces unnecessary volatility.

---

### Step 12: Integration tests

**File:** `liquers-core/tests/injection.rs`

**Action:** implement **I1–I8** and corner cases **C1**, **C3**, **C4** from Phase 3.
`test_payload_not_inherited_in_nested_evaluation` is **replaced** by
`test_payload_inherited_in_nested_evaluation` — assertion flips from `"parent:laura|child:None"` to
`"parent:laura|child:window:777"`.

Add the regression guard `test_unannotated_payload_command_is_payload_free_when_nested`, documenting
the migration hazard as designed behavior.

**Validation:**
```bash
cargo test -p liquers-core --test injection
cargo test -p liquers-lib --lib --tests
```

**Agent:** sonnet · skills: liquers-unittest, rust-best-practices · knowledge: `phase3-examples.md`,
`tests/injection.rs`, `phase2-architecture.md`.
*Rationale:* I5 (keyed rejection) needs a `MemoryStore` + `RecipeProvider` setup, and I7 needs
dependency-manager introspection — neither is boilerplate.

---

### Step 13: Documentation

**Files:** `specs/PAYLOAD_GUIDE.md`, `specs/PROJECT_OVERVIEW.md`, `specs/ISSUES.md`

**Action:**
- `PAYLOAD_GUIDE.md`: the "Inheritance" bullet (`:65-67`) becomes true; add `payload: required` to
  every example; correct the availability claims at `:48-51` and the summary table at `:1124`;
  document the keyed boundary and the `Optional` deferral.
- `PROJECT_OVERVIEW.md`: lines `271` and `390`.
- `ISSUES.md`: close PAYLOAD-NESTED-EVALUATION-INHERITANCE; record the keyed limitation and the
  deferred `Optional` state as follow-ups.

**Validation:** manual review; no code impact.

**Agent:** haiku · skills: none · knowledge: all phase docs, the three target files.

---

## Agent Assignment Summary

| Step | Model | Skills | Rationale |
|---|---|---|---|
| 1 | haiku | rust-best-practices, liquers-unittest | Self-contained enum + field, fully specified |
| 2 | haiku | rust-best-practices, liquers-unittest | Five mirrored sites, each with an exact counterpart |
| 3 | sonnet | rust-best-practices, liquers-unittest | Plan splitting + metadata wiring need judgment about threading |
| 4 | sonnet | rust-best-practices | Proc-macro parse/codegen; errors surface downstream |
| 5 | sonnet | rust-best-practices | Keyed-boundary divergence from `IsVolatile` is easy to "correct" wrongly |
| 6 | haiku | rust-best-practices | Structural copy of two existing environments |
| 7 | sonnet | rust-best-practices | Two structurally different manager overrides |
| 8 | sonnet | rust-best-practices | **Behavioral cut-over**; skip/keep split is the core of the design |
| 9 | sonnet | rust-best-practices | Requires locating the right validation choke point |
| 10 | haiku | rust-best-practices | One getter mirroring an adjacent one |
| 11 | sonnet | rust-best-practices | Per-site judgment on whether payload is genuinely read |
| 12 | sonnet | liquers-unittest, rust-best-practices | I5 and I7 need store/DM setup, not boilerplate |
| 13 | haiku | — | Documentation edits against a specified list |

Steps 1, 2, 4, 6, 10 are independent of one another and may run in parallel; 3 depends on 1;
5 depends on 3; 7 depends on 1; **8 depends on 3, 5, 7**; 9 depends on 5; 11–13 depend on 8.

## Testing Plan

| Stage | Command | Gate |
|---|---|---|
| Per step 1–7 | `cargo check -p liquers-core` | compiles; behavior unchanged |
| After 1, 2, 3, 4 | `cargo test -p liquers-core --lib <module>` | unit suites U1–U7 pass |
| After 7 | `cargo test -p liquers-lib --lib --tests` | **all existing tests still pass** — proves additivity |
| After 8 | same | only `test_payload_not_inherited_in_nested_evaluation` fails, by design |
| After 12 | `cargo test -p liquers-core --test injection` | I1–I8 pass |
| Final | `cargo test -p liquers-lib --lib --tests` + `cargo check -p liquers-core --target wasm32-unknown-unknown` | green |

**Disk discipline (CLAUDE.md):** use `cargo test -p liquers-lib --lib --tests`, never
`cargo test --workspace`. Run `cargo clean` if a profile setting changes. Browser tests are out of
scope for this feature.

## Rollback Plan

One commit per step. Steps 1–7 and 9–13 are independently revertible.

**Step 8 is the only irreversible-in-effect step**, and reverting it alone restores current behavior
while leaving the additive machinery in place. Structure the branch so Step 8 is a single commit
touching only `context.rs`.

## Risk Register

| Risk | Likelihood | Mitigation |
|---|---|---|
| Plan splitting misses `payload_required` | **High** — two easily-missed lines | U4 written before the production change (Step 3) |
| Keyed `Step` arms "corrected" to consult the manager | Medium — `IsVolatile` does exactly that | Called out in Step 5; `test_keyed_step_does_not_propagate_payload` |
| A payload command left un-annotated | **High** — not compiler-visible | Step 11 grep pass + the regression guard in Step 12 |
| Interpreter pre-pass orphans a payload query | Medium | `interpreter.rs:85-86` already excludes volatile queries; `payload ⟹ volatile` means it is covered — **verify, do not assume** |
| Serialization breaks stored metadata/plans | Low | `#[serde(default)]` everywhere; U2 and U6 assert legacy loads |
| Inline path untested | Medium | Step 6 unblocks I8 |

## Post-Implementation Verification

Against Phase 1's original verification list:

- [ ] Payload across multiple actions in one immediate evaluation — existing tests
- [ ] Direct payload and extracted-newtype injection — existing tests
- [ ] Nested `Context::evaluate` — I1
- [ ] Nested `Context::get_dependency_state` — I2
- [ ] `Context::apply` — I2
- [ ] Queued **and** inline asset managers — I1 and I8
- [ ] Chosen caching / asset-sharing behavior — I3, I7, C1
- [ ] Docs and the non-inheritance test updated together — Steps 12 and 13
