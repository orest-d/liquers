---
id: WEB-ENVIRON-VALIDATION-REPORT
kind: design
title: A language-visible validation report for JavaScript command declarations
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [web, core/commands]
issues: [WEB-ENVIRON-VALIDATION-REPORT-NOT-EXPOSED]
affects_docs: [specs/reference/COMMAND_DECLARATION.md, specs/guides/LANGUAGE-INTEGRATION_GUIDE.md, liquers-web/README.md]
created: 2026-10-17
---
# A language-visible validation report for JavaScript command declarations

Bulk design (`specs/guides/autonomous_bulk_design.md`). Phases 1-4 are written but not approved, and
nothing is implemented.

## Phase 1: High-Level Design

### Purpose

`ENVIRON08` (`LANGUAGE-INTEGRATION_GUIDE.md` §ENVIRON, added 2026-09-05) requires that invalid
imported command metadata expose a complete language-visible report (severity, message, command
identity) before the consuming operation raises its compact error. In `liquers-web` the imported
metadata is the JavaScript command declaration, and the only consumer is `registerCommand`. Today
every finding is flattened into one `ParameterError`: `CommandDeclaration::build`
(`liquers-core/src/command_declaration.rs`) computes `metadata.check()` and keeps only
`short_summary`, and `JsCommandSpec::parse` (`liquers-web/src/command/spec.rs`) adds its own
JavaScript-specific refusals as single errors. A page has no way to list the findings.

### Problem Example

```js
await liquers.init();
const spec = { name: "join", run: (...xs) => xs.join(""),
               arguments: [{ name: "xs", multiple: true }, { name: "a" }, { name: "b" }] };
liquers.registerCommand(spec);
```

Today: throws `LiquersError` with `type: "parameter_error"` and message `Command metadata contains 2
errors; first: Argument 'a' is starved by multiple argument 'xs' on --join`. The second finding
(`b`) is reported only as a count, and there is no call that returns both.

Should be: `liquers.validateCommand(spec)` returns, without registering anything,

```js
[{ severity: "error", message: "Argument 'a' is starved by multiple argument 'xs'",
   command: { realm: "", namespace: "", name: "join" } },
 { severity: "error", message: "Argument 'b' is starved by multiple argument 'xs'",
   command: { realm: "", namespace: "", name: "join" } }]
```

and `registerCommand(spec)` still throws the same compact error.

### Scope and Acceptance Criteria

- **AC-1** Complete report
  - WHEN a declaration with several metadata errors is passed to `validateCommand`
  - THEN it returns one entry per finding, each with `severity`, `message` and `command`
- **AC-2** Preflight, not a side effect
  - WHEN `validateCommand` is called on any declaration
  - THEN nothing is registered and no environment is built or rebuilt, before or after `init()`
- **AC-3** The compact error is unchanged
  - WHEN `registerCommand` receives the same invalid declaration
  - THEN it throws a `parameter_error` whose message equals the report's compact summary, and the
    command is not registered
- **AC-4** A valid declaration
  - WHEN `validateCommand` receives a declaration that `registerCommand` accepts
  - THEN it returns an empty array
- **AC-5** JavaScript-specific refusals are findings too
  - WHEN the declaration lacks a `run` function, has an empty `name`, or uses the reserved namespace
  - THEN `validateCommand` returns one `error` entry carrying the message `registerCommand` throws,
    with `command` set when a name is known and `null` otherwise
- **AC-6** Core keeps one validation path
  - WHEN `CommandDeclaration::build` refuses metadata
  - THEN its message equals `CommandDeclaration::check().short_summary("Command metadata")`

Non-goals: adopting `EnvironmentBuilder` in `liquers-web`; a report on the explicit `Environment`
instance (it cannot register commands); surfacing declaration `Warning`s (JavaScript runs with
`conventions: false`, so the convention warnings are unreachable).

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — adds a `pub` method to `liquers-core`'s `CommandDeclaration`
  and a function to the JavaScript binding (rule 4), across two crates (rule 6).
- **Leading issue:** Proposed resolution — the JavaScript surface.
- **Explanation:** The report already exists in core and only needs to stop being flattened; the
  rest is a mapping and one new binding function, so Phases 3-4 are safe under the proposed shape.
