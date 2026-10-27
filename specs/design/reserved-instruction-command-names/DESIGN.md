---
id: RESERVED-INSTRUCTION-COMMAND-NAMES
kind: design
title: Every planner instruction name is refused as a command name
form: compact
status: complete
gh_pr: [190]
readiness: ready
autofix: eligible
area: [core/commands, core/query]
issues: [COMMAND-NAMES-Q-AND-V-ARE-NOT-RESERVED]
created: 2026-10-20
---
# Every planner instruction name is refused as a command name

> **Complete on 2026-10-27**: implemented by automatic fix in #190.

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-20: Phases 1-4 written; Phase 4
implemented on 2026-10-27 (automatic fix).

## Phase 1: High-Level Design

### Purpose

The planner intercepts three action names before it looks up a command: `ns`, `q` and `v`.
`CommandMetadata::check` refuses only `ns`, so a command called `q` or `v` registers without any
message and can never run. The reserved names should live in one list that the planner and the
validation both read, and every name on that list should be refused in the same way.

### Problem Example

Verified at HEAD (18732dd). `liquers-core/src/command_metadata.rs` `CommandMetadata::check` refuses
the name `"ns"` only. `liquers-core/src/plan.rs` `PlanBuilder::process_action` intercepts `"v"` by
a string literal, and `process_query` / `strip_q_instruction` intercept `q` through
`ActionRequest::is_q`. The validator shows that a registered `v` never runs. A stub command named
`v` is added, and the query still plans as the instruction:

```
$ liquers-validate --command v --command q -- 'v' 'q'
[0] OK      v      init Info: Volatile due to instruction 'v'
[1] OK      q
```

From Python, `@env.first_command def v(): return 7` succeeds, and `env.evaluate("v")` returns
`None`. Expected: the registration fails with *Command name 'v' is reserved*, as it already does
for `ns`, and the same happens for `q`. This holds in every language, because Python and
JavaScript declarations, and `EnvironmentBuilder::build` for Rust registrations, all go through
`CommandMetadata::check`.

### Scope and Acceptance Criteria

- **AC-1** `v` is refused
  WHEN `CommandMetadata::check` runs on a command named `v` (in any realm or namespace)
  THEN the report has an `Error` whose message names `v` as reserved
- **AC-2** `q` is refused
  WHEN `CommandMetadata::check` runs on a command named `q`
  THEN the report has an `Error` whose message names `q` as reserved
- **AC-3** `ns` stays refused
  WHEN `CommandMetadata::check` runs on a command named `ns`
  THEN the report still has an `Error` naming `ns` as reserved
- **AC-4** One list
  WHEN the planner decides whether an action is the `ns`, `q` or `v` instruction
  THEN it reads the same constants the reserved-name check reads, and there is no second copy of
  the names in `plan.rs`
- **AC-5** Building refuses it
  WHEN a Rust environment whose registry holds a command named `v` is built with
  `EnvironmentBuilder::build`
  THEN `build` returns `Err`, and the message names the reserved command
- **AC-6** Ordinary names are unaffected
  WHEN commands named `qq`, `vv`, `nsx` or `value` are checked
  THEN no reserved-name issue is reported

Non-goals: rejecting the names at compile time in `register_command!` (another crate, and
`EnvironmentBuilder::build` already reaches every Rust registration); reserving resource-header
names; any change to how the instructions plan.

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible. A bug fix in `liquers-core` (`query.rs`, `plan.rs`,
  `command_metadata.rs`) that restores the documented intent: the instructions are not command
  names (`guides/PYTHON_GUIDE.md`, `reference/PYTHON_INTEGRATION.md`). The constants are
  `pub(crate)`, so no `pub` item is added. No command, registry file or serialized form changes.
