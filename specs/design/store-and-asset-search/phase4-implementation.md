# Phase 4: Implementation Plan — Store and asset search

> Written 2026-10-11 under the pre-approval given after Phase 3 (`proceed all`). Decisions met
> while implementing go to Phase 1's `## Design Readiness`. A discovery that changes an approved
> contract stops the work and returns to that phase's gate.

## Overview

The build runs bottom-up, each step proven before the next:
1. the pure kernels in `liquers-records` (Steps 1–6), which build and test without `liquers-lib`;
2. the shared conversion helpers and registration in `liquers-lib` (Step 7);
3. the five commands, one per step (Steps 8–12);
4. the generated registry and the feature matrix (Steps 13–14);
5. the documents (Step 15).

No prerequisite issue blocks the work (Phase 2 preflight). Signatures re-checked at HEAD
`e748168`:
- `impl dyn RecordView`: `filter`, `take` and `with_columns(fields, columns)` (`views.rs`);
- `record_stream(inner, schema)` and `RecordStreamExt::materialize` (`batch.rs`);
- `RecordSource` (`value.rs`).

Builds follow `CLAUDE.md`: `CARGO_INCREMENTAL=0`, and no `--workspace`.

## Progress

- [x] Step 1: `liquers-records` dependencies and `FieldSchema::media_type` — 0ff76d2
- [x] Step 2: predicate trait, AST, grammar and parser — e1cb4a7
- [x] Step 3: matching kernels and binding — e1cb4a7
- [x] Step 4: ranking, `search_view` and `SearchReport` — e1cb4a7
- [x] Step 5: structure parsing — e1cb4a7
- [x] Step 6: `FilteredSource` — e1cb4a7
- [x] Step 7: `liquers-lib` search module and shared helpers — b70227a
- [x] Step 8: `catalog`, and registration — b70227a
- [x] Step 9: `field_extract` — b70227a
- [x] Step 10: `md_split` — b70227a
- [x] Step 11: `search` — b70227a
- [x] Step 12: `commands` — b70227a
- [x] Step 13: exporter group, `full_registry`, regenerated `command_registry.yaml` — 1a52fcf
- [x] Step 14: build matrix — all 32 configurations OK; `records`-only `search_commands` 22/22; default loop green (2026-10-11)
- [x] Step 15: documentation — ea6cf01

## Implementation Steps

Steps 1–6 are proven with `cargo test -p liquers-records --all-features --lib search::<module>`.
The filter is a substring match on `search::<module>::tests::…`. Steps 8–12 are proven with
`cargo test -p liquers-lib --test search_commands <prefix>`.

### Step 1: `liquers-records` dependencies and `FieldSchema::media_type`
- Files: `liquers-records/Cargo.toml`; `liquers-records/src/schema.rs` (`FieldSchema`,
  `FieldSchemaSpec`, `with_media_type`).
- Change: add `nom = "8.0.0"`, `nom_locate = "5.0.0"`, `regex = "1.13.1"` and `toml = "0.8"`. Add
  `media_type: Option<String>` (`#[serde(default, skip_serializing_if = "Option::is_none")]`).
- Proof: `cargo test -p liquers-records --all-features --lib schema`, including
  `field_schema_media_type_round_trips`.
- Rollback: revert the two files.

### Step 2: predicate trait, AST, grammar and parser
- Files: `liquers-records/src/search/{mod,predicate,syntax}.rs`; `liquers-records/src/lib.rs`.
- Change: the Phase 2 signatures. The parser runs over `LocatedSpan`, with a hand-written regex
  scanner.
- Proof: `--lib search::syntax`.
- Rollback: drop the module.

### Step 3: matching kernels and binding
- Files: `liquers-records/src/search/kernels.rs`.
- Change:
  - `search_fields`, and field paths through `resolve_field`;
  - substring, glob and regex masks;
  - typed compares, with lexicographic compares on Text;
  - JSON-declared columns;
  - warnings.
- Proof: `--lib search::kernels`.
- Rollback: revert the file.

### Step 4: ranking, `search_view` and `SearchReport`
- Files: `liquers-records/src/search/rank.rs`.
- Change: BM25 (`phase2-details.md` §D), `matched` and `excerpt`, a stable sort, `with_columns`,
  and the limit.
- Proof: `--lib search::rank`.
- Rollback: revert the file.

### Step 5: structure parsing (can run in parallel with Steps 2–4)
- Files: `liquers-records/src/search/structure.rs`.
- Change: `parse_structure`, with explicit TOML and YAML conversions.
- Proof: `--lib search::structure`.
- Rollback: revert the file.

### Step 6: `FilteredSource`
- Files: `liquers-records/src/search/source.rs`.
- Change: the `RecordSource` impl. A stop at `limit` part-way through a chunk cuts that view to
  exactly `limit` rows.
- Proof: `--lib search::source`.
- Rollback: revert the file.

### Step 7: `liquers-lib` search module and shared helpers
- Files: `liquers-lib/src/search/mod.rs` (new, `#[cfg(feature = "records")]`), with `folder_of`,
  `read_text` and `is_text_media_type`; `liquers-lib/src/lib.rs`.
