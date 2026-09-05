---
id: VARIADIC-METADATA-TAIL-CHECK-PHASE4
kind: design-phase
title: Implementation plan for runtime metadata validation
---

# Phase 4: Implementation Plan - Runtime Metadata Validation

## Overview

**Feature:** Runtime validation of variadic command metadata.

**Architecture:** `IssueReport` is a composable core diagnostic container. Command metadata
creates command-scoped errors, and `EnvironmentBuilder` owns the validation boundary: it exposes
the full report before consuming itself and rejects errors before any store is built.

**Estimated complexity:** Medium. **Estimated time:** 5-7 hours.

**Prerequisites:** Phases 1-3 are approved. The target-only wasm dependencies already exist in
the lockfile through `liquers-web`; core needs its own target-gated declarations. No command
namespace, public trait change, or new error type is required.

The wasm declarations are an internal diagnostic-transport dependency, not a new Web/API/UI
integration or cross-crate registration surface. Native-only core consumers do not resolve them.

## Implementation Steps

### Step 1: Add the central issue-report module

**Files:** `liquers-core/src/issue_report.rs` (new), `liquers-core/src/lib.rs`, and
`liquers-core/Cargo.toml`.

**Action:**

- Export `pub mod issue_report` from core.
- Define serializable `IssueSeverity` and `Issue` with explicit `Generic` and `CommandRegistry`
  variants; implement `generic`, `command_registry`, `severity`, `is_error`, and `message` with
  exhaustive variant matches.
- Define `IssueReport` with a private `Vec<Issue>` and `issues`, `append`, `extend`, counts,
  `has_errors`, `is_empty`, `short_summary`, `to_error`, `emit`, and full ordered `Display`.
- Make the short summary count error diagnostics but show no more than five unique command keys.
  Generic errors have no key, so they contribute their first message without consuming a key slot.
- Keep library diagnostics off stdout: native `emit` uses stderr; wasm uses
  `web_sys::console::error_1` and `wasm_bindgen::JsValue`. Add both dependencies only under
  `cfg(target_arch = "wasm32")`, with the `web-sys` `console` feature.

**Validation:** `cargo fmt --check`, `cargo check -p liquers-core`, and focused unit tests in
`issue_report.rs` for all four generic severities, composition ordering, warning-only conversion,
summary truncation/deduplication, and `Display` completeness.

**Rollback:** Remove the new module/export and the two wasm-target dependencies as one atomic
revert; no other module depends on it until Step 2.

**Agent specification:** Sonnet tier; `rust-best-practices` and `liquers-unittest`; read Phase 2,
Phase 3, `error.rs`, and current wasm console use in `liquers-web`. This step defines the public
diagnostic API and target-gated behavior.

### Step 2: Make command metadata checks authoritative

**Files:** `liquers-core/src/command_metadata.rs` and `liquers-core/src/command_declaration.rs`.

**Action:**

- Replace `Vec<CommandRegistryIssue>` with `IssueReport` in `ArgumentInfo::check`,
  `CommandMetadata::check`, and a new registry `check` that appends metadata reports in stored
  order. Retire `CommandRegistryIssue` only after all local call sites use `Issue`.
- Preserve empty-name and reserved-command-name diagnostics. Reject `multiple && injected`.
- During declaration-order scanning, remember the first ordinary variadic argument. For every
  later ordinary argument, report the blocker and starved argument; injected followers are valid
  and do not reset the remembered variadic argument.
- Replace `CommandDeclaration`'s duplicate private layout validation with an adapter over
  `CommandMetadata::check` that returns its existing typed parameter error when the report has
  errors.

**Validation:** `cargo test -p liquers-core command_metadata`, then
`cargo test -p liquers-core command_declaration`.

**Rollback:** Restore the command-metadata and declaration checker changes together, retaining
Step 1 as an unused independent module if needed.

**Agent specification:** Sonnet tier; `rust-best-practices`, `liquers-unittest`; read existing
metadata/declaration tests and Phase 3 corner cases. This changes the safety invariant and must
not leave two incompatible validators.

### Step 3: Add builder preflight and fail before construction

**File:** `liquers-core/src/environment_builder.rs`.

**Action:**

- Add private `validation_report: IssueReport`, initialized to default.
- Add `validate(&mut self) -> &IssueReport`, which clears then appends the current command
  registry report, and `validation_report(&self) -> &IssueReport`.
- Change the receiver binding to `pub fn build(mut self) -> Result<EnvRef<GenericEnvironment<V, P, K>>, Error>`;
  this does not change the callable API. At its first line, validate and clone only the small
  report as required to end the mutable borrow. Emit a nonempty report, return
  `to_error("Command metadata registry")`
  on errors, and only then select/build a store and assemble the environment.
- Do not move validation into `GenericEnvironment::try_to_ref`; that path retains readiness work
  but is not the builder-owned safety boundary.

**Validation:** `cargo test -p liquers-core environment_builder` plus a test with a concrete
counting `StoreFactory` proving invalid metadata prevents factory construction and assembly.

