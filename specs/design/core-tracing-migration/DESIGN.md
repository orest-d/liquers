---
id: CORE-TRACING-MIGRATION-DESIGN
kind: design
title: Route library diagnostics through tracing, with binaries choosing the subscriber
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/error, build]
issues: [CORE-TRACING-MIGRATION]
affects_docs: [CLAUDE.md, specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md, specs/reference/ASSETS.md, specs/reference/VALUE_TYPE_SYSTEM.md, specs/reference/RECORD_STREAMS.md, specs/guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md]
created: 2026-10-12
---
# Route library diagnostics through tracing, with binaries choosing the subscriber

Bulk design: Phases 1-4 written, not approved, nothing implemented.

## Phase 1: High-Level Design

### Purpose

Library crates write about 95 unstructured `eprintln!` lines, chatter and swallowed failures alike,
with no level, module filter or routing. Replace them with `tracing` events and let each binary or
integration install the subscriber. Panic hygiene, the other half of WP-6, stays with
`LIBRARY-CODE-USES-UNWRAP-AND-EXPECT`.

### Problem Example

Evaluating `-R/data/a.txt` from a store that holds it prints, unconditionally, the fast-track lines
of `liquers-core/src/assets.rs` `AssetData::try_fast_track` (`Trying fast track for asset 7`, `Asset
7 is a resource with key data/a.txt`, `... loaded successfully`) and `interpreter.rs`'s `Applying
step 1/1: GetResource(...)`. A real failure, `Metadata save failed for key data/a.txt: ...`, looks
the same, nothing can hide one and keep the other, and concurrent evaluations interleave anonymously.

Should be: with the subscriber a binary installs (default `warn`) only the failure appears, as
`WARN liquers_core::assets` with a `key` field; `RUST_LOG=liquers_core::assets=debug` brings the
fast-track events back for that module only.

### Scope and Acceptance Criteria

- **AC-1** No print macros in library code
  - WHEN the lib targets of `liquers-core`, `-store`, `-records`, `-lib`, `-axum`, `-py`, `-web` are
    linted with `clippy::print_stdout` and `clippy::print_stderr` denied
  - THEN there are no findings (bins, examples and tests are not covered)
- **AC-2** Context logging becomes events
  - WHEN a command calls `context.warning("disk almost full")` under a capturing subscriber
  - THEN one `WARN` event with that message and an `asset` field is recorded, the metadata log
    still has the entry, and `debug`/`info`/`error` map to their levels the same way
- **AC-3** Chatter is filterable by level and module
  - WHEN `-R/data/a.txt` is evaluated from a memory store with the filter at `warn`
  - THEN no event is recorded; at `liquers_core::assets=debug` the fast-track events are, with that
    target
- **AC-4** A swallowed failure is a warning
  - WHEN the listing refresh after a write fails and the write stands
  - THEN a `WARN` event names the key
- **AC-5** CLI stdout stays machine-readable
  - WHEN `liquers-validate` runs with `RUST_LOG=trace`
  - THEN stdout is exactly the JSON envelope and every event is on stderr
- **AC-6** Python sees warnings by default
  - WHEN the `liquers_py` module initializes and no global subscriber exists
  - THEN a stderr subscriber at `warn` (`RUST_LOG` overrides) is installed; a second call is a no-op
- **AC-7** The browser receives warnings
  - WHEN `liquers-web` `init()` runs
  - THEN a global subscriber sends `WARN`/`ERROR` events to the console; a second `init()` is fine

Non-goals: spans, `unwrap`/`expect`, the `LogEntry` asset log, the `IssueReport` model, message
wording. Crossing crates is `L` by §4.5; compact because each change is mechanical.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — new dependencies and seven crates (rules 4 and 6), open
  decisions (rule 5).
- **Leading issue:** Open design - facade and whether the dependency is optional.
- **Explanation:** Every site is inventoried and mechanical; Phases 3-4 follow the recommended
  answers, which decide what a library user sees.
- **Open questions:**
  1. **Open design - facade:** `tracing` or `log`. Recommended `tracing`: `liquers-axum` already
     uses it, only it can carry spans for concurrent evaluations, and its `log` feature can bridge.
  2. **Open design - mandatory dependency:** recommended unconditional in core, records, lib
     (`default-features = false, features = ["std"]`): small, wasm-safe, one atomic load per
     disabled event; a feature means a shim at ~95 sites and a build-matrix row.
  3. **Open design - silence without a subscriber:** a library user who installs none no longer sees
     warnings. Recommended: accept, as every `tracing` library does, and document it; the one
     deliberate report, `IssueReport::emit`, keeps a guaranteed sink on wasm (Phase 2).
  4. **Proposed resolution - who installs:** the three CLI bins (stderr, `RUST_LOG`, default
     `warn`), `liquers-py` module init (same, `try_init`), `liquers-web` `init()` (console, `WARN`+).
     The `liquers-axum` examples already do; the crate has no binary. `pyo3-log` is deferred.
  5. **Proposed resolution - spans:** later; a feature is filed when this lands (step 10).
  6. **Proposed resolution - unwrap/expect:** out of scope, owned by
     `LIBRARY-CODE-USES-UNWRAP-AND-EXPECT` (`L`); no new issue.

### Design Dependencies

- `axum-assets-endpoints`, **overlaps**: linked from `LIBRARY-CODE-USES-UNWRAP-AND-EXPECT`, the
  other half of WP-6. Independent sites, no order.
- `variadic-metadata-tail-check`, **overlaps**: owns `ISSUE-REPORT-PLAN-AND-METADATA-LOGGING`, which
  may reroute `IssueReport`; this design changes only `emit_report`'s native branch.
- `DOC_04`'s gap row "Context convenience logging always writes to stderr" (T4, a document row, not
  an issue) and `wp2-terminal-outcome`'s interim `eprintln!` are resolved here.

### Consolidated Findings

- **At HEAD:** non-test library `eprintln!` — core 71 (`assets.rs` 58), lib 22 (`egui/` 16, `ui/`
  6), axum 1, records 1, store/web/macro/py 0. The stdout half is not quite done:
  `liquers-py/src/commands.rs` `println!("pycall")`. Non-test `unwrap()`/`expect(`: core 38/6 (and 9
  `panic!`), lib 36/1, records 6/3, py 10, macro 2, store and axum 0.
- Only `liquers-axum` has `tracing`/`tracing-subscriber` (both normal dependencies); no crate has
  `log`.
- `tracing-subscriber`'s `fmt` writes to **stdout** by default: every subscriber here sets
  `with_writer(std::io::stderr)` (AC-5).
- On `wasm32-unknown-unknown` `eprintln!` is discarded, so AC-7 is new behaviour. `fmt`'s timer
  panics there, so the web subscriber is a timestamp-free console layer.
- Filed in passing: `RESOURCE-STEP-VOLATILITY-IS-NEVER-CHECKED`.

## Phase 2: Architecture

### Solution

Each library crate depends on `tracing` and replaces each `eprintln!` with the macro of its level,
interpolated values as fields (`tracing::warn!(key = %key, error = %error, "metadata save failed")`).
Targets stay module paths. Binaries and integrations install the subscriber; libraries never do. A
lint attribute keeps print macros out.

Rejected: `log` (no spans); a feature flag (shim everywhere); a library `init_logging()` (takes the
choice from the binary); `eprintln!` behind `cfg!(debug_assertions)` (unfilterable).

**Level rule** (per-site assignment is implementation detail):

| Level | Sites |
|---|---|
| `error!` | A record is lost: `assets.rs` "Failed to add log entry", `type_system.rs` `TypeRegistry::from_value_type`, axum `websocket.rs` serialization, `Context::error` |
| `warn!` | Failure reported then ignored: metadata save, store write kept in memory, listing refresh, delete notices, records `read_table`, `ui/runner.rs` root query errors, egui failures, missing `show_in_egui`, `Context::warning` |
| `info!` | `Context::info` |
| `debug!` | Lifecycle: fast track, recipe evaluation, "Applying step", the late-message drop (guard removed), egui loading, `text_editor`, `Context::debug` |
| `trace!` | Chatter: "Received message", resource-directory lines, the volatility TODO lines, "Rendering child", egui sort state, `pycall` |

**Known-issue preflight.** `LIBRARY-CODE-USES-UNWRAP-AND-EXPECT` (P2) and
`ISSUE-REPORT-PLAN-AND-METADATA-LOGGING` (P3) are independent; neither blocks. Namespaces: none.

### Changes

**Dependencies.** `tracing = { version = "0.1", default-features = false, features = ["std"] }` in
core, records, lib, py, web. `tracing-subscriber = "0.3"`: optional in core and lib via
`cli = ["dep:clap", "dep:tracing-subscriber"]` (`fmt`, `env-filter`); normal in py (`fmt`,
`env-filter`) and web (`default-features = false`, `registry`, `std`); a core dev-dependency for
tests. `liquers-axum` moves it to `[dev-dependencies]`. MSRV 1.95 exceeds both crates' minimum.

**Sites.** `liquers-core/src/{assets,interpreter,context,recipes,type_system}.rs`,
`liquers-lib/src/egui/{widgets,dataframe,commands}.rs`, `liquers-lib/src/ui/{runner,element,
app_state}.rs`, `ui/widgets/ui_spec_element.rs`, `liquers-records/src/formats/mod.rs`,
`liquers-axum/src/assets/websocket.rs`, `liquers-py/src/commands.rs`. `Context::{debug,info,
warning,error}` keep signatures and `add_log_entry`; the print becomes
`tracing::<level>!(asset = self.assetref.id(), "{}", message)`. The commented-out print in
`query.rs` `Query::all_predecessors` goes. `egui/commands.rs` `text_editor` (`version: auto`) gets a
new `impl_version`, so stored widget results recompute once.

**`IssueReport::emit`:** native `emit_report` becomes `error!`/`warn!` by worst severity; the wasm
branch keeps `web_sys::console::error_1`, so a wasm build without `liquers-web` still shows it.

**Subscribers** (private, no public API):

```rust
fn init_diagnostics();                  // each bin's main: fmt().with_writer(std::io::stderr),
                                        // EnvFilter from RUST_LOG else "warn", try_init
fn install_stderr_subscriber() -> bool; // liquers-py/src/lib.rs, first in #[pymodule]
pub(crate) fn install_console_subscriber() -> bool; // liquers-web/src/diagnostics.rs, first in init()
struct ConsoleLayer;                    // impl Layer<S>: target + message to console warn_1/error_1
```

**Lint:** `#![cfg_attr(not(test), deny(clippy::print_stdout, clippy::print_stderr))]` in the seven
`lib.rs` files; with no CI, the attribute is the enforcement.

**Documents:** `CLAUDE.md` Diagnostic Output and Constraints (libraries emit events, binaries
install a stderr subscriber, none means silence); `DOC_04` §Logging and gap row; "reported on stderr"
becomes "a `warn` event" in the other `affects_docs` and `assets.rs` doc comments.

### Risks

| Risk | Containment | Certainty |
|---|---|---|
| Users lose warnings with no subscriber | Documented; bins, py, web install one | high |
| `fmt` writes stdout | `with_writer(stderr)`; AC-5 test | high |
| Thread-local test subscriber misses worker-thread events | Current-thread `#[tokio::test]`; else one global capturing subscriber, tests serialized | medium |
| `deny` breaks an unlinted feature configuration | Clippy default and `--no-default-features` | high |

No test asserts stderr content and no public signature changes. Each step reverts alone.

## Phase 3: Examples and Tests

### Examples

The Problem Example (AC-3), and `RUST_LOG=trace liquers-validate -- '-R/data/a.txt' | python3 -m
json.tool` succeeding with events on stderr (AC-5).

### Tests

A capture layer in `liquers-core/tests/common/` records `(level, target, message, fields)` under
`tracing::subscriber::set_default`.

- `liquers-core/tests/tracing_diagnostics.rs` (new): `context_warning_is_a_warn_event_and_a_log_entry`
  (AC-2), `context_levels_match_their_methods` (AC-2), `resource_evaluation_is_silent_at_warn`
  (AC-3), `resource_evaluation_emits_fast_track_events_at_debug` (AC-3)
- `dependency_audit_integration.rs` `listdir_error_after_write_is_logged_not_fatal`, extended: AC-4
- `liquers-core/tests/validate_cli.rs` (new, `#![cfg(feature = "cli")]`)
  `stdout_is_only_the_envelope_at_trace_level`, via `env!("CARGO_BIN_EXE_liquers-validate")`: AC-5
- `liquers-py/src/lib.rs` `tests::second_install_is_a_no_op`: AC-6
- `liquers-web/tests/environment_ENVIRON.rs` `environ08_init_installs_a_console_subscriber`
  (`tracing::dispatcher::has_been_set()` after two `init()`): AC-7
- Static: `cargo clippy --lib` on the seven crates with the attribute: AC-1

Run: the proof commands of Phase 4 steps 1, 2, 6, 7 and 8.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-core` — `tracing`; `assets.rs`, `interpreter.rs`, `context.rs`, `recipes.rs`, `type_system.rs`, native `emit_report`; drop the `query.rs` comment; capture layer, `tracing_diagnostics.rs`, extend `listdir_error_after_write_is_logged_not_fatal` — `cargo test -p liquers-core --test tracing_diagnostics --test dependency_audit_integration`. Rollback: revert the crate.
- [ ] 2. `liquers-core` `cli` + `init_diagnostics` in `liquers_validate.rs`, `generate_entities.rs`; `validate_cli.rs` — `CARGO_INCREMENTAL=0 cargo test -p liquers-core --features cli --test validate_cli`
- [ ] 3. `liquers-records` — `formats/mod.rs` — `cargo test -p liquers-records --all-features --lib --tests`
- [ ] 4. `liquers-lib` — `egui/`, `ui/` sites; `cli` + `export_command_registry.rs` `init_diagnostics`; regenerate `specs/command_registry.yaml` with a CHANGELOG line — `cargo test -p liquers-lib --lib --tests` (includes `registry_export`)
- [ ] 5. `liquers-axum` — `websocket.rs`; `tracing-subscriber` to dev-dependencies — `cargo test -p liquers-axum --lib --tests && cargo check -p liquers-axum --examples`
- [ ] 6. `liquers-py` — drop `println!("pycall")`; `install_stderr_subscriber`; its test — `cargo test -p liquers-py --lib`, else `cargo check -p liquers-py` and `pytest liquers-py/tests`
- [ ] 7. `liquers-web` — `diagnostics.rs`, call in `init()`; `environ08_…` — after `cargo clean`, `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles`. Rollback: drop the module.
- [ ] 8. Lint attribute in seven `lib.rs` — `cargo clippy -p liquers-core -p liquers-store -p liquers-records -p liquers-lib -p liquers-axum -p liquers-py --lib`, `liquers-lib` again with `--no-default-features`, `liquers-web` with `--target wasm32-unknown-unknown`
- [ ] 9. Documents in `affects_docs` and `assets.rs` doc comments, with History rows and `reviewed:` — `python3 scripts/docs_index.py --check`
- [ ] 10. Close `CORE-TRACING-MIGRATION` with a note; file the spans feature; refresh counts in `LIBRARY-CODE-USES-UNWRAP-AND-EXPECT` — `python3 scripts/docs_index.py && python3 scripts/docs_index.py --check`

### Validation

`bash scripts/check-build-matrix.sh`; `cargo test -p liquers-lib --lib --tests`; a `grep` for
`eprintln!`/`println!` in each `src/` outside `bin/` and test modules finds nothing;
`specs/command_registry.yaml` differs only in `text_editor`'s `impl_version` and the CHANGELOG line;
diff review for subscribers in library code and wording changes.