- **Open questions:**
  1. **Proposed resolution - binding surface:** add a module-level `validateCommand(spec):
     LiquersIssue[]` preflight, and leave `registerCommand`'s thrown error as it is. Alternatives:
     attach an `issues` array to the thrown `LiquersError` (changes the ERROR shape for one call
     site, and the guide asks for the report *before* the consuming call), or a
     `validationReport()` accessor holding the last failure (stateful, stale after a rebuild).
     Recommended: `validateCommand`.
  2. **Proposed resolution - entry shape:** `{severity, message, command: {realm, namespace,
     name} | null}` with severity as the lowercase `IssueSeverity` display name. Not serde's
     externally tagged `Issue` enum, which would surface `{CommandRegistry: {...}}`.

### Design Dependencies

- `overlaps` `WEB-STORE-CONFIG-NOT-APPLIED-THROUGH-ENVIRONMENT-CONFIG` (no design yet): both
  concern how `liquers-web` assembles its environment, but this design does not adopt
  `EnvironmentBuilder` and touches the command path, not `apply_store`. Weak overlap; either order.
- `overlaps` the design for `LANGUAGE-GUIDE-STALE-LIQUERS-WEB-CLAIMS`:
  both edit `LANGUAGE-INTEGRATION_GUIDE.md`, in different paragraphs; whichever lands second
  rebases its History row.

### Consolidated Findings

- `EnvironmentBuilder::validate` checks only the assembled registry. In `liquers-web` an invalid
  JavaScript declaration is refused before it reaches the registry, so adopting the builder would
  report nothing ENVIRON08 is about. The report has to come from the declaration path.
- Core's `build` must derive its error from the new `check`, so the compact message and the report
  cannot drift (AC-6).
- `validateCommand` must reuse `JsCommandSpec::parse`'s stage-1 logic rather than copy it: the
  plan splits `parse` into a report-producing core and a thin error wrapper.
- The guide's ENVIRON paragraph speaks of "an integration using `EnvironmentBuilder`"; it gains one
  sentence covering integrations that validate per declaration.
- Two crates, one new binding function, one TypeScript declaration and stub entry.

## Phase 2: Architecture

### Solution

Expose the per-declaration report that core already computes, and map it to JavaScript.

1. **Core:** `CommandDeclaration::check(&self) -> IssueReport` runs what `build` runs (the
   `state_argument` normalisation and deserialization) and returns `metadata.check()`; a
   deserialization failure becomes one `Issue::command_registry` error with the command's name.
   `build` becomes `check` + `short_summary` + deserialize, so there is one path.
2. **Web:** `JsCommandSpec::check(spec: &JsValue) -> IssueReport` runs stage 1 and stages 2-5
   (`apply_conventions`, `fill_defaults`, then `check`) and returns every finding; a stage-1
   refusal is one error issue. `JsCommandSpec::parse` calls `check`, returns
   `report.to_error("Command metadata")` with the `ParameterError` type when it has errors, and
   otherwise finishes as today.
3. **Binding:** module-level `validateCommand(spec)` in `liquers-web/src/lib.rs` returns
   `js_sys::Array` of plain objects built by a private `issue_to_js` in
   `liquers-web/src/error.rs`. It touches no thread-local, so it is callable before `init()`.

Rejected: routing `liquers-web` through `EnvironmentBuilder` (reports registry findings only, see
Consolidated Findings, and would rework the rebuild path); duplicating `build`'s normalisation in
`liquers-web` (drift); an `issues` field on the thrown error (open question 1).

Known-issue preflight: `WEB-STORE-CONFIG-NOT-APPLIED-THROUGH-ENVIRONMENT-CONFIG` (P3) does not
block. `POST-INIT-COMMAND-REGISTRATION` (accepted) is unaffected: `validateCommand` never rebuilds.
`LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT` concerns the JS→Rust direction only.

### Changes

- `liquers-core/src/command_declaration.rs`: new `pub fn check(&self) -> IssueReport` on
  `CommandDeclaration`; private `validate` removed, `build` rewritten over `check`. Uses
  `crate::issue_report::{Issue, IssueReport, IssueSeverity}`.
- `liquers-web/src/command/spec.rs`: `pub fn check(spec: &JsValue) -> IssueReport` on
  `JsCommandSpec`; `parse` delegates to it. Messages of stage-1 refusals are unchanged.
- `liquers-web/src/error.rs`: `pub(crate) fn issue_report_to_js(report: &IssueReport) ->
  js_sys::Array`.
- `liquers-web/src/lib.rs`: `#[wasm_bindgen(js_name = validateCommand)] pub fn
  validate_command(spec: typescript::JsCommandSpecObject) -> js_sys::Array`. Infallible: a
  non-object argument is itself a finding.
