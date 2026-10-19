---
id: PURE-COMMAND-CONTEXT-DESIGN
kind: design
title: Running a command with a pure context
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/context, core/commands]
issues: [COMMAND-CANNOT-BE-RUN-WITH-A-RESTRICTED-CONTEXT]
created: 2026-10-19
---
# Running a command with a pure context

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written, not approved, nothing
implemented.

## Phase 1: High-Level Design

### Purpose

A caller applying a command speculatively (a search projection, a preview, a UI hint) needs an
enforced guarantee that it evaluates nothing, reads no store and writes no asset. A **pure**
context refuses those operations with an `Error`; command metadata says whether a command may run
under one, and the registry refuses one that may not.

### Problem Example

Inside a command that already holds `context` and a candidate's bytes in `state`:

```rust
let executor = context.get_envref().get_command_executor();
let value = executor
    .execute_async(&CommandKey::new_name("front_matter"), state, args, context.clone())
    .await?;
```

Today, if the body of `front_matter` calls `context.evaluate(&parse_query("-R/big/recipe.csv")?)`,
that recipe runs, and its asset is created, cached and recorded as a dependency of the caller.
Nothing tells the caller.

What should happen: the caller passes `context.to_pure()` instead. If `front_matter` was declared
pure (or takes no context at all), it runs, and the `evaluate` call returns `Err` (`NotSupported`)
without creating an asset. If it was not declared pure, `execute_async` refuses it before its body
runs.

Verified at HEAD: `Context` (`liquers-core/src/context.rs`) has no access mode; `evaluate`,
`submit`, `apply`, `apply_ad_hoc` and `record_listing` reach the asset manager unconditionally, and
`CommandRegistry` (the only `CommandExecutor`) passes the context through. The issue's line numbers
are stale; its claims hold.

### Scope and Acceptance Criteria

- **AC-1** Evaluation is refused
  WHEN a command running with a pure context calls `submit`, `evaluate`, `get_dependency_state`, `wait_for_dependency`, `evaluate_local_queue`, `apply` or `apply_ad_hoc`
  THEN the call returns `Err` with `ErrorType::NotSupported` naming the operation, and the asset manager creates or runs no asset
- **AC-2** Store reads are refused
  WHEN it calls `record_listing`
  THEN the call returns `Err` (`NotSupported`) and registers no listing version
- **AC-3** The caller's asset is not written
  WHEN it calls `set_filename`, `set_title`, `set_description`, `set_volatile`, `set_expires`, `set_payload_required` or `set_error`
  THEN the call returns `Err` (`NotSupported`) and the asset's metadata is unchanged. `add_dependency` / `add_dependencies` leave the caller's pending dependencies unchanged
- **AC-4** Observation still works
  WHEN it calls `progress`, `secondary_progress`, the log methods, `is_cancelled`, `check_cancelled`, `is_volatile`, `get_payload_clone` or `get_metadata`
  THEN they behave exactly as on a full context
- **AC-5** The registry refuses a command not declared pure
  WHEN `CommandRegistry::execute` or `execute_async` receives a pure context for a command whose metadata has `pure == false`
  THEN it returns `Err` (`NotSupported`) and the command body does not run
- **AC-6** Purity is declared and derived
  WHEN a command is registered with `register_command!`
  THEN `CommandMetadata::pure` is `true` if the signature has no `context` and no `injected` argument, or if it says `pure: true`. It is `false` if the signature takes `context` or an injected argument and has no `pure: true`, or if it says `pure: false`. An alias copies its target's value, and the exported registry shows it
- **AC-7** A pure context stays pure
  WHEN a pure context is cloned (`clone`, `clone_context`, `with_volatile`, `to_pure`)
  THEN the clone is pure. No method returns a full context from a pure one
- **AC-8** Full contexts are unchanged
  WHEN commands run through the interpreter
  THEN the context is full and all existing core and lib tests pass

Non-goals: a pure entry point for callers holding no `Context` (a pure `AssetManager::apply`,
follow-up); sandboxing arbitrary Rust (files, globals), as purity covers Liquers' own services only;
Python commands, which stay `pure == false` unless a declaration says otherwise.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible. It is a new feature that adds a `pub` enum and `pub` methods
  on `Context`, a `pub` field on `CommandMetadata` and a macro statement, and it changes
  `specs/command_registry.yaml` (rules 2-4).
- **Leading issue:** the escape handles (question 1).
- **Explanation:** the design works under the recommended answers below, but each changes the
  public contract of `Context` or `register_command!`, so a maintainer decides.
