# Phase 4: Implementation Plan - Memory-Store Prefix Support

## Overview

**Feature:** Memory-store support predicates respect prefixes.

**Architecture:** `AsyncMemoryStore` and `MemoryStore` retain their existing synchronous trait
method and return `!key.is_relative() && key.has_key_prefix(&self.prefix)`. The trait rustdoc
describes absolute keys, configured-prefix membership, and optional backend-specific exclusions as
cumulative support criteria; direct operations keep their separate fallible absolute-key guard.

**Estimated complexity:** Low.

**Estimated time:** 1-2 hours for an experienced Rust developer, including validation and Phase 5
documentation.

**Prerequisites:**
- Phases 1-3 approved and no architectural questions open.
- No dependency, feature, command, API, serialization, or cross-crate change.
- The independent synchronous `MemoryStore::makedir` issue remains explicitly out of scope.

## Implementation Steps

### Step 1: Correct support predicates and trait contract

**File:** `liquers-core/src/store.rs`

**Action:**
- Replace the two memory-store `is_supported` bodies with the approved boolean expression.
- Remove the obsolete comments saying memory-store prefixes are deliberately ignored.
- Update `Store::is_supported` and `AsyncStore::is_supported` rustdoc to state the cumulative
  support contract and distinguish it from `Key::as_absolute()` enforcement in fallible methods.
- Remove references to nonexistent `with_overlay` and `with_fallback` implementations; state that
  truthful direct predicates support direct callers and future composition.

**Code changes:**
```rust
fn is_supported(&self, key: &Key) -> bool {
    !key.is_relative() && key.has_key_prefix(&self.prefix)
}
```

The expression borrows both keys, allocates nothing, and intentionally uses the existing
segment-aware helper rather than a string comparison or new abstraction.

**Validation:**
```bash
cargo check -p liquers-core
```

**Rollback:** Revert only the Step 1 hunk in `liquers-core/src/store.rs`; do not discard unrelated
working-tree changes.

**Agent Specification:**
- **Model:** Sonnet-equivalent
- **Skills:** rust-best-practices
- **Knowledge:** Phase 1-3 documents, `Store`/`AsyncStore` docs, memory-store implementations,
  `Key::has_key_prefix`, and `STORE_SEMANTICS.md`
- **Rationale:** One source module contains public API documentation and two parallel trait
  implementations, requiring contract-level judgment rather than mechanical editing.

---

### Step 2: Add direct prefix-support regression tests

**File:** `liquers-core/src/store.rs`, existing `#[cfg(test)] mod tests`

**Action:**
- Add the test-only `memory_store_support(&Key, &Key) -> (bool, bool)` helper from Phase 3.
- Add six named plain `#[test]` cases: equal prefix, descendant, outside prefix,
  `data`/`database` segment lookalike, empty prefix, and relative `data/../secret`.
- In the relative fixture, assert its prefix match and relative shape before asserting both stores
  reject it, so the test isolates the absolute-key term.
- Keep existing router and `keyabs` tests unchanged. Do not test out-of-prefix `get` or `set`;
  prefix enforcement for direct operations is not part of this issue.

**Code changes:**
```rust
fn memory_store_support(prefix: &Key, key: &Key) -> (bool, bool) {
    let sync_store = MemoryStore::new(prefix);
    let async_store = AsyncMemoryStore::new(prefix);
    (sync_store.is_supported(key), async_store.is_supported(key))
}
```

Each test returns `Result<(), Error>` to use `parse_key(...) ?`; the async-store predicate does
not need Tokio because it is synchronous.

**Validation:**
```bash
cargo test -p liquers-core --lib memsupport
cargo test -p liquers-core --lib store::tests
```

**Rollback:** Revert only the new helper and `memsupport01`-`memsupport06` test hunks.

**Agent Specification:**
- **Model:** Haiku-equivalent
- **Skills:** liquers-unittest, rust-best-practices
- **Knowledge:** Phase 3 test plan, existing inline store tests, key-prefix semantics, and
  absolute-key conformance tests
- **Rationale:** The cases and placement are approved and follow a local, repetitive test pattern.

---

### Step 3: Run affected-crate regression and formatting checks

**Files:** No source edits expected.

**Action:**
- Format only through the workspace formatter, inspect its diff, and retain only formatting that
  belongs to the touched file.
- Run the full affected crate suite after focused tests pass.
- Confirm direct prefix tests fail before Step 1 and pass afterward when practical; never weaken
  assertions merely to make a pre-fix tree pass.

**Validation:**
```bash
cargo fmt --all -- --check
cargo test -p liquers-core
git diff --check
```

**Rollback:** No rollback for successful checks. If formatting creates unrelated churn, restore
only the unrelated formatting hunks before continuing.

**Agent Specification:**
- **Model:** Haiku-equivalent
- **Skills:** rust-best-practices, liquers-unittest
- **Knowledge:** Steps 1-2, crate test conventions, and current working-tree status
- **Rationale:** Deterministic validation and narrow diff inspection.

---

### Step 4: Enter Phase 5 with verified current-state evidence

**Files:** `specs/reference/STORE_SEMANTICS.md`,
`specs/issues/CORE-ASYNC-MEMORY-STORE-IS-SUPPORTED-IGNORES-PREFIX.md`,
`specs/README.md`, `specs/index.csv`, and
`specs/design/async-memory-store-prefix-support/phase5-documentation.md`.

