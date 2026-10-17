---
id: LIB-BUILD-WARNING-CLEANUP
kind: design
title: A default liquers-lib build is warning-free, and the three suspicious warnings are fixed
form: compact
status: in_review
phase: implementation
readiness: ready
autofix: not-eligible
area: [lib/commands, build]
issues: [LIB-BUILD-EMITS-UNUSED-AND-UNHANDLED-RESULT-WARNINGS]
created: 2026-10-17
---
# A default liquers-lib build is warning-free, and the three suspicious warnings are fixed

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-17 backlog compaction. Phases 1-4
are written but not approved, and nothing is implemented.

## Phase 1: High-Level Design

### Purpose

About 50 warnings hide any new one. Most are unused imports; four sites hide lost behaviour, and
reading them at HEAD confirms two real bugs.

### Problem Example

- `liquers-lib/src/egui/dataframe.rs` `display_polars_dataframe`: `let columns = df.columns();` is
  taken from the *parameter* before `let mut df = df.clone()` and the sort, and the row loop reads
  `columns` — so clicking a column header changes the arrow but **the rows are never sorted**.
  Should be: rows shown in the sorted order.
- `liquers-lib/src/image/geometric.rs` `thumb`: `let filter = parse_filter_type(&method)?;` is
  never used; `thumbnail(max_width, max_height)` ignores it. `img/thumb-300-300-nearest` and
  `…-lanczos3` give the same image although `method` is registered with an `enum` of filters
  (`liquers-lib/src/image/commands.rs`). Should be: the filter applied.
- `liquers-lib/src/commands.rs` `from_yaml` (three `context.info(…)`) and
  `liquers-lib/src/ui/commands.rs` `ui_spec` (`context.warning(…)`) drop the `Result` of
  `Context::info` / `warning` (`liquers-core/src/context.rs`), which fails when the asset's service
  channel is closed. Every other command uses `?` (`search/extract.rs`, `records/table_ops.rs`).

### Scope and Acceptance Criteria

- **AC-1** Sorted rows
  - WHEN `display_polars_dataframe` is called with `sort_state = Some((i, asc))`
  - THEN the rows are read from the sorted frame
- **AC-2** Thumbnail filter
  - WHEN `img/thumb` runs on an image larger than the box
  - THEN the result fits the box with its aspect ratio, resampled with the named `method`
- **AC-3** Log results propagate
  - WHEN `from_yaml` or `ui_spec` logs
  - THEN a failed log write is returned as the command's error, as elsewhere
- **AC-4** Warning-free
  - WHEN `cargo build -p liquers-lib` (default features) and `cargo build -p liquers-lib --features cli --bins` run
  - THEN neither prints a warning for `liquers-lib`, and every configuration of
    `scripts/check-build-matrix.sh` still builds

Non-goals: warnings in other crates; `clippy` lints; `#![deny(warnings)]`.

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** not-eligible — `thumb` and `ui_spec` are `version: auto` commands, so their
  `impl_version` changes and `specs/command_registry.yaml` must be regenerated (rule 4).
- **Leading issue:** None
- **Explanation:** Each fix restores documented or plainly intended behaviour: the `method` enum
  documents the filter; the table's header shows a sort arrow; `?` is the convention.
- **Open questions:** None. *Resolved implementation detail:* `thumb` uses
  `DynamicImage::resize(max_width, max_height, filter)`, which fits within the box and keeps the
  aspect ratio, the documented contract; removing the `method` argument instead would change a
  command signature and is rejected.

### Design Dependencies

None.

### Consolidated Findings

Two of the four "suspicious" warnings are user-visible bugs (unsorted table, ignored filter). Unused
imports must be removed per configuration with care: an import used only under another feature must
get a `#[cfg]`, not be deleted — the build matrix is the proof. Size stays **S** (mechanical, one
crate), at its upper edge.

## Phase 2: Architecture

### Solution

1. `egui/dataframe.rs`: sort first, then take `columns`, `nrows` and `col_names` from the sorted
   frame (move the `let mut df = df.clone()` and sort block above them).
2. `image/geometric.rs` `thumb`: `resize(max_width, max_height, filter)` in place of `thumbnail`.
3. `commands.rs` `from_yaml`, `ui/commands.rs` `ui_spec`: append `?`.
4. Remove each unused import reported by the default build and by `--no-default-features` builds;
   where an import is used under one feature only, gate it. `cargo fix --lib -p liquers-lib
   --allow-dirty` may be used for the first pass, then the matrix decides.

### Changes

Files named above plus every `liquers-lib/src/polars/*.rs`, `egui/commands.rs`,
`image/commands.rs`, `ui/widgets/ui_spec_element.rs` (imports only). No signature, no new item.
`specs/command_registry.yaml`: regenerated `impl_version`s and a CHANGELOG line.

### Risks

Image output of `thumb` changes (filtered, not integer-sampled): cached thumbnails expire through
the version change, as intended. Certainty: high for 1-3; medium for 4 until the matrix runs.

## Phase 3: Examples and Tests

### Examples

`-R/photos/product.jpg/-/ns-img/thumb-300-300-nearest/to_png` (from
`specs/reference/IMAGE_COMMAND_LIBRARY.md`) differs from `…-lanczos3`.

### Tests

- `liquers-lib/src/image/geometric.rs` `test_thumb_uses_method_filter` — a 2-colour checkerboard
  200×100 thumbed to 50×50 with `nearest` and `triangle`: both 50×25, and their pixels differ — AC-2
- `liquers-lib/src/egui/dataframe.rs` — a private `fn sorted_frame(df, sort_state) -> DataFrame`
  extracted from step 1, tested by `sorted_frame_orders_rows` (no egui context needed) — AC-1
- existing `liquers-lib/tests/commands_doc.rs` / `register_all_commands.rs` cover `from_yaml` and
  `ui_spec` registration — AC-3
- `cargo build -p liquers-lib 2>&1 | grep -c "^warning"` is 0, and `bash scripts/check-build-matrix.sh` passes — AC-4

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `egui/dataframe.rs` `sorted_frame` + sort before `columns` — `cargo test -p liquers-lib --lib egui::dataframe`
- [ ] 2. `image/geometric.rs` `thumb` uses `resize` with `filter` — `cargo test -p liquers-lib --lib image::geometric`
- [ ] 3. `?` on the four log calls — `cargo test -p liquers-lib --lib --tests`
- [ ] 4. Unused imports removed or gated — `cargo build -p liquers-lib` shows no warning; `bash scripts/check-build-matrix.sh`
- [ ] 5. Regenerate `specs/command_registry.yaml`, CHANGELOG line — `cargo test -p liquers-lib --test registry_export`
- [ ] 6. Close the issue — `python3 scripts/docs_index.py --check`

### Validation

`cargo test -p liquers-lib --lib --tests`; the build matrix. Rollback: revert per step; step 4 is
independent of 1-3.