- **Open questions:**
  1. **Open design: escape handles.** `Context::get_envref()` and `get_asset_ref()` return full
     handles and cannot fail. From them a command can reach the store, the asset manager and
     `EnvRef::evaluate`. Options: (a) keep them infallible and rely on the `pure` declaration, so a
     command that takes `context` and says `pure: true` promises not to use them; (b) panic in a
     pure context, which the issue rules out; (c) make them return `Result`, which breaks 16 call
     sites in `liquers-lib` / `liquers-records` plus core and tests and turns this into an L design.
     **Recommended: (a).** A derived-pure command cannot reach a context at all, and a declared one
     is reviewed like any other `pure:` claim.
  2. **Open design: derive purity from the signature.** **Recommended: yes**, with `pure: false` as
     an opt-out. Consequence: most commands in `specs/command_registry.yaml` gain `pure: true`.
     Without derivation, every command has to be annotated before a caller can use it.
  3. **Open design: granularity.** **Recommended: one mode,** `ContextAccess::Pure`. It denies
     evaluation, store reads and asset writes together. An enum leaves room for a finer mode later,
     and the issue names no caller that needs one.
  4. **Proposed resolution: refuse, not omit.** One `Context` type with an access mode; a separate
     type needs a second `CommandExecutor` signature and closure per command.
  5. **Proposed resolution: `add_dependency` cannot return `Err`.** A pure context has its own
     empty pending-dependency buffer, so these records never reach the caller.
  6. **Proposed resolution: orthogonal flags.** `pure` is independent of `volatile`, `cached` and
     `payload:`. A clock-reading command can be pure and volatile at once.

### Design Dependencies

- `store-and-asset-search` (complete, E4): where the need came up. The implemented search applies
  no projection command (`SEARCH.md` §Non-evaluation), so nothing at HEAD waits on this design.
- `environment-manager-reference-cycle`: **overlaps** (`in_review`). It adds `try_get_envref` on
  assets and managers, not on `Context`. No order.
- None `requires` or `required-by`.

### Consolidated Findings

- Every `Context` method is enforced and the registry gate decides whether a command runs; the
  only gap is the two escape handles (question 1), closable only by breaking signatures.
- A field on `Context`, not a task-local (unavailable on wasm32, where core's tokio has only
  `sync`), works on every target and for sync and async executors.
- The gate lives in `CommandRegistry`, the only `CommandExecutor`; the trait docs tell a custom
  executor to apply it.
- Declarations merge on the serialized form (`COMMAND_DECLARATION.md` §2.1), so `pure` becomes
  declarable with no merge change.

## Phase 2: Architecture

### Solution

In `liquers-core/src/context.rs`:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextAccess { Full, Pure }