- `liquers-web/src/typescript.rs`: `export interface LiquersIssue { severity: "debug" | "info" |
  "warning" | "error"; message: string; command: { realm: string; namespace: string; name: string }
  | null }` and the return type `LiquersIssue[]`.
- `liquers-web/scripts/check-stubs.sh` free-function list, `tests/stubs/valid_usage.ts`.
- Errors: unchanged types; sync only; no command or registry change.
- Documents: `COMMAND_DECLARATION.md` (the `check` entry point), the guide's ENVIRON paragraph,
  `liquers-web/README.md` API section.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| `build`'s error text changes for an existing caller | AC-6 test; `short_summary` input is the same report | high |
| A stage-1 message changes while being moved | Existing `commands_COMMAND.rs` / `commands_DECLARATION.rs` assertions on messages | high |
| Deserialization-failure wording differs between `check` and `build` | Both format through one helper | high |
| Stub drift | `check-stubs.sh` STUBS02/03 | high |

Likely test impact: none failing; `environ07_documented_operations_are_callable` gains a call.

## Phase 3: Examples and Tests

### Examples

The Problem Example (AC-1, AC-3). A valid `{ name: "hello", run: () => "hi" }` gives `[]` (AC-4).
`validateCommand({ name: "x" })` gives `[{severity: "error", message: "Command \"x\" must have a
`run` function", command: {realm: "", namespace: "", name: "x"}}]`; `validateCommand(42)` gives one
entry with `command: null` (AC-5).

### Tests

Core, `liquers-core/src/command_declaration.rs` `mod tests`:

- `check01_report_lists_every_metadata_issue` — two starved arguments give two `CommandRegistry`
  errors naming the command: AC-1, AC-6
- `check02_build_error_is_the_report_summary` — `build` error message equals
  `check().short_summary("Command metadata")`: AC-6
- `check03_deserialization_failure_is_one_issue` — a wrongly typed field gives one error naming the
  command: AC-1

Web, `liquers-web/tests/environment_ENVIRON.rs`:

- `environ08_validate_command_reports_every_issue` — the Problem Example: AC-1
- `environ08_validation_registers_nothing` — before and after `init()`, the command is absent and
  `commandNames` is unchanged: AC-2
- `environ08_report_precedes_compact_registration_error` — `registerCommand` then throws
  `parameter_error` with the summary message, command absent: AC-3
- `environ08_valid_declaration_has_empty_report`: AC-4
- `environ08_javascript_refusals_are_report_entries` — missing `run`, empty `name`, reserved
  namespace, non-object: AC-5
- `environ07_documented_operations_are_callable` (extended) — `validateCommand` is callable: AC-2

Run: `cargo test -p liquers-core --lib command_declaration`;
`cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles --test
environment_ENVIRON`; `./liquers-web/scripts/check-stubs.sh`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-core/src/command_declaration.rs` `CommandDeclaration::check` — add it, rewrite `build` over it, remove `validate`; add `check01`-`check03` — `cargo test -p liquers-core --lib command_declaration`. Rollback: revert the file; nothing depends on it yet.
- [ ] 2. `liquers-web/src/command/spec.rs` `JsCommandSpec::check` / `parse` — split as in Phase 2 — the existing `commands_COMMAND` and `commands_DECLARATION` suites under Node pass unchanged.
- [ ] 3. `liquers-web/src/error.rs` `issue_report_to_js`; `liquers-web/src/lib.rs` `validate_command` — add the binding; add the `environ08_*` tests and extend `environ07_documented_operations_are_callable` — `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles --test environment_ENVIRON`.
- [ ] 4. `liquers-web/src/typescript.rs`, `scripts/check-stubs.sh`, `tests/stubs/valid_usage.ts` — declare `LiquersIssue` and `validateCommand` — `./liquers-web/examples-web/quickstart/build.sh && ./liquers-web/scripts/check-stubs.sh`.
- [ ] 5. Documents — `COMMAND_DECLARATION.md` (entry point), `LANGUAGE-INTEGRATION_GUIDE.md` ENVIRON paragraph (per-declaration integrations; History row, `reviewed:`), `liquers-web/README.md` API list; close the source issue — `python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-core --lib`; the full Node loop
`cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles`;
`bash scripts/check-build-matrix.sh` wasm32 rows; `python3 scripts/docs_index.py --check`; a final
diff review for scope (no `EnvironmentBuilder` adoption, no registry change).
