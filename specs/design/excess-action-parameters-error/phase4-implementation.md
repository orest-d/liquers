# Phase 4: Implementation Plan - Excess Action Parameters Error

## Overview

Seven steps, all inside `liquers-core`. Steps 1-3 are the change (one error constructor, one
leftover check, two header edits); steps 4-6 are the tests from Phase 3; step 7 is the regression
sweep. No other crate is touched, and no public signature changes, so there is no cross-crate
sequencing to get right.

The order is chosen so the tree compiles after every step: the constructor exists before its callers,
and each caller lands with its own tests before the next begins.

Total expected diff: roughly 60 lines of implementation and 200 of tests.

## Implementation Steps

### Step 1 — `Error::too_many_parameters`

**File:** `liquers-core/src/error.rs`, immediately after `missing_argument` (`:174`), whose dual it
is.

```rust
/// An action or resource header supplied a parameter beyond what is accepted.
///
/// The dual of [`Self::missing_argument`]. `subject` names what rejected the parameter
/// ("command 'select_columns'", "resource header"), `accepted` is how many parameters that
/// subject consumes, and `excess_index` is the 1-based position of the first surplus
/// parameter in the written parameter list.
pub fn too_many_parameters(
    subject: &str,
    accepted: usize,
    excess_index: usize,
    excess_value: &str,
    position: &Position,
) -> Self {
    Error {
        error_type: ErrorType::TooManyParameters,
        message: format!(
            "Too many parameters for {subject}: accepts {accepted}, \
             but parameter #{excess_index} '{excess_value}' was supplied"
        ),
        position: position.clone(),
        query: None,
        key: None,
        command_key: None,
    }
}
```

`ErrorType::TooManyParameters` already exists — **do not add a variant.** Adding one would force an
arm into every exhaustive match across four crates.

**Validation:** `cargo check -p liquers-core`

---

### Step 2 — the leftover check in the action path

**File:** `liquers-core/src/plan.rs`, in `ResolvedParameterValues::from_action_extended` (`:871`).
The signature does not change.

Add a private helper next to it:

```rust
/// Number of action parameters this command can consume, from argument slot `skip` onward.
///
/// Injected arguments are excluded: they are supplied by the execution context and consume
/// no query parameter. `skip` accounts for alias head parameters, which fill leading slots
/// before the action is consulted.
fn accepted_parameter_count(command_metadata: &CommandMetadata, skip: usize) -> usize {
    command_metadata
        .arguments
        .iter()
        .skip(skip)
        .filter(|a| !a.injected)
        .count()
}
```

Then, after the existing `for a in command_metadata.arguments.iter().skip(n)` loop and before
`Ok(ResolvedParameterValues(values))`:

```rust
if let Some(excess) = parameters.next() {
    return Err(Error::too_many_parameters(
        &format!("command '{}'", command_metadata.name),
        accepted_parameter_count(command_metadata, n),
        parameters.parameter_number,
        &excess.encode(),
        &excess.position(),
    ));
}
```

Three details that are easy to get wrong and are each pinned by a test:

- **`parameters.parameter_number` is already 1-based here.** `ActionParameterIterator::next()`
  (`:1003`) increments *before* returning, so after the call it equals the index of the parameter
  just returned, counting from one. No `+ 1`.
- **Ask the iterator, never the counts.** Do not compute surplus as
  `action_request.parameters.len() > command_metadata.arguments.len()`. That breaks the variadic
  exemption (a `multiple` argument legitimately consumes many) and the injected exemption.
- **`accepted` is not `arguments.len()`.** See the helper.

**Validation:**
```bash
cargo check -p liquers-core
cargo test -p liquers-core plan::tests
```

---

### Step 3 — the resource header

**File:** `liquers-core/src/plan.rs`, in `process_resource_query` (`:1239`).

**3a. Surplus parameters (`:1250-1255`) become an error.** Replace the `init_warning` block:

```rust
if let Some(excess) = header.parameters.get(1) {
    return Err(Error::too_many_parameters(
        "resource header",
        1,
        2,
        &excess.value,
        &excess.position,
    ));
}
```

**3b. Remove the `unwrap` while the block is open.** `header.parameters.first().unwrap()` (`:1257`)
is sound today but is an `unwrap` in library code. Restructure the `else` branch to
`if let Some(first) = header.parameters.first()` and match on `first.value.as_str()`.

**3c. The `_` arm (`:1298`) gets an accurate, positioned message:**

```rust
_ => {
    return Err(Error::not_supported(format!(
        "Unknown resource header instruction '{}'. Valid instructions: \
         b, bin, binary, meta, metadata, dir, directory, sdir, store_directory, \
         r, recipe, data, value, stored, stored_binary, stored_bin, sbin, \
         stored_meta, stored_metadata, cwd, key",
        first.value
    ))
    .with_position(&first.position));
}
```

