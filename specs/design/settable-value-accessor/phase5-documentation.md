# Phase 5: Documentation - settable-value-accessor

## Completion Preconditions

- [x] Implementation is finished and validated (Phase 4 Steps 1–12; see Validation)
- [x] All user comments are answered or incorporated (the corrections on roots, language levels,
  documentation, serializable accessors and the web API's configurable, opt-in prefix)
- [x] All review comments are answered or incorporated (none open on PR #274 at the time of writing)
- [x] Documentation is consistent with the implemented and tested behavior
- [x] Documentation is included in the implementation PR (#274)

## Implementation Summary

A **settable value accessor** is a live handle on the place a value lives. It is implemented as the
Phase 2 architecture describes: an `AccessorRoot` does the I/O, synchronous `ProjectionStep`s lens
into parts, and one `AccessorHub` per root keeps every projection in step. Every read carries a
fingerprint (anchor and content) whose token is the HTTP `ETag` and the Python and JavaScript token.
A set is conditional on the part's own fingerprint, so the first writer wins and disjoint parts never
conflict. A structural change invalidates the projections it moved. Events go through a `watch`
channel (`Changed`, `Invalidated`, and the relayed status, progress, log and removal).

It conforms to the request:

- **Get and set:** `get`, `set`, `set_if`.
- **External updates, expiry and deletion:** relayed from keyed assets and other assets; the store
  root is checked on `refresh`.
- **Overlapping projections:** `recipes.yaml` → recipe → query → action → argument, plus document
  fields and items, all validated through the pipeline editors.
- **First-class roots, as corrected:**
  - store, in its binary, metadata and value views, read-write, with status `None` for a key that
    does not exist (`get` fails, a set creates it);
  - keyed asset, where a set is an `Override` with a recipe and a `Source` without one;
  - read-only asset;
  - memory, a variable shared by cloning.
- **Language levels:** using existing accessors is implemented for Python (`lq.Accessor`) and
  JavaScript (`Accessor`); constructing new ones from a language is filed.
- **Web API:** `/api/acc` is opt-in (`Api::Accessors`), under the server's configurable prefix, and
  refused for a value type without accessors.

**Additions beyond Phase 2**, each recorded in Phase 4's Execution Notes:

- `resolve_accessor` in core, shared by the web, Python and JavaScript layers;
- `If-Match: *` on `PUT`;
- `PipelineEditor::argument_source`;
- `SimpleValue`'s JSON forms for metadata, recipes and pipelines;
- `missing_part_error`, which reports a step's own error;
- `Environment.accessor` and `memoryAccessor` in JavaScript beside the module functions.

**Omitted, with issues:**

- locking (`VALUE-ACCESSOR-CANNOT-LOCK`);
- defining roots and steps in Python or JavaScript (`ACCESSOR-CANNOT-BE-IMPLEMENTED-IN-PYTHON-OR-JAVASCRIPT`);
- serializing an accessor for a client (`ACCESSOR-CANNOT-BE-SENT-TO-A-CLIENT`);
- `acc` commands for the record row, column and cell steps, which exist as Rust API
  (`RECORD-ACCESSOR-STEPS-HAVE-NO-COMMANDS`);
- template-link projections, which Phase 2 left for later.

## Documentation Delivered

### New Reference Documents

- `specs/reference/VALUE_ACCESSORS.md` covers:
  - root versus projection, and the hub and how it classifies a change;
  - fingerprints, tokens and conflicts;
  - the `ValueAccessor`, `AccessorRoot` and `ProjectionStep` contracts;
  - events and the holder contract;
  - the first-class accessors and their store views, and the wrappers;
  - every core and record step;
  - the accessor value and the `acc` commands;
  - the web routes;
  - the Python and JavaScript wrappers side by side.

### New Guide Documents

- `specs/guides/VALUE_ACCESSOR_GUIDE.md` covers, in Rust, Python and JavaScript where the language
  allows:
  - making an accessor from a query, in Rust, or as a memory variable;
  - getting and setting it, and projecting it;
  - listening and reacting to events;
  - handling a conflict, and using it over HTTP;
  - implementing a new root, a new step and a wrapper, with their rules.

### Existing Documents Reviewed or Updated

The authoritative `affects_docs` set: the eight listed by Phase 2 plus the two new documents. Each
change added a History row and moved `reviewed:` to 2026-11-16.

| Document | Result |
|---|---|
| `reference/VALUE_TYPE_SYSTEM.md` | updated: the `Accessor` identifier; `ExtValue::Accessor` as a justified handle that is never stored; `from_accessor` / `try_into_accessor` |
| `reference/UI_INTERFACE_FSD.md` | updated: element-owned values are memory accessors, and binding is left to the UI designs (that subsection only) |
| `reference/RECIPES_AND_PLANS.md` | updated: "Editing recipes through accessors" (the projections, validated edits, invalidation, what the YAML keeps) |
| `reference/WEB_API_SPECIFICATION.md` | updated: §11 Accessor API; §1 and §8.1 name the fifth builder and `apis: accessors` |
| `reference/PYTHON_INTEGRATION.md` | updated: §19 Accessors; §1, §2 module and classes; §6 `StatusConflictError.token` |
| `guides/LANGUAGE-INTEGRATION_GUIDE.md` | updated: the new feature `ACCESSOR` (§4 row, soft dependency, profiles; §5 section with both directions, wrapper rules and tests `ACCESSOR01`–`07`; §6 Browser JavaScript and Python entries; Appendix A blueprints) |
| `guides/PYTHON_GUIDE.md` | updated: §7 "Editing values through accessors"; §2 module and names |
| `guides/JAVASCRIPT_GUIDE.md` | updated: §7 "Editing values through accessors" (a runnable example); §2 names |
| `reference/VALUE_ACCESSORS.md`, `guides/VALUE_ACCESSOR_GUIDE.md` | new |

### Module Docs

Each module the design created or changed was reviewed against `API_DOCUMENTATION_GUIDE.md` §3:

- in `liquers-core/src/accessor/`: `mod.rs`, `hub.rs`, `root.rs`, `steps.rs` and `wrap.rs`;
- `liquers-lib/src/accessor/mod.rs` and `liquers-lib/src/records/accessor.rs`;
- in `liquers-axum/src/accessor/`: `mod.rs`, `handlers.rs` and `websocket.rs`;
- `liquers-py/src/accessor.rs` and `liquers-py/python/liquers/accessors.py`;
- `liquers-web/src/accessor.rs`.

`root.rs`'s "Where it is used" section was corrected in Phase 5 to name `resolve_accessor`.
`python3 scripts/check_api_docs.py` reports `0 warnings, 0 errors` for each file.

### User-Guide Chapters

`docs_index.py chapters-for` lists almost every chapter of the Python and Web guides, because
`PYTHON_GUIDE.md` and `JAVASCRIPT_GUIDE.md` are sources of nearly all of them. The change is
additive: no existing behavior, route, setting default or name changed. The chapters were therefore
reviewed by searching for what it touches (the server's `apis`, the value kinds, `StatusConflict`,
the module lists), and the ones that describe those were updated:

| Chapter | Result |
|---|---|
| `{rust,python,web}/04-values.md` | updated: a new 4.6 on accessors in each guide; Rust adds `Accessor` to the extended kinds (with a `// Sketch` example), Web adds it to the handles of 4.5; the Python and Web examples run |
| `{rust,python,web}/12-serving.md` | updated through `_shared/serving.md`: `apis` names the opt-in `accessors` |
| `{rust,python,web}/e-web-api.md` | updated through `_shared/web-api.md`: E.1 names the fifth API, new E.10 Accessor API |
| every other listed chapter | reviewed by search; nothing the design changed is described there, no change |

`HISTORY.md` of each guide has a row for each updated chapter. `cargo run -p liquers-docs -- check`
reports `0 errors, 0 warnings`.

### Links and Capability Map

`specs/README.md`: the "Settable value accessors" capability moved from designing to built, links
the reference and the guide, and names the four open issues. The new reference links to the guide,
`VALUE_TYPE_SYSTEM.md`, `WEB_API_SPECIFICATION.md`, `PYTHON_INTEGRATION.md`, `JAVASCRIPT_GUIDE.md`
and the language integration guide. Each updated document links back to `VALUE_ACCESSORS.md`.

## Issues Filed

- `VALUE-ACCESSOR-CANNOT-LOCK` (Phase 1): holding a value against other writers while editing it.
- `ACCESSOR-CANNOT-BE-SENT-TO-A-CLIENT` (Phase 1): a serialized accessor that a client can rebuild
  and use.
- `ACCESSOR-CANNOT-BE-IMPLEMENTED-IN-PYTHON-OR-JAVASCRIPT` (Phase 1): direction 2 of `ACCESSOR`.
- `RECORD-ACCESSOR-STEPS-HAVE-NO-COMMANDS` (Phase 5): the record steps have no `acc` command, so
  they cannot be reached from a query, the web API or the language wrappers.

## Important Learning

- **A shared handle needs a token per holder.** One query asset is one Rust handle, so `set`, which
  uses the handle's last-seen fingerprint, cannot catch another holder of the same handle. Every
  language wrapper therefore keeps its own token. The rule is in the reference §2 and in the
  language integration guide's `ACCESSOR` section, because any future wrapper needs it.
- **Events are wake-ups.** A `watch` channel coalesces, so a holder must read after an event rather
  than treat the events as a log. The holder contract (reference §4) is what makes this safe.
- **`info` never reads.** A token is therefore unknown until the first read. That is why the web
  API has `If-Match: *`, and why the WebSocket test reads before it writes.
- **Report the step's error.** A hub that reported every failed derivation as "the part does not
  exist" hid real failures; the record steps' JSON problem was found only after
  `missing_part_error` was added.
- **wasm has no spawner.** A root's watcher runs only if the host spawns `driver()`. liquers-web does
  this when a handle is created, and any other wasm host must do the same.

## Conformance and Remaining Work

| | Requested | Approved (Phases 1–2) | Implemented |
|---|---|---|---|
| get / set, conflicts (first writer wins) | yes | yes | yes |
| external updates, expiry, deletion | yes | yes (relayed; store on `refresh`) | yes |
| overlapping projections down to an argument | yes | yes | yes |
| store (3 views), keyed asset, asset, memory roots | yes (corrections) | yes | yes |
| progress and messages relayed | yes | yes | yes |
| use existing accessors from Python and JavaScript | yes | yes | yes |
| construct accessors in a language | issue | issue | issue |
| reference and guide; Value docs | yes | yes | yes |
| serializable accessors | issue | issue | issue |
| web API: configurable prefix, opt-in, refused without accessors | yes | yes | yes |
| locking | deferred | issue | issue |
| record row/column/cell | — | Rust API | Rust API; commands filed |

Nothing remains in this design. Every deferral has an issue.

## Validation

Run on 2026-11-16, after a `cargo clean`:

- `cargo test -p liquers-core --lib --tests` and `cargo test -p liquers-lib --lib --tests`: pass.
- `bash scripts/check-build-matrix.sh`: all 49 configurations OK.
- `cargo test -p liquers-axum`: the accessor tests pass.
- `bash scripts/test-liquers-py.sh`: the full Python suite, the guide examples included, passes
  after two fixes found here: the second Python guide example now builds its own accessor, and
  `StatusConflictError.token` is a documented property, which DOCS04 requires on CPython 3.10 and
  3.11, where `None` has no docstring. Stubtest (mypy 2.4.1) reports no issues.
- `liquers-web/scripts/test-guide-examples.sh`: 83 passed, 0 failed; the `JAVASCRIPT_GUIDE.md` and
  Web guide examples run under Node, the new accessor examples included.
- `cargo test -p liquers-docs` and `cargo run -p liquers-docs -- check`: pass, `0 errors, 0 warnings`.
- rustdoc with `-D rustdoc::broken_intra_doc_links` on the crates CI documents: passes.
- `python3 scripts/docs_index.py --check`: 0 errors.
- `liquers-validate` accepts every accessor query quoted in the documents: 8 queries, 8 ok.
- `python3 scripts/check_api_docs.py` on the 14 modules listed above: `0 warnings, 0 errors`.
