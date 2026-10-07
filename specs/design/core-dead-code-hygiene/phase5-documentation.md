# Phase 5: Documentation - Account for `entities.rs` and `cache.rs`

**Status: approved** on 2026-10-07 (executed 2026-10-06, plan written 2026-10-05).

## Completion Preconditions

- [x] Phase 4 steps 1-3 complete and the Phase 3 checks pass
- [x] User and review comments answered or incorporated (none outstanding)

## Documentation Plan

### New Reference Documents

None.

### New Guide Documents

None.

### Existing Documents to Update (`affects_docs`)

| Document | Planned change |
|---|---|
| `reference/PROJECT_OVERVIEW.md` | `cache.rs` row (≈103) describes the module as legacy and names the removal issue; History row + `reviewed:` |

### Candidates Considered and Discarded

By area (`build`): `UNITTEST_GUIDE.md`, `autonomous_bulk_design.md`, `autonomous_issue_fixing.md` —
none mentions these modules. By content: `LANGUAGE-INTEGRATION_GUIDE.md` ≈244 already states the
module is legacy (correct, unchanged); `DOC_02_QUERY_LANGUAGE_REFERENCE.md` ≈98 describes
`entities` correctly (unchanged).

### Links and Capability Map

None.

### Issues to Close

`REPO-DEAD-CODE-HYGIENE` → `status: closed`, resolution carrying the Phase 3 evidence table and
naming `CORE-SYNC-STORE-TRAIT-OBSOLETE` as the owner of the deletion.

## Implementation Summary

As planned. `liquers-core/src/cache.rs` begins with the Phase 2 module doc (legacy, obsolete, do
not use, removal owned by `CORE-SYNC-STORE-TRAIT-OBSOLETE`) and no longer imports
`chrono::format`. No other code change; `entities.rs` is untouched.

The Phase 3 audit re-ran at HEAD on 2026-10-06 with the recorded result: `entities::` is used by
`escape.rs` and `bin/generate_entities.rs`; `cache` has no caller in `liquers-core` beyond
`lib.rs`'s `pub mod cache;`; its consumers are `liquers-py/src/context.rs` (compiled) and the
orphan `liquers-py/src/cache.rs`.

## Documentation Delivered

- `reference/PROJECT_OVERVIEW.md`: the `cache.rs` row now reads "Legacy synchronous cache;
  obsolete (assets cache results), removal tracked in `CORE-SYNC-STORE-TRAIT-OBSOLETE`" (and the
  size estimate ~350, matching the file); History row and `reviewed: 2026-10-06`.
- `issues/CORE-SYNC-STORE-TRAIT-OBSOLETE.md`: the scope note added on 2026-10-05 still stands;
  unchanged.
- `issues/REPO-DEAD-CODE-HYGIENE.md`: `status: closed`, resolution with the evidence table.

## Issues Filed

`PY-PYO3-REJECTS-PYTHON-3-13`: `cargo check -p liquers-py --lib` fails on a Python 3.13 host
(`pyo3` 0.21 predates 3.13) unless `PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1` is set. Found while
running this design's validation; unrelated to the change.

## Important Learning

`liquers-py` is outside every routine build loop, so its build environment can rot unseen: the
py check needed an undocumented override.

## Conformance and Remaining Work

Conforms to Phases 1-4. Remaining work is owned elsewhere: deletion of `cache.rs` by
`CORE-SYNC-STORE-TRAIT-OBSOLETE`, the orphan `liquers-py/src/cache.rs` by
`PY-MODULES-NOT-DECLARED-IN-LIB`.

## Validation

- `cargo check -p liquers-core` and `cargo check -p liquers-core --target wasm32-unknown-unknown`:
  build, no warning from `cache.rs`.
- `PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1 cargo check -p liquers-py --lib`: builds.
- `python3 scripts/docs_index.py --check`: 0 errors.