- **Leading issue:** None
- **Explanation:** the refusal path for `ns` already exists and is already reached by every
  registration route (Rust through `EnvironmentBuilder::build`, Python and JavaScript through
  `CommandDeclaration`, which calls `CommandMetadata::check`; Python's `import_metadata` calls it
  directly).
  Extending it to the two other names is a check against a list.
- **Open questions:** None. Implementation detail settled in Phase 2: the list sits in
  `query.rs` next to `ActionRequest::is_ns` / `is_q`, which are the planner's existing tests.

### Design Dependencies

None. `variadic-link-array-expansion`, `macro-argument-hints-and-query-literals` and
`global-enum-registration` touch other functions of the same files and do not order against this.

### Consolidated Findings

- `register_command!` and `CommandRegistry::register_command` do not call `check`. A Rust
  environment assembled by hand and shared with `to_ref` therefore still accepts `v` silently. Only
  `EnvironmentBuilder::build` refuses it. This is the same today for `ns`, and the design leaves it
  that way (non-goal). The guides already recommend the builder.
- A command named `q` placed in the middle of a query (`a/q/b`) is also intercepted, because the
  predecessor `a/q` ends in `q` (`plan.rs` `process_query`, predecessor branch). So refusing `q`
  everywhere is correct, not only at the end.
- `reference/PYTHON_INTEGRATION.md` and `guides/PYTHON_GUIDE.md` describe the defect as open and
  name the issue. Both change with the fix.

## Phase 2: Architecture

### Solution

1. In `liquers-core/src/query.rs`, add crate-private constants beside `ActionRequest`:

   ```rust
   pub(crate) const NS_INSTRUCTION: &str = "ns";
   pub(crate) const Q_INSTRUCTION: &str = "q";
   pub(crate) const V_INSTRUCTION: &str = "v";
   /// Action names the planner intercepts before command lookup; never valid command names.
   pub(crate) const RESERVED_ACTION_NAMES: [&str; 3] = [NS_INSTRUCTION, Q_INSTRUCTION, V_INSTRUCTION];
   ```

   `ActionRequest::is_ns` and `is_q` compare with the constants. Add
   `pub(crate) fn is_v(&self) -> bool`.
2. In `liquers-core/src/plan.rs` `PlanBuilder::process_action`, replace `action_request.name ==
   "v"` with `action_request.is_v()`. The `ns` action built in `plan.rs` `append_action` uses
   `NS_INSTRUCTION`.
3. In `liquers-core/src/command_metadata.rs` `CommandMetadata::check`, replace the `"ns"` test
   with `RESERVED_ACTION_NAMES.contains(&self.name.as_str())`. The message becomes
   `format!("Command name '{}' is reserved: it is a query instruction", self.name)`. The `ns`
   message keeps its leading words, so nothing that matches "is reserved" changes.

Rejected alternatives: a `pub` constant (adds API for no caller outside the crate); refusing in
`CommandRegistry::register_command` (it would change a `pub` method's error behaviour for direct
callers and still miss imported metadata, which `check` already covers); a compile-time check in
`liquers-macro` (separate crate, and `EnvironmentBuilder::build` covers Rust).

### Changes

- `liquers-core/src/query.rs`: four constants, `ActionRequest::is_v`, `is_ns`/`is_q` bodies.
- `liquers-core/src/plan.rs`: `process_action` (`v` test), the `ns` literal in `append_action`.
- `liquers-core/src/command_metadata.rs`: `CommandMetadata::check`, unit tests.
- `liquers-core/tests/environment_builder.rs`: one integration test.
- Documents: `reference/PYTHON_INTEGRATION.md` ("Reserved names"), `guides/PYTHON_GUIDE.md`
  (§ registration paragraph), `reference/api/DOC_02_QUERY_LANGUAGE_REFERENCE.md` (§Planner
  instructions: one sentence saying the three names cannot be command names). Each gets a History
  row and a `reviewed:` bump.

Errors: `Issue::command_registry(IssueSeverity::Error, …)`, as for `ns` today. Sync only.

### Risks

