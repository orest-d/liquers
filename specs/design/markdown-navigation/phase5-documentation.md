# Phase 5: Documentation - markdown-navigation

## Completion Preconditions

- [x] Implementation is finished and validated
- [x] All user comments are answered or incorporated (results presented; maintainer: "proceed all", 2026-11-01)
- [x] All review comments are answered or incorporated
- [x] Documentation is consistent with the implemented and tested behavior
- [x] Documentation is included in the implementation PR when practical

## Implementation Summary

Implemented as designed, in PR #228:

- **`liquers-markdown`**, a new crate on `liquers-core`. It provides the lossless block layer, the
  section tree with numbers, slugs and addresses, and inline links and code spans with query
  recognition.
- **The `ns-md` commands** in `liquers-lib/src/markdown/`: `outline`, `section`, `sections`,
  `links` and `table`, behind the new default-on `markdown` feature.
- **`ns-search/md_split`, `ns-mem` and `markdown_tiers`** now read Markdown through the parser,
  and the line scanner is gone. `search/md_split.rs` keeps thin adapters over `Document`.

Departures from Phase 2, all recorded in the phases:

- `table` takes no `context`, because the Markdown table reader never warns.
- `Link::resolve` returns `Resolved::{Key, OutsideRoot}`, so `dead_links` still reports a link
  that climbs above the root as dead.
- The scanner's helpers became adapters rather than being deleted.
- The agent binary gets `ns-md`: its feature list, its query tool's allow list and its query
  guide. This followed the Phase 4 review.

## Documentation Delivered

### New Reference Documents

`specs/reference/MARKDOWN.md` covers:

- the block rules;
- sections and the address rule, with the warning that section numbers move;
- link and target kinds;
- the five commands and their fields.

### New Guide Documents

None. The commands are queries over one document, so the reference and the agent's query guide
cover them.

### Existing Documents Reviewed or Updated

`affects_docs`, each change with a History row and a `reviewed:` bump where the document has them:

- `specs/reference/AGENT_MEMORY_SERVICE.md` §4: where sections, tiers and dead links now come
  from, and navigating a document with `ns-md`.
- `specs/reference/PROJECT_OVERVIEW.md`: the crate and the dependency flow.
- `CLAUDE.md`: the structure, the dependency flow, where code goes, the feature list, test
  commands and the seven registry groups.
- `liquers-agent/tools/query-guide.md`: a section on navigating one document.

Also updated:

- `specs/command_registry.yaml`, regenerated with a CHANGELOG line;
- `scripts/check-build-matrix.sh`.

### Links and Capability Map

`specs/README.md` gains:

- a quick-start row for reading a document's outline, a section, its links or a table;
- a capability line for Markdown navigation.

## Issues Filed

- `MARKDOWN-CODE-BLOCKS-FRONT-MATTER-AND-BACKLINKS-ARE-NOT-COMMANDS` covers the rest of
  `NO-MARKDOWN-NAVIGATION-COMMANDS`: fenced blocks, front matter, folder input and incoming links.
- `NO-MARKDOWN-NAVIGATION-COMMANDS` is closed with this slice.
- `NO-QUERY-AWARE-MARKDOWN-PARSER` records which of its requirements landed.

## Important Learning

- `liquers-validate` shows what a query means, not that an address resolves. The first draft's
  `phase~.2.solution` could not resolve, because a document's title is section 1. A test in
  `liquers-markdown/tests/specs_round_trip.rs` (`documented_addresses_resolve`) now resolves the
  documented examples against the real files.
- Comparing the old `dead_links` scanner with `Document::links` over `specs/` gave the same 2442
  relative targets, so moving `check` onto the parser changes nothing there today.
- `~~` is refused inside a resource key but accepted in an action parameter.
- An evaluation that fails in a command returns an error state rather than `Err`, so tests check
  `State::error_result`.

## Conformance and Remaining Work

Requested and approved: AC-1 to AC-12. All are implemented and each is tested (Phase 3 test plan).

Remaining, each with an issue:

- the second command slice;
- the rest of the parser: nesting, emphasis, Mustache, editing and rendering;
- a wasm32 build of `liquers-markdown`. Its matrix row is added, but this session had no wasm32
  target installed, so CI or the matrix script runs it.

## Validation

- `cargo test -p liquers-markdown`: 32 tests, including the round trip over every
  `specs/**/*.md`.
- `cargo test -p liquers-lib --lib --tests`: all suites pass, including the new
  `markdown_commands` (8 tests) and `dead_links_checks_reference_links`.
- `cargo test -p liquers-agent`: passes.
- `cargo check -p liquers-lib --no-default-features [--features records | mem | markdown] --lib
  --tests`: passes.
- `register_all_commands` and `search_commands`, run with `records` and without `markdown`:
  pass.
- `scripts/docs_index.py --check`: no errors.
