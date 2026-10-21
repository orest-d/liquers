# Phase 1: High-Level Design — No unwrap or expect in library code

## Purpose

`CLAUDE.md` forbids `unwrap()` / `expect()` in library code, but nothing checks it, so the count
only moves when someone happens to look. Remove the remaining sites and make the rule a lint that
fails, crate by crate, so a server (`liquers-axum`), a browser instance (`liquers-web`) or an agent
tool (`liquers-mcp`) never aborts on a panic a caller could have handled (VISION: "Trust the
result"; strategy "Async-first, wasm as a forcing function" — a wasm panic kills the instance).

## Problem Example

At HEAD (18732dd), non-test library code still holds 56 sites. One of them:

```rust
// liquers-core/src/commands.rs, CommandRegistry::register_command
self.executors.insert(key.clone(), Arc::new(Box::new(f)));
Ok(self.command_metadata_registry.get_mut(key).unwrap())   // panics if add_command ever stops inserting
```

Adding another `.unwrap()` anywhere in `liquers-core/src` today passes `cargo build`, `cargo test`
and CI. Should be: the sites are gone, and the same addition makes
`cargo clippy -p liquers-core --lib` fail with `clippy::unwrap_used` naming the line, in the
build-matrix CI job as well as locally.

## Scope and Acceptance Criteria

- **AC-1** No site left
  WHEN `cargo clippy --lib` runs on any workspace library crate with the lint attribute
  THEN it reports no `clippy::unwrap_used` or `clippy::expect_used` diagnostic and exits 0
- **AC-2** A new site fails
  WHEN a `.unwrap()` or `.expect(..)` is added outside `#[cfg(test)]` in a linted crate
  THEN `cargo clippy --lib` for that crate fails and names the line, and so does the CI matrix
- **AC-3** Tests may still unwrap
  WHEN unit tests (`#[cfg(test)]`), integration tests, examples, doc examples or `[[bin]]` targets
  use `unwrap()` / `expect()`
  THEN no lint fires and nothing in them changes
- **AC-4** Behaviour preserved
  WHEN the existing test suites of `liquers-core`, `liquers-lib` and `liquers-macro` run after the
  sites are rewritten
  THEN they pass unchanged, and the rewritten functions return what they returned before for every
  input that did not panic
- **AC-5** Generated code clean
  WHEN `register_command!` expands a command with a float default
  THEN the expansion contains no `unwrap()`, and `specs/command_registry.yaml` is unchanged
  (`registry_export` passes)
- **AC-6** Exceptions are visible
  WHEN a site is kept because no error path exists and restructuring is impossible
  THEN it carries `#[allow(clippy::…, reason = "<invariant>")]` on the smallest item, so
  `grep -rn 'allow(clippy::unwrap_used\|allow(clippy::expect_used' liquers-*/src` lists every one

Non-goals: `panic!`, `unreachable!`, `todo!` and slice indexing (a separate audit; `clippy::panic`
and `clippy::indexing_slicing` are much noisier); other clippy lints (`-D warnings` is not
proposed); test code; `liquers-core`'s `[[bin]]` targets.

## Core Interactions

- Query (`query.rs` predecessor helpers), Value/metadata (`metadata.rs`), Commands
  (`commands.rs` registry, `liquers-macro` expansion), Plan (`plan.rs` `PlanBuilder`), Assets
  (`AssetData::asset_reference`), UI (`liquers-lib/src/egui/`), Polars (`filtering.rs`).
- Build: crate-root lint attributes; the CI build matrix (`.github/workflows/build-matrix.yml`,
  `scripts/check-build-matrix.sh`).

## Crate Placement

Every library crate: the attribute goes into each `lib.rs`. Code changes only in `liquers-core`,
`liquers-lib` and `liquers-macro`, where all remaining sites are. No dependency edge changes.

## Documentation Intent

- Reference: none — no behaviour a user sees changes.
- Guide: none new; `CLAUDE.md` "Building and testing" gains the lint command, and "Do NOT" points
  at the lint and the `reason` exception.
- Other documents: `.claude/skills/rust-best-practices/SKILL.md` hard rule names the lint.
- Documents to update: the issue (re-scoped, closed at the end).

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — complexity `L` (three crates plus tooling; rule 1) and
  touches more than one crate (rule 6). Each Phase 4 slice is eligible-shaped once Q1-Q3 are
  decided.
- **Leading issue:** where clippy runs (Q3); without it the lint attribute enforces nothing.
- **Explanation:** every remaining site is an invariant guarded by an earlier check and has a
  rewrite with the same result, so the plan is concrete; three policy choices remain proposals and
  none changes the code rewrites.
- **Open questions:**
  1. **Proposed resolution - exceptions:** none needed today; a future unavoidable one uses
     `#[allow(clippy::expect_used, reason = "…")]` on the smallest item (recommended), rather
     than banning exceptions outright or accepting a bare `expect` with a comment.
  2. **Proposed resolution - mechanism:** `#![cfg_attr(not(test), deny(clippy::unwrap_used,
     clippy::expect_used))]` in each `lib.rs` (recommended: library targets only, same form as
     `core-tracing-migration`'s print lints), rather than `[workspace.lints]` in `Cargo.toml`,
     which also lints tests, examples and binaries.
  3. **Open design - where clippy runs:** the CI build matrix runs `cargo clippy` instead of
     `cargo check` for every row (recommended: every feature configuration and wasm32 linted at
     almost no extra cost), or a separate `lint.yml` job with default features only.

## Design Dependencies

- `core-tracing-migration` (in_review) — **overlaps**: adds `deny(clippy::print_stdout,
  clippy::print_stderr)` to the same seven `lib.rs` attributes and plans the same clippy check.
  Whichever lands second extends the other's attribute line; with Q3 both are enforced by CI. No
  order. (Its "with no CI" premise is also stale: `build-matrix.yml` exists.)
- `build-matrix-disk-budget` (complete) — **overlaps**: owns `scripts/check-build-matrix.sh`; Q3
  changes the command it runs, not its disk guard.
- `axum-assets-endpoints` (implemented, PR #73) — origin: removed the `liquers-axum` sites; frozen.

## Consolidated Findings

- **Re-measured at HEAD** (textual scan of `liquers-*/src`, stopping at `#[cfg(test)] mod`, skipping
  comments): `liquers-core` 23 (`metadata.rs` 12, `query.rs` 6, `commands.rs` 2, `plan.rs` 2,
  `assets.rs` 1), `liquers-lib` 31 (`egui/dataframe.rs` 18, `egui/widgets.rs` 10, `egui/mod.rs` 2,
  `polars/filtering.rs` 1), `liquers-macro` 2 (in generated code). `liquers-axum`, `-store`,
  `-records`, `-template`, `-mcp`, `-agent`, `-py`, `-web`: 0. The issue's table (core ~50, lib ~38,
  axum ~9, store ~3) is stale: axum fixed by PR #73, store by a3b3652.
- Every site is guarded by an earlier check (`is_some`, `is_err`, a matched dtype, a constant);
  none is reachable today, and none needs a signature change (Phase 2 table).
- The lint only acts when clippy runs, so Q3 is what makes AC-2 true; `CLAUDE.md` should name the
  command for local use.
- `egui/` changes are UI-only; `liquers-lib` must be linted with `egui` on (default features) and
  the `webui` row, or the gated code is never seen.
- The macro sites are in the expansion, i.e. in every crate that registers commands; they are fixed
  at the source in `liquers-macro`, not by an `allow`.
- Order: slices are independent; the already-clean crates can get the attribute first (no code
  change), then `liquers-core`, `liquers-macro`, `liquers-lib`, then the CI switch.