**Rollback:** Remove the field/methods and the early `build` preflight as one revert; Steps 1-2
remain independently testable.

**Agent specification:** Sonnet tier; `rust-best-practices`, `liquers-unittest`; read builder
ownership flow, `StoreFactory`, and Phase 3's consuming-builder caveat. This requires careful
borrow ordering and error propagation.

### Step 4: Add regression tests and the runnable example

**Files:** inline tests in `issue_report.rs`, `command_metadata.rs`, `command_declaration.rs`,
and `environment_builder.rs`; `liquers-core/examples/issue_report_validation_demo.rs` (new).

**Action:**

- Cover programmatic, JSON, and YAML invalid registries; assert direct `check` output and builder
  failure use the same rule.
- Pin registration/append ordering, repeated validation clearing, generic warning success,
  generic error summaries, command-key deduplication, and all variadic corner cases.
- Build the example with explicit `EnvironmentBuilder::<Value>::new()` types as needed. It must
  preflight, print the full report, and use an explicit `match` for the expected build failure so
  it does not require `EnvRef: Debug`.
- Add a wasm-target compile/callable-adapter smoke check; do not claim to intercept browser console
  output from a native test. Manual web validation verifies console output and absence of local paths.

**Validation:** `cargo test -p liquers-core`,
`cargo run -p liquers-core --example issue_report_validation_demo`, and
`cargo check -p liquers-core --target wasm32-unknown-unknown` when the target is installed.

**Rollback:** Remove only the tests/example that exercise unavailable behavior; retain production
code only when its focused tests continue to pass.

**Agent specification:** Sonnet tier; `rust-best-practices`, `liquers-unittest`; read Phase 3 and
existing core test conventions. This step proves semantics at each public boundary.

### Step 5: Final integration and evidence capture

**Files:** all Step 1-4 files, `specs/design/variadic-metadata-tail-check/`, and Phase 5 target
documents only after implementation is accepted.

**Action:**

- Format, run the focused and package-level validation matrix, review public API docs, and record
  actual deviations or new issues for Phase 5.
- Do not update current-state reference/guides or close the issue in this step; Phase 5 verifies
  those claims against the implemented result.

**Validation:** `cargo fmt --all -- --check`, `cargo check -p liquers-core`,
`cargo test -p liquers-core`, `git diff --check`, plus the wasm check where available.

**Rollback:** Revert incomplete implementation commits by step; preserve the approved design and
test findings for a later attempt.

**Agent specification:** Sonnet tier; `rust-best-practices`; read all changed core files and
Phases 1-4. This is a cross-cutting acceptance pass.

## Testing Plan

Run report tests after Step 1, metadata/declaration tests after Step 2, builder tests after Step
3, then the entire core package and example after Step 4. Native tests verify `Display`, not
process-global stderr capture. The wasm smoke test verifies target compilation and callable
adapter; browser-console behavior is a manual `liquers-web` development check.

Success requires valid final variadics to remain accepted; invalid programmatic, JSON, and YAML
registries to fail before store work; warning-only generic reports not to fail; and a report with
more than five faulty keys to return a deterministic bounded summary while displaying all issues.

## Agent Assignment

| Step | Model tier | Skills | Rationale |
| --- | --- | --- | --- |
| 1 | Sonnet | rust-best-practices, liquers-unittest | New reusable public core type and wasm boundary |
| 2 | Sonnet | rust-best-practices, liquers-unittest | Replaces two validators with one invariant |
| 3 | Sonnet | rust-best-practices, liquers-unittest | Borrow-sensitive construction safety boundary |
| 4 | Sonnet | rust-best-practices, liquers-unittest | Cross-boundary regression and runnable coverage |
| 5 | Sonnet | rust-best-practices | Final integration judgment |

## Rollback Plan

Each step is independently reversible in source control as described above. A full rollback
restores `liquers-core` source and manifest changes while retaining this design and its follow-up
issue as planning evidence. No migration or persisted schema change is made. If work pauses,
commit only passing completed steps and leave `DESIGN.md` at `phase: implementation`.

## Documentation Updates

Phase 5 will update the authoritative `affects_docs` set:
`specs/reference/COMMAND_DECLARATION.md`,
`specs/guides/COMMAND_REGISTRATION_GUIDE.md`, and
`specs/guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md`. It will close
`VARIADIC-ARGUMENT-STARVES-LATER-ARGUMENTS`, update capability-map links as required, and retain
`ISSUE-REPORT-PLAN-AND-METADATA-LOGGING` as future scope. Neither `CLAUDE.md` nor
`PROJECT_OVERVIEW.md` needs an update unless implementation reveals a broader core contract.
Step 5 must also check generated registry documentation and update it only if the validation
tests or command metadata representation actually change generated output.

## Phase 5 Entry Criteria

- [ ] All implementation steps and validations pass.
- [ ] Review comments and user feedback are incorporated.
- [ ] Current-state documentation is verified against behavior at HEAD.
- [ ] The issue-resolution and follow-up issue status are ready to update.