- Proof: `cargo build -p liquers-lib`.
- Rollback: drop the module.

### Step 8: `catalog`, and registration
- Files: `liquers-lib/src/search/catalog.rs`; `register_search_commands!` in
  `liquers-lib/src/search/mod.rs`; `liquers-lib/src/commands.rs`. In the latter, it is invoked
  inside `register_all_commands!` beside `register_records_commands!`, with a no-op stand-in in
  the ungated part of the file; `liquers-lib/tests/search_commands.rs` holds the fixture store.
- Change: a bounded breadth-first walk, `read_text`, and the title fallback.
- Proof: `--test search_commands catalog`, and `--test register_all_commands`. The latter asserts
  `has("search", "catalog") == cfg!(feature = "records")`, following the file's own pattern.
- Rollback: revert the files.

### Step 9: `field_extract`
- Files: `liquers-lib/src/search/extract.rs`.
- Proof: `--test search_commands field_extract`, by asserting columns directly. Assertions that go
  through `search` run at Step 11.
- Depends on: Steps 5 and 8.

### Step 10: `md_split`
- Files: `liquers-lib/src/search/md_split.rs`.
- Proof: `--test search_commands md_split`, as for Step 9.
- Depends on: Step 8.

### Step 11: `search`
- Files: `liquers-lib/src/search/search_command.rs` (named so as to avoid `module_inception`), with
  `search_input`.
- Proof: `--test search_commands search`, plus the deferred search assertions of Steps 9–10.
- Depends on: Steps 2–10.

### Step 12: `commands`
- Files: `liquers-lib/src/search/commands.rs`.
- Change: `query` is `ns-<namespace>/<name>`, or the bare name for the root or empty namespace;
  the realm is ignored.
- Proof: `--test search_commands commands`.
- Depends on: Step 11.

### Step 13: exporter group, `full_registry`, regenerated `command_registry.yaml`
- Files:
  - `liquers-lib/src/bin/export_command_registry.rs`: a `Search` group, so `Group::all` grows to
    7;
  - `liquers-lib/tests/registry_export.rs`: `full_registry` plus a non-empty assertion;
  - `specs/command_registry.yaml`, with a CHANGELOG line.
- Why here: before this step, `full_registry` and the YAML both lack `search`, so
  `committed_registry_is_fresh` stays green throughout Steps 8–12.
- Proof: `cargo test -p liquers-lib --test registry_export`. Then re-run the Phase 3 queries
  without the overlay, and delete `search-commands.registry.yaml`.
- Rollback: restore the previous YAML.

### Step 14: build matrix
- Proof: `bash scripts/check-build-matrix.sh` (wasm32 `webui,records` included), and
  `cargo test -p liquers-lib --no-default-features --features records --test search_commands`.

### Step 15: documentation
- Create `specs/guides/SEARCH_GUIDE.md`; its queries are listed in `guide_queries_run`, and none
  uses an unbuilt `record-filter-commands` command.
- Create `specs/reference/SEARCH.md`, which takes over `phase2-details.md` §A–F.
- Update `RECORD_STREAMS.md` and `COMMAND_REGISTRATION_GUIDE.md`, each with a History row and a
  `reviewed:` bump.
- Update `specs/README.md`, where "three commands" becomes five and the capability becomes built.
- Proof: `python3 scripts/docs_index.py --check`, and `guide_queries_run`.

## Testing Plan

Each step runs its own Phase 3 tests. After Step 6:
`cargo test -p liquers-records --all-features --lib --tests`. After Step 13, the default loop:
`cargo test -p liquers-lib --lib --tests`. After Step 14, the matrix. The commands of Phase 3 §Test
Plan are the final check.

## Agent Assignment

One implementer, sequentially. Step 5 may run in parallel with Steps 2–4. Each step is proven by its
own tests before the next starts.

## Rollback Plan

Everything is additive: a new module in each crate, one optional schema field, and new commands.
Reverting the branch removes it cleanly. If only Steps 1–6 land, `liquers-records` has an unused
search module and nothing user-visible. If only some commands land, the registry and documents
describe exactly those commands, and the rest become an issue (§5.6).

## Documentation Updates

| Document | Change |
|---|---|
| `specs/guides/SEARCH_GUIDE.md`, `specs/reference/SEARCH.md` | Created (Step 15) |
| `RECORD_STREAMS.md`, `COMMAND_REGISTRATION_GUIDE.md` | Updated, each with a History row and a `reviewed:` bump |
| `specs/command_registry.yaml` | Regenerated, with a CHANGELOG line |
| `specs/index.csv`, `specs/README.md` | Regenerated; the capability line changes to built |
| `RECORD-SOURCE-WRAPPERS-UNSPECIFIED` | Updated in Phase 5: the filtering half is done; the mapping wrapper stays open |

## Phase 5 Entry Criteria

- [x] Implementation finished and validated
- [x] User and review comments answered
- [x] Documentation checkable against implemented and tested behaviour
