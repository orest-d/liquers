# Phase 5: Documentation - Store Conformance Backlog

## Completion Preconditions

- [x] Implementation is finished and validated
- [x] All user comments are answered or incorporated
- [x] All review comments are answered or incorporated
- [x] Documentation is consistent with implemented and tested behavior
- [x] Documentation is included in the implementation PR when practical (PR #74)

## Implementation Summary

All seven issues are resolved, and every parking mechanism the conformance suite had accumulated
is gone:
- no rule reports `Blocked`;
- no suite carries an `AllowedFailure`;
- C9 runs in a real browser;
- no unit test reuses a rule ID.

Every in-tree store now passes every rule it runs, across 43 rules. The full table is in
[`STORE_IMPLEMENTATION_GUIDE.md` §9](../../guides/STORE_IMPLEMENTATION_GUIDE.md).

| Area | What shipped | Commits |
|---|---|---|
| A. `children` (§2) | Directory metadata lists its direct children, one level deep. The `AsyncStore` default `get_asset_info` answers a directory without reading its metadata, which bounds the depth. OpenDAL fills `children` in both directory branches through one helper. `dir07` is live. | `1db2274`, `d8db873`, `bebc8c7` |
| B, C. `JsStore` | `null`/`undefined` from `get` or `getMetadata` means `KeyNotFound`, and a thrown value means `KeyReadError`. After an "absent" answer, `isDir` yields directory metadata. The protocol docs, the TypeScript declaration and the README say so. C10 has no allowed failures. | `9d504ad` |
| D. Metadata-only keys (§8) | `AsyncFileStore::listdir` lists the implied key of a sidecar. The new rule `sidecar04` requires such keys to be listed, unless the store refuses the write with `KeyNotFound`. | `888e2dc`, `579c99d` |
| D′. `LocalStorageStore` | `removedir` on an absent directory is `Ok`, relative keys are refused, `keys()` follows §9, and metadata-only keys are indexed. C9 is in `store_conformance_browser_CONF.rs`. | `0f040e7` |
| E. Test IDs | 12 renames and a `refute_<rule id>_…` convention. D1 fails on any `fn <family><digits>_`. Two duplicates were deleted, after break-and-fail trials. | `9cc750f`, `6233acf` |
| F. `http` media type | No store change. `STORE10` asserts the metadata level model. | `671121c` |

The approved design is followed, with five deviations, all found during implementation:
1. **`sidecar04` is gated on `Directories` as well as `StoredMetadata` and `Write`.** The trait
   defaults (C4) declare no directory support, and their `contains` consults only `is_dir`. "Listed
   by its parent" cannot apply to a store with no listing.
2. **The deletion trial for `dir02` made the rule `Errored`, not `Failed`.** `Errored` is also a
   defect outcome and fails the suite, so the unit test was deleted as a duplicate. The criterion
   written in Phase 3 said `Failed`.
3. **Several plan commands needed a feature flag.** `cargo test -p liquers-core --lib store_conformance`
   runs zero tests without `--features store-conformance`, because the module's tests are
   feature-gated. The commands actually run all passed the feature.
4. **The `liquers-lib` loop and the build matrix ran on Rust 1.95.** The container's 1.94 cannot
   build `liquers-lib` (`BUILD-SYSINFO-REQUIRES-NEWER-RUSTC`).
5. **C9 ran through `NO_HEADLESS=1` with Playwright's Chromium 141.** The container's chromedriver
   (147) does not match its Chromium, and `liquers-web/README.md` documents this route.

The synchronous `FileStore` is deliberately unchanged (`CORE-SYNC-STORE-TRAIT-OBSOLETE`), as the
preflight in Phase 2 decided.

## Documentation Delivered

### New Reference Documents

None. The contract already existed; it is now consistent (Phase 1 decision).

### New Guide Documents

None. The guidance extends the existing store guide.

### Existing Documents Reviewed or Updated

`affects_docs`, which is authoritative:
- `STORE_SEMANTICS.md`:
  - §2 is settled, with its depth bound and width cost.
  - §4 says a delegating store must be able to express absence.
  - §8 says metadata-only keys are enumerable, with the two consequences for callers.
  - History rows for steps 4 and 6 and for Phase 5; `reviewed: 2026-09-30`.
- `STORE_IMPLEMENTATION_GUIDE.md`:
  - §2 covers delegated absence.
  - §5 has a new subsection, "Naming your tests".
  - §8 lists `sidecar04`.
  - §9 is rewritten from the final reports, and now says honestly that the table is maintained by
    hand.
  - History rows; `reviewed: 2026-09-30`.
- `LANGUAGE-INTEGRATION_GUIDE.md`: the STORE section says a language-defined store needs a way to
  express absence. History row added; `reviewed: 2026-09-30`.

Other candidates by area were reviewed and left unchanged, because nothing in them depends on
`children`, sidecar listing or the `JsStore` protocol:
- `ENVIRONMENT_CONFIG.md`
- `PROJECT_OVERVIEW.md`
- `STORE_FACTORY_GUIDE.md`
- `ENVIRONMENT_CONSTRUCTION_GUIDE.md`
- the web and axum API documents
- `RECORD_STREAMS.md`
- `COMMAND_DECLARATION.md`

Outside `specs/`: `liquers-web/README.md` (step 12), the `JsStore` module docs, and
`typescript.rs`.

### Links and Capability Map

The "Store behavioural semantics" entry in `specs/README.md` now links the guide and this design.
Both had been listed as unplaced, and the regenerated map drops them from that list. The three
stub designs this one absorbed are `superseded` and point here. Their `readiness:` labels were
removed, because readiness only applies to a live design.

## Issues Filed

- `STORE-GUIDE-STATUS-TABLE-HAS-NO-GENERATOR` (P3). The guide's §9 table claimed to be generated,
  and no generator exists.
- `CORE-STORE-ROUTER-DIRECTORY-ABOVE-MEMBERS-HAS-NO-METADATA` (P3). The router returns
  `KeyNotFound` for a directory above its members' prefixes. Found by the final review.
- `JS-STORE-WRAPPER-HAS-NO-EFFECTIVE-MEDIA-TYPE` (P3). A page cannot ask for the effective media
  type; this is the remainder of area F.
- `WEB-OBJECT06-EXPECTS-A-STALE-ERROR-TYPE-COUNT` (P2). This test already failed in the
  `liquers-web` Node loop before this work, and it is unrelated to this design.

Not filed, because it already exists: `JS-STORE-RESOURCE-NOT-FOUND-WITHOUT-A-RECIPE` is the one e2e
failure (`STORE07`). It is in the asset layer and outside this group.

## Important Learning

- **Two of the seven issues were narrower than written.**
  - The `JsStore` sentinel already existed for `get`.
  - The `http` store was correct; its test predated the metadata level model.

  Reading the code before designing turned two fixes into a documentation change and a test
  correction.
- **A new rule found a real divergence at once.** `sidecar04` also failed `LocalStorageStore`,
  whose index ignored metadata entries. That gap was not in any issue. Writing the rule was cheaper
  than auditing every store by hand.
- **A rule is only proven by a store that breaks it.** Every changed or new rule has a
  `refute_<id>_…` test. The same check is what separated real duplicate unit tests from tests that
  only looked duplicated.
- **Feature-gated test files pass vacuously.** Built without their feature, they compile to zero
  tests and report `ok`. A validation counts only with a non-zero `running N tests` line.

## Conformance and Remaining Work

The requested, approved and implemented scope match, apart from the deviations above. Nothing in
scope remains. Follow-up work is represented only by the issues filed above.

## Validation

- **Native:**
  - `liquers-core` with `store-conformance`, `--lib --tests`: all green.
  - `liquers-store` with `store-conformance`: all green.
  - `liquers-lib --lib --tests` on Rust 1.95: 32 suites green.
  - `scripts/check-build-matrix.sh` on Rust 1.95: 32 of 32 configurations OK.
- **`liquers-web`:**
  - Node loop: every suite green except the pre-existing `OBJECT06`.
  - Browser, C9: 30 of 43 rules, 0 failed.
  - Browser, `store_local_STORE`: 11 of 11.
  - e2e: 15 passed, 1 pre-existing failure (`STORE07`).
- **Documentation:** `python3 scripts/docs_index.py --check` reports 0 errors, and D1 is green,
  including the new test-name scan.
