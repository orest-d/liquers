---
id: DIAGNOSTICS-CARRY-THEIR-ORIGIN
kind: design
title: Log entries record their source, and plan diagnostics become typed issues
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/value, core/plan, core/validate]
issues: [LOG-ENTRY-HAS-NO-SOURCE, ISSUE-REPORT-PLAN-AND-METADATA-LOGGING]
merged: 2026-10-20
affects_docs: [specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md, specs/reference/RECIPES_AND_PLANS.md, specs/reference/AGENT_MEMORY_SERVICE.md]
created: 2026-10-20
---
# Log entries record their source, and plan diagnostics become typed issues

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-20: Phases 1-4 written, not
approved, nothing implemented. Two sources, strongly overlapping (`overlap.md`): **T2** — the
plan-diagnostic-to-log conversion `ISSUE-REPORT-PLAN-AND-METADATA-LOGGING` asks for must set the
source vocabulary `LOG-ENTRY-HAS-NO-SOURCE` defines; **T3** — both change
`interpreter::log_init_steps`, the one place planning diagnostics become log entries. Leading
source: `LOG-ENTRY-HAS-NO-SOURCE` (concrete, with a consumer). Neither is eligible for automatic
fixing (E1 n/a); merged size `M` (E2 n/a). Origin designs `agent-memory-mvp` and
`variadic-metadata-tail-check` are complete (E4).

## Phase 1: High-Level Design

### Purpose

A consumer of an asset's log cannot tell what the command said from what the planner and engine
said, and plan diagnostics cannot join the typed `IssueReport` that environment validation
produces. Give every log entry its source and give plan diagnostics typed `Issue`s, so a UI, CLI or
agent selects and routes diagnostics without matching message text.

### Problem Example

Evaluate a recipe with a `cwd` and a dependency whose command calls `context.info("3 rows")`. The
asset log holds `Recipe set CWD to 'notes'`, `Dependency detected: …`, `Predecessor boundary …`
and `3 rows`, all `LogEntryKind::Info`, none with a source (`liquers-core/src/metadata.rs`
`LogEntry`, verified at HEAD 18732dd). `liquers-mcp/src/server.rs` therefore drops engine lines by
prefix (`ENGINE_NOTE_PREFIXES`), and a reworded planner message leaks into tool results. Should:
the entries carry `source: plan`, `plan`, `plan`, `command`; MCP keeps `source == command`.

Separately, `Plan::error` and `Step::Warning`s reach callers as an `Error` and strings
(`validate::collect_warnings` re-classifies them); `IssueReport` (`liquers-core/src/issue_report.rs`)
has only `Generic` and `CommandRegistry` variants. Should: `plan.issue_report()` returns typed
`Issue::Plan` entries that compose with `EnvironmentBuilder::validation_report()`.

### Scope and Acceptance Criteria

- **AC-1** Command notes are marked
  WHEN a command calls `Context::debug/info/warning/error` or `add_log_entry` with an entry without a source (Rust, Python or JavaScript command)
  THEN the stored entry has `source: command`
- **AC-2** Planning diagnostics are marked
  WHEN `init_steps` reach the log (`log_init_steps`) or a `Step::Info/Warning/Error` executes
  THEN the entry has `source: plan`
- **AC-3** Engine lines are marked
  WHEN the interpreter, the asset manager or a resource read writes a log entry (`Relative key/query has no CWD…`, `Getting resource`, `Not cached for reuse: …`, expiry lines)
  THEN the entry has `source: engine`
- **AC-4** Old records load
  WHEN metadata serialized before this change is read
  THEN it loads with `source` absent (`None`), and new entries serialize `source` only when set
- **AC-5** MCP selects by source
  WHEN `liquers-mcp` returns a command's notes
  THEN it keeps `source == command` entries, using the prefix list only for entries with no source
- **AC-6** Plan issues are typed
  WHEN `Plan::issue_report()` runs on a plan with `error`, init warnings and a nested plan's error
  THEN it returns `Issue::Plan` items with severity, message, the query and position when known, the `Plan::error` echo in `init_steps` reported once
- **AC-7** Reports compose and log
  WHEN a plan report is appended to an environment validation report, and `Issue::to_log_entry` is applied
  THEN counts and summaries cover both; the log entry keeps severity, query and position, with `source: plan` for `Issue::Plan` and `engine` otherwise
- **AC-8** Validator output unchanged
  WHEN `liquers-validate` runs on any query
  THEN its JSON envelope is byte-identical to today's (`collect_warnings` derived from the report)

Non-goals: converting a log into an `IssueReport`; changing which messages are written or their
kind; a Python/JavaScript accessor for `source` (the field rides along in serialized metadata);
liquers-validate envelope changes.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — a new `pub` enum and field on `LogEntry`, a new `Issue`
  variant, serialized metadata gains a field (rules 3, 4).
