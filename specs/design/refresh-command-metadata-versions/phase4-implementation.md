# Phase 4: Implementation Plan - Refresh Command Metadata Versions

## Overview

**Feature:** Refresh Command Metadata Versions

**Architecture:** `CommandMetadataRegistry::refresh_metadata_versions` recomputes stored command
metadata versions, and `Environment::to_ref(mut self)` invokes it before wrapping the environment in
`EnvRef`.

**Estimated complexity:** Medium

**Estimated time:** 2-3 hours for an experienced Rust developer

**Prerequisites:**
- Phase 1, 2, and 3 approved
- No new dependencies
- `MACRO-LEAVES-STALE-METADATA-VERSION` remains the only blocking issue

## Implementation Steps

### Step 1: Add the Registry Lifecycle Method

**File:** `liquers-core/src/command_metadata.rs`

**Action:**
- Add public `CommandMetadataRegistry::refresh_metadata_versions(&mut self) -> &mut Self`.
- Make `update_all_metadata_versions` delegate to it and mark it deprecated.
- Keep `update_command_metadata_version` unchanged.

**Code changes:**
```rust
impl CommandMetadataRegistry {
    /// Recomputes every command's metadata version from the metadata currently stored
    /// in this registry.
    pub fn refresh_metadata_versions(&mut self) -> &mut Self {
        for command in &mut self.commands {
            command.metadata_version = Self::calculate_metadata_version(command);
        }
        self
    }

    #[deprecated(note = "use refresh_metadata_versions")]
    pub fn update_all_metadata_versions(&mut self) -> &mut Self {
        self.refresh_metadata_versions()
    }
}
```

**Validation:**
```bash
cargo check -p liquers-core
```

**Rollback:** revert `liquers-core/src/command_metadata.rs`.

**Agent Specification:**
- **Model:** haiku
- **Skills:** rust-best-practices
- **Knowledge:** `command_metadata.rs`, Phase 2 architecture
- **Rationale:** small API addition using an existing helper

---

### Step 2: Refresh During Environment Finalization

**File:** `liquers-core/src/context.rs`

**Action:**
- Add `fn get_mut_command_metadata_registry(&mut self) -> &mut CommandMetadataRegistry` to
  `Environment`.
- Change default `to_ref(self)` to `to_ref(mut self)` and call
  `self.get_mut_command_metadata_registry().refresh_metadata_versions()` before `EnvRef::new(self)`.
- Implement the mutable accessor for all core environment impls:
  `SimpleEnvironment`, `SimpleEnvironmentWithPayload`, `ImmediateEnvironment`, and
  `ImmediateEnvironmentWithPayload`.

**Code changes:**
```rust
pub trait Environment: Sized + MaybeSync + MaybeSend + 'static {
    fn get_command_metadata_registry(&self) -> &CommandMetadataRegistry;
    fn get_mut_command_metadata_registry(&mut self) -> &mut CommandMetadataRegistry;

    fn to_ref(mut self) -> EnvRef<Self> {
        self.get_mut_command_metadata_registry().refresh_metadata_versions();
        let envref = EnvRef::new(self);
        envref.0.init_with_envref(envref.clone());
        envref
    }
}
```

Each core implementor adds:

```rust
fn get_mut_command_metadata_registry(&mut self) -> &mut CommandMetadataRegistry {
    &mut self.command_registry.command_metadata_registry
}
```

**Validation:**
```bash
cargo check -p liquers-core
```

**Rollback:** revert `liquers-core/src/context.rs`.

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices
- **Knowledge:** `context.rs`, `command_metadata.rs`, Phase 2 soundness decision
- **Rationale:** trait API change across multiple implementors

---

### Step 3: Wire Cross-Crate Environment Implementors

**Files:**
- `liquers-lib/src/environment.rs`
- `liquers-py/src/context.rs`

**Action:**
- Add `get_mut_command_metadata_registry` to `DefaultEnvironment<V, P>`.
- Add the same accessor to `liquers-py::context::Environment` for trait source compatibility.
- Do not attempt unrelated `liquers-py` cleanup.

