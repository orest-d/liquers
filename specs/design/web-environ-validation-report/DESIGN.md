---
id: WEB-ENVIRON-VALIDATION-REPORT
kind: design
title: A language-visible validation report for imported command metadata in liquers-web
form: compact
status: complete
readiness: ready
autofix: not-eligible
area: [web, core/commands]
issues: [WEB-ENVIRON-VALIDATION-REPORT-NOT-EXPOSED]
affects_docs: [specs/reference/COMMAND_DECLARATION.md, specs/guides/LANGUAGE-INTEGRATION_GUIDE.md, liquers-web/README.md]
created: 2026-10-17
---
# A language-visible validation report for imported command metadata in liquers-web

Bulk design (`specs/guides/autonomous_bulk_design.md`), revised on 2026-10-17 after the
maintainer's answers (see Design Readiness). Implemented on 2026-10-18; see "Implementation notes"
at the end for where the code differs from the text below.

## Phase 1: High-Level Design

### Purpose

`ENVIRON08` (`LANGUAGE-INTEGRATION_GUIDE.md` §ENVIRON, from `specs/design/variadic-metadata-tail-check/`)
is about **imported command metadata**: metadata deserialized from JSON or YAML, which only the
environment build validates (`EnvironmentBuilder::build` → `CommandMetadataRegistry::check()`). The
integration must expose that registry's complete report (severity, message, command identity) to
the language before the consuming operation raises its compact error.

`liquers-web` has no import path at all: a page can only register one declaration object at a time
with `registerCommand`, and every finding is flattened into one `ParameterError`
(`CommandDeclaration::build` keeps only `short_summary`; `JsCommandSpec::parse` adds single-error
refusals). This design adds a batch import, accepting a serialized command metadata registry or an
array of declarations, that produces the registry's own report, and makes registering and reporting
one call.

### Problem Example

A page ships its command metadata as YAML in the `specs/command_registry.yaml` format and binds the
functions by name:

```js
const yaml = `
commands:
  - name: join
    arguments:
      - { name: xs, argument_type: string, multiple: true }
      - { name: a,  argument_type: string }
`;
const report = liquers.registerCommands(yaml, { join: (...xs) => xs.join("") });
```

Today: there is no such call; a serialized registry cannot be imported, and `registerCommand` would
report only `Command metadata contains 1 error; first: …` for a declaration.

Should be: the call registers nothing and throws `LiquersError` (`type: "parameter_error"`, message
`Command metadata contains 1 error; first: Argument 'a' is starved by multiple argument 'xs' on
--join`) whose `report` property holds the full report:

```js
{ hasErrors: true, errorCount: 1, warningCount: 0,
  summary: "Command metadata contains 1 error; first: …",
  issues: [{ severity: "error", message: "Argument 'a' is starved by multiple argument 'xs'",
             command: { realm: "", namespace: "", name: "join" } }] }
```

`liquers.validateCommands(yaml, impls)` returns the same report without registering anything.

### Scope and Acceptance Criteria

- **AC-1** Complete report from the registry check
  - WHEN `validateCommands` receives commands with several metadata errors
  - THEN `report.issues` has one entry per finding of `CommandMetadataRegistry::check()` over the
    candidate registry (current commands plus the batch), each with `severity`, `message`, `command`
- **AC-2** One call registers or reports
  - WHEN `registerCommands(input, impls)` finds no error
  - THEN every command in the batch is registered, with at most one environment rebuild, and the
    call returns the report (empty, or warnings only)
- **AC-3** All or nothing, compact error with the report
  - WHEN the batch has any error
  - THEN nothing is registered, and the thrown `parameter_error`'s message equals `report.summary`
    and its `report` property is the report `validateCommands` returns
- **AC-4** Serialized registry input
  - WHEN `input` is a JSON or YAML string in the `CommandMetadataRegistry` format
  - THEN each command binds `impls[name]`; a command with no implementation is an error entry for
    that command; an implementation naming no command is a warning
- **AC-5** Declaration input
  - WHEN `input` is a declaration object or an array of them
  - THEN each goes through the existing declaration pipeline and its findings (including the
    JavaScript-specific refusals: no `run`, empty name, reserved namespace, not an object) join the
    same report; `command` is `null` when no name is known
- **AC-6** Preflight is pure
  - WHEN `validateCommands` is called, before or after `init()`
  - THEN nothing is registered and no environment is built or rebuilt
- **AC-7** Existing behaviour kept
  - WHEN `registerCommand(spec)` is called
  - THEN it behaves as `registerCommands([spec])`: same compact message as today, now also carrying
    `report`
- **AC-8** Core keeps one validation path
  - WHEN `CommandDeclaration::build` refuses metadata
  - THEN its message equals `CommandDeclaration::check().short_summary("Command metadata")`

Non-goals: adopting `EnvironmentBuilder` in `liquers-web`; registering on an explicit
`Environment` instance (it cannot register today); exporting a registry document from the page.

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** not-eligible — new JavaScript functions and a new error property (rule 4), a
  `pub` method in `liquers-core` (rule 4), two crates (rule 6).