- **Leading issue:** question 1 (source granularity).
- **Explanation:** a working plan exists in `liquers-core` plus one-arm matches in `liquers-web` and
  `liquers-py` and the `liquers-mcp` consumer; the remaining choices change a persisted vocabulary.
- **Open questions:**
  1. **Open design - granularity:** `Command | Plan | Engine` (recommended: what consumers
     distinguish today) versus finer (`Interpreter`, `Assets`, `Store`), which every engine site
     would have to choose correctly.
  2. **Open design - absent source:** `Option<LogSource>`, `None` = written before sources existed
     (recommended: old records stay truthful) versus a default variant (`Command`), which would
     relabel old engine lines as command notes.
  3. **Proposed resolution - who marks `Command`:** `Context::add_log_entry` fills a missing source
     with `Command`; core's engine call sites set `Plan`/`Engine` explicitly. A command that sets
     another source deliberately keeps it.
  4. **Proposed resolution - `Issue::Plan` fields:** `severity, query: Option<String>,
     position: Option<Position>, message` (encoded query text keeps `Issue: Eq` simple).

### Design Dependencies

- `cwd-default-link-silent-root` — **overlaps**: decides whether the root-fallback warning is
  emitted for a working-directory default link; this design only labels it `engine`. No order.
- `web-environ-validation-report` (complete) — its JS shape is flat and declared, so a new variant
  maps to `command: null` without a shape change; E4.
- `query-validation` (complete) — `collect_warnings` stays its public API; E4.

### Consolidated Findings

- Engine call sites that go through the command-facing `Context` API (`interpreter.rs` lines 64,
  331, 630-652, 757-767, 807, 943-957; `context.rs` 1243, 1265) must set a source explicitly, or
  AC-1's default would label them `command`. Direct record writes in `assets.rs` (about 25
  `metadata.add_log_entry(LogEntry::…)` sites) set `Engine`.
- `Issue` gains a variant: `liquers-web/src/error.rs` `issue_report_to_js` and `liquers-py/src/environment.rs`
  `From<&CoreIssue> for Issue` match exhaustively and each need one arm (no command key); neither
  binding's shape changes. This is the only cross-crate edit besides the MCP consumer; sized `M`
  because those edits are compile-forced single arms, though §4.5 read literally says `L`.
- `ENGINE_NOTE_PREFIXES` stays for legacy entries only, so new planner messages need no list edit.

## Phase 2: Architecture

### Solution

`liquers-core/src/metadata.rs`:

```rust
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogSource { Command, Plan, Engine }
// LogEntry:
#[serde(default, skip_serializing_if = "Option::is_none")]
pub source: Option<LogSource>,
impl LogEntry { pub fn with_source(self, source: LogSource) -> Self; }
```

`Default for LogEntry` sets `source: None`. `context.rs` `Context::add_log_entry`: a `None` source
becomes `Some(Command)`; the crate-private engine calls (`interpreter.rs`, `context.rs` relative-CWD
warning) build entries with `.with_source(Engine)` and call `add_log_entry`, and
`Step::Info/Warning/Error` execution and `log_init_steps` use `Plan`. `assets.rs` direct writes
use `.with_source(Engine)` (a private helper `engine_entry(kind, message)` keeps call sites short).

`liquers-core/src/issue_report.rs`:

```rust
Issue::Plan { severity: IssueSeverity, query: Option<String>, position: Option<Position>, message: String }
impl Issue { pub fn plan(severity, query: Option<&Query>, position: Option<Position>, message) -> Self;
             pub fn to_log_entry(&self) -> LogEntry; }
impl IssueReport { pub fn to_log_entries(&self) -> Vec<LogEntry>; }
```

`liquers-core/src/plan.rs`: `impl Plan { pub fn issue_report(&self) -> IssueReport }` — the walk
`collect_warnings` does today (error, `init_steps` then `steps`, echo skipped, nested plans
recursed with a "nested plan: " prefix), plus `Info` steps as `IssueSeverity::Info`.
`validate/report.rs` `collect_warnings` becomes a projection of it (Info dropped, `WarningSource`
from the variant and severity) so AC-8 holds. `interpreter.rs` `log_init_steps` becomes
`plan.issue_report()` restricted to `init_steps` → `to_log_entries()` (a private
`Plan::init_issue_report`), keeping order and the single append.

`liquers-mcp/src/server.rs` `notes_of`: keep `Some(Command)`; for `None` apply
`is_engine_note` (legacy). `liquers-web/src/error.rs`: `Issue::Plan { .. } => JsValue::NULL`; `liquers-py/src/environment.rs`
`From<&CoreIssue>`: `CoreIssue::Plan { .. } => None`.

