---
id: CORE-BENCHMARK-SUITE
kind: design
title: Reproducible benchmarks for liquers-core's stores, parser, planner and asset manager
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [build]
issues: [BENCHMARK-SUITE]
created: 2026-10-20
---
# Reproducible benchmarks for liquers-core's stores, parser, planner and asset manager

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-20. Phases 1-4 are written but not
approved, and nothing is implemented.

## Phase 1: High-Level Design

### Purpose

A refactor of a store, the asset manager or the planner should come with numbers: run one command
before and after, and read the difference. This is the first slice of `BENCHMARK-SUITE`, limited to
`liquers-core`, where all four scenario groups of the feature live.

### Problem Example

At HEAD (18732dd) the workspace has no `benches/` directory, no `[[bench]]` target and no
benchmark dependency. The nearest things are `liquers-core/tests/dependency_chain_scaling.rs`, a
scaling *test* with `Instant` and fixed thresholds, and `liquers-web/tests/boundary_benchmark.rs`,
which is wasm32-only and prints a table. A change to `AsyncMemoryStore` locking today can only be
judged by ad-hoc timing.

Should be: `cargo bench -p liquers-core --bench store_memory -- --save-baseline before`, then the
change, then `… -- --baseline before`, which prints the change per benchmark with its confidence
interval.

### Scope and Acceptance Criteria

- **AC-1** Store concurrency benchmarks
  WHEN `cargo bench -p liquers-core --bench store_memory` and `--bench store_file` run
  THEN each reports `set`, `get` and `listdir` throughput for 1, 8 and 64 concurrent tasks on a
  multi-threaded tokio runtime
- **AC-2** Parse and plan benchmarks
  WHEN `cargo bench -p liquers-core --bench parse_plan` runs
  THEN it reports `parse_query` and `PlanBuilder::build` time for three fixed queries
- **AC-3** Asset manager benchmarks
  WHEN `cargo bench -p liquers-core --bench asset_manager` runs
  THEN it reports evaluation time for a repeated query and for a distinct query per iteration,
  serially and with 8 concurrent callers
- **AC-4** Baseline workflow
  WHEN a developer follows `specs/guides/BENCHMARK_GUIDE.md`
  THEN they can save a named baseline, compare against it, and record the machine and toolchain
- **AC-5** Benchmarks stay compilable and runnable
  WHEN `cargo test -p liquers-core --benches` runs (debug, each benchmark executed once)
  THEN it passes, and the build matrix checks `liquers-core --benches`

