# Phase 2: Solution & Architecture — No unwrap or expect in library code

## Overview

Rewrite the 56 sites so the invariant is expressed by control flow (`if let`, `while let`, `match`,
`?`), add `#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]` to every library
crate root, and make the CI build matrix run `cargo clippy` so the attribute is enforced.

Rejected: `[workspace.lints.clippy]` (applies to every target, so tests, examples and binaries
would need `clippy.toml` `allow-unwrap-in-tests` plus per-target allows); `warn` first (the counts
are already small enough to fix in one pass, and a warning nobody reads is the status quo);
replacing sites with `Error::unexpected_error` where a restructure removes the branch entirely (an
error path that cannot occur is dead code that still needs a test).

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `CORE-TRACING-MIGRATION` (`core-tracing-migration`) | accepted | P2 | Same `lib.rs` attribute line and clippy run | No | No | Share the attribute line and the CI clippy run |
| `BUILD-MATRIX-NOT-RUN-IN-CI` | closed | — | `build-matrix.yml` exists; it is the CI this design uses | — | No | None |

## Interfaces

None. No `pub` signature, trait, type, command or serialized format changes. The macro expansion
changes in one expression (below), with the same value for every finite default.

## Integration Points

Site rewrites (line numbers at 18732dd):

| File and symbol | Sites | Rewrite |
|---|---|---|
| `liquers-core/src/metadata.rs` `AssetInfo::with_key`, `with_query`, `MetadataRecord::with_key` | 3 | clone `key.filename()`'s name before moving `key` into `self.key` |
| `metadata.rs` `AssetInfo::extension`, `MetadataRecord::extension`, `Metadata::extension` | 3 | `filename.rsplit_once('.').map(\|(_, ext)\| ext.to_string())` — same result for `a.b.c`, `a`, `.x`, `a.` |
| `metadata.rs` the two `deserialize` helpers (`Option<Query>`, `Option<Key>`) | 2 | `match String::deserialize(d) { Err(_) => Ok(None), Ok(s) => … }` |
| `metadata.rs` `Metadata::add_log_entry` (×2), `set_status`, `set_volatile` (legacy branches) | 4 | `serde_json::to_value(..).map_err(\|e\| Error::from_error(ErrorType::SerializationError, e))?` — the functions already return `Result` |
| `liquers-core/src/query.rs` `TransformQuerySegment::predecessor` | 1 | `if let Some(last) = self.query.last()` |
| `query.rs` `Query::predecessor` | 1 | `match p` on the `Option` already tested |
| `query.rs` `Query::all_predecessors`, `all_predecessor_tuples` | 4 | `while let Some(q) = qp.take()`; the commented-out `eprintln!` holding a fifth goes (also planned by `core-tracing-migration`) |
| `liquers-core/src/commands.rs` `CommandRegistry::register_command`, `register_async_command` | 2 | `.ok_or_else(\|\| Error::unexpected_error(format!("Command {key} was not registered")))` |
| `liquers-core/src/plan.rs` `PlanBuilder::process_query` | 2 | `if let Some(filename) = …filename` replacing `is_filename()` + `unwrap` |
| `liquers-core/src/assets.rs` `AssetData::asset_reference` | 1 | `if let (Ok(true), Ok(q)) = (self.is_pure_query(), self.recipe.get_query())` |
| `liquers-macro/src/registration.rs` (two float-default expansions) | 2 | `serde_json::Number::from_f64(#value).map(serde_json::Value::Number).unwrap_or(serde_json::Value::Null)` — see Risks |
| `liquers-lib/src/egui/dataframe.rs` cell formatting | 18 | `col.i64().ok().and_then(\|c\| c.get(row_idx))` etc.; a failed date/datetime format shows the cell empty, as a null does |
| `liquers-lib/src/egui/widgets.rs` (`RwLock` writes, `Color32::from_hex` constants) | 10 | lock: `unwrap_or_else(PoisonError::into_inner)`; colours: `Color32::from_rgba_unmultiplied` constants |
| `liquers-lib/src/egui/mod.rs` `Mutex` locks | 2 | `unwrap_or_else(PoisonError::into_inner)` |
| `liquers-lib/src/polars/filtering.rs` unix epoch | 1 | `NaiveDate::from_ymd_opt(1970, 1, 1).ok_or_else(…)?` (the function returns `Result`) |

Lint attribute, first inner attribute of each crate root: `liquers-core`, `-macro`, `-store`,
`-records`, `-template`, `-lib`, `-axum`, `-mcp`, `-agent` (`lib.rs` only, not `main.rs`), `-py`,
`-web` (after its `#![cfg(target_arch = "wasm32")]`).

CI: `scripts/check-build-matrix.sh` `check()` runs `cargo clippy` (Q3) where it runs `cargo check`;
label text and the disk guard are unchanged. `.github/workflows/build-matrix.yml` installs the
`clippy` component on the toolchain step.

## Error Handling

Only the four legacy-metadata sites and the two registry sites gain an error path:
`ErrorType::SerializationError` via `Error::from_error` (a `Status` or `LogEntry` that does not
serialize) and `Error::unexpected_error` (a registry that lost an entry it just inserted). Neither is
reachable; both propagate through functions that already return `Result`. No `Error::new`.

## Relevant Commands

None added or changed. `egui/` and `polars/` command bodies are not `version: auto` functions (the
edited code is in helpers and widgets), so `specs/command_registry.yaml` does not change; Step 6
verifies it with `registry_export`.

## Documentation Architecture

- `CLAUDE.md` (agent instructions, docs): "Building and testing" adds `cargo clippy -p <crate>
  --lib`; "Do NOT" cites the lint and the `reason` exception. No History table in `CLAUDE.md`.
- `.claude/skills/rust-best-practices/SKILL.md`: the hard rule names `clippy::unwrap_used` /
  `expect_used` and the `allow(…, reason)` form.
- `scripts/check-build-matrix.sh` header comment: why clippy, not check.
- Proposed `affects_docs`: `CLAUDE.md`, `.claude/skills/rust-best-practices/SKILL.md`.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | the files in the table; eleven `lib.rs`; `scripts/check-build-matrix.sh`; `build-matrix.yml` |
| Crates and workflows affected | core, lib, macro (code); every library crate (attribute); CI |
| Existing tests likely to change | none; they are the proof of AC-4 |
| New validation | clippy per crate (AC-1, AC-2); three focused unit tests (Phase 3) |
| Compatibility | the macro change turns a non-finite float default (`f64::NAN` as a default literal; no registered command has one) into JSON `null` instead of `0.0`; every finite default is unchanged |
| Concurrency / performance | `PoisonError::into_inner` keeps a UI responsive after a panicked frame instead of panicking again — strictly better |
| Security | none |
| Feature gating | a site under a feature not linted is invisible; the matrix covers every `liquers-lib` feature row, which is why Q3 prefers it |
| CI cost | clippy re-checks every row; expected within a few percent of `cargo check` (no codegen in either) |
| Recovery | each slice is one commit; dropping a crate's attribute line disables enforcement there |
| Certainty | high for the rewrites (all opened); medium for clippy's treatment of proc-macro output, which is why the macro is fixed at the source |