**Action:**
- Record the actual code and test names, not planned names, in the reference enforcement evidence
  and source-issue resolution.
- Replace the reference warning with the established cumulative support rule, correct stale
  layering claims, update `reviewed:`, and add a History row linking this design.
- Close the source issue, repair stale source locations/comparison counts, and explain sync parity.
- Remove the temporary capability link from `specs/README.md` once `STORE_SEMANTICS.md` is the
  authoritative current-state record; regenerate `specs/index.csv` and README generated blocks.
- Summarize requested, implemented, omitted, and discovered scope, including the separately filed
  sync `MemoryStore::makedir` issue.

**Validation:**
```bash
python3 scripts/docs_index.py
python3 scripts/docs_index.py --check
python3 .claude/skills/liquers-project/scripts/validate_phase.py async-memory-store-prefix-support 5
```

**Rollback:** Revert only documentation hunks that conflict with verified behavior, then regenerate
the index after correction.

**Agent Specification:**
- **Model:** Sonnet-equivalent
- **Skills:** rust-best-practices
- **Knowledge:** verified source diff and test output, Phase 2 documentation architecture,
  `DOCS_STRUCTURE_GUIDE.md`, and source/reference documents
- **Rationale:** Current-state documentation must accurately distinguish completed behavior from
  planning and preserve the reference hierarchy.

## Testing Plan

### Unit Tests

**When to run:** Immediately after Step 2, then again as part of Step 3.

**File:** `liquers-core/src/store.rs`.

```bash
cargo test -p liquers-core --lib memsupport
cargo test -p liquers-core --lib store::tests
cargo test -p liquers-core
```

**Expected:** all six direct predicate cases and existing crate tests pass. The added out-of-prefix
cases must fail against the pre-fix predicate.

### Integration Tests

None. Router tests already apply their own prefix filter and therefore cannot demonstrate this
regression. Existing router tests are retained as non-target regression coverage.

### Manual Validation

No interactive command or service is involved. Inspect the final diff to confirm both methods use
the same expression and run:

```bash
cargo fmt --all -- --check
git diff --check
```

**Success criteria:** the code contains no new allocation, trait signature, or direct-operation
policy; focused and full crate tests pass; documentation is generated and validated after Phase 5.

## Agent Assignment Summary

| Step | Model | Skills | Rationale |
|---|---|---|---|
| 1 | Sonnet-equivalent | rust-best-practices | Parallel trait contracts and public rustdoc require local architectural judgment. |
| 2 | Haiku-equivalent | liquers-unittest, rust-best-practices | Approved, localized test templates follow established conventions. |
| 3 | Haiku-equivalent | rust-best-practices, liquers-unittest | Deterministic checks and narrow diff inspection. |
| 4 | Sonnet-equivalent | rust-best-practices | Verified current-state documentation spans source, issue, and reference. |

## Rollback Plan

### Per-Step Rollback

Revert only the affected hunks in `liquers-core/src/store.rs` for Steps 1-2, rerun the focused
tests, and revise the plan only if the current APIs invalidate its assumptions. Do not use a broad
checkout or reset because this worktree may contain unrelated changes.

### Full Feature Rollback

No new dependencies, files, public APIs, or migrations are created by implementation. Revert the
predicate, rustdoc, tests, and Phase 5 documentation as one reviewable change set; retain the
design history and any independently filed issue unless explicitly superseded.

### Partial Completion

If paused after code edits, record passing/failing commands and the completed step in `DESIGN.md`.
Do not mark the design complete before Phase 5 current-state documentation is approved.

## Documentation Updates

No new reference or guide is planned. `specs/reference/STORE_SEMANTICS.md` remains the
authoritative internal reference for `core/store`: revise section 6 to state absolute key +
configured segment prefix + optional store-specific exclusions; retain section 7 as the fallible
absolute-key enforcement contract. Update its `reviewed:` field and History row.

The source issue is the other authoritative affected document: close it only after the code and
tests pass, cite actual test names, replace line-number-only evidence, correct its stale comparison
and nonexistent-layering wording, and note `MemoryStore` parity. The temporary README capability
link is removed after this reference update; regenerate `specs/index.csv` and README blocks.

Phase 5 evidence must capture the requested async fix, the intentionally included sync parity,
unchanged router/direct-operation behavior, tests run, docs updated, and the separate
`MEMORYSTORE-MAKEDIR-SUCCEEDS-WITHOUT-CREATING-A-DIRECTORY` issue discovered during planning.
Neither `CLAUDE.md` nor `PROJECT_OVERVIEW.md` changes because no reusable development pattern or
core concept is introduced.

## Phase 5 Entry Criteria

- [ ] Both predicates and rustdoc are implemented and formatted.
- [ ] All six direct tests and full `liquers-core` suite pass.
- [ ] All review and user comments are incorporated.
- [ ] The reference and issue updates are checked against actual final code and output.
- [ ] Documentation index and Phase 5 validation pass.

## Execution Options

After Phase 4 approval: execute this plan now; create a task list for later; revise the plan; or
exit for manual implementation. Executing now proceeds directly to implementation, validation, and
then mandatory Phase 5 documentation.