Non-goals: CI regression gating (the feature's own non-goal); `liquers-lib` command benchmarks
(polars, images), which are a follow-up; committing result files.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — adds a dev-dependency (rule 4).
- **Leading issue:** **Proposed resolution — harness.** `criterion`, as a native-only
  dev-dependency of `liquers-core` with `default-features = false` and features
  `cargo_bench_support` and `async_tokio`. It has baselines built in (`--save-baseline`,
  `--baseline`), which is what AC-4 needs. With plotting and rayon off, the build stays small.
  Alternative: `divan` (lighter, no built-in baseline comparison).
- **Explanation:** The design is complete under the recommended answers. Only the dependency choice
  and the results policy need the maintainer.
- **Open questions:**
  1. **Proposed resolution — harness:** as above.
  2. **Open design question — where results live.** *Recommended:* nowhere in git. Baselines stay
     in `target/criterion/`, which is machine-specific. A PR that claims a performance effect
     pastes the comparison output into its description. Alternative: a committed
     `specs/reference/BENCHMARK_RESULTS.md` per release, which goes stale per machine.
  3. **Proposed resolution — scope:** `liquers-core` only (AC-1-AC-3). The feature's
     "evaluation pipeline" is covered by the asset-manager benchmark with core commands.

### Design Dependencies

- `CORE-TOKIO-REMOVAL`, `core-tracing-migration`, store designs such as `store-conditional-write`
  — required-by (soft): their performance claims become measurable after this lands; no ordering
  constraint.
- `workspace-rustfmt-check` — overlaps: new files are formatted from the start.

### Consolidated Findings

The benchmarks live in `liquers-core/benches/`, are native-only, and run with
`cargo test --benches` (one iteration each) so they cannot rot. `cargo bench` builds in release.
For `liquers-core` alone this is cheap; it would not be for `liquers-lib`, which is one more reason
to leave lib out of the first slice. The existing scaling test stays a test.

## Phase 2: Architecture

### Solution

- `liquers-core/Cargo.toml`: under `[target.'cfg(not(target_arch = "wasm32"))'.dev-dependencies]`,
  `criterion = { version = "<current>", default-features = false, features =
  ["cargo_bench_support", "async_tokio"] }`. Then four `[[bench]]` entries with `harness = false`:
  `store_memory`, `store_file`, `parse_plan`, `asset_manager`.
- `benches/store_memory.rs`: `AsyncMemoryStore::new(&Key::new())` behind `Arc<dyn AsyncStore>`.
  Groups `set`, `get` and `listdir`; parameter = task count (1, 8, 64), each task doing 100
  operations on keys `bench/t{task}/k{i}`, via `b.to_async(&runtime).iter(...)`.
- `benches/store_file.rs`: the same over `AsyncFileStore::new(dir, &Key::new())` in a fresh
  directory under `std::env::temp_dir()`, removed at the end.
- `benches/parse_plan.rs`: `parse_query` on `-R/data/report.csv/-/to_text`, `world/greet-Hello`,
  `-R/data/sales.csv/-/ns-pl/from_csv/head-10/to_csv`, and `PlanBuilder::new(query,
  &registry).build()` for the second, with a registry holding `world` and `greet` registered as in
  `liquers-core/tests/async_hellow_world.rs`.
- `benches/asset_manager.rs`: `SimpleEnvironment<Value>` with `world`/`greet`; `evaluate(envref,
  "world/greet-Hello", None)` repeated, and `world/greet-{i}` distinct per iteration; serial, and 8
  concurrent `tokio::spawn` callers.
- `scripts/check-build-matrix.sh`: add `check liquers-core "--benches"`.
- New `specs/guides/BENCHMARK_GUIDE.md`: commands, baseline naming (`<branch>-<short-sha>`), what
  to record (`rustc -vV`, CPU model from `lscpu`, `nproc`), a note to close other workloads, and how
  to add a benchmark. Link it from `specs/README.md` and `CLAUDE.md` "Building and testing".

Every query above was checked with `liquers-validate`. Rejected: benchmarks as `#[ignore]` tests
(no statistics, no baselines); a custom harness (re-implements criterion).

### Changes

`liquers-core/Cargo.toml`, four new bench files, `scripts/check-build-matrix.sh`, new guide,
`specs/README.md`, `CLAUDE.md`. No library code changes; benches use the public API only.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| Noisy numbers on shared or cloud machines | The guide asks for repeated runs; criterion reports a confidence interval; no gating | high |
| Release build time and disk in constrained sessions | Core only; criterion without plotting | medium |
| A bench uses a non-public API | Benches are external crates, so the compiler enforces this | high |
| The temp directory is left behind on a panic | A unique name per run, removed at the end and at the start | medium |

## Phase 3: Examples and Tests

### Examples

```bash
cargo bench -p liquers-core --bench store_memory -- --save-baseline main-18732dd
# … change AsyncMemoryStore …
cargo bench -p liquers-core --bench store_memory -- --baseline main-18732dd
```

### Tests

- `cargo test -p liquers-core --benches` — every benchmark function runs once; AC-1, AC-2, AC-3
  (that they exist and execute) and AC-5
- `cargo bench -p liquers-core -- --list` lists the groups `store_memory/{set,get,listdir}/{1,8,64}`,
  `store_file/…`, `parse_plan/…` and `asset_manager/…` — AC-1-AC-3
- the guide's commands, followed once on a clean checkout, print a comparison — AC-4
- `bash scripts/check-build-matrix.sh` includes and passes the new row — AC-5

## Phase 4: Implementation Plan

### Steps

- [ ] 1. Record the decisions (harness, results policy) in this design — maintainer
- [ ] 2. `liquers-core/Cargo.toml` — native-only criterion dev-dependency, four `[[bench]]` — `cargo check -p liquers-core --benches`
- [ ] 3. `benches/parse_plan.rs` — `cargo test -p liquers-core --bench parse_plan`
- [ ] 4. `benches/store_memory.rs`, `benches/store_file.rs` — `cargo test -p liquers-core --bench store_memory --bench store_file`
- [ ] 5. `benches/asset_manager.rs` — `cargo test -p liquers-core --bench asset_manager`
- [ ] 6. `scripts/check-build-matrix.sh` row — `MATRIX_DRY_RUN=1 bash scripts/check-build-matrix.sh` lists it
- [ ] 7. `specs/guides/BENCHMARK_GUIDE.md`, `specs/README.md`, `CLAUDE.md` — follow the guide once (AC-4); `python3 scripts/docs_index.py --check`
- [ ] 8. Close `BENCHMARK-SUITE` (or narrow it to the lib follow-up and file that as an issue) — `--check`

### Validation

`cargo test -p liquers-core --benches`; one real `cargo bench -p liquers-core` run; the matrix's
core rows, including the wasm32 row (the dev-dependency is native-only). Diff review: no change
under `liquers-core/src/`.
