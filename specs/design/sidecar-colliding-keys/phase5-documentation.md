# Phase 5: Documentation — Sidecar-colliding keys refused by the path builders

## Completion Preconditions

| Criterion | State |
|---|---|
| Steps 1-9 complete, every validation command run | Yes |
| `C2` reports no allowed failures; `prefix03` / `sibling05` passing rather than "not run" | Yes, and asserted in the test rather than read off a report |
| Build matrix clean | **No — 17 of 20.** Three `liquers-lib` test rows fail on `rustc 1.94.1` against packages requiring `1.95`. Pre-existing, unrelated to this change, and already tracked: `BUILD-SYSINFO-REQUIRES-NEWER-RUSTC` |
| Review comments answered | Yes — four review rounds, recorded in Phases 2-4 |

Test results at completion: `liquers-core --lib` 784 passed; `store_conformance_CONF` 5 suites
passed; `liquers-store` 46 passed; `conformance_docs_CONF` (`D1`) passed.

## Implementation Summary

**What was asked:** fix `CORE-FILE-STORE-WRITES-METADATA-COLLIDING-KEYS` — `AsyncFileStore` refuses
a sidecar-colliding key in `is_supported` but writes it in `set`, silently corrupting the metadata
of the key it collides with.

**What was built.** `ReservedNames` in `liquers-core::store` owns the rule; `is_supported`, the path
builders and the listing filters all consult it, in `AsyncFileStore`, `FileStore` and
`AsyncOpenDALStore`. `acquire_lock` builds the lock path first, so `set`, `set_metadata`, `remove`
and `removedir` refuse before any directory is created or byte written — no half-done state. Eight
unit tests (`reserved01`-`reserved08`), three OpenDAL tests (`pathmap03`, `pathmap07` extended;
`pathmap08` new), and the `C2` fixture change.

**Wider than the issue described, both settled at gates:**

1. **The rule covers every segment and both name forms.** The issue, `is_supported` and
   `PathMap::is_suffix_ambiguous` were all filename-scoped, and reserved only `.__metadata__`.
   The predecessor Python implementation (`orest-d/liquer`, `liquer/store.py` at `2eb4e64`) refuses
   the name as a filename *and* in any interior position (1513, 1526) and filters it from listings
   (543, 1468-1469). The Rust port had narrowed all three. This restores them, and reserves
   `.__lock__` besides — a data file at `foo.__lock__` makes every later write to `foo` block for
   three seconds and then fail, permanently, which is worse than the metadata case and which no
   conformance rule would have found.
2. **The listing filters were mandatory, not tidiness.** `listdir_keys_deep` calls `is_dir` on every
   child, so guarding the path builders alone turns silent corruption into a store whose `keys()`
   fails outright. `reserved06` is that test.

**Deviations from the approved plan:** none in substance. Two corrections during execution: the
`pathmap07` rewrite needed a loop rather than a single key (formatting fixed by `cargo fmt`), and
`RuleOutcome` had to be added to the conformance test's imports.

**Not done, deliberately:** `PathMap::is_suffix_ambiguous` was removed rather than kept as a
compatibility shim. It is `pub`, so this is a public API removal in `liquers-store`; nothing in-tree
outside its two call sites and two tests used it, and `liquers-py` does not depend on that crate.

## Documentation Delivered

| Document | Change |
|---|---|
| `specs/reference/STORE_SEMANTICS.md` §8 | Retitled *Metadata sidecars and reserved names* and restated: reserved in any segment, declared per store by its own layout, both the suffix form and the exact name. Names the three kinds of caller and why satisfying only `is_supported` is the defect the section prevents. Adds the refusal type and ordering, that listings skip rather than fail, and that `get` repairs unparseable metadata — the fact the recovery path depends on and which the contract had never stated. `## History` row; `reviewed: 2026-09-03` |
| `specs/guides/STORE_IMPLEMENTATION_GUIDE.md` §"The key space" | Now says *how*: one `ReservedNames` predicate for all three callers, declaring what your own layout reserves and no more. Records the three failure modes behind the advice, the `as_absolute`-first ordering, and the recovery routes for a store already holding a colliding file. `## History` row; `reviewed: 2026-09-03` |
| `specs/issues/CORE-FILE-STORE-WRITES-METADATA-COLLIDING-KEYS.md` | `closed`, with a resolution note; `complexity` `M` → `L`; `design` re-pointed from the design that found it to the one that fixed it |
| `specs/issues/STORE-KEY-REFUSAL-ORDER-DIVERGES-BETWEEN-STORES.md` | `closed` — absorbed rather than deferred |
| `specs/index.csv`, `specs/README.md` | Regenerated |

