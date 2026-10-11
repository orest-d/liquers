# Phase 5: Documentation - Store and asset search

## Completion Preconditions

- [x] Implementation is finished and validated — Phase 4 Steps 1–15 ticked; tests listed under
  §Validation
- [x] All user comments are answered or incorporated — D4–D6 decided by the user on 2026-10-11;
  no open thread
- [x] All review comments are answered or incorporated — two independent reviews per phase for
  Phases 2–3, answered in the phase documents
- [x] Documentation is consistent with the implemented and tested behavior — the guide's queries
  are executed by `guide_queries_run`; the reference was checked against the code
- [x] Documentation is included in the implementation PR when practical — same branch

## Implementation Summary

Five commands in namespace `search` (feature `records`), over records:

| Command | Makes |
|---|---|
| `catalog` | A folder from the input state (root when none), recursive or one level, as records — status, key, filename, extension, title, description, size, updated, and with `content=true` the text. Never evaluates; a recipe-only key is listed as `Recipe` |
| `field_extract` | Front-matter (YAML or TOML), JSON or YAML fields as columns; reads a folder directly or takes catalog records; `prefix`, `flatten` |
| `md_split` | Markdown sections, or lines of any text file; rows carry `key`, `filename`, `n`, `line`, `end_line` |
| `commands` | The command registry as records, `query` column ready to paste |
| `search` | Filters any records (a view, a source, a folder key, a key or query value, a CSV) by a short expression; `rank=true` ranks with BM25 and adds `score`, `matched`, `excerpt`; `rank=false` filters a source lazily |

The engine lives in `liquers-records/src/search/`: the `Predicate` / `BoundPredicate` traits,
the expression grammar (a hand-written parser over `nom_locate` spans, so every error has a
position), the matching kernels, BM25 ranking, structure parsing and `FilteredSource`. The
`liquers-lib` side (`liquers-lib/src/search/`) is only the commands. `FieldSchema` gains
`media_type`, which marks JSON columns.

**Conformance.** The build matches the approved Phase 1 (revision 8) and Phase 2. AC-23 was
withdrawn and AC-27 moved to `design/record-filter-commands/` before approval, and neither is a
gap. Decisions D1–D3 were noted. D4–D6 were decided by the user: lexicographic text compare, a
bare name for the root namespace, and a cut to exactly `limit`.

**Additions, each additive and recorded:**
- `BoundPredicate::take_warnings` (D2) reports bad JSON cells.
- The exporter header leaves the `search` group out of its feature list, because `search` is
  compiled in by `records`.
- The test `guide_queries_run` reads its queries from the guide itself, rather than from a copied
  list as Phase 4 said, so that the two cannot drift apart.
- `RECORD_STREAM_GUIDE.md` joins `affects_docs` (see below).

**Omissions:** none from the approved scope.

## Documentation Delivered

### New Reference Documents
- [`reference/SEARCH.md`](../../reference/SEARCH.md) covers:
  - the grammar;
  - the matching and comparison rules;
  - the ranking constants;
  - the schemas of every command;
  - the input kinds;
  - the extension traits.

  It takes over `phase2-details.md` §A–F.

### New Guide Documents
- [`guides/SEARCH_GUIDE.md`](../../guides/SEARCH_GUIDE.md), for people and agents:
  - recipes;
  - expression writing and escaping;
  - reading results;
  - an agent section;
  - pitfalls.

  Every query in it is executed by the test suite.

### Existing Documents Reviewed or Updated
`affects_docs` lists six documents, each with a `reviewed: 2026-10-11` bump and a phase-5 History row:
- `reference/RECORD_STREAMS.md`:
  - `FieldSchema::media_type`;
  - `FilteredSource` in the `RecordSource` table;
  - roles now have a consumer;
  - Limits corrected.
- `guides/COMMAND_REGISTRATION_GUIDE.md`: new §5 *Commands that list or describe the store must not
  evaluate it*.
- `guides/RECORD_STREAM_GUIDE.md`: added in Phase 5. It explains which role to give a field so that
  search finds it, and `with_media_type`.
- The two new documents above.

Other candidates sharing an area were discarded. The `core/assets` reference documents and guides
describe behaviour this work reads but does not change, and the `docs` guides do not cover it.

### Links and Capability Map
`specs/README.md` now shows:
- the capability as *built*, linking to the reference and the guide;
- a new task row, "Find files, documents, lines or commands in a store";
- a narrative that lists five commands, not three.

## Issues Filed

During the design:
- `QUERY-API-ARGUMENTS-ONLY-IN-QUERY-PATH`
- `NO-MARKDOWN-NAVIGATION-COMMANDS`
- `NO-SOURCE-CODE-AND-API-DOC-NAVIGATION`
- `RECORD-AND-POLARS-COMMANDS-NOT-INTEROPERABLE`

The follow-up designs `external-index-sync` (external engines, semantic search) and
`record-filter-commands` (`ns-rec` filters and `sort`) carry the scope split off. During the
implementation:
- `LIB-BUILD-EMITS-UNUSED-AND-UNHANDLED-RESULT-WARNINGS`: about 50 warnings in a default build,
  three of them suspicious.
- `COMMAND-REGISTRY-HEADER-NAMES-GROUPS-NOT-FEATURES`: the header says `image`, but the feature is
  `image-support`.

Linked issues:
- `STORE-NO-CONTENT-OR-METADATA-SEARCH` stays `closed_not_planned`, superseded by this design.
- `RECORD-SOURCE-WRAPPERS-UNSPECIFIED` stays open. Its filtering half is built, as
  `FilteredSource`, and its general and mapping wrappers remain.

## Important Learning

- **Positional arguments are the usual query mistake.** `search-x-f-0` sets `case_sensitive`, not
  `limit`. An overlay registry (`--registry-file`) caught it, where `--command` would accept
  anything. Validate drafted queries against real signatures.
- **A document whose queries are not executed drifts.** Extracting them from the guide in a test
  costs a few lines.
- **An `Id` field must keep the default role.** `RecordSchema::new` refuses `keyword()` on it.
- **A listing command must not evaluate.** This is now a guide rule, pinned by
  `catalog_never_evaluates_recipe_keys`.

## Conformance and Remaining Work

The requested scope was catalog, search, commands, field extraction and splitting. All of it was
approved and is implemented. The remaining work belongs to issues and the two follow-up designs
above; nothing is partially done here.

## Validation

- `cargo test -p liquers-records --all-features --lib --tests` and the no-feature configuration
  pass; 36 search tests.
- `cargo test -p liquers-lib --test search_commands` passes 22 tests, `guide_queries_run`
  included. `register_all_commands` and `registry_export` (registry fresh, 113 commands) also
  pass.
- The default loop `cargo test -p liquers-lib --lib --tests` passes with no failures.
  `scripts/check-build-matrix.sh` reports all 32 configurations OK, wasm32 included. The
  `records`-only configuration passes `search_commands` 22/22.
- `liquers-validate` with no flags reports all 11 guide queries and all 13 Phase 3 queries Ok.
- `python3 scripts/docs_index.py --check` reports 0 errors.
