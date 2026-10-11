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
`19aed3c`:
- `impl dyn RecordView`: `filter`, `take` and `with_columns(fields, columns)` (`views.rs`);
- `record_stream(inner, schema)` and `RecordStreamExt::materialize` (`batch.rs`);
- `RecordSource` (`value.rs`).

Builds follow `CLAUDE.md`: `CARGO_INCREMENTAL=0`, and no `--workspace`.

## Progress

- [ ] Step 1: `liquers-records` dependencies and `FieldSchema::media_type`
- [ ] Step 2: predicate trait, AST, grammar and parser
- [ ] Step 3: matching kernels and binding
- [ ] Step 4: ranking, `search_view` and `SearchReport`
- [ ] Step 5: structure parsing
- [ ] Step 6: `FilteredSource`
- [ ] Step 7: `liquers-lib` search module, helpers and registration
- [ ] Step 8: `catalog`
- [ ] Step 9: `commands`
- [ ] Step 10: `search`
- [ ] Step 11: `field_extract`
- [ ] Step 12: `md_split`
- [ ] Step 13: regenerate `command_registry.yaml`; registry tests
- [ ] Step 14: build matrix
- [ ] Step 15: documentation

## Implementation Steps

### Step 1: `liquers-records` dependencies and `FieldSchema::media_type`
- Files: `liquers-records/Cargo.toml`; `liquers-records/src/schema.rs` (`FieldSchema`,
  `FieldSchemaSpec`, `with_media_type`).
- Change: add `nom = "8.0.0"`, `nom_locate = "5.0.0"`, `regex = "1.13.1"` and `toml = "0.8"`. Add
  `media_type: Option<String>` with `#[serde(default, skip_serializing_if = "Option::is_none")]`.
- Proof: `cargo test -p liquers-records --all-features --lib schema`, plus a new
  `field_schema_media_type_round_trips` test showing an old schema reads unchanged.
- Rollback: revert the two files; nothing else uses the field yet.

### Step 2: predicate trait, AST, grammar and parser
- Files: `liquers-records/src/search/{mod,predicate,syntax}.rs`; `liquers-records/src/lib.rs`
  (`pub mod search`).
- Change: the Phase 2 signatures for `Predicate`, `PredicateSyntax`, `Bound`, `BoundPredicate`,
  `RankTerm`, `SearchOptions`, `SearchExpr`, `Node` and `FieldTest`, plus `TryFrom<&str>`,
  `parse_lenient` and `to_syntax`.
  - The parser uses `nom` over `LocatedSpan`, with a hand-written regex scanner.
  - The `Predicate` impl is a stub that later steps complete.
- Proof: the Phase 3 tests in `syntax.rs` (`cargo test -p liquers-records --all-features --lib search::syntax`).
- Rollback: drop the module.

### Step 3: matching kernels and binding
- Files: `liquers-records/src/search/kernels.rs`; the `SearchExpr::bind` and `matches_json` impls.
- Change:
  - `search_fields` (roles, then names, then all Text columns);
  - field-path resolution through `resolve_field`;
  - substring, glob and regex masks, with case folding;
  - typed compares via `Column::parse_text` and `Column::compare`, with date widening and presence;
  - JSON-declared columns: any-element matching, and nested tests through `matches_json` with a
    per-bind parse cache;
  - warnings for unknown fields.
- Proof: the Phase 3 tests in `kernels.rs`.
- Rollback: revert the file; Step 2 still compiles with the stub.

### Step 4: ranking, `search_view` and `SearchReport`
- Files: `liquers-records/src/search/rank.rs`.
- Change:
  - BM25 with the constants of `phase2-details.md` §D;
  - `evidence` into the `matched` column, and `excerpt` skipping front-matter;
  - a stable sort through `take`;
  - `with_columns` for `score`, `matched` and `excerpt`;
  - the limit and the `SearchReport`.
- Proof: the Phase 3 tests in `rank.rs`.
- Rollback: revert the file.

### Step 5: structure parsing (can run in parallel with Steps 2–4)
- Files: `liquers-records/src/search/structure.rs`.
- Change: `StructFormat` and `parse_structure`, with explicit `toml::Value` and `serde_yaml::Value`
  conversions (`phase2-details.md` §E).
- Proof: the Phase 3 tests in `structure.rs`.
- Rollback: revert the file.