**Code changes:**
```rust
fn get_mut_command_metadata_registry(&mut self) -> &mut CommandMetadataRegistry {
    &mut self.command_registry.command_metadata_registry
}
```

**Validation:**
```bash
cargo check -p liquers-lib
cargo check -p liquers-py
```

`liquers-py` has runtime-incomplete `todo!()` paths, but it is a default workspace member and
currently compiles. Treat `cargo check -p liquers-py` as a required source-compatibility gate for the
new trait method; do not attempt unrelated runtime cleanup.

**Rollback:** revert the two files above.

**Agent Specification:**
- **Model:** haiku
- **Skills:** rust-best-practices
- **Knowledge:** `liquers-lib/src/environment.rs`, `liquers-py/src/context.rs`, Phase 2 integration table
- **Rationale:** repeated accessor implementation with one known caveat

---

### Step 4: Add Registry Unit Tests

**File:** `liquers-core/src/command_metadata.rs`

**Action:**
- Split the existing broad `test_update_metadata_versions_on_demand` if useful.
- Add tests:
  - `refresh_metadata_versions_recomputes_mutated_commands`
  - `refresh_metadata_versions_refreshes_every_command`
  - `refresh_metadata_versions_preserves_impl_version`
  - `update_all_metadata_versions_delegates_to_refresh_metadata_versions`
- Use `ArgumentInfo::integer_argument("count", false)` for argument mutation.
- Allow deprecated use locally on the compatibility test.

**Validation:**
```bash
cargo test -p liquers-core refresh_metadata_versions
cargo test -p liquers-core update_all_metadata_versions_delegates_to_refresh_metadata_versions
```

**Rollback:** remove the new or changed tests from `command_metadata.rs`.

**Agent Specification:**
- **Model:** haiku
- **Skills:** liquers-unittest, rust-best-practices
- **Knowledge:** Phase 3 test plan, `command_metadata.rs`
- **Rationale:** focused unit tests following existing local test style

---

### Step 5: Add Environment Lifecycle Tests

**File:** `liquers-core/src/context.rs`

**Action:**
- Add tests for:
  - `immediate_environment_to_ref_refreshes_metadata_versions`
  - `simple_environment_to_ref_refreshes_metadata_versions`
  - `immediate_environment_with_payload_to_ref_refreshes_metadata_versions`
  - `simple_environment_with_payload_to_ref_refreshes_metadata_versions`
- Keep immediate tests synchronous.
- Gate `SimpleEnvironment` tests with the existing native-only cfg when needed.
- Use `#[tokio::test]` for queued native `SimpleEnvironment` tests.
- Reuse a helper inside the test module if it reduces duplicated setup without obscuring assertions.

**Validation:**
```bash
cargo test -p liquers-core to_ref_refreshes_metadata_versions
```

**Rollback:** remove the new tests from `context.rs`.

**Agent Specification:**
- **Model:** sonnet
- **Skills:** liquers-unittest, rust-best-practices
- **Knowledge:** `context.rs`, Phase 3 examples
- **Rationale:** tests must respect cfg differences between queued, immediate, payload, and wasm-relevant paths

---

### Step 6: Update Macro/Declaration Regression Test

**File:** `liquers-core/tests/command_declaration.rs`

**Action:**
- Change `int02_declaration_and_macro_agree_including_metadata_version` to register the macro
  command on `SimpleEnvironment<Value>`.
- Import `Environment` so `env.to_ref()` resolves.
- Read macro metadata from `envref.get_command_metadata_registry()`.
- Convert the test to `#[tokio::test]` when it calls native `SimpleEnvironment::to_ref()`, because
  `init_with_envref` starts background work through `tokio::spawn`.
- Remove the manual `update_command_metadata_version` call and the bug-expecting `assert_ne!`.
- Keep separate `impl_version` normalization before full metadata comparison.

**Validation:**
```bash
cargo test -p liquers-core --test command_declaration int02_declaration_and_macro_agree_including_metadata_version
```

**Rollback:** revert `liquers-core/tests/command_declaration.rs`.

**Agent Specification:**
- **Model:** sonnet
- **Skills:** liquers-unittest, rust-best-practices
- **Knowledge:** existing INT02 test, Phase 3 Example 1
- **Rationale:** this flips the regression from bug-documenting to fix-enforcing

