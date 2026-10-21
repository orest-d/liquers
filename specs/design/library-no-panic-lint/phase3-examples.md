# Phase 3: Examples & Use-cases — No unwrap or expect in library code

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | A new unwrap fails the lint | the attribute plus clippy catch a regression | AC-2 |
| 2 | Example | An exception, written down | the only accepted form of a kept site | AC-6 |
| 3 | Test | lint run per crate (`cargo clippy --lib`, every crate and matrix row) | zero diagnostics | AC-1 |
| 4 | Test | lint probe (manual, Step 8) | a planted `unwrap()` fails clippy; a test-module one does not | AC-2, AC-3 |
| 5 | Test | `liquers_core::metadata::tests::extension_of_edge_filenames` | `rsplit_once` rewrite equals the old split | AC-4 |
| 6 | Test | `liquers_core::query::tests::all_predecessor_tuples_of_mixed_query` | `while let` rewrite returns the same tuples | AC-4 |
| 7 | Test | `liquers_core::commands::tests::register_command_returns_its_metadata` | the registry returns the inserted entry, sync and async | AC-4 |
| 8 | Test | existing suites: `cargo test -p liquers-core`, `-p liquers-lib --lib --tests`, `-p liquers-macro` | unchanged behaviour | AC-3, AC-4 |
| 9 | Test | `liquers-lib --test registry_export` | macro expansion change leaves the exported registry identical | AC-5 |

## Example 1: A new unwrap fails the lint

```rust
// liquers-core/src/lib.rs, first line
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

// someone later writes, in liquers-core/src/query.rs:
let first = self.segments.first().unwrap();
```

```text
$ cargo clippy -p liquers-core --lib
error: used `unwrap()` on an `Option` value
  --> liquers-core/src/query.rs:…
   = note: requested on the command line / by `#![deny(clippy::unwrap_used)]`
```

The CI build matrix runs the same command in every configuration row, so the pull request fails.
The same line inside `#[cfg(test)] mod tests` compiles and lints clean (the attribute is off under
`cfg(test)`), and so does any integration test, example or binary (separate crates, no attribute).

## Example 2: An exception, written down

No site needs it today. If one ever does, the allow sits on the smallest item and states the
invariant:

```rust
#[allow(clippy::expect_used, reason = "the regex is a compile-time constant checked by test re01")]
static PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-z]+$").expect("valid regex"));
```

`grep -rn 'allow(clippy::\(unwrap\|expect\)_used' liquers-*/src` is the inventory a reviewer reads.

## Edge and Error Cases

- **Feature-gated code.** `egui/` is linted only with `egui` on; `ui/web` only with `webui`. Matrix
  rows `--tests` (defaults) and `--no-default-features --features webui` cover them → Test 3.
- **wasm32.** `liquers-web` and the wasm rows of `liquers-lib`/`-records`/`-template`/`-core`
  are linted with `--target wasm32-unknown-unknown` in the matrix → Test 3.
- **Proc-macro output.** Fixed at the source, so the result does not depend on whether clippy lints
  external-macro expansions → Test 9 proves nothing observable changed.
- **Legacy metadata serialization.** `serde_json::to_value` of a `Status` or `LogEntry` cannot fail
  (plain derives); the new error path is not tested, by design (Phase 2, Error Handling).
- **Poisoned lock in egui.** Recovering the inner value instead of panicking is the intended
  behaviour; not unit-tested (needs a UI frame), covered by review.
- **Edge filenames.** `"a.b.c"` → `Some("c")`, `"a"` → `None`, `".x"` → `Some("x")`, `"a."` →
  `Some("")`: identical to `split('.').last()` with the `len > 1` guard → Test 5.

## Test Plan

- Unit: `liquers-core/src/metadata.rs` — `extension_of_edge_filenames` (AC-4): the four filenames
  above through `AssetInfo`, `MetadataRecord` and `Metadata::extension`; plus `with_key` and
  `with_query` on `-R/a/b.txt` set `filename` to `b.txt`.
- Unit: `liquers-core/src/query.rs` — `all_predecessor_tuples_of_mixed_query` (AC-4):
  `-R/data/x.csv/-/a-1/b/c.txt` gives the same tuples as recorded before the change (record them
  with the old code in Step 2, before rewriting).
- Unit: `liquers-core/src/commands.rs` — `register_command_returns_its_metadata` (AC-4): the
  returned `&mut CommandMetadata` has the registered key, for `register_command` and
  `register_async_command`.
- Integration: `cargo test -p liquers-lib --test registry_export` (AC-5).
- Lint: `cargo clippy -p <crate> --lib` for each of the eleven crates, and
  `bash scripts/check-build-matrix.sh` after the switch (AC-1).
- Probe (AC-2, AC-3): Step 8 plants one `unwrap()` in `liquers-core/src/query.rs` and one in its
  `tests` module, runs clippy, expects exactly one error at the first, then removes both. Not
  committed: a permanent "must fail to compile" test would need a trybuild-style harness for one
  attribute line, which costs more than it proves.
- Commands: `cargo test -p liquers-core --lib`, `cargo test -p liquers-macro`,
  `cargo test -p liquers-lib --lib --tests`.

The query in Test 6 parses as intended (`liquers-validate --no-registry`: resource
`data/x.csv`, actions `a-1`, `b`, filename `c.txt`).

## Learning Log

- The issue's "no CI" premise is out of date; `build-matrix.yml` is where a lint can live.
- `cfg_attr(not(test), …)` in `lib.rs` scopes the rule to library targets exactly as `CLAUDE.md`
  words it, with no `clippy.toml`.
