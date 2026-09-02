# Phase 5: Documentation — OpenDAL path mapping and shared directory support

## Completion Preconditions

All met on 2026-09-02:

- Steps 1-8 of the Phase 4 plan are implemented and committed (9 commits, `d9d150a`..`b44b8b8`).
- Every validation command passes: `cargo test -p liquers-core --lib` (765), `cargo test -p
  liquers-store` (43), `cargo test -p liquers-lib --lib --tests` (the full default loop),
  `bash scripts/check-build-matrix.sh` (**all 16 configurations OK**), and, after a `cargo clean`,
  `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles` (14 suites,
  all green).
- The three absolute gates held: `keyabs16` and `keyabs17` pass **unchanged**; `MEMDIR01-05` pass
  unchanged after the extraction — `git diff` reported zero lines touched in them — with `MEMDIR04`
  changed only by the separate commit that fixes `makedir`; and `SIBLING01-03` were **observed
  failing** before the fix and passing after.
- `liquers-store` builds with zero warnings; no `unwrap()` or `expect()` remains in it outside
  tests.

## Implementation Summary

### What was asked

`STORE-OPENDAL-SLASH-HANDLING`: *"keys containing `/` are not reliably addressable through an
OpenDAL-backed store"*, citing a `FIXME` and four `//TODO: create_dir` markers.

### What was implemented

**The issue's headline was right, and the design folder's first rebuttal of it was wrong.** A
reproduction on 2026-08-29 probed a single key, `sub/deeper/foo.txt`, found it correct end to end on
the filesystem backend, and restated the issue as three defects, none about slashes. A second
reproduction on 2026-09-02 probed **two directories whose names share a prefix** — `sub/` and
`subway/` — and the headline reappeared, with a defect worse than the one filed:

```
FS   removedir("sub")  = Ok        subway/ still on disk = false     ← data loss
```

`op.remove_all("sub")` is a *prefix* delete. So is `list_with("sub").recursive(true)`. Six defects,
fixed in nine commits:

| # | Defect | Fix |
|---|---|---|
| 1 | `removedir` deleted prefix-sharing siblings — reachable through `DELETE /api/store/removedir/{*key}` | `PathMap::directory`, supplying the trailing `/` |
| 2 | `listdir_keys_deep` leaked sibling keys, and `keys()` with it | same |
| 3 | `key_prefix()` returned the root key, so a prefixed store answered `is_dir`/`listdir` for every key in a router | returns `self.prefix` |
| 4 | Directory keys unaddressable where the backend has no directory objects | `has_children` + shared semantics in core |
| 5 | Path mapping spread across four methods, no round-trip guarantee | one `PathMap`, one `DecodedPath` |
| 6 | `make_sub_dirs` a no-op behind two `//TODO` markers; two `unwrap()`s; two warnings | deleted; removed; fixed |

**The scope widened once, at the gate, and it was the right call.** The directory fallback was going
to be private to `AsyncOpenDALStore`. Asked whether it belonged in core, the codebase answered:
**four stores already derived directory structure from a flat key set, no two alike** —
`AsyncMemoryStore` (refcounted `scc` index), the sync `MemoryStore` (no index, an O(n) scan per
call), `FetchStore` (immutable map from a configured key set), `LocalStorageStore` (mutable map plus
an explicit-directory set) — and `AsyncOpenDALStore` had none. A fifth private solution would have
been the fifth mistake. `liquers-core/src/store_dir_index.rs` is the shared mechanism.

### Added beyond the request

- `specs/reference/STORE_SEMANTICS.md` — the behavioural contract the defects violated.
- Two folded-in P3 issues in the same files: `OPENDAL-LOCALFS-TEST-SILENT-ON-WRONG-VALUE-TYPE`,
  `STORE-OPENDAL-WITHOUT-ASYNC-STORE-BROKEN`.
- `CORE-ASYNC-MEMORY-STORE-MAKEDIR-DOES-NOTHING`, found while writing a characterization test.
- 43 new tests.

### Deviations from the approved plan

| Deviation | Why |
|---|---|
| The round-trip corpus has no non-ASCII key | `parse_key("données/…")` fails at `HEAD` — `RESOURCE-NAME-ASCII-ONLY`. Such a key cannot reach a store to be mapped, so the corpus records the boundary instead of testing past it. |
| `ROUTER01` was split; its OpenDAL-side `is_dir` assertion became `DIR04` | Phase 4 put `ROUTER01` in step 3, but the assertion cannot pass until step 6 fixes defect 4. Sequencing error in the plan, split rather than reordered. |
| `directory_metadata_includes_children` was never added | Phase 4 R2: every in-tree store overrides `get_metadata`, so the hook would have had no consumer. Speculative API on a trait every integration implements. |
| `PathMap` does not itself refuse a suffix-ambiguous key | Phase 4 R1: `Error::key_not_supported` needs a store name an associated function cannot reach. The predicate is shared; the error is raised at the store. `CORE-ERROR-STORE-NAME-NOT-STRUCTURED` is what would close it. Documented on the type. |
| `wasm-bindgen-test-runner` had to be installed | Not present in the environment, so the wasm loop initially compiled without running. Installed at the pinned 0.2.127 and re-run. |

### Not done, deliberately

`FetchStore` and `LocalStorageStore` keep their private indexes. Both work, both are wasm-only with
their own Node/browser/Playwright loops, and migrating them is cleanup rather than repair. Recorded
as follow-up on `CORE-DIRECTORY-INDEX-NOT-SHARED`, whose requirement — that the mechanism be
*available* in core — is met.