| Assessment | Finding |
|---|---|
| Files likely to change | the three core files above, one test file, three documents |
| Existing tests | none assert that `q` or `v` registers. `plan.rs` tests use `v` and `q` as queries, not commands |
| Compatibility | a registry that already holds a command named `q` or `v` now fails `build`. Such a command could never run, so nothing working breaks. The committed `specs/command_registry.yaml` has neither name |
| Concurrency / performance | none: a three-element comparison in `check` |
| Recovery | revert the commit |
| Certainty | high; every route to `check` was read at HEAD |

## Phase 3: Examples and Tests

### Examples

```rust
let mut cm = CommandMetadata::new("v");
assert!(cm.check().has_errors());          // AC-1: "Command name 'v' is reserved: …"
cm.with_name("value");
assert!(!cm.check().has_errors());         // AC-6
```

Query example (validated, unchanged by the fix): `v` plans as the instruction, `ns-pl/select_columns-date-amount` as
`pl/select_columns`.

### Tests

- `liquers-core/src/command_metadata.rs` `validation_tests::instruction_names_are_reserved_command_names`
  (AC-1, AC-2, AC-3): for `ns`, `q`, `v`, in the default and in a named namespace, `check()` has an
  error whose message contains the name and "reserved".
- `validation_tests::names_resembling_instructions_are_not_reserved` (AC-6): `qq`, `vv`, `nsx`,
  `value` report no error.
- `liquers-core/src/query.rs` `tests::reserved_action_names_match_instruction_predicates` (AC-4):
  for each name in `RESERVED_ACTION_NAMES`, an `ActionRequest` of that name satisfies exactly one of
  `is_ns`, `is_q`, `is_v`, and every predicate is reached.
- `liquers-core/tests/environment_builder.rs` `build_refuses_a_command_named_like_an_instruction`
  (AC-5): register `v` with `register_command`, `build()` is `Err`, and the message names `v`.
- Command: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib reserved` and
  `cargo test -p liquers-core --test environment_builder`.

## Phase 4: Implementation Plan

### Steps

- [x] 1. `liquers-core/src/query.rs` — add the four constants and `ActionRequest::is_v`, rewrite
  `is_ns` / `is_q` on them, add `reserved_action_names_match_instruction_predicates` —
  `cargo test -p liquers-core --lib reserved_action_names`
- [x] 2. `liquers-core/src/plan.rs` `process_action` and `append_action` — use `is_v()` and
  `NS_INSTRUCTION` — `cargo test -p liquers-core --lib plan::` (existing `v`/`q` tests stay green)
- [x] 3. `liquers-core/src/command_metadata.rs` `CommandMetadata::check` — test against
  `RESERVED_ACTION_NAMES`, add the two `validation_tests` — `cargo test -p liquers-core --lib validation_tests`
- [x] 4. `liquers-core/tests/environment_builder.rs` — add
  `build_refuses_a_command_named_like_an_instruction` — `cargo test -p liquers-core --test environment_builder`
- [x] 5. Documents — `PYTHON_INTEGRATION.md`, `PYTHON_GUIDE.md`, `DOC_02_QUERY_LANGUAGE_REFERENCE.md`:
  state that `ns`, `q` and `v` are refused, drop the "registers but never runs" text; History row
  and `reviewed:` each — `python3 scripts/docs_index.py --check`
- [x] 6. Close `COMMAND-NAMES-Q-AND-V-ARE-NOT-RESERVED` (`status: closed`, resolution note naming
  the tests); this design to `complete` — `python3 scripts/docs_index.py` then `--check`

### Validation

`cargo fmt --check -p liquers-core`; `CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib --tests`;
`cargo test -p liquers-lib --test registry_export` (the committed registry has no reserved name);
optionally `bash scripts/test-liquers-py.sh` to see the Python refusal end to end. Final diff review:
no change outside `liquers-core` and the three documents.
