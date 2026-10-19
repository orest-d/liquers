---
id: STORE-CONFORMANCE-CHECK-TOOL
kind: design
title: liquers-store-check, the conformance suite as a command
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [store/backends, core/store]
issues: [STORE-CONFORMANCE-VALIDATION-TOOL]
created: 2026-10-19
---
# liquers-store-check, the conformance suite as a command

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written, not approved, nothing
implemented. Keeps the decisions `design/store-conformance-suite/` recorded in the issue, and
answers the one it left open.

## Phase 1: High-Level Design

### Purpose

Run `liquers_core::store_conformance` against a store described by a configuration document, or
against a throwaway store of a named type, from the command line — without writing a fixture, a
test file or a `cargo test` invocation.

### Problem Example

A deployment mounts `data` with `type: fs, config: { root: /srv/data }` in `stores.yaml`. To check
that mount today an operator copies what `liquers-store/tests/store_conformance_CONF.rs`
`a3_opendal_read_only_report` does: write Rust that builds the store with `StoreRouterBuilder`,
wraps it in a `GenericFixture` with hand-written `StoreCapabilities`, calls `run_all` and asserts.
Nothing runs the contract against what the document builds. Should be:

```text
$ liquers-store-check --config stores.yaml --claims data-claims.yaml --store data
# the document as parsed, with ${VAR} unexpanded; then the report at level read-only:
# N rules ran, 0 failed; M not run, needing create-only or scratch; residue: none
$ echo $?   # 0 conformant · 1 non-conformant · 2 invocation or setup failure
```

and `liquers-store-check --scratch filesystem` runs the whole suite against a fresh temporary
directory, then removes it.

### Scope and Acceptance Criteria

- **AC-1** Scratch run
  WHEN `--scratch memory`, `--scratch filesystem` or `--scratch fs` is given
  THEN every rule runs at level `scratch` against a fresh store, the exit code is 0, and any temporary root is removed
- **AC-2** A document defaults to read-only
  WHEN `--config` and `--claims` are given without `--level`
  THEN only read-only rules run, the backing directory is byte-identical afterwards, and the report names how many rules did not run and the level that would run them
- **AC-3** Claims are complete or refused
  WHEN `--config` comes without `--claims`, or the claims omit a capability field or carry an unknown key
  THEN the exit code is 2, stdout is empty and stderr names the missing or unknown field
- **AC-4** A defect is exit 1 with its location
  WHEN a rule fails or errors
  THEN the exit code is 1 and the report names the rule, its contract section and the subject keys
- **AC-5** Residue before the summary
  WHEN the level is `create-only`
  THEN the residue list is printed before the summary and equals the created set
- **AC-6** Setup failure shows the document
  WHEN the document parses but its stores fail to build, or a `--scratch` type has no scratch recipe
  THEN the exit code is 2 and, for a document, its parsed form is printed first
- **AC-7** Rule selection
  WHEN `--rule` is given one or more times
  THEN only those rules run (followed by cleanup and residue as usual), and an unknown id is exit 2
- **AC-8** Machine-readable output
  WHEN `--format json` or `--format yaml`
  THEN stdout is one parseable document holding the parsed configuration, the report, the not-run counts and the residue
- **AC-9** No secret is printed
  WHEN the document uses `${VAR}` references
  THEN the printed configuration shows the references, never their values
- **AC-10** Raising the level names where to write
  WHEN `--config` runs above `read-only` with neither `--store` nor a claims `key_base`
  THEN the exit code is 2 and nothing is written

Non-goals: `liquers-web` stores (browser only; a stated limit); an allowed-failure list; async
factory setup (a scratch bucket); any guarantee at `scratch` beyond rule discipline.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — new binary, features and an optional dependency, and new
  `pub` items in `liquers-store` (rules 3 and 4).
- **Leading issue:** where a `--config` store's fixture comes from (question 1, the issue's
  "unsolved problem").
- **Explanation:** every other contract point was decided in `store-conformance-suite`; the plan is
  complete under the recommended answers, and all of it sits in `liquers-store`.
- **Open questions:**
  1. **Open design - `--config` fixture source:** a required claims file (`--claims <yaml>`):
     `capabilities` with all eight fields (deserialized into `StoreCapabilities`, which has no
     `Default`, so a missing field is an error — the same discipline as in-tree fixtures) plus the
     the optional preconditions `GenericFixture`'s builders take (`key_base`, `existing`, …,
     `run_id`). Recommended: it is exactly what each in-tree configured suite (`A1`,
     `A3`) writes in Rust, and no production listing is needed. Rejected: eight boolean flags (not
     reviewable); probing (an inferred claim cannot be refuted by the refuting rules); dropping
     `--config` (loses the primary use).
  2. **Proposed resolution - scratch fixtures without `StoreFactory::create_fixture`:** the issue
     records `create_fixture` as decided; it has still no consumer but this tool, and the tool
     links only core's and `liquers-store`'s factories. Recommended: a scratch-recipe table in the
     tool (`memory`, `filesystem`, `fs`: type, temp-root argument, capabilities as in `C1`, `C2`,
     `C7`), keeping the change in one crate (`M`); add `create_fixture` when a factory outside
     these crates needs it. Keeping the decision instead adds a `liquers-core` trait method,
     `StoreTypeMap` support and chain forwarding (size `L`).
  3. **Proposed resolution - printed configuration:** print the document as parsed, before
     `${VAR}` expansion (AC-9), so credentials never reach a terminal or CI log; a mis-parsed option
     still shows, since parsing is where values take their types.