- **Leading issue:** None
- **Explanation:** Maintainer decisions of 2026-10-17: a preflight report (accepted), but ergonomic,
  so one call accepts several commands and either registers them or hands back the report; ENVIRON08
  concerns imported (serialized) command metadata, whose registry check produces its own report; the
  entry shape follows the existing `IssueReport` / `Issue` (checked: `Issue::CommandRegistry` carries
  exactly `severity, realm, namespace, name, message`; `Issue::Generic` maps to `command: null`).
- **Open questions:** None. Implementation detail resolved in Phase 2: in the registry form,
  `impls[name]` is a function, or `{ run, state, async }` using the declaration fields' names and
  defaults.

### Design Dependencies

- `overlaps` `WEB-STORE-CONFIG-NOT-APPLIED-THROUGH-ENVIRONMENT-CONFIG` (no design): same environment
  assembly, different change site (`apply_store`); either order.
- `overlaps` `language-guide-web-claims`: different paragraphs of the same guide; the later one
  rebases its History row.

### Consolidated Findings

- `EnvironmentBuilder::validate` is exactly `CommandMetadataRegistry::check()`; running that check
  on a candidate registry gives the guide's report without adopting the builder.
- A batch registers through one rebuild, which also softens `POST-INIT-COMMAND-REGISTRATION` for
  pages that register many commands after the first evaluation.
- Registry-form commands must survive rebuilds like declarations do, so retention gains a second
  entry kind (Phase 2).

## Phase 2: Architecture

### Solution

1. **Core:** `CommandDeclaration::check(&self) -> IssueReport` returns what `build` validates
   (normalisation, deserialization, `metadata.check()`); `build` becomes `check` + `short_summary` +
   deserialize (AC-8).