No new reference or guide. The Phase 1 decision to extend rather than create held: everything
learned fits in the two documents that already own this ground.

## Issues Filed

Five, all from things noticed in passing rather than the task itself.

| Issue | P/C | Why it exists |
|---|---|---|
| `STORE-METADATA-LAYOUT-HARDCODED-PER-STORE` | P2 L | Six layouts in the tree, each hard-coded, with no shared abstraction — including *which keys the layout makes unrepresentable*, the coupling that made this bug possible. Records that implementing it must revisit `is_supported` and the path builders |
| `CORE-FILE-STORE-LISTDIR-DROPS-METADATA-ONLY-KEYS` | P2 S | §8 says a sidecar implies its data key and OpenDAL obeys; the file stores drop it, so a metadata-only key is invisible to `keys()` while `contains()` reports it present. The two sidecar stores disagree, and no rule covers it |
| `STORE-KEY-REFUSAL-ORDER-DIVERGES-BETWEEN-STORES` | P3 S | Filed during review, **closed by this design** |
| `DOCS-BUILD-MATRIX-CONFIGURATION-COUNT-STALE` | P3 S | `CLAUDE.md` says 11 configurations; the script runs 20 |
| *(added to)* `BUILD-SYSINFO-REQUIRES-NEWER-RUSTC` | P2 S | Not filed again — extended with what this run observed: `main`'s dependency upgrade added a second package family demanding rustc 1.95, so pinning `sysinfo` alone no longer restores the matrix |

## Important Learning

1. **A routing hint that doubles as a correctness check is a trap.** `is_supported` answers two
   questions — which router member takes this key, and what can this store address — and only the
   first has a caller obliged to ask. Every store answering the second question *only* there has
   this bug latent. That is now the opening claim of §8.
2. **The half-fix was worse than the bug.** Guarding the path builders without the listing filters
   makes `keys()` fail on any store containing a legacy `__metadata__` folder. Found by tracing
   `listdir_keys_deep`; neither the issue nor the conformance report pointed at it.
3. **A fixture that does not declare a shape silently skips the rules about it.** `prefix03` and
   `sibling05` had never run against a file store. Nothing failed — the report said "not run".
   Worse, `assert_conformant` cannot see a declined precondition and the report only reaches
   stderr, so re-declaring the shape and then losing it would be invisible again. `C2` now asserts
   the two outcomes directly.
4. **Check the predecessor before assuming a rule is new.** The interior-segment rule and the
   listing filter were not inventions; they are in `orest-d/liquer` and the Rust port dropped them.
   The design spent a full phase treating the widening as speculative before the citation surfaced.
5. **Two reviewers can be confidently wrong in different directions.** On the build-matrix count one
   repeated the documented 11, another computed 17; the answer is 20. A number in prose duplicating
   one a program computes will drift, and reviewers will defend the drift.

## Conformance and Remaining Work

`C2` is the only suite whose report changed: `sidecar03` from allowed-failure to passing, and
`prefix03` and `sibling05` from "not run" to passing. `C1`, `C3`, `C4`, `C5` and both `liquers-store`
suites are unchanged. `D1` still passes, so code, contract and guide share one rule-ID vocabulary.

Remaining, all tracked above: the pluggable metadata layout (`L`, needs its own design), the
metadata-only key divergence, the stale matrix count, and the toolchain blocking three matrix rows.

## Validation

```
cargo test -p liquers-core --lib                                        784 passed
cargo test -p liquers-core --features store-conformance --test store_conformance_CONF   5 passed
cargo test -p liquers-core --features store-conformance --test conformance_docs_CONF    1 passed
cargo test -p liquers-store                                              46 passed
cargo check -p liquers-core --target wasm32-unknown-unknown              clean
bash scripts/check-build-matrix.sh                                       17/20, 3 pre-existing
python3 scripts/docs_index.py --check                            276 documents, 0 errors
```
