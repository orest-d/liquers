# Phase 5: Documentation — `AsyncStore` conformance

**PR:** [orest-d/liquers#59](https://github.com/orest-d/liquers/pull/59) · **Issue:**
`STORE-ASYNC-STORE-NO-BEHAVIOURAL-CONFORMANCE-SUITE` (closed)

## Completion Preconditions

- All planned implementation work is finished and validated, or explicitly not carried out with the
  reason recorded (step 15, below).
- Tests green: 789 `liquers-core` lib tests, 5 core conformance suites, 2 OpenDAL suites, 2 wasm
  suites under Node, `D1`. The `liquers-lib` default loop is clean.
- `scripts/check-build-matrix.sh` passes on both new rows; the two rows that fail are pre-existing
  and unrelated (`BUILD-SYSINFO-REQUIRES-NEWER-RUSTC`).
- `python3 scripts/docs_index.py --check`: 0 errors.
- Every rule a store is allowed to fail cites an open issue, and `H5` fails the suite if such a rule
  starts passing — so no allowed-failure entry can outlive its reason.
- Review comments on PR #59: none outstanding.

## Implementation Summary

`AsyncStore` now has an executable contract. `liquers_core::store_conformance` holds **42 rules**,
one per claim in `specs/reference/STORE_SEMANTICS.md`, behind the non-default `store-conformance`
feature. The module names no runtime — no `tokio`, no test attribute, no panic — so each crate
supplies its own harness, which is what lets `liquers-web` run the same rules under
`wasm_bindgen_test` while `liquers-core` runs them under `#[tokio::test]`.

Nine suites cover **nine in-tree implementations plus the trait defaults**:

| Suite | Store | Result |
|---|---|---|
| `C1`–`C5` (`liquers-core`) | `AsyncMemoryStore`, `AsyncFileStore`, `AsyncStoreRouter`, trait defaults, `NoAsyncStore` | conformant |
| `C6`–`C7` (`liquers-store`) | `AsyncOpenDALStore` over the memory and filesystem services | conformant — the widest coverage in tree |
| `C8`, `C10` (`liquers-web`, Node) | `FetchStore`, `JsStore` | conformant / `BLOCKED` on two filed issues |
| `C9` (`liquers-web`, browser) | `LocalStorageStore` | **written, never executed** — needs a chromedriver |
| `D1` | the documents | rule IDs in code, contract and guide are one set |

Three documents carry it: `STORE_SEMANTICS.md` (completed — the contract), the new
`STORE_IMPLEMENTATION_GUIDE.md` (operational — how to build a store that satisfies it), and the new
`CONFORMANCE_TERMS.md` (the vocabulary both guides share).

## Deviations from the approved design

**The validation tool was cut**, at the Phase 4 gate, on the final review's judgement that the
project was `XL` with it. Its design is preserved in full on
`STORE-CONFORMANCE-VALIDATION-TOOL` — command surface, provenance-based level defaults, residue
requirement, exit codes, the `StoreFactory::create_fixture` extension — along with the one question
it never answered: where a `--config` store's capabilities and preconditions come from.

**A fourth safety level was removed.** Phase 1 specified `unrestricted`; Phase 3 counted the rules
runnable at each level and found none reach for it. A level no rule needs can only permit damage no
check asked for.

**Step 15, the adoption deletions, was not carried out** — and this is the one deviation that is a
finding rather than a scope cut. The mapping table assumed each adopted ID generalizes the
like-named unit test; the names show otherwise. `dir04` and `dir05` are *swapped* between the two
schemes, three `sibling` IDs name different claims, and `traitdef01` is rule `dir05` — a rule the
trait-defaults suite correctly skips, so `traitdef01` is its only coverage of that contract.
Deleting on that table would have removed real coverage while looking like tidying. Filed as
`STORE-TEST-IDS-COLLIDE-WITH-CONFORMANCE-RULE-IDS`.

**Rule count is 32, not 31.** `prefix04` was added by the Phase 4 review: `prefix02` and `prefix03`
both assert `is_supported` is *false*, and the trait default returns `false` unconditionally, so
without a positive case a store refusing every key passed both and looked conformant.

## Issues Filed

| Issue | P/C | Found by |
|---|---|---|
| `CORE-LISTDIR-KEYS-DEEP-TESTS-THE-WRONG-KEY` | P2/S | reading the methods no rule covered — **fixed here** |
| `STORE-SEMANTICS-CHILDREN-RULE-CONTRADICTS-EVERY-STORE` | P2/S | `dir07`, first census run |
| `CORE-STORE-ROUTER-KEYS-FAILS-ON-AN-EMPTY-MEMBER` | P2/S | `C3` |
| `WEB-JS-STORE-CANNOT-EXPRESS-KEY-NOT-FOUND` | P2/S | `C10` |
| `WEB-JS-STORE-HAS-NO-DIRECTORY-METADATA` | P2/S | `C10` |
| `STORE-TEST-IDS-COLLIDE-WITH-CONFORMANCE-RULE-IDS` | P2/M | step 15 |
| `CORE-SYNC-STORE-TRAIT-OBSOLETE` | P2/M | scoping |
| `DOCS-ASYNC-STORE-WRAPPER-NO-LONGER-EXISTS` | P2/S | scoping |
| `STORE-CONFORMANCE-VALIDATION-TOOL` | P2/M | the Phase 4 cut |
| `BUILD-SYSINFO-REQUIRES-NEWER-RUSTC` | P2/S | the build matrix — pre-existing, unrelated |

Closed: `STORE-ASYNC-STORE-NO-BEHAVIOURAL-CONFORMANCE-SUITE`,
`CORE-STORE-KEYS-MEANS-TWO-DIFFERENT-THINGS`, `CORE-LISTDIR-KEYS-DEEP-TESTS-THE-WRONG-KEY`.

## Important Learning

**A rule needs ground truth the store cannot supply.** `prefix01` as first written compared
`key_prefix()` with itself, so the divergence it exists for — a store returning `Key::new()` —
would have passed. `Fixture::expected_prefix` fixed it. The design's own examples had disagreed
about this without anyone noticing: one derived keys from `store.key_prefix()`, another from a
fixture field. **Where a rule's subject is also its oracle, it checks nothing.**

**Declaring a capability `false` must be a claim, not an exit.** Under-declaring is how a broken
`makedir` escapes `explicit01` — the store least likely to be given the capability. Partly
addressed (`StoreCapabilities` has no `Default`, so adding a field breaks every fixture); the
negative rules the review proposed are not implemented and remain the strongest available fix.

**Three defects were in this work, not in the stores**, and each is the general case of something a
store author will hit:

- `H1`/`H2` asserted "the rule was not called" against a recorder the rule never wrote the checked
  key into. Vacuous on the first attempt, in the tests of a suite built to prevent exactly that.
- The harness tests reimplemented the gate instead of calling it, so a bug in `run_one` would have
  been invisible to all of them.
- `GenericFixture` derived its key stem from `SystemTime::now()`, which **panics on wasm32**. A
  fixture meant to be shared was unusable on the target that motivated the whole design.

**`assert_conformant` failing in both directions earned its cost four times.** Each time a fix
landed, it reported the now-stale allowed-failure entry rather than waiting to be noticed — once
contradicting my own assumption that the router would inherit `AsyncMemoryStore`'s `keys()`
divergence. It does not.

**Capability gating does most of the work; `KeyRequest` is the long tail.** For the restricted store
worked in Phase 3, gating accounts for 15 of 16 gaps and a precondition decline for one. Both are
needed — the decline catches what capabilities cannot express, such as a store that *has*
directories but whose names can never form a prefix pair.

**The implementation count was wrong in the contract, and the suite is what found out.** It said
five and listed seven; it is nine plus the defaults. `NoAsyncStore` is `pub`, is what an
`Environment` holds until a store is configured, and nobody had counted it.

## Documentation Delivered

**Created:** `specs/guides/STORE_IMPLEMENTATION_GUIDE.md`, `specs/reference/CONFORMANCE_TERMS.md`.
**Updated:** `STORE_SEMANTICS.md` (contract completed; implementation count corrected; §2's
`children` claim marked unsettled), `LANGUAGE-INTEGRATION_GUIDE.md` (§3 vocabulary extracted, §STORE
cross-linked), `STORE_FACTORY_GUIDE.md`, `STORE_CONFIG_FSD.md`, `CLAUDE.md`, `specs/README.md`,
`scripts/check-build-matrix.sh`.

`affects_docs` is the six documents above. Each was reviewed against implemented behaviour; the
guide's per-store status table is generated from the reports rather than maintained by hand, which
is why `ConformanceReport` derives serde even though the tool that justified it was cut.

## Conformance and Remaining Work

The design's four Phase 1 deliverables: the contract is complete, the suite is implemented and
applied, the guide exists, and the validation tool was deliberately deferred with its design intact.
Three items remain open.

### Outstanding

1. **The `children` question** (`STORE-SEMANTICS-CHILDREN-RULE-CONTRADICTS-EVERY-STORE`) — `dir07`
   reports `Blocked`, and settling it makes the rule live by deleting one branch. Needs a decision,
   not a fix.
2. **`C9` has never run.** It compiles behind `browser-tests` and needs a chromedriver matching the
   installed browser. A gap in the census, not a passing result.
3. **`CORE-FILE-STORE-WRITES-METADATA-COLLIDING-KEYS`** (P1/M) — `AsyncFileStore` refuses a
   sidecar-colliding key in `is_supported` and writes it in `set`, corrupting the metadata of the
   key it collides with. Recorded as an allowed failure on `C2` so `H5` reports it when fixed.

## Post-review round

A Codex review on PR #59 raised five findings, **all five correct**, and two of them were things
this design had seen and reasoned past. The inventory went from 32 rules to 42.

- **`keyshape01` invoked `remove` and `removedir` at `CreateOnly`** — a level that forbids removal —
  against a traversal key, on precisely the store whose broken refusal would let it escape the
  namespace. Split into `keyshape01` (reads, `ReadOnly`) and `keyshape02` (mutations, `Scratch`),
  each stopping at the first method that accepts the key rather than continuing to call the
  destructive ones. Phase 4 had moved this rule from `ReadOnly` to `CreateOnly` for exactly this
  reason and stopped one step short.
- **`GenericFixture::cleanup` deleted at `CreateOnly`**, contradicting the level's promise and the
  residue accounting the design makes much of. Now a no-op below `Scratch`.
- **No rule requested `Existing` or `ExistingDirectory`.** The design documented `Existing` as the
  only source of subjects for a read-only store, and no rule asked for one — so `FetchStore` passed
  six rules without ever reading anything. Added `data03` and `dir08`.
- **Capability declarations were unverified** — review finding F2, which Phase 5 had listed as
  outstanding. Now implemented: `RuleMeta::refutes` runs a rule only when a capability is declared
  *absent*, and six refuting rules assert the store really does refuse. A fully writable store can
  no longer declare everything `false` and report conformant.
- **`sidecar01` checked only `is_supported`**, a routing hint a direct caller never consults. Added
  `sidecar03` — which immediately found the `AsyncFileStore` write above.

Two defects surfaced by the new rules on their first run: the `AsyncFileStore` metadata collision,
and `NoAsyncStore::get` reporting a relative key as `KeyNotFound` rather than `KeyNotAbsolute`
(fixed here — a store holding nothing still has to say *no* correctly, and its own `contains`
already did).

**The pattern worth keeping:** every one of the five was a place where a rule's *shape* let a
non-conforming store through — not a missing rule, but a rule that could not fail. That is the same
failure this suite exists to catch in other people's code, found in its own.

## Validation

```bash
cargo test -p liquers-core  --features store-conformance          # 789 lib + C1–C5 + D1
cargo test -p liquers-store --features store-conformance          # C6–C7
cargo test -p liquers-web --target wasm32-unknown-unknown         # C8, C10 (Node)
CHROMEDRIVER=$(which chromedriver) cargo test -p liquers-web \
  --target wasm32-unknown-unknown --features browser-tests        # C9 — not yet run
bash scripts/check-build-matrix.sh
python3 scripts/docs_index.py --check
```

Run the wasm loops after `cargo clean`, separately from the native one: they build a different
target, and a combined run exhausts a constrained session's disk allowance.