---

### Step 7: Add `DefaultEnvironment` Cross-Crate Test

**File:** `liquers-lib/tests/environment_metadata_versions.rs`

**Action:**
- Add `default_environment_to_ref_refreshes_macro_metadata_versions`.
- Register a simple macro command with `DefaultEnvironment<Value>`.
- Call `to_ref` and assert the shared registry exposes refreshed metadata.
- Use `#[tokio::test]` on native targets because `DefaultEnvironment::init_with_envref` starts
  background work through `tokio::spawn`.
- Frame this as validation of the core lifecycle invariant through an existing `Environment`
  implementor, not as implementation ownership in `liquers-lib`.

**Validation:**
```bash
cargo test -p liquers-lib default_environment_to_ref_refreshes_macro_metadata_versions
```

**Rollback:** delete `liquers-lib/tests/environment_metadata_versions.rs`.

**Agent Specification:**
- **Model:** haiku
- **Skills:** liquers-unittest, rust-best-practices
- **Knowledge:** `liquers-lib/src/environment.rs`, existing `liquers-lib/tests/*` import style
- **Rationale:** narrow integration test in a separate crate

---

### Step 8: Documentation and Design Consistency

**Files:**
- `specs/reference/COMMAND_DECLARATION.md`
- `specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md`
- `specs/guides/COMMAND_REGISTRATION_GUIDE.md` only if author-facing behavior changed
- `specs/issues/MACRO-LEAVES-STALE-METADATA-VERSION.md`
- `specs/index.csv`
- `specs/README.md`

**Action:**
- Update references to state that `metadata_version` is refreshed by the registry at environment
  sharing.
- Keep guide changes conditional; command authors should not need a new manual step.
- Leave the issue open until Phase 5, but ensure design/status links are current.
- Run docs index regeneration after front-matter changes.

**Validation:**
```bash
python3 scripts/docs_index.py
python3 scripts/docs_index.py --check
```

**Rollback:** revert the documentation files and rerun `python3 scripts/docs_index.py`.

**Agent Specification:**
- **Model:** haiku
- **Skills:** none beyond repository docs guide
- **Knowledge:** `DOCS_STRUCTURE_GUIDE.md`, Phase 2 documentation architecture, Phase 3 learning log
- **Rationale:** documentation update follows established docs structure

---

### Step 9: Final Validation

**Files:** all touched files

**Action:**
- Run focused tests first.
- Run crate-level checks once focused failures are resolved.
- Record any skipped validation with reason.

**Validation:**
```bash
cargo test -p liquers-core refresh_metadata_versions
cargo test -p liquers-core update_all_metadata_versions_delegates_to_refresh_metadata_versions
cargo test -p liquers-core to_ref_refreshes_metadata_versions
cargo test -p liquers-core --test command_declaration int02_declaration_and_macro_agree_including_metadata_version
cargo test -p liquers-lib default_environment_to_ref_refreshes_macro_metadata_versions
cargo check -p liquers-py
cargo test -p liquers-core
cargo test -p liquers-lib --lib --tests
python3 scripts/docs_index.py --check
```

Optional:

```bash
cargo check -p liquers-lib --target wasm32-unknown-unknown --no-default-features --features webui
```

The wasm check is optional because it depends on the local wasm target being installed. It must use
`--no-default-features` so the default `polars` feature does not create an unrelated wasm failure,
and `--features webui` so the wasm UI dependencies used by `liquers-lib` are present.

**Rollback:** if final validation exposes a design-level mismatch, stop and return to Phase 2 or
Phase 3 rather than layering workarounds into implementation.

**Agent Specification:**
- **Model:** sonnet
- **Skills:** liquers-unittest, rust-best-practices
- **Knowledge:** all changed files and test output
- **Rationale:** final integration requires interpreting failures across crates

## Testing Plan

### Unit Tests

Run after Steps 1, 2, 4, and 5:

```bash
cargo test -p liquers-core refresh_metadata_versions
cargo test -p liquers-core update_all_metadata_versions_delegates_to_refresh_metadata_versions
cargo test -p liquers-core to_ref_refreshes_metadata_versions
```

