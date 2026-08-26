---
id: FOREIGN-VALUE-TYPE-REGISTRATION-PHASE4
kind: design
title: "Phase 4: Implementation plan — foreign and Python value types in the type registry"
status: in_review
phase: implementation
area: [core/value, lib/value, web, py]
created: 2026-08-26
---
# Phase 4: Implementation Plan — Foreign Value Type Registration

## Overview

Nine steps across four crates, each its own commit and each independently validatable. Steps 1–6 are
the native path and run in the ordinary `cargo test -p liquers-lib --lib --tests` loop; step 6 is
where the fix is proved end to end. Step 7 is `liquers-py`, which needs a repair before it can be
described. Step 8 is `liquers-web` and is deliberately last, because it is the only step needing a
`wasm32` toolchain and a `cargo clean` — one target install, one clean, one build session.

**The order is load-bearing in two places.** Step 5 (`DefaultEnvironment`) must precede step 6 and
step 8, because both construct one. Step 3 must precede step 4, because step 4 routes to what step 3
adds. Everything else could be reordered.

## Implementation Steps

### Step 1: Environment constructors that accept a registry

**File:** `liquers-core/src/context.rs`

**Action:**
- Add `new_with_type_registry(type_registry: TypeRegistry) -> Self` to `SimpleEnvironment`,
  `ImmediateEnvironment`, `SimpleEnvironmentWithPayload`, `ImmediateEnvironmentWithPayload`.
- Rewrite each `new()` to delegate, so the field initialisation lives in one place per type.
- Add `fvt2.1` and `fvt2.2` to the file's `mod tests`.

```rust
impl<V: ValueInterface> SimpleEnvironment<V> {
    pub fn new() -> Self {
        Self::new_with_type_registry(crate::type_system::TypeRegistry::from_value_type::<V>())
    }

    /// Creates an environment with a caller-supplied type registry.
    ///
    /// For an integration that adds a type `V` cannot describe statically — a foreign language
    /// handle whose identifier belongs to the integration crate. **Extend**
    /// `TypeRegistry::from_value_type::<V>()`; starting from `TypeRegistry::new()` loses every
    /// type the build already had, including the `error` pseudo-type.
    ///
    /// The registry is not written after this point: it is shared without a lock, which is what
    /// `Environment::get_type_registry` returning `&TypeRegistry` depends on.
    pub fn new_with_type_registry(type_registry: crate::type_system::TypeRegistry) -> Self { … }
}
```

**Validation:** `cargo test -p liquers-core --lib context` — expected: `fvt2.1`, `fvt2.2` pass, and
every existing environment test still passes because `new()` is behaviourally unchanged.

**Rollback:** `git checkout liquers-core/src/context.rs`

**Agent:** sonnet · rust-best-practices · knowledge: Phase 2 "Function Signatures", `context.rs`
lines 960–1300 and 1700–1800. *Rationale:* four near-identical edits where the risk is a
copy-paste slip between the payload and non-payload variants, and the `#[cfg(feature = "async_store")]`
fields differ between them.

---

### Step 2: State the one-identifier-per-variant rule

**File:** `liquers-core/src/value.rs`, `liquers-core/src/type_system.rs`

**Action:**
- Replace the `identifier` doc comment (`value.rs:229–231`). The current text, "Several types can be
  linked to the same identifier", is contradicted by `type_descriptions_match_identifier` in the
  same file (`:1155`, "one description per variant, no more and no less").
- Add `fvt1.1` and `fvt1.2` to `type_system.rs`'s `mod tests` — characterization tests for the
  extend-a-base flow, so the documented workflow has a test even though the API is not new.

```rust
/// The type identifier of this value.
///
/// **Exactly one identifier per value variant.** The correspondence is one-to-one and
/// `type_descriptions_match_identifier` enforces it. Detail that varies per instance belongs in
/// [`ValueInterface::type_name`], which is informational and is never dispatched on.
/// The identifier must be cross-platform.
fn identifier(&self) -> Cow<'static, str>;
```