## Documentation Delivered

| Document | Action |
|---|---|
| `specs/reference/STORE_SEMANTICS.md` | **Created.** The sibling rule; three sources of directory truth; derived vs explicit directories; absence vs failure; removal; prefixes and routing; key shape; metadata sidecars. Three questions marked ⚠ as unsettled, each naming its issue. |
| `specs/reference/STORE_CONFIG_FSD.md` | Cross-linked, `## History` row, `reviewed:` bumped. |
| `specs/README.md` | §Stores updated; the design moves to `documented`. |
| `specs/index.csv` | Regenerated. |
| Module rustdoc | `store_dir_index.rs` carries the three-sources table and the derived/explicit distinction at the point of use; `PathMap` carries the trailing-slash rule and the residual R1 gap. |

**`affects_docs`:** `[reference/STORE_SEMANTICS.md, reference/STORE_CONFIG_FSD.md]`. Candidates by
area — `ENVIRONMENT_CONFIG.md`, `STORE_FACTORY_GUIDE.md`, `DOC_01_ARCHITECTURE_REFERENCE.md`,
`LANGUAGE-INTEGRATION_GUIDE.md` — were reviewed and dropped: the first two describe configuration
and factories, and the last two describe implementing a store without asserting any of the
semantics that changed. No guide was created; Phase 2's `neither` decision held, since no repeatable
developer task was introduced.

## Issues Filed

Five closed with resolution notes: `STORE-OPENDAL-SLASH-HANDLING`, `CORE-DIRECTORY-INDEX-NOT-SHARED`,
`CORE-ASYNC-MEMORY-STORE-MAKEDIR-DOES-NOTHING`, `OPENDAL-LOCALFS-TEST-SILENT-ON-WRONG-VALUE-TYPE`,
`STORE-OPENDAL-WITHOUT-ASYNC-STORE-BROKEN`.

Four filed and left open, each with its reason on the issue:

| Issue | Pri | Why not now |
|---|---|---|
| `STORE-ASYNC-STORE-NO-BEHAVIOURAL-CONFORMANCE-SUITE` | P1/L | The umbrella, carrying all **eleven** contract divergences found. This work fixes six and documents two. The suite must run under `wasm32` too, which is what makes it `L`. |
| `CORE-ERROR-STORE-NAME-NOT-STRUCTURED` | P2/S | Changes a `serde` payload reaching `liquers-web`, `liquers-py` and the axum API. |
| `CORE-ASYNC-MEMORY-STORE-IS-SUPPORTED-IGNORES-PREFIX` | P1/S | Needs a test sweep, not a one-line edit; masked today by the router's separate prefix test. |
| `CORE-STORE-KEYS-MEANS-TWO-DIFFERENT-THINGS` | P2/S | Needs a contract decision before an implementation change. |

## Important Learning

1. **Probe two siblings, not one key.** The 2026-08-29 reproduction was competent, thorough, and
   wrong — because a prefix-versus-path confusion is invisible with one key. Any store test corpus
   should hold `sub/` and `subway/`.
2. **Probe both backend shapes.** The memory and filesystem backends differ in exactly the way that
   matters. Defect 4 was invisible on `fs`, and `fs` is what the first pass used.
3. **Read the vendored dependency when its contract is the question.** `create_dir`'s trailing-slash
   requirement is one line in OpenDAL's source. That line disproved a claim two design documents had
   already repeated: `make_sub_dirs` has never created a directory, and `let _ignore` on a call that
   always fails is indistinguishable from one that always succeeds.
4. **Count the tests before resting a refactor on them.** Phase 2 argued the extraction was safe
   because "the existing tests pass unchanged". There was one, covering a single key and never
   checking `is_dir` after a removal. Counting took one grep; the fix was to write the
   characterization tests *first*.
5. **A silent no-op reads like success.** `AsyncMemoryStore::makedir` was
   `let key = key.as_absolute()?; Ok(())` — validation followed by success, to the eye. It was found
   by writing a test that asserted what it actually did.
6. **"Where does this belong?" is a question that requires looking.** The four duplicate directory
   indexes had been there all along and were found only when the gate asked whether the fallback
   belonged in core.

## Conformance and Remaining Work

Every Phase 1 acceptance criterion is met:

| Criterion | Evidence |
|---|---|
| 1. No operation reaches a sibling key | `sibling01`-`sibling04`, both backends |
| 2. One mapping, round-trip property, suffix keys refused | `pathmap01`-`pathmap06` |
| 3. `key_prefix()` correct, routing correct | `prefix01`, `router01`, `dir04`, `opendal03` |
| 4. Directory keys addressable; fallback shared in core | `dir01`-`dir04`, `diridx01`-`diridx08`, `memdir01`-`memdir05` |
| 5. Markers and dead code resolved; no `unwrap`; no warnings | steps 7a/7b; zero warnings |
| 6. What already worked still works | `fsreg01` |

Remaining: the four open issues above, and the `liquers-web` store migration.

## Validation

```
cargo test -p liquers-core --lib                     765 passed
cargo test -p liquers-store                           43 passed
cargo test -p liquers-lib --lib --tests              full default loop, all green
bash scripts/check-build-matrix.sh                   All 16 configurations OK
cargo clean && cargo test -p liquers-web \
  --target wasm32-unknown-unknown --features debug-handles    14 suites, all green
python3 scripts/docs_index.py --check                229 documents · 0 errors
```

`export-command-registry` was not run and `specs/command_registry.yaml` is untouched: no command was
added, removed or changed.