Expected: registry refresh tests pass, and all core environment variants refresh before sharing.

### Integration Tests

Run after Steps 6 and 7:

```bash
cargo test -p liquers-core --test command_declaration int02_declaration_and_macro_agree_including_metadata_version
cargo test -p liquers-lib default_environment_to_ref_refreshes_macro_metadata_versions
```

Expected: no test manually recomputes macro metadata after `to_ref`.

### Manual Validation

Run after all implementation steps:

```bash
cargo test -p liquers-core
cargo test -p liquers-lib --lib --tests
python3 scripts/docs_index.py --check
```

Expected: all focused and crate-level tests pass; docs index is clean.

## Agent Assignment Summary

| Step | Model | Skills | Rationale |
|---|---|---|---|
| 1 | haiku | rust-best-practices | Small API addition using existing helper |
| 2 | sonnet | rust-best-practices | Trait method and lifecycle ordering across core environments |
| 3 | haiku | rust-best-practices | Repeated cross-crate accessor wiring |
| 4 | haiku | liquers-unittest, rust-best-practices | Focused registry unit tests |
| 5 | sonnet | liquers-unittest, rust-best-practices | Environment variant tests with cfg/runtime details |
| 6 | sonnet | liquers-unittest, rust-best-practices | Regression test semantics change |
| 7 | haiku | liquers-unittest, rust-best-practices | Narrow `liquers-lib` integration test |
| 8 | haiku | docs guide | Reference and index updates |
| 9 | sonnet | liquers-unittest, rust-best-practices | Cross-crate validation and failure triage |

## Rollback Plan

Per-step rollback is listed above. Full rollback restores:

```text
liquers-core/src/command_metadata.rs
liquers-core/src/context.rs
liquers-lib/src/environment.rs
liquers-py/src/context.rs
liquers-core/tests/command_declaration.rs
liquers-lib/tests/environment_metadata_versions.rs
specs/reference/COMMAND_DECLARATION.md
specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md
specs/guides/COMMAND_REGISTRATION_GUIDE.md, if touched
specs/issues/MACRO-LEAVES-STALE-METADATA-VERSION.md
specs/index.csv
specs/README.md
```

No Cargo dependencies are added, so rollback has no dependency cleanup.

## Documentation Updates

### Reference Documents

- `specs/reference/COMMAND_DECLARATION.md`: update the computed-field section for
  `metadata_version`; add a `## History` row and bump `reviewed`.
- `specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md`: add registry metadata refresh to
  the `Environment::to_ref` initialization sequence; add a `## History` row and bump `reviewed`.

### Guide Documents

- `specs/guides/COMMAND_REGISTRATION_GUIDE.md`: update only if implementation changes the
  command-author workflow. Expected result is no guide change.

### Design, Capability, and Cross-Links

- Keep `specs/design/environment-builder/DESIGN.md` note added in Phase 2.
- Regenerate `specs/index.csv` and README generated blocks after design/issue phase changes.
- In Phase 5, close `MACRO-LEAVES-STALE-METADATA-VERSION` with validation evidence.

### Phase 5 Evidence Capture

Capture:

- Whether `to_ref(mut self)` compiled cleanly with the new mutable accessor.
- Whether `cargo check -p liquers-py` passed as the source-compatibility gate.
- Any deviations from the planned test set.
- Whether `COMMAND_REGISTRATION_GUIDE.md` remained unchanged because no author action is needed.

### CLAUDE.md

No update expected. This is a local lifecycle invariant, not a new repository-wide development
pattern.

### PROJECT_OVERVIEW.md

No update expected unless implementation reveals that the environment lifecycle summary there is now
misleading.

## Phase 5 Entry Criteria

- [ ] Implementation is finished and validated
- [ ] All user comments are answered
- [ ] All review comments are answered
- [ ] Documentation can be verified against implemented and tested behavior
- [ ] Phase 5 documentation is included in the implementation PR when practical

## Execution Options

After Phase 4 approval, choose one:

- Execute now
- Create task list for later execution
- Revise Phase 4
- Exit and leave implementation to a later session