### Design Dependencies

- `store-conformance-suite` (complete) — **requires**: `run_all`, `run_rule`, `ConformanceReport`
  (serializable, `Display`), `SafetyLevel`, `GenericFixture`; all exist at HEAD.
- `store-conditional-write` — **overlaps**: its rules `cond01`-`cond04` appear in the tool
  automatically. No order.
- `store-guide-status-table` (complete) — **overlaps**: renders the same reports; no shared code.

### Consolidated Findings

- Nothing in `liquers-core` changes under the recommended answers; `liquers-store` gains a
  `check` module, a binary, a `cli` feature and optional `clap` and `tokio` (already in the graph
  through `liquers-core`; a runtime is needed because `AsyncFileStore` uses `tokio::fs`).
- `run_rule` neither cleans up nor reports residue, so `--rule` runs reuse `run_all`'s tail in the
  tool (cleanup, then `contains` per created key) and build `ConformanceReport` from its `pub`
  fields.
- A `--config` store is built whole (`StoreRouterBuilder::build_store`), as an environment builds
  it; `--store` picks the mount used as label and default `key_base`, and the expected prefix is
  the router's (root), as in `a3_opendal_report`.
- Q2 decides the size: `M` as recommended, `L` with `create_fixture`.

## Phase 2: Architecture

### Solution

A library module does the work; the binary parses arguments and maps the result to an exit code.

- **Sources.** `Config`: parse the document (`StoreRouterConfig::from_yaml`, keeping an
  unexpanded copy for printing), build it with `StoreRouterBuilder::build_store` over
  `liquers_store::default_store_factory()`, wrap in `SharedStore`, and build a `GenericFixture`
  from the claims. `Scratch`: look the type up in the recipe table, create a unique temp root,
  write a one-mount document with the root argument (`path` for `filesystem`, `root` for `fs`) and
  any `--arg k=v`, and build it the same way; the recipe supplies capabilities and the
  `unsupported_shape` / `metadata_collision` key (`collide.__metadata__`) its in-tree suite uses.
- **Level.** `--config` defaults to `ReadOnly`, `--scratch` to `Scratch`; `--level` overrides.
  Above `ReadOnly` with `--config`, `key_base` (claims) or `--store` is required (AC-10).
- **Run.** All rules: `run_all`. Selected: `run_rule` per id, then cleanup and residue as `run_all`.
- **Output.** Text: parsed configuration, then `ConformanceReport`'s `Display`, then the residue,
  then the not-run line (`not_run_by_level`) and the summary. JSON/YAML: one envelope. Errors go to
  stderr with stdout empty, as in `liquers-validate`.
- **Exit.** 0 when `report.failures()` is empty, 1 otherwise, 2 for any `Err` before the report.

Rejected: the binary in `liquers-core` (no OpenDAL types); a `conformance:` section in
`stores.yaml` (a check run is not deployment configuration; core schema change); `create_fixture` (Q2).

Known-issue preflight: `STORE-OPENDAL-LIST-OPTION-MISPARSED` and `CORE-CONFIGURATION-ERROR-KIND`
are closed; no open issue blocks. No command namespace is involved.

### Changes

`liquers-store/Cargo.toml`: `cli = ["dep:clap", "dep:tokio"]`; optional `clap` (`derive`) and
`tokio` (`rt-multi-thread`, `macros`); `[[bin]] name = "liquers-store-check"`,
`path = "src/bin/liquers_store_check.rs"`, `required-features = ["cli", "store-conformance"]`.
`liquers-store/src/lib.rs`: `#[cfg(feature = "store-conformance")] pub mod check;`.

```rust
// liquers-store/src/check.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Claims {
    pub capabilities: StoreCapabilities,
    #[serde(default)] pub key_base: Option<String>,
    // existing, existing_directory, outside_prefix, unsupported_shape,
    // metadata_collision, supported, run_id: the same shape
}
pub enum CheckSource {
    Config { document: String, claims: Claims, store: Option<String> },
    Scratch { store_type: String, args: Vec<(String, String)> },
}
pub struct CheckRequest { pub source: CheckSource, pub level: Option<SafetyLevel>, pub rules: Vec<String> }
pub struct CheckOutcome { pub config: StoreRouterConfig, pub report: ConformanceReport }
pub enum CheckFormat { Text, Json, Yaml }

pub async fn run_check(request: CheckRequest) -> Result<CheckOutcome, Error>;
pub async fn run_fixture(fixture: &dyn Fixture, rules: &[String]) -> Result<ConformanceReport, Error>;
pub fn render(outcome: &CheckOutcome, format: CheckFormat) -> Result<String, Error>;
impl CheckOutcome { pub fn exit_code(&self) -> i32 }
```