**Validation:** `cargo test -p liquers-core --lib type_system`

**Rollback:** `git checkout liquers-core/src/value.rs liquers-core/src/type_system.rs`

**Agent:** haiku · — · knowledge: Phase 1 "The rule is nowhere stated", `VALUE_TYPE_SYSTEM.md`
"Two axes". *Rationale:* a doc comment and two tests against an unchanged API.

---

### Step 3: `ForeignValue::type_info`

**File:** `liquers-lib/src/value/foreign.rs`

**Action:**
- Add `fn type_info(&self) -> TypeInfo` with a default body derived from the existing `&self`
  methods, declaring **no** data formats.
- Add `fvt3.1` and `fvt3.2` in a new `#[cfg(test)] mod tests` with a mock implementation.

**The two constraints that shape this:** it must take `&self` (so it stays in the vtable and
`Arc<dyn ForeignValue>` keeps working), and it must have a default body (so no implementor breaks —
`CLAUDE.md`'s "add new methods with default implementations when possible"). An associated
`fn type_info() -> TypeInfo where Self: Sized` satisfies object safety too but can carry no useful
default and is unreachable through the trait object; Phase 1 records why, so it is not retried here.

**Validation:** `cargo test -p liquers-lib --lib value::foreign`

**Rollback:** `git checkout liquers-lib/src/value/foreign.rs`

**Agent:** haiku · rust-best-practices · knowledge: Phase 2 "Trait Implementations", `foreign.rs`.
*Rationale:* one defaulted method against a fully specified signature.

---

### Step 4: Route `type_info` to the value that owns it

**Files:** `liquers-lib/src/value/extended.rs`, `liquers-lib/src/value/mod.rs`

**Action:**
- `ValueExtension` gains `fn type_info(&self) -> TypeInfo` with the same derivation
  `ValueInterface::type_info` uses (search `Self::type_descriptions()`, else build from defaults).
- `ExtValue` overrides it, delegating **only** the `Foreign` arm to `value.type_info()`.
- `CombinedValue` overrides `ValueInterface::type_info`, delegating to base or extension.
- Add `fvt4.1` (an `Image` still resolves to its declared description — the routing changes nothing
  for existing types) and `fvt4.2`.

**Every arm explicit.** `ExtValue`'s match needs `#[cfg(feature = "polars")]` and
`#[cfg(feature = "egui")]` arms; a missing one compiles under default features and breaks the
minimal or wasm build, which is exactly what step 4's matrix check is for.

**Validation:**
```bash
cargo test -p liquers-lib --lib value
bash scripts/check-build-matrix.sh     # 11 configurations, library and test targets, plus wasm32
```

**Rollback:** `git checkout liquers-lib/src/value/extended.rs liquers-lib/src/value/mod.rs`

**Agent:** sonnet · rust-best-practices · knowledge: Phase 2 "Trait Implementations" and the
"why this chain is needed" note, `extended.rs`, `mod.rs`, `value.rs:198–216`. *Rationale:* three
coordinated trait edits where getting the derivation subtly wrong would silently change `type_info`
for every existing value type, not only for foreign ones.

---

### Step 5: `DefaultEnvironment::new_with_type_registry`

**File:** `liquers-lib/src/environment.rs`

**Action:** the same pair as step 1, on `DefaultEnvironment<V, P>`; `new()` delegates.

**Validation:** `cargo check -p liquers-lib`

**Rollback:** `git checkout liquers-lib/src/environment.rs`

**Agent:** haiku · rust-best-practices · knowledge: step 1's result, `environment.rs:30–70`.
*Rationale:* mechanical repeat of step 1 with the `PhantomData<P>` field.

---

### Step 6: The integration test — where the fix is proved

**File:** `liquers-lib/tests/foreign_value_registration.rs` (new)

**Action:** `fvt7.1`–`fvt7.5` with the `MockForeign` implementation from Phase 3 Scenario 2, using
`mock.Value` as its identifier — never `js.Value`, which another crate owns.

| Test | Before this step |
|---|---|
| `fvt7.1` an unregistered foreign value is refused | passes — records the hard-refusal decision |
| `fvt7.2` a registered foreign value can be stored | **would not compile** before step 1/5 |
| `fvt7.3` it persists as metadata only | **would not compile**; verifies a claim inherited unverified from the issue |
| `fvt7.4` the refusal names the identifier | passes |
| `fvt7.5` an empty base registry loses `error` | **would not compile** |

**Validation:** `cargo test -p liquers-lib --test foreign_value_registration`

**Rollback:** `git rm liquers-lib/tests/foreign_value_registration.rs`

**Agent:** sonnet · rust-best-practices, liquers-unittest · knowledge: Phase 3 test plan,
`liquers-core/tests/type_consistency.rs` (the closest existing pattern),
`liquers-lib/tests/value_type_system.rs`. *Rationale:* `fvt7.3` has no precedent in the codebase —
what "metadata-only persistence" looks like on read has to be discovered, not transcribed.

---

### Step 7: `liquers-py` — repair, then describe

**Files:** `liquers-py/src/lib.rs`, `liquers-py/src/value.rs`

**Action, in this order** (each half depends on the previous compiling):

1. Declare `pub mod value;` and `pub mod context;` in `lib.rs`. Leave `commands`, `store`,
   `interpreter`, `cache`, `state` undeclared — they belong to `PY-MODULES-NOT-DECLARED-IN-LIB`.
2. Repair the four compile errors: `try_into_query`'s return type; `from_asset_info`'s signature and
   its `todo!()`; the incompatible `match` arms; the four missing trait items
   (`from_command_metadata`, `try_into_bytes`, `try_into_key`, `try_into_command_metadata`).
3. Add the `AssetInfo { value: Vec<crate::metadata::AssetInfo> }` variant and its match arms.
4. Realign identifiers per Phase 2's table; add `PY_OBJECT_TYPE_IDENTIFIER = "py.Object"`.
5. Add `type_descriptions()`, one entry per variant.
6. Add `fvt6.1`–`fvt6.4`.

**The tests must be GIL-free.** Measured 2026-08-26: `cargo test -p liquers-py --lib` links and the
harness runs (`0 tests`), so ordinary tests work — but `pyo3` is built with `extension-module` and
without `auto-initialize`, so a test calling `Python::with_gil` has no interpreter to attach to.
Every `fvt6` assertion is therefore written against `type_descriptions()` and against variants that
construct without the GIL. `Value::Py`'s identifier is tied to its description by
`PY_OBJECT_TYPE_IDENTIFIER` appearing in both places rather than by a constructed sample — the same
constant-plus-inspection shape used for `js.Value`.

**Validation:**
```bash
cargo check -p liquers-py --lib      # expected: 4 errors -> 0
cargo test -p liquers-py --lib       # fvt6.1-6.4
```

**Rollback:** `git checkout liquers-py/src/lib.rs liquers-py/src/value.rs`

**Agent:** sonnet · rust-best-practices · knowledge: Phase 2 "liquers-py's ValueInterface impl" and
the identifier table, `liquers-py/src/value.rs`, `liquers-py/src/metadata.rs:527`,
`liquers-core/src/value.rs` (the mirror). *Rationale:* the largest step, and the only one restoring
a file that has never compiled — every repair is a judgment about what the original author meant.

**Risk:** PyO3 may reject `Vec<crate::metadata::AssetInfo>` in a complex-enum variant. Precedent
says it will not (`Array { value: Vec<Value> }` and `Metadata { value: MetadataRecord }` already do
the same things), but this is the one genuinely unproven assumption in the plan. If it is rejected,
stop and raise it rather than inventing a shape — it changes what `py.AssetInfo` *is*.

---

### Step 8: `liquers-web` — register `js.Value`, and green the suite

**Files:** `liquers-web/src/value.rs`, `liquers-web/src/environment.rs`,
`liquers-web/tests/value_bridge_VALUE.rs`, `liquers-web/tests/second_value_type.rs`,
`liquers-web/tests/environment_ENVIRON.rs`

**Action:**

1. **Toolchain and baseline, before editing anything:**
   ```bash
   rustup target add wasm32-unknown-unknown
   cargo clean                                  # the wasm loop needs the disk (CLAUDE.md)
   cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles
   ```
   Record which assertions actually fail. Phase 3 predicts four, from reading rather than a run;
   this is where that prediction is confirmed or corrected. **A different count is a finding, not a
   nuisance** — it means the suite drifted somewhere else too.
2. `value.rs`: `JS_VALUE_TYPE_IDENTIFIER`, `js_value_type_info()`, `JsOpaque::identifier` reading the
   constant, `JsOpaque::type_info` refining the free function with the instance `type_name`.
3. `environment.rs`: build and register inside `new_environment()`, before `WebEnvironment::new_with_type_registry`.
4. Repair the stale assertions: `second_value_type.rs:324`, `:336`, `value_bridge_VALUE.rs:156`,
   `:343` — plus any the baseline turned up.
5. Add `fvt5.1` to `value.rs`'s tests, `fvt8.1` and `fvt8.2` to `environment_ENVIRON.rs`.

**`fvt8.2` is the one worth writing carefully:** register a command *after* the first evaluation, to
force `rebuild_with`, then assert the registry still contains `js.Value`. It is the test for the
pitfall in Phase 3 §3.2 and the reason the registration lives in `new_environment()` rather than in
`REGISTERED_SPECS`.

**Validation:**
```bash
cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles
# Expected: the four stale assertions gone, fvt5.1/fvt8.1/fvt8.2 passing, suite green
```

**Rollback:** `git checkout liquers-web/`

**Agent:** sonnet · rust-best-practices · knowledge: Phase 2 "liquers-web" section, Phase 3
Scenario 1 and §3.2, `liquers-web/src/environment.rs` (the `PENDING_ENV`/rebuild machinery and its
borrow rule), `liquers-web/README.md`. *Rationale:* the rebuild lifecycle and the wasm borrow
discipline both have to be held in mind at once, and the step is the first to run against a
toolchain nobody in this environment has used yet.

---

### Step 9: Full validation sweep

**Action:** run everything, in an order that keeps the disk inside its allowance.

```bash
cargo test -p liquers-lib --lib --tests           # the native loop
cargo test -p liquers-core --lib
cargo test -p liquers-lib --test registry_export  # no command changed, so this must not move
cargo check -p liquers-py --lib && cargo test -p liquers-py --lib
bash scripts/check-build-matrix.sh                # 11 configurations
# wasm, after cargo clean, as in step 8
```

**Expected:** every suite green, `registry_export` untouched, `specs/command_registry.yaml`
unmodified in the diff — this design adds no command, so a change there means something leaked.

**Agent:** haiku · — · *Rationale:* running documented commands and reporting.

## Testing Plan

### Unit Tests

| Where | IDs | When they run |
|---|---|---|
| `liquers-core/src/type_system.rs` | fvt1.1, fvt1.2 | Step 2 |
| `liquers-core/src/context.rs` | fvt2.1, fvt2.2 | Step 1 |
| `liquers-lib/src/value/foreign.rs` | fvt3.1, fvt3.2 | Step 3 |
| `liquers-lib/src/value/extended.rs` | fvt4.1, fvt4.2 | Step 4 |
| `liquers-web/src/value.rs` | fvt5.1 | Step 8 (wasm) |
| `liquers-py/src/value.rs` | fvt6.1–fvt6.4 | Step 7 (GIL-free) |

### Integration Tests

| Where | IDs | When |
|---|---|---|
| `liquers-lib/tests/foreign_value_registration.rs` | fvt7.1–fvt7.5 | Step 6 |
| `liquers-web/tests/environment_ENVIRON.rs` | fvt8.1, fvt8.2 | Step 8 |
| `liquers-web/tests/*_VALUE.rs`, `second_value_type.rs` | VALUE04, VALUE13 repairs | Step 8 |

### Manual Validation

```bash
# The bug, gone. Before: [General] Type identifier 'mock.Value' is not registered in this build
cargo test -p liquers-lib --test foreign_value_registration -- --nocapture
```

There is nothing to demonstrate interactively: this change has no user-facing surface, and its whole
observable effect is an error that stops happening. Phase 3 records why no `examples/` demo is planned.

## Agent Assignment

| Step | Model | Skills | Rationale |
|---|---|---|---|
| 1 | sonnet | rust-best-practices | Four near-identical constructors whose fields differ by `#[cfg]` and payload |
| 2 | haiku | — | Doc comment plus two tests against an unchanged API |
| 3 | haiku | rust-best-practices | One defaulted method, fully specified |
| 4 | sonnet | rust-best-practices | Three coordinated trait edits; a wrong derivation changes `type_info` for every value type |
| 5 | haiku | rust-best-practices | Mechanical repeat of step 1 |
| 6 | sonnet | rust-best-practices, liquers-unittest | `fvt7.3` has no precedent; metadata-only persistence must be discovered |
| 7 | sonnet | rust-best-practices | Largest step; restores a file that has never compiled |
| 8 | sonnet | rust-best-practices | Rebuild lifecycle plus wasm borrow discipline, on an unexercised toolchain |
| 9 | haiku | — | Running documented commands |

**Execution note.** This session does not spawn subagents, so the steps are executed here directly.
The tier labels are kept because the skill's host-compatibility contract requires the artifact to
read the same on every host, and because they record *where the risk is* — which is useful to a
reviewer regardless of who runs the step.

## Rollback Plan

### Per-Step Rollback

Each step is one commit on `claude/foreign-value-types-fix-eq6jxy`. To undo step *N*:
`git revert <sha>` if later steps have landed, `git checkout <files>` if not. The per-step
`Rollback:` lines above give the exact paths.

### Full Feature Rollback

```bash
git checkout main
git branch -D claude/foreign-value-types-fix-eq6jxy
```

**Files to delete:** `liquers-lib/tests/foreign_value_registration.rs`,
`specs/design/foreign-value-type-registration/`, `specs/issues/PY-VALUE-TYPE-DESCRIPTIONS-MISSING.md`.

**Files to restore:** `liquers-core/src/context.rs`, `liquers-core/src/value.rs`,
`liquers-core/src/type_system.rs`, `liquers-lib/src/value/{foreign,extended,mod}.rs`,
`liquers-lib/src/environment.rs`, `liquers-py/src/{lib,value}.rs`, `liquers-web/src/{value,environment}.rs`,
`liquers-web/tests/`, and the three issue files whose status this work changed.

**Cargo.toml changes:** none. No dependency is added or removed anywhere.

### Partial Completion

The natural stopping points are after **step 6** — the reported bug is fixed and proved, with
`liquers-py` and `liquers-web` untouched — and after **step 7**. Stopping after step 6 leaves
`FOREIGN-VALUE-TYPES-NOT-REGISTERED` closable and the other two issues open at their current status;
that is a coherent, shippable state rather than a half-finished one. Stopping *mid-step-7* is not:
declaring a module and leaving it uncompilable is worse than not declaring it. Record the stopping
point in `DESIGN.md` and open an issue for the remainder, per `DOCS_STRUCTURE_GUIDE.md` §5.6 —
there is no partial design status.

## Documentation Updates

All of it is Phase 5 work, listed here so the plan is complete. Phase 2 §"Documentation Architecture"
holds the detail.

**New reference or guide documents:** none.

**Existing documents and `affects_docs`:** `specs/reference/VALUE_TYPE_SYSTEM.md` (the rule, a
runtime-registration subsection, the identifier list), `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md`
§VALUE (open problem → procedure), `specs/guides/TYPE_SYSTEM_GUIDE.md` §2 and §4, `CLAUDE.md`
"Adding a Value Type" (one sentence). Each reference and guide gets a `## History` row and a
`reviewed:` bump in the same commit (§9.2).

**Design, capability and cross-links:** `specs/README.md` — the design moves to `complete` and the
capability is linked through the reference and guide rather than this folder.

**Issues to close in Phase 5:** `FOREIGN-VALUE-TYPES-NOT-REGISTERED`,
`PY-VALUE-TYPE-DESCRIPTIONS-MISSING`, `WEB-VALUE04-BYTES-IDENTIFIER-CASE-MISMATCH`.
`PY-MODULES-NOT-DECLARED-IN-LIB` stays open with a note recording which two modules were declared.

**Phase 5 evidence capture:** the four items in Phase 3's learning log, plus the step-8 baseline
count and whatever the PyO3 variant risk turns into.

## Risk Register

| Risk | Likelihood | Detected by | Response |
|---|---|---|---|
| PyO3 rejects `Vec<AssetInfo>` in a complex-enum variant | low | Step 7, `cargo check` | Stop and raise; it changes what `py.AssetInfo` is |
| A `#[cfg]`-gated arm missed in `ExtValue::type_info` | medium | Step 4, `check-build-matrix.sh` | Add the arm; the matrix is why it is run at step 4 rather than at the end |
| The wasm baseline differs from Phase 3's predicted four | medium | Step 8 baseline | A finding: record it, fix what is stale, and correct the issue |
| Disk exhaustion during the wasm build | medium | `No space left on device` | `cargo clean`, or delete `target/debug/incremental` first (CLAUDE.md) |
| `liquers-py` `match` arms beyond the trait impl need the new variant | medium | Step 7, `cargo check` | Expected; add explicit arms rather than `_ =>` |
| `registry_export` moves | very low | Step 9 | Something leaked into command metadata; investigate before proceeding |

## Phase 5 Entry Criteria

- [ ] Steps 1–9 complete, every listed validation command green
- [ ] The wasm suite green, with the baseline count recorded
- [ ] All user comments answered or incorporated
- [ ] All review comments answered or incorporated
- [ ] Documentation verifiable against implemented and tested behaviour
- [ ] Phase 5 documentation included in this PR
- [ ] Evidence from Phase 3's learning log and the step-8 baseline collected

## Review record

Sequential passes, as in Phases 2 and 3.

**Phase 1 conformity.** Every step traces to a Phase 1 decision. Nothing was added that Phase 1 did
not sanction; the plan's only additions to Phase 3 are ordering and validation.

**Phase 2 conformity.** Signatures match. One Phase 2 statement is *tightened* here: it said five
constructors, and step 1 plus step 5 make that four plus one, in that order, because
`DefaultEnvironment` is in a different crate and must follow `liquers-core`.

**Phase 3 conformity.** All 20 tests are placed in a step. `fvt6`'s placement changed on evidence:
Phase 3 assumed ordinary unit tests in `liquers-py`, and step 7 now requires them to be GIL-free,
which was established by running the harness rather than assumed.

**Codebase compatibility.** Four findings, all folded in: the `liquers-py` test harness runs but has
no interpreter (measured); `cargo clean` must precede the wasm loop or the allowance is exceeded;
`check-build-matrix.sh` belongs at step 4, where the gated `match` is written, not at the end; and
step 7's sub-steps must be ordered, because the identifier realignment cannot compile until the four
errors are repaired.

**Holistic review across all four phases.** One inconsistency found and corrected: Phase 1 said
"six constructors", Phase 2 corrected it to five without amending Phase 1's text. Phase 1's Registry
Lifecycle section is left as written — it records what was believed at the time, and Phase 2's
correction is explicit — but the discrepancy is noted here so a reader of Phase 1 alone is not
misled. No other contradiction between the phases.