2. **Web, intake:** `ImportBatch::read(input, impls) -> (Vec<JsCommandSpec>, IssueReport)` in a new
   private module `liquers-web/src/command/import.rs`. A string is parsed as YAML (which accepts
   JSON, as `parse_store_config` does) into `CommandMetadataRegistry`; each command gets a
   `JsCommandSpec::from_metadata(metadata, impl)`. An object or array goes through
   `JsCommandSpec::check(spec)` (the report-producing core of today's `parse`). Duplicate keys
   within the batch are error entries.
3. **Web, validation:** clone the current command metadata registry (pending or shared
   environment), add the batch's metadata, run `CommandMetadataRegistry::check()`, append to the
   intake report.
4. **Web, registration:** `register_commands_on(input, impls)` validates; on errors returns
   `Err(report)`; otherwise registers each spec into the pending environment, or rebuilds once with
   the whole batch, and retains it for replay.
5. **Binding:** `validateCommands(input, impls?)` and `registerCommands(input, impls?)`;
   `registerCommand(spec)` delegates to `registerCommands`. The thrown error is the existing
   `LiquersError` with an added `report` property.

Rejected: `EnvironmentBuilder` adoption (reworks the rebuild path, reports nothing more);
attaching the report only to errors with no preflight (the guide wants it *before*); a stateful
`validationReport()` accessor (stale after a rebuild); a non-throwing `registerCommands` that
returns `{ok, report}` (a page that ignores the result continues silently).

Known-issue preflight: `POST-INIT-COMMAND-REGISTRATION` (accepted) unchanged in kind, improved in
cost; `LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT` concerns JS→Rust only. Nothing blocks.

### Changes

- `liquers-core/src/command_declaration.rs`: `pub fn check(&self) -> IssueReport`; `build` over it.
- `liquers-web/src/command/spec.rs`: `pub(crate) fn check(spec) -> (Option<JsCommandSpec>,
  IssueReport)`, `pub(crate) fn from_metadata(metadata: CommandMetadata, implementation: &JsValue)
  -> Result<JsCommandSpec, Issue>`; `parse` delegates to `check`.
- `liquers-web/src/command/import.rs` (new, private): batch intake as above.
- `liquers-web/src/environment.rs`: `validate_commands_on`, `register_commands_on`;
  `REGISTERED_SPECS` becomes `Vec<Retained>` with `enum Retained { Declaration(JsValue),
  Metadata(CommandMetadata, JsValue) }` (private), replayed by `rebuild_with` in order.
- `liquers-web/src/error.rs`: `pub(crate) fn issue_report_to_js(&IssueReport) -> JsValue` and
  attaching `report` to the `LiquersError` built for a refused batch.
- `liquers-web/src/lib.rs`: the two bindings; `registerCommand` delegates.
- `liquers-web/src/typescript.rs`: `LiquersIssue`, `LiquersIssueReport`, `CommandImplementation =
  Function | { run: Function; state?: string; async?: boolean }`, `LiquersError.report?`.
- `scripts/check-stubs.sh`, `tests/stubs/valid_usage.ts`.
- Sync only; no command, registry export or core signature change besides `check`.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| `registerCommand` messages change | AC-7 test plus existing `commands_COMMAND` / `commands_DECLARATION` message assertions | high |
| A replayed registry-form command differs from first registration | Replay uses the retained `CommandMetadata` and function, through `from_metadata` | high |
| Registry check flags pre-existing commands | Built-ins pass `check()` today (`EnvironmentBuilder` builds them); a test asserts an empty report for a valid batch | high |
| Stub drift | `check-stubs.sh` | high |

## Phase 3: Examples and Tests

### Examples

The Problem Example (AC-1, AC-3, AC-4). A valid batch `registerCommands([{name: "a", run: () => 1},
{name: "b", run: () => 2}])` returns an empty report and both commands evaluate (AC-2).
`validateCommands(42)` gives one entry with `command: null` (AC-5).

### Tests

Core, `liquers-core/src/command_declaration.rs` `mod tests`:

- `check01_report_lists_every_metadata_issue`: AC-1, AC-8
- `check02_build_error_is_the_report_summary`: AC-8
- `check03_deserialization_failure_is_one_issue`: AC-1

Web, `liquers-web/tests/environment_ENVIRON.rs` (Node loop):

- `environ08_serialized_registry_reports_every_issue` — the Problem Example: AC-1, AC-4
- `environ08_refused_batch_registers_nothing_and_carries_the_report`: AC-3
- `environ08_valid_batch_registers_with_one_rebuild` — after `init()` and one evaluation, a
  three-command batch; all evaluate; the asset cache is discarded once: AC-2
- `environ08_missing_and_unknown_implementations` — error for the first, warning for the second: AC-4
- `environ08_declaration_refusals_are_report_entries`: AC-5
- `environ08_validation_is_pure` — before and after `init()`: AC-6
- `environ08_register_command_is_a_batch_of_one` — message unchanged, `report` present: AC-7
- `environ08_registry_form_commands_survive_a_rebuild` — import, evaluate, register another
  command, the imported one still evaluates: AC-2, AC-4
- `environ07_documented_operations_are_callable` (extended): AC-6

Run: `cargo test -p liquers-core --lib command_declaration`;
`cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles --test environment_ENVIRON`;
`./liquers-web/scripts/check-stubs.sh`.

## Phase 4: Implementation Plan

### Steps

- [x] 1. `liquers-core/src/command_declaration.rs` `CommandDeclaration::check` — add, rewrite `build` over it, tests `check01`-`check03` — `cargo test -p liquers-core --lib command_declaration`. Rollback: revert the file.
- [x] 2. `liquers-web/src/command/spec.rs` — split `parse` into `check` + wrapper; add `from_metadata` — existing `commands_COMMAND` / `commands_DECLARATION` suites pass unchanged.
- [x] 3. `liquers-web/src/command/import.rs` and `environment.rs` `validate_commands_on` — intake plus candidate-registry check — `environ08_serialized_registry_reports_every_issue`, `environ08_missing_and_unknown_implementations`, `environ08_declaration_refusals_are_report_entries`, `environ08_validation_is_pure`.
- [x] 4. `environment.rs` `register_commands_on`, `Retained`, `rebuild_with` — batch registration and replay — `environ08_valid_batch_registers_with_one_rebuild`, `environ08_registry_form_commands_survive_a_rebuild`.
- [x] 5. `error.rs`, `lib.rs` — `issue_report_to_js`, `report` on the error, the two bindings, `registerCommand` delegating — `environ08_refused_batch_registers_nothing_and_carries_the_report`, `environ08_register_command_is_a_batch_of_one`, then the full Node loop.
- [x] 6. `typescript.rs`, `check-stubs.sh`, `tests/stubs/valid_usage.ts` — `./liquers-web/examples-web/quickstart/build.sh && ./liquers-web/scripts/check-stubs.sh`.
- [x] 7. Documents — `COMMAND_DECLARATION.md` (`check`), the guide's ENVIRON paragraph (an integration without `EnvironmentBuilder` runs the registry check on its import path; History row, `reviewed:`), `liquers-web/README.md` (registering commands in bulk); close the source issue — `python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-core --lib`; the full Node loop; `bash scripts/check-build-matrix.sh`
wasm32 rows; `python3 scripts/docs_index.py --check`; a final diff review (no `EnvironmentBuilder`
adoption, no command or registry-export change).

## Implementation notes (2026-10-18)

- **Refusal messages (AC-7).** A metadata error keeps today's message exactly. A JavaScript-specific
  refusal (no `run`, empty name, reserved namespace, not an object) is now a report entry like any
  other, so its thrown message is the report summary: the old text follows
  `Command metadata contains 1 error; first:`. Every existing test asserts with `contains`, and
  all pass.
- **Registry check scope (AC-1).** `CommandMetadataRegistry::check()` runs over a registry holding
  the batch only. The check is per command, so adding the commands already registered changes no
  finding about the batch and would repeat their own findings in every report.
- **Registry input (AC-4).** Read into a local `ImportedRegistry` (`commands`, `global_enums`), since
  `CommandMetadataRegistry` requires `default_namespaces` and a hand-written document need not
  carry it. Global enums are not imported, and a document defining them gets a warning.
- **Declarations and `impls`.** A declaration without `run` takes `impls[name]`, so one
  implementations object serves both input forms.
- **Browser tests.** The Node loop covers everything; there is no browser-only test.