Rejected: downgrading engine lines to `Debug` (loses warnings consumers want, and the relative-CWD
warning is a `Warning`); a separate engine log (two places to read, a metadata format change).

Known-issue preflight: `CWD-DEFAULT-LINK-WARNS-WITHOUT-WORKING-DIRECTORY` (open design) changes when
one engine warning is emitted; independent. No command namespace.

### Changes

Files: `liquers-core/src/{metadata,context,interpreter,assets,issue_report,plan}.rs`,
`liquers-core/src/validate/report.rs`; `liquers-mcp/src/server.rs`; `liquers-web/src/error.rs`;
`liquers-py/src/environment.rs`.
Errors: none new (pure data). Sync except the existing async log paths. No command or registry
change. Documents: DOC-04 §Logging (source rule), DOC-08 §Plan fields (`issue_report`),
`AGENT_MEMORY_SERVICE.md` (notes selected by source); History rows, `reviewed:`.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| An engine site missed, so it reads as `command` | grep every `add_log_entry`/`context.info|warning|error` in core; T-3 asserts the known engine lines | medium |
| `collect_warnings` output drifts | AC-8 test compares envelopes on the existing validate fixtures | high |
| Python/JS see an extra `source` key in metadata dicts | additive; documented | high |
| `Issue` variant breaks an outside exhaustive match | `liquers-web` and `liquers-py` (both fixed here; grepped) | high |

## Phase 3: Examples and Tests

### Examples

MCP: a `read` tool on a recipe with `cwd` returns only `3 rows` as notes; the planner lines remain
in the asset's log with `source: plan`. Application:

```rust
let mut report = builder.validation_report().clone();
report.append(plan.issue_report());
if report.has_errors() { /* render report, or route report.to_log_entries() */ }
```

### Tests

- `liquers-core/src/metadata.rs`: `log_entry_source_round_trips_and_old_records_load` (AC-4).
- `liquers-core/src/context.rs`: `command_log_calls_are_marked_command` (AC-1).
- `liquers-core/tests/log_sources.rs` (new; environment as `tests/async_hellow_world.rs`):
  `recipe_log_separates_command_plan_and_engine` — recipe with `cwd`, a dependency and a command
  note; assert each entry's source (AC-1, AC-2, AC-3).
- `liquers-core/src/issue_report.rs`: `plan_issue_to_log_entry_keeps_fields` (AC-7).
- `liquers-core/src/plan.rs`: `issue_report_types_plan_diagnostics` — plan with `set_error`, an init
  warning, a nested erroring plan (AC-6); `plan_and_registry_reports_compose` (AC-7).
- `liquers-core/tests/validate_integration.rs` (existing): unchanged assertions pass (AC-8).
- `liquers-mcp/src/server.rs`: `notes_select_command_entries` and `legacy_entries_use_prefixes` (AC-5).
- Run: `cargo test -p liquers-core --lib --tests`, `cargo test -p liquers-mcp`; `liquers-web` Node
  loop after `cargo clean` for the match arm.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `metadata.rs` `LogSource`, `LogEntry::source`, `with_source` — round-trip test — `cargo test -p liquers-core --lib metadata`. Rollback: revert (additive).
- [ ] 2. `context.rs` `add_log_entry` default; engine sites in `context.rs`, `interpreter.rs` (Engine/Plan), `assets.rs` (Engine, helper) — `command_log_calls_are_marked_command`, `tests/log_sources.rs` — `cargo test -p liquers-core --test log_sources`. Depends on 1.
- [ ] 3. `issue_report.rs` `Issue::Plan`, `to_log_entry`, `to_log_entries`; `liquers-web/src/error.rs` and `liquers-py/src/environment.rs` arms — unit tests; `cargo check -p liquers-web --target wasm32-unknown-unknown`; `bash scripts/test-liquers-py.sh`. Depends on 1.
- [ ] 4. `plan.rs` `issue_report`, `init_issue_report`; `validate/report.rs` `collect_warnings` as a projection; `interpreter.rs` `log_init_steps` via the report — `cargo test -p liquers-core --lib plan validate`, `--test validate_integration`. Depends on 2, 3.
- [ ] 5. `liquers-mcp/src/server.rs` `notes_of` — `cargo test -p liquers-mcp`, then `cargo test -p liquers-agent`. Depends on 2.
- [ ] 6. Documents (DOC-04, DOC-08, AGENT_MEMORY_SERVICE.md; History, `reviewed:`); close both sources — `python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-lib --lib --tests` (UI and
commands read logs); `cargo test -p liquers-mcp`; the `liquers-web` conformance loop after
`cargo clean`; `bash scripts/check-build-matrix.sh` wasm rows (the web arm).