impl<E: Environment> Context<E> {
    pub fn access(&self) -> ContextAccess;
    /// A pure clone; never loosens.
    pub fn to_pure(&self) -> Self;
    fn require_full(&self, operation: &str) -> Result<(), Error>;   // explicit match, no `_`
}
```

- `Context` gets a field `access: ContextAccess`. `Context::new` sets `Full`, and `Clone`,
  `clone_context` and `with_volatile` copy it.
- `to_pure` copies everything, sets `Pure`, and replaces `pending_dependencies` and
  `submitted_dependencies` with fresh, empty buffers (question 5). `service_tx` and `cancellation`
  stay shared, so logs and progress reach the caller's asset and a cancel still reaches the
  command.
- `require_full` returns
  `Error::not_supported(format!("'{operation}' is not permitted in a pure context"))`. It is called
  at the top of each method listed in AC-1, AC-2 and AC-3.
  `schedule_dependency_asset_with_key` (`pub(crate)`) is reached only through those methods.

In `liquers-core/src/command_metadata.rs`, `CommandMetadata` gets:

```rust
#[serde(default, skip_serializing_if = "is_false")]
pub pure: bool,
```

In `liquers-core/src/commands.rs`:

- `CommandRegistry::execute` and `execute_async`: after `refuse_non_executable_realm`, a private
  `refuse_impure_under_pure_context(&self, key, &context)` checks
  `command_metadata_registry.get(key.clone())`. A missing entry counts as not pure.
- `register_alias` copies `pure`.
- The `CommandExecutor` trait documentation gains a "Pure contexts" paragraph.

In `liquers-macro/src/registration.rs`, a statement `pure: <bool>` is parsed beside `volatile:`
into `CommandSignature` (`pure: Option<bool>`); codegen emits `cm.pure = …;` with the declared
value, else `true` when no parameter is `CommandParameter::Context` or `injected`. Hand registration
keeps `false`.

Rejected: a task-local scope checked in `EnvRef` and the store (no wasm32, escaped by
`tokio::spawn`); a `PureContext` type (question 4); `Result` escape handles (1c); several flags (3).

Known-issue preflight: no open issue touches `Context` access or `CommandMetadata` flags; nothing
blocks.

### Changes

- `liquers-core`: `context.rs` (enum, field, three methods, checks in 15 methods);
  `command_metadata.rs` (field); `commands.rs` (gate, alias copy, trait docs).
- `liquers-macro`: the `pure:` statement and derivation.
- `liquers-lib`: no code. Regenerate `specs/command_registry.yaml` and add a CHANGELOG line.
- `liquers-py`: optionally a `pure` getter beside `volatile`.
- Documents (each with a History row and a `reviewed:` bump):
  - `specs/reference/REGISTER_COMMAND_FSD.md`: the statement and the derivation rule.
  - `specs/guides/COMMAND_REGISTRATION_GUIDE.md`: a section on pure commands.
  - `specs/reference/COMMAND_DECLARATION.md`: a `pure` row in the key and defaults tables.
  - `specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md`: access modes.
  - The `CLAUDE.md` DSL summary gains `pure:`.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| A declared-pure command uses `get_envref` | contract and review (question 1); doc comment on both handles | medium |
| A missed `require_full` in some method | AC-1 to AC-3 tests call every listed method | high |
| The registry diff is large (derived `pure: true`) | regenerated, not hand-edited; `registry_export` tests | high |
| The interpreter receives a pure context by accident | only `to_pure` creates one; AC-8 suites | high |
| Declarations or Python claim `pure` falsely | same contract as `volatile`; documented | medium |
| wasm32 | no runtime use; `check-build-matrix.sh` wasm rows | high |

Recovery: core and macro land in separate commits; reverting the macro leaves every command
`pure: false`, which the gate refuses (fails closed).

## Phase 3: Examples and Tests

### Examples

The Problem Example, with `front_matter` registered as
`register_command!(cr, fn front_matter(state) -> result)` (derived pure), is AC-5 and AC-6. A
command registered as `fn peek(state, context) -> result` is refused under a pure context. The same
command with `pure: true` runs, and its `context.evaluate(..)` returns `NotSupported`.

### Tests

The test file is a new `liquers-core/tests/pure_context.rs`. Contexts come from
`create_dummy_asset().create_context()`, as in `commands.rs` `test_command_registry_execute`. Each
test that runs a sentinel command `probe` checks that its call counter stays 0.

- `pure01_evaluation_methods_refused`: each AC-1 method, and the `probe` counter is 0. Covers AC-1.
- `pure02_record_listing_refused`: on an `AsyncMemoryStore` folder; the dependency manager has no
  version for `-R-dir/`. Covers AC-2.
- `pure03_asset_writes_refused`: each setter returns `Err`. The metadata `filename`, `title` and
  volatility are unchanged, and `add_dependency` leaves the caller's `take_pending_dependencies`
  empty. Covers AC-3.
- `pure04_observation_allowed`: progress and log entries reach the asset, and the
  `check_cancelled` and payload calls work. Covers AC-4.
- `pure05_registry_refuses_undeclared_command`: both `execute` and `execute_async`, with the counter
  at 0. Covers AC-5.
- `pure06_purity_declared_and_derived`: the macro cases (no context, `context`, `injected`,
  `pure: true`, `pure: false`) and an alias. Covers AC-6.
- `pure07_pure_context_stays_pure`: `clone`, `clone_context`, `with_volatile(true)` and `to_pure`.
  Covers AC-7.
- Existing suites (`cargo test -p liquers-core --lib --tests`, `cargo test -p liquers-lib --lib
  --tests`, `--test registry_export`). Covers AC-8.

Run: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --test pure_context`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-core/tests/pure_context.rs`: add `pure01` to `pure05` and `pure07`. They do not
  compile at HEAD. Proof: `cargo test -p liquers-core --test pure_context`. Rollback: delete the
  file.
- [ ] 2. `context.rs`: `ContextAccess`, the `access` field, `access`, `to_pure`, `require_full`
  and its calls. Proof: `pure01` to `pure04` and `pure07`. Rollback: revert.
- [ ] 3. `command_metadata.rs` `pure` field; `commands.rs` gate, alias copy and trait docs (depends
  on 2). Proof: `pure05`, then `cargo test -p liquers-core --lib --tests`.
- [ ] 4. `liquers-macro/src/registration.rs`: the `pure:` statement and derivation. Add `pure06`.
  Proof: `cargo test -p liquers-macro` and `cargo test -p liquers-core --test pure_context`.
- [ ] 5. Regenerate `specs/command_registry.yaml` (`export-command-registry`) and add a CHANGELOG
  line. Proof: `cargo test -p liquers-lib --test registry_export`.
- [ ] 6. Documents listed in Phase 2 Changes, plus `CLAUDE.md`. Close
  `COMMAND-CANNOT-BE-RUN-WITH-A-RESTRICTED-CONTEXT` with a resolution note. Proof:
  `python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-lib --lib --tests`;
`bash scripts/check-build-matrix.sh`; `python3 scripts/docs_index.py --check`; a diff review for
`_ =>` arms, `unwrap`, and edits outside the crates named above.
