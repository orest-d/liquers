---
id: LIBRARY-CODE-USES-UNWRAP-AND-EXPECT
kind: issue
title: Library code uses unwrap and expect despite the no-panic rule
status: draft
priority: P2
complexity: L
area: [core/query, core/value, lib/egui, macro, build]
design: library-no-panic-lint
created: 2026-08-17
github:
---
## Problem

**Re-scoped 2026-10-20 (bulk design).** Still reproduces, in part — see "Status at 18732dd"
below; the text that follows it is the original filing, kept as history. The original design
link, `axum-assets-endpoints` (implemented, PR #73), removed the `liquers-axum` sites; the
remainder is designed in `specs/design/library-no-panic-lint/`.

`CLAUDE.md` states the rule without qualification: *"Do NOT use `unwrap()` or `expect()` in library
code (only in tests)."* They panic, and library code must return `Result<_, Error>` and propagate
with `?`.

A scan of `src/` in each crate, excluding everything from the first `#[cfg(test)]` marker onward,
counts roughly:

| Crate | Occurrences |
|---|---|
| `liquers-core` | ~50 |
| `liquers-lib` | ~38 |
| `liquers-axum` | ~9 |
| `liquers-store` | ~3 |

The counts are approximate — the scan is textual, so it over-counts helper modules that sit before
their `#[cfg(test)]` block and under-counts test helpers defined above one. The order of magnitude
is what matters: this is systemic, not a handful of oversights.

A concrete example, found while implementing `store-key-guard`:

```rust
// liquers-store/src/opendal_store.rs, AsyncOpenDALStore::make_sub_dirs
let sub_key = key.prefix_of_size(i).unwrap();
```

`prefix_of_size` returns `None` when the key is shorter than `i`. The loop bounds it by
`key.len()`, so it is unreachable *today* — which is exactly the shape that becomes a panic when
someone later changes the loop. The function already returns `Result`, so `?` costs nothing.

## Impact

A panic in library code is not an error a caller can handle. It aborts the surrounding task, and in
wasm it kills the instance — `CORE-IMMEDIATE-MANAGER-KEYED-RECURSION` records that a panic there
surfaced as a hung `Promise` rather than an error, which is far harder to diagnose than a returned
`Error`. `LIB-RECIPE-PROVIDER-PANIC` (closed, P0) was one such panic reaching users.

Most individual sites are probably unreachable in practice, which is why this is P2 rather than
higher. The cost is that the rule is not enforced anywhere, so the count grows, and each new one
has to be judged by hand rather than by a lint.

## Expected behaviour

Two separable pieces of work, and the second matters more than the first:

1. **Replace the reachable ones.** Each site returns `Err(Error::…)` with a typed constructor, or
   documents in one line why the invariant cannot break. Triage by crate; `liquers-core` and
   `liquers-lib` dominate.
2. **Make the rule enforceable.** Without a lint the count only goes up. Options:
   - `#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]` per crate, which is
     the direct expression of the rule but will not build until (1) is done for that crate — so it
     lands crate by crate, as a ratchet.
   - `#![warn(...)]` first, to size the work honestly before committing to `deny`.

   Note the repository has no CI (`liquers-web/scripts/check-stubs.sh` says so explicitly), so a
   lint attribute in the source is worth more here than a CI job would be.

Complexity `L`: it crosses every crate and touches public error paths, so per §4.5 it needs a
design folder — mostly to decide the triage order and whether `expect` with a proof comment is
acceptable in genuinely unreachable spots.

## Discovery

Noticed 2026-08-17 while implementing `specs/design/store-key-guard/`: making
`AsyncOpenDALStore::key_to_path` fallible required touching `make_sub_dirs`, whose neighbouring
`unwrap()` is the example above. The survey that followed showed it was not an isolated case.

Also noticed 2026-10-20 while documenting planner instructions: the two filename branches of
`PlanBuilder` (`liquers-core/src/plan.rs`, `transform.filename.unwrap()` in `process_query` and
`tqs.filename.as_ref().unwrap()` in its last-segment branch) unwrap right after an `is_filename()`
check, the same guarded-today shape.

## Progress

Progress 2026-09-28 (`design/axum-assets-endpoints/`, I7): `liquers-axum` library code has no
`unwrap()`/`expect()` — the nine sites use `build_or_500` or `HeaderValue::from_static`, and
`cargo clippy -p liquers-axum --no-deps -- -D clippy::unwrap_used -D clippy::expect_used` passes.
The other crates remain.

## Status at 18732dd (2026-10-20)

Re-measured with the same textual scan (non-test `src/`, stopping at `#[cfg(test)] mod`, comments
skipped):

| Crate | Sites | Where |
|---|---|---|
| `liquers-core` | 23 | `metadata.rs` 12, `query.rs` 6, `commands.rs` 2, `plan.rs` 2, `assets.rs` 1 |
| `liquers-lib` | 31 | `egui/dataframe.rs` 18, `egui/widgets.rs` 10, `egui/mod.rs` 2, `polars/filtering.rs` 1 |
| `liquers-macro` | 2 | in the expansion of float defaults (`registration.rs`), so in every crate that registers commands |
| `liquers-axum`, `-store`, `-records`, `-template`, `-mcp`, `-agent`, `-py`, `-web` | 0 | axum: PR #73; store: `make_sub_dirs` rewritten in a3b3652 |

Every remaining site is guarded by an earlier check, so none is reachable today. Two premises above
are out of date: the `make_sub_dirs` example no longer exists, and the repository now has CI
(`.github/workflows/build-matrix.yml`), which is where the lint can be enforced.

Problem example at HEAD: `CommandRegistry::register_command` (`liquers-core/src/commands.rs`) ends
with `self.command_metadata_registry.get_mut(key).unwrap()`, and adding another `.unwrap()`
anywhere in library code passes build, tests and CI. Expected: the sites gone, and
`cargo clippy -p <crate> --lib` (and the CI matrix) failing on a new one.

