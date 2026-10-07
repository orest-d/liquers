# Phase 5: Documentation - Effective media type from the JavaScript store wrapper

**Status: executed 2026-10-07**, after implementation (Wave 5 step 30 of
`archive/2026-10-06-p2-p3-s-implementation-order-revised.md`). Awaiting approval.

## Completion Preconditions

- [x] Implementation is finished and validated (Phase 4)
- [x] All user comments are answered or incorporated
- [x] All review comments are answered or incorporated (none yet)
- [x] Documentation is consistent with the implemented and tested behavior
- [x] Documentation is included in the implementation PR

## Documentation Plan

### New Reference Documents

None.

### New Guide Documents

None.

### Existing Documents to Update

| Document | Change |
|---|---|
| `liquers-web/README.md` (outside `specs/`) | A `getAssetInfo` paragraph beside `getMetadata`: effective values versus the raw record |
| `liquers-web/tests/stubs/valid_usage.ts` | A `getAssetInfo` usage line, so STUBS02 type-checks the new declaration |

### Candidates Considered and Discarded

`reference/STORE_SEMANTICS.md` defines store behaviour, not the JavaScript wrapper's surface.
`guides/LANGUAGE-INTEGRATION_GUIDE.md` does not list wrapper methods. So `affects_docs` stays empty.

### Issues to Close

`JS-STORE-WRAPPER-HAS-NO-EFFECTIVE-MEDIA-TYPE`.

## Implementation Summary

- `liquers-web/src/store/wrapper.rs`: `#[wasm_bindgen(js_name = getAssetInfo)] get_asset_info`,
  which calls `AsyncStore::get_asset_info` and converts the result.
- `liquers-web/src/store/mod.rs`: `asset_info_to_js_value`, serializing through JSON like
  `metadata_to_js_value`, so fields keep their serde names.
- `liquers-web/tests/store_wrapper_STORE.rs`: three tests. The effective media type of
  `data/input.csv` is `text/csv` while `getMetadata` keeps no derived `media_type`; a directory reports
  `is_dir`; an absent key rejects with `key_not_found`.

## Documentation Delivered

As planned: the README paragraph and the `valid_usage.ts` line.

## Issues Filed

`STORES-DISAGREE-ON-SEEDING-THE-DATA-FORMAT-FROM-THE-KEY` (P3). Phase 3's E1 writes `{}` and
expects `text/csv`. That holds for the HTTP store, whose `infer_metadata` seeds the format from the
extension, but not for a memory store, which leaves the format unset unless a filename is written.
The test therefore writes `{filename: "input.csv"}`, as a page does.

## Important Learning

The effective media type is only as good as the data format beneath it, and stores do not agree on
whether the key's extension supplies one.

## Conformance and Remaining Work

The seeding inconsistency above.

## Validation

- `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles`: every suite
  passes, `store_wrapper_STORE` 3 passed.
- `./liquers-web/scripts/check-stubs.sh --build`: all stub and package checks passed, including
  STUBS02 (`valid_usage.ts` type-checks).
- `python3 scripts/docs_index.py --check`: 0 errors.