The current text — "Resource header parameters must be string or link" — describes a parse-shape
failure, which is not what happened. The error *type* stays `NotSupported`; only the message and the
position change. Listing the aliases mirrors the enum-alias error at `plan.rs:625`.

**Do not touch `plan.rs:1242-1245`.** The "Resource header name is ignored" warning stays a warning:
the name is reserved for a future realm interpretation, which `// TODO: RQS realm should should be
supported` (`:1238`) records. An error there would reject queries a later version accepts.

**Validation:**
```bash
cargo test -p liquers-core plan::tests
```

---

### Step 4 — constructor test

**File:** `liquers-core/src/error.rs`, in its `#[cfg(test)] mod tests`.

`too_many_parameters_constructor` (**T1**): asserts `ErrorType::TooManyParameters`; that the position
is preserved verbatim; and that the message contains the subject, the accepted count, the 1-based
index and the excess value.

**Validation:** `cargo test -p liquers-core error::`

---

### Step 5 — plan tests

**File:** `liquers-core/src/plan.rs`, in the existing `#[cfg(test)] mod tests`, beside
`test_resolved_parameter_values` (`:2639`).

Ten tests, exactly as tabulated in Phase 3:

| Test | Covers |
|---|---|
| `excess_action_parameter_is_rejected` | E1 |
| `arity_boundaries_still_build` | E2 — saturated, under-supplied, **variadic** |
| `excess_reports_first_surplus_only` | T2 |
| `excess_link_parameter_encodes` | T3 |
| `accepted_count_excludes_injected` | T4 |
| `accepted_count_excludes_head_parameters` | T5 |
| `excess_errors_under_allow_placeholders` | T6 |
| `header_surplus_errors_but_ignored_name_only_warns` | E3 |
| `unknown_header_instruction_is_positioned` | T7 |
| `plan_builder_rejects_excess_end_to_end` | T8 |

Conventions: `#[test] fn … -> Result<(), Error>`; `unwrap`/`expect` are permitted **in tests only**;
`PlanBuilder::new(parse_query(q)?, &cr).build()` is the end-to-end entry point, following the
existing test at `:2409`.

`arity_boundaries_still_build` is the load-bearing one — it is what fails if someone reimplements
the check from parameter counts instead of asking the iterator.

**Validation:** `cargo test -p liquers-core plan::tests`

---

### Step 6 — validator contract test

**File:** `liquers-core/src/validate/mod.rs`, in its `#[cfg(test)] mod tests` (`:244`), reusing the
existing `registry_with(name, namespace)` helper, which builds a zero-argument command.

`over_supplied_query_reports_error` (**T9**): `to_text-extra` against that registry reports
`ValidationStatus::Error`, not `Ok` and not `Warning`.

This records the contract change: the issue anticipated `Warning` and exit 0; the resolution is an
error, so it is `Error` and exit 1. `ValidationStatus::Warning` keeps its meaning for the
header-name case and other `init_warning` sources.

**Validation:** `cargo test -p liquers-core validate::`

---

### Step 7 — regression sweep

```bash
cargo test -p liquers-core                  # the crate that changes
cargo test -p liquers-lib --lib --tests     # the default loop; builds core, macro, store
```

Phase 3 measured the expected outcome as **zero failures**, with the residual risk confined to
test-local commands whose queries were not harvested. Any failure here is therefore a finding, not
noise: read it before adjusting anything, since a genuine over-supply in committed material is
information about the codebase.

`liquers-web` and the browser suites are not required — this design touches neither, and its error
variant was already mapped there. Run them only for a full pre-merge sweep, after `cargo clean`, per
`CLAUDE.md`.

## Testing Plan

| Stage | Command | Gate |
|---|---|---|
| After step 1 | `cargo check -p liquers-core` | Compiles |
| After step 2 | `cargo test -p liquers-core plan::tests` | Existing plan tests still pass |
| After step 3 | `cargo test -p liquers-core plan::tests` | Header behaviour intact |
| After steps 4-6 | `cargo test -p liquers-core` | All 13 new tests pass |
| After step 7 | `cargo test -p liquers-lib --lib --tests` | No regression across crates |
| Manual | `liquers-validate -- 'ns-pl/select_columns-a-b' 'ns-pl/select_columns-a~_b'` | First errors with a position; second stays `Ok` |

The manual check is worth running by hand: it is the exact before/after pair that goes into
`POLARS_COMMAND_LIBRARY.md` in Phase 5, and it exercises the real 95-command registry rather than a
hand-built fixture.

