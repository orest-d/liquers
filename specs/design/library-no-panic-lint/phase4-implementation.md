# Phase 4: Implementation Plan — No unwrap or expect in library code

## Overview

Ratchet: first the crates that are already clean get the attribute (no code change), then each
crate with sites is fixed and gets its attribute in the same commit, then CI enforces it. Every
step is its own commit and can land as its own PR. Prerequisite: Q1-Q3 decided (Phase 1); the plan
follows the recommended answers. No other issue must be fixed first.

## Progress

- [ ] Step 1: attribute in the eight clean crates
- [ ] Step 2: record the characterization values, add the three unit tests
- [ ] Step 3: liquers-core metadata.rs sites
- [ ] Step 4: liquers-core query.rs, commands.rs, plan.rs, assets.rs sites; core attribute
- [ ] Step 5: liquers-macro expansion; macro attribute
- [ ] Step 6: liquers-lib egui and polars sites; lib attribute
- [ ] Step 7: CI and the build matrix run clippy
- [ ] Step 8: lint probe, documents, issue closure

## Implementation Steps

### Step 1: attribute in the eight clean crates
- Files / symbols: `liquers-store`, `-records`, `-template`, `-axum`, `-mcp`, `-agent`, `-py`,
  `-web` `src/lib.rs` (in `liquers-web`, after `#![cfg(target_arch = "wasm32")]`).
- Change: `#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]`. If
  `core-tracing-migration` has landed, add the two lints to its existing `deny(…)` instead.
- Depends on: none.
- Proof: `cargo clippy -p liquers-store -p liquers-records -p liquers-template -p liquers-axum
  -p liquers-mcp -p liquers-py --lib` exits 0 (Test 3); `liquers-agent` alone (needs Rust 1.96);
  `cargo clippy -p liquers-web --lib --target wasm32-unknown-unknown`.
- Rollback: delete the line in the failing crate.

### Step 2: characterization tests
- Files / symbols: `liquers-core/src/metadata.rs`, `query.rs`, `commands.rs` `tests` modules.
- Change: add `extension_of_edge_filenames`, `all_predecessor_tuples_of_mixed_query` (expected
  tuples captured from the current code first), `register_command_returns_its_metadata`.
- Depends on: none (parallel with Step 1).
- Proof: `cargo test -p liquers-core --lib extension_of_edge_filenames all_predecessor_tuples
  register_command_returns` passes on the unmodified code (Tests 5-7).
- Rollback: delete the tests.

### Step 3: `metadata.rs` sites
- Files / symbols: `AssetInfo::with_key`, `with_query`, `extension`; the two `deserialize`
  helpers; `MetadataRecord::with_key`, `extension`; `Metadata::extension`, `add_log_entry`,
  `set_status`, `set_volatile`.
- Change: the rewrites in Phase 2's table (12 sites).
- Depends on: Step 2.
- Proof: `cargo test -p liquers-core --lib` (Test 5 still passes; Test 8).
- Rollback: revert the commit.

### Step 4: remaining core sites and the core attribute
- Files / symbols: `query.rs` `TransformQuerySegment::predecessor`, `Query::predecessor`,
  `all_predecessors` (drop the commented-out `eprintln!`), `all_predecessor_tuples`;
  `commands.rs` `CommandRegistry::register_command`, `register_async_command`; `plan.rs`
  `PlanBuilder::process_query`; `assets.rs` `AssetData::asset_reference`; `lib.rs` attribute.
- Change: Phase 2's rewrites (11 sites).
- Depends on: Step 3.
- Proof: `cargo test -p liquers-core --lib --tests` (Tests 6-8); `cargo clippy -p liquers-core
  --lib` and `cargo clippy -p liquers-core --lib --no-default-features` exit 0 (Test 3).
- Rollback: revert; drop the attribute if a gated site surfaces, and file it.

### Step 5: macro expansion
- Files / symbols: `liquers-macro/src/registration.rs`, the two `EnumValueLiteral::Float`
  arms; `liquers-macro/src/lib.rs` attribute.
- Change: `serde_json::Number::from_f64(#value).map(serde_json::Value::Number)
  .unwrap_or(serde_json::Value::Null)` in place of the nested `unwrap_or(…unwrap())`, wrapped in
  `CommandParameterValue::Value(…)` as before.
- Depends on: none (parallel with Steps 3-4).
- Proof: `cargo test -p liquers-macro`; `cargo clippy -p liquers-macro --lib`; `cargo test -p
  liquers-lib --test registry_export` (Test 9).
- Rollback: revert the commit.

### Step 6: `liquers-lib` sites and attribute
- Files / symbols: `egui/dataframe.rs` cell formatting; `egui/widgets.rs` lock writes and colour
  constants; `egui/mod.rs` locks; `polars/filtering.rs` epoch; `liquers-lib/src/lib.rs`.
- Change: Phase 2's rewrites (31 sites).
- Depends on: Step 5 (so the crate's own expansions are clean whatever clippy does with them).
- Proof: `cargo test -p liquers-lib --lib --tests` (Test 8, includes `registry_export`);
  `cargo clippy -p liquers-lib --lib` and `--no-default-features --features webui` exit 0.
- Rollback: revert; the attribute line alone can be dropped while a site is investigated.

### Step 7: CI enforcement
- Files / symbols: `scripts/check-build-matrix.sh` `check()` (and its header comment);
  `.github/workflows/build-matrix.yml` toolchain step `components: clippy`.
- Change: run `cargo clippy` with the row's arguments where `cargo check` ran; `MATRIX_DRY_RUN`
  output shows the new command.
- Depends on: Steps 1, 4, 5, 6 (every crate clean, or the matrix fails).
- Proof: `MATRIX_DRY_RUN=1 bash scripts/check-build-matrix.sh` lists clippy rows; one full run in CI
  green (AC-1).
- Rollback: restore `cargo check` in `check()`; the attributes stay harmless.

### Step 8: probe, documents, issue
- Files / symbols: temporary edit of `liquers-core/src/query.rs` (not committed); `CLAUDE.md`;
  `.claude/skills/rust-best-practices/SKILL.md`; `specs/issues/LIBRARY-CODE-USES-UNWRAP-AND-EXPECT.md`.
- Change: run the probe (Phase 3, Test 4) and revert it; document the lint command and the
  `reason` exception; close the issue with the evidence (counts 0, the commits).
- Depends on: Step 7.
- Proof: probe output shows one error, at the non-test site (AC-2, AC-3); `python3
  scripts/docs_index.py --check`.
- Rollback: documents only.

## Testing Plan

Steps 1-6 each run their crate's tests and clippy. After Step 7 the build matrix is the final
proportionate check (`bash scripts/check-build-matrix.sh`; locally mind the disk guard). No
`--workspace` run.

## Rollback Plan

Each step is one commit. A partial landing is safe: a crate with the attribute and no sites is
enforced; one without the attribute is as today. Reverting Step 7 alone keeps the attributes as
local documentation of the rule.

## Documentation Updates

`CLAUDE.md` ("Building and testing": `cargo clippy -p <crate> --lib`; "Do NOT": the lint and the
`allow(…, reason)` exception); `.claude/skills/rust-best-practices/SKILL.md`;
`scripts/check-build-matrix.sh` header; the issue closed (§4.3) with its resolution;
`specs/index.csv` regenerated. No reference or guide document changes, so no History rows.
`specs/command_registry.yaml` is not regenerated (Test 9 proves it unchanged).

## Phase 5 Entry Criteria

None — four-phase design.
