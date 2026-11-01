# Phase 4: Implementation Plan — Markdown navigation on a Liquers block parser

## Overview

The parser crate comes first and is tested on its own (it changes nothing else). Then the
`ns-md` commands, which are additive. Then the existing callers move onto the parser one module
at a time, each behind its own unchanged tests, and the line scanner is deleted last. Registry,
build matrix and documents close the work. No prerequisite issue.

## Progress

- [x] Step 1: create the `liquers-markdown` crate: blocks — 9a6e60d2
- [x] Step 2: sections, addresses and lookups — 9a6e60d2
- [x] Step 3: inline links and code spans — 9a6e60d2
- [x] Step 4: the `ns-md` commands and the `markdown` feature — d324a8c6
- [x] Step 5: move `md_split`, `ns-mem` and `markdown_tiers` onto the parser — d324a8c6
- [x] Step 6: registry, build matrix, workspace documents — d324a8c6

## Implementation Steps

### Step 1: create the `liquers-markdown` crate: blocks

- Files / symbols: `liquers-markdown/Cargo.toml` (`liquers-core`; dev: `liquers-records`),
  `src/lib.rs`, `src/block.rs` (`Span`, `Block`, `BlockKind`, `FrontMatterFormat`, `parse_blocks`);
  `tests/specs_round_trip.rs`; root `Cargo.toml` `members` and `default-members`.
- Change: the block scanner of Phase 2's parsing rules; `Document::parse`, `text`, `blocks`,
  `front_matter`.
- Depends on: none.
- Proof: `cargo test -p liquers-markdown` — the `block.rs` tests and `every_spec_document_round_trips`.
- Rollback: delete the crate and its workspace lines.

### Step 2: sections, addresses and lookups

- Files / symbols: `src/section.rs` (`Section`, tree building, numbers, slugs, `section_text`,
  `find_by_heading`, `title`, `first_paragraph`), `src/address.rs` (`Step`, `Address`,
  `Document::resolve`).
- Change: as Phase 2; `Section` also carries `body_end_line`, the line before its first
  subsection, for `ns-mem/search unit=section`.
- Depends on: Step 1.
- Proof: `cargo test -p liquers-markdown` — the `section.rs` and `address.rs` tests.
- Rollback: revert the two files.

### Step 3: inline links and code spans

- Files / symbols: `src/inline.rs` (`Link`, `LinkKind`, `TargetKind`, `CodeSpan`, `Document::links`,
  `code_spans`, `Link::resolve`).
- Change: as Phase 2; inlines are scanned in paragraph, list, quote, table and heading blocks.
- Depends on: Step 2 (a link's `section`).
- Proof: `cargo test -p liquers-markdown` — the `inline.rs` tests.
- Rollback: revert the file.

### Step 4: the `ns-md` commands and the `markdown` feature

- Files / symbols: `liquers-lib/Cargo.toml` (`records` adds `dep:liquers-markdown`;
  `markdown = ["records"]` in `default`), `liquers-lib/src/markdown/{mod,commands}.rs`,
  `register_markdown_commands!` and its stand-in, `register_all_commands!`, `src/lib.rs`;
  `liquers-lib/tests/markdown_commands.rs`.
- Change: `outline`, `section`, `sections`, `links`, `table` as Phase 2.
- Depends on: Step 3.
- Proof: `cargo test -p liquers-lib --test markdown_commands`.
- Rollback: drop the module, the feature and the macro call.

### Step 5: move `md_split`, `ns-mem` and `markdown_tiers` onto the parser

- Files / symbols: `search/md_split.rs` (`md_split` rows from `Document::sections`; the scanner
  replaced by adapters over `Document`), `mem/read.rs`, `write.rs`, `rules.rs`, `check.rs`
  (`dead_links` on `Document::links`); `tiers.rs` and `store_config.rs` (`build_markdown_tiers`)
  keep calling the adapters; `liquers-agent` (`Cargo.toml` features, `tools/memory.yaml`,
  `tools/query-guide.md`).
- Change: internals only; command signatures unchanged.
- Depends on: Step 4, which gives `liquers-lib` its `liquers-markdown` dependency.
- Proof: `cargo test -p liquers-lib --lib --tests` with the regression tests of Phase 3 unchanged,
  and the new `md_split_counts_setext_and_ignores_html_headings` and
  `dead_links_checks_reference_links`; `cargo test -p liquers-agent`.
- Rollback: revert one module at a time; the scanner is deleted only when all have moved.

### Step 6: registry, build matrix, workspace documents

- Files / symbols: `liquers-lib/src/bin/export_command_registry.rs` group list,
  `specs/command_registry.yaml` (regenerated, CHANGELOG line), `tests/registry_export.rs` gate,
  `scripts/check-build-matrix.sh` (`markdown` row, `liquers-markdown` wasm32 check).
- Change: as Phase 2's integration points.
- Depends on: Steps 4 and 5.
- Proof: `cargo test -p liquers-lib --test registry_export`; the new build-matrix rows run one
  at a time; `cargo check -p liquers-markdown --target wasm32-unknown-unknown`.
- Rollback: regenerate the registry from the reverted code.

## Testing Plan

Each step runs its own crate's tests. After Step 5: the default `liquers-lib` loop, then
`--no-default-features --features records` and `--features mem` (the parser without `ns-md`),
then `liquers-agent`. Build-matrix rows run singly, deleting `target/` when the session disk runs
low (`BUILD-MATRIX-EXHAUSTS-SESSION-DISK`).

## Rollback Plan

The crate and `ns-md` are additive, so reverting Steps 4–6 leaves a working tree with an unused
crate. If only some callers moved in Step 5, the scanner stays until the last one moves; nothing
depends on both at once.

## Documentation Updates

- New `specs/reference/MARKDOWN.md` (block model, sections and addresses, commands, fields).
- `specs/reference/AGENT_MEMORY_SERVICE.md` §4, `specs/reference/PROJECT_OVERVIEW.md`, `CLAUDE.md`
  (structure, dependency flow, where code goes, feature list), the `liquers-agent` query guide,
  `specs/README.md` — each reference change with a History row and a `reviewed:` bump.
- Issues: `NO-MARKDOWN-NAVIGATION-COMMANDS` closed for this slice or narrowed;
  `NO-QUERY-AWARE-MARKDOWN-PARSER` records what landed.
- Regenerated: `specs/command_registry.yaml`, `specs/index.csv` and the indexes
  (`scripts/docs_index.py`).

## Phase 5 Entry Criteria

- [ ] Implementation finished and validated
- [ ] User and review comments answered
- [ ] Documentation checkable against implemented and tested behaviour