**Disk:** the whole plan stays within `cargo test -p liquers-core` and the default `liquers-lib`
loop, which `CLAUDE.md` measures at 4.2 GB. No `--workspace`, no examples.

## Agent Assignment

Steps 1-3 are one coherent edit to two files and are not worth splitting across agents; the
cross-step invariants (the 1-based index, the `accepted` computation, the untouched name warning)
are exactly what gets lost in a handoff.

| Step | Tier | Skills | Knowledge required |
|---|---|---|---|
| 1-3 | Sonnet | `rust-best-practices` | Phase 2 §Function Signatures; `error.rs:174`; `plan.rs:871`, `:1239-1300` |
| 4-6 | Sonnet | `liquers-unittest`, `rust-best-practices` | Phase 3 overview table and corner cases; existing test modules at `plan.rs:2639`, `validate/mod.rs:244` |
| 7 | Haiku | — | `CLAUDE.md` §Building and testing |

**Execution note:** unless subagents are explicitly requested, these steps run inline in the current
session. The tier column records the work's weight for anyone re-running this plan elsewhere; it is
not an instruction to spawn agents.

## Rollback Plan

Every step is independently revertible, and the design adds no migration, no persisted format and no
public signature change — so rollback is `git revert`, with nothing to undo in stored data.

| Step | Rollback | Blast radius if left reverted |
|---|---|---|
| 1 | Remove the constructor | None; `ErrorType::TooManyParameters` returns to being unconstructed, as at HEAD |
| 2 | Remove the leftover check and the helper | Action path returns to silently dropping surplus |
| 3a | Restore the `init_warning` block | Header returns to warn-and-ignore |
| 3b | Restore the `unwrap` | Cosmetic only |
| 3c | Restore the old message | Misleading message returns; independent of 3a |
| 4-6 | Remove the tests | Steps 1-3 become unverified |
| 7 | n/a | n/a |

Steps 3a and 3c are deliberately separable: the message fix is correct whether or not the header
becomes strict, so it can survive a rollback of the strictness.

**If step 7 surfaces genuine over-supply in committed material**, the response is to fix the query,
not to weaken the check — a query that supplies parameters no command consumes is a defect the check
just found. If the volume were large enough to make that impractical, that is a finding worth
raising rather than absorbing, and it would reopen Phase 2 decision 2 (no opt-out).

## Phase 5 Entry Criteria

Phase 5 begins when all of the following hold:

1. Steps 1-7 complete; `cargo test -p liquers-core` and `cargo test -p liquers-lib --lib --tests`
   both pass.
2. The manual `liquers-validate` check shows the before/after pair from Phase 3 P1.
3. All review comments on the PR are answered or incorporated.
4. No unresolved question remains about the header's two warnings — the distinction is settled
   (surplus errors, reserved name warns) and must reach `PROJECT_OVERVIEW.md` in that form.

Phase 5 then delivers: the summary; the arity rule in `PROJECT_OVERVIEW.md`; the `~_` spelling in
`POLARS_COMMAND_LIBRARY.md`; the `multiple` note in `COMMAND_REGISTRATION_GUIDE.md` linking
`COMMAND-VARIADIC-ARGUMENTS-NOT-DECLARABLE`; `## History` rows and `reviewed:` bumps on all three;
closure of `PLAN-EXCESS-ACTION-PARAMETERS-DROPPED` recording that the resolution is an error rather
than the proposed warning; and the `specs/README.md` link.

## Cross-Phase Conformity

| Source | Requirement | Where satisfied |
|---|---|---|
| Phase 1 purpose | Error naming the position of the excess parameter | Step 2; asserted in E1 |
| Phase 1 decision 1 | Fires regardless of `allow_placeholders` | Step 2 reads no flag; T6 |
| Phase 1 decision 2 | No opt-out | No configuration added anywhere in this plan |
| Phase 1 decision 3 | Header errors on surplus | Step 3a; E3 |
| Phase 1 decision 5 | Header `_` arm message fixed and positioned | Step 3c; T7 |
| Phase 2 §Function Signatures | Constructor shape, `accepted` excludes injected and head | Steps 1-2; T4, T5 |
| Phase 2 resolved Q2 | Header *name* warning untouched | Step 3 explicitly excludes `:1242-1245`; E3 asserts it |
| Phase 2 deferral | No `liquers-lib` code change | No step touches `liquers-lib` |
| Phase 3 test plan | 13 tests | Steps 4-6 |
| Phase 3 measurement | Expect zero regressions | Step 7 |
| `CLAUDE.md` | No `unwrap` in library code | Step 3b removes one rather than adding any |
| `CLAUDE.md` | Typed error constructors, no `Error::new` | Step 1 |