Errors are `Error::configuration_error` (bad claims, missing `key_base`, unknown rule, no recipe)
or the builder's own. `src/bin/liquers_store_check.rs`: clap (no short flags, as
`liquers-validate`), a tokio runtime, `std::process::exit`. Documents:
`STORE_IMPLEMENTATION_GUIDE.md` §5 and §7 (the tool, claims file, levels), `STORE_CONFIG_FSD.md`
(cross-link), History rows and `reviewed:`; `scripts/check-build-matrix.sh` gains the bin row.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| Pointed at production with a raised level | read-only default; AC-10; `lqconf` stems; `scratch` documented as discipline | medium |
| Claims lie (a capability declared `false` to skip rules) | refuting rules already fail an honest-looking `false` | high |
| Printing secrets | unexpanded document only (AC-9) | high |
| Temp root left after a panic | best-effort removal by the bin; path named on stderr | medium |
| Feature build breaks the default build | `required-features`; matrix row | high |

## Phase 3: Examples and Tests

### Examples

Claims for the Problem Example (`data-claims.yaml`):

```yaml
capabilities: { write: true, remove: true, directories: true, derived_directories: false,
                explicit_directories: true, remove_directories: true, stored_metadata: true,
                enumerate_keys: true }
existing: data/readme.txt
existing_directory: data
key_base: data/_conformance
```

`--level scratch --rule sibling01 --rule cond01` runs two rules and removes what they created.

### Tests

`liquers-store/tests/store_check.rs`, `#![cfg(feature = "store-conformance")]`:

- `check01_scratch_types_run_every_rule` — `memory`, `filesystem`, `fs` (`services-fs`): AC-1
- `check02_scratch_root_is_removed` — AC-1
- `check03_config_defaults_to_read_only_and_writes_nothing` — temp-dir snapshot before/after: AC-2
- `check04_incomplete_or_unknown_claims_are_setup_errors` — AC-3
- `check05_failing_store_is_exit_one_with_rule_and_keys` — `run_fixture` over a test store whose `removedir` deletes a name-extending sibling: AC-4
- `check06_create_only_lists_residue_before_summary` — AC-5
- `check07_unbuildable_document_and_unknown_scratch_type_are_setup_errors` — AC-6
- `check08_rule_selection_runs_only_named_rules` — and an unknown id: AC-7
- `check09_json_and_yaml_envelopes_parse` — AC-8
- `check10_printed_configuration_keeps_variables` — AC-9
- `check11_raised_level_without_key_base_is_refused` — AC-10

`liquers-store/tests/store_check_bin.rs`, `#![cfg(all(feature = "cli", feature = "store-conformance"))]`:
`bin01_exit_codes` — `CARGO_BIN_EXE_liquers-store-check` with `--scratch memory` (0), a missing
claims file (2, empty stdout): AC-1, AC-3.

Run: `cargo test -p liquers-store --features cli,store-conformance --test store_check --test store_check_bin`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-store/Cargo.toml`, `src/lib.rs` — features, optional deps, `[[bin]]`, `check` module stub — `cargo build -p liquers-store` and `cargo build -p liquers-store --features cli,store-conformance`. Rollback: revert.
- [ ] 2. `src/check.rs` — `Claims`, `CheckSource`, `CheckRequest`, the recipe table, `run_check` for both sources, level defaults and AC-10 — `check01`-`check04`, `check07`, `check11`.
- [ ] 3. `src/check.rs` `run_fixture` (selection, cleanup, residue), `render`, `exit_code` — `check05`, `check06`, `check08`-`check10`.
- [ ] 4. `src/bin/liquers_store_check.rs` — clap surface from the issue, runtime, exit codes, temp-root removal — `bin01_exit_codes`.
- [ ] 5. `scripts/check-build-matrix.sh` — `check liquers-store "--features cli,store-conformance --bins"` — `bash scripts/check-build-matrix.sh`.
- [ ] 6. Docs: `STORE_IMPLEMENTATION_GUIDE.md` §5, §7; `STORE_CONFIG_FSD.md` cross-link; History rows, `reviewed:`; close the source issue with a resolution note — `python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-store --features cli,store-conformance --lib --tests`;
`cargo test -p liquers-store --lib --tests` (default build unaffected);
`bash scripts/check-build-matrix.sh`; a manual run against a directory holding a pre-existing file
at `read-only` and `create-only`, confirming the file is untouched and the residue printed;
`python3 scripts/docs_index.py --check`; a diff review for edits outside `liquers-store`, the
script and the two documents.