### Step 6: `FilteredSource`
- Files: `liquers-records/src/search/source.rs`.
- Change: the `RecordSource` impl with re-binding per schema, a limit stop that sets `stopped`,
  warnings, and the stream built with `record_stream`.
- Proof: the Phase 3 tests in `source.rs`, using a test-local counting source and resolver.
- Rollback: revert the file.

### Step 7: `liquers-lib` search module, helpers and registration
- Files: `liquers-lib/src/search/mod.rs` (new, `#[cfg(feature = "records")]`) with `folder_of`,
  `read_text`, `is_text_media_type` and `search_input`; `liquers-lib/src/lib.rs`;
  `liquers-lib/src/commands.rs`; `liquers-lib/src/bin/export_command_registry.rs`.
- Change:
  - `register_search_commands!`, invoked beside `register_records_commands!`, with a no-op when
    `records` is off;
  - the exporter's `Search` group;
  - `registry_export.rs::full_registry`.
- Proof: `cargo build -p liquers-lib`, then `cargo test -p liquers-lib --test register_all_commands`
  with the `find_command("", "search", "catalog")` assertion.
- Rollback: revert the registration lines; the module stays uncompiled.

### Steps 8–12: the commands

Each step adds one file under `liquers-lib/src/search/` and its registration line, then makes its
`liquers-lib/tests/search_commands.rs` tests pass (`cargo test -p liquers-lib --test search_commands`).
The fixture store and helpers arrive with Step 8. Each step rolls back by reverting its file and
its registration line.

### Step 8: `catalog`
- Files: `liquers-lib/src/search/catalog.rs`.
- Change: a bounded breadth-first walk, `read_text`, and the Markdown title fallback.
- Depends on: Step 7.
- Proof: the `catalog_*` tests.

### Step 9: `commands`
- Files: `liquers-lib/src/search/commands.rs`.
- Change: read the registry through `get_envref()`, with the generated signature in
  `description`.
- Depends on: Step 7.
- Proof: `commands_catalog_and_discovery`.

### Step 10: `search`
- Files: `liquers-lib/src/search/search.rs`.
- Change: `search_input`, the rank and source paths, `set_description`, the logs, and the wrapped
  materialize error.
- Depends on: Steps 2–8.
- Proof: the `search_*` tests, `rank_over_large_source_advises_rank_false` and
  `unknown_field_logs_warning`.

### Step 11: `field_extract`
- Files: `liquers-lib/src/search/extract.rs`.
- Change: the extension enum, folder or records input, parsing, column building, and the
  replacement rule.
- Depends on: Steps 5 and 8.
- Proof: the `field_extract_*` tests.

### Step 12: `md_split`
- Files: `liquers-lib/src/search/md_split.rs`.
- Change: sections (`.md` only; lines inside fences are never headings) and lines (any text).
- Depends on: Step 8.
- Proof: `md_split_sections_and_lines`.

### Step 13: regenerate `command_registry.yaml`; registry tests
- Files: `specs/command_registry.yaml`, with a CHANGELOG line;
  `liquers-lib/tests/registry_export.rs` (the non-empty assertion).
- Change: run the exporter, as `CLAUDE.md` describes.
- Proof: `cargo test -p liquers-lib --test registry_export`. Then re-run the Phase 3 queries
  without the overlay, and delete `search-commands.registry.yaml`.
- Rollback: restore the previous YAML.

### Step 14: build matrix
- Proof: `bash scripts/check-build-matrix.sh`, including wasm32 with `webui,records`, and
  `cargo test -p liquers-lib --no-default-features --features records --test search_commands`.
- Rollback: fix whichever `cfg` failed.

### Step 15: documentation
- `specs/guides/SEARCH_GUIDE.md` (new), whose queries are listed in `guide_queries_run`.
- `specs/reference/SEARCH.md` (new), which takes over `phase2-details.md` §A–F.
- `specs/reference/RECORD_STREAMS.md` and `specs/guides/COMMAND_REGISTRATION_GUIDE.md`, each with
  a `## History` row and a `reviewed:` bump.
- `specs/README.md`.
- Proof: `python3 scripts/docs_index.py --check`, plus `guide_queries_run`.
- Rollback: revert the documents.

## Testing Plan

Each step runs its own Phase 3 tests. After Step 6:
`cargo test -p liquers-records --all-features --lib --tests`. After Step 12, the default loop:
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

- [ ] Implementation finished and validated
- [ ] User and review comments answered
- [ ] Documentation checkable against implemented and tested behaviour
