---
id: STORE-TEST-IDS-COLLIDE-WITH-CONFORMANCE-RULE-IDS
kind: issue
title: Store unit tests share IDs with conformance rules that check different contracts
status: draft
priority: P2
complexity: M
area: [core/store, store/backends, docs]
design: store-conformance-suite
created: 2026-09-02
github:
---
## Problem

The conformance suite adopted the existing `sibling`, `dir`, `remove` and `prefix` ID families on
the assumption that each new rule generalizes the like-named unit test. **It does not.** The same
ID means different things in the two places:

| ID | Unit test in `liquers-store/src/opendal_store.rs` | Conformance rule |
|---|---|---|
| `sibling02` | `removedir` is scoped at depth | `listdir` excludes a sibling's entries |
| `sibling03` | `listdir_keys_deep` excludes siblings | `remove` on a data key spares a name-extending key |
| `sibling04` | a prefixed store enumerates only its own subtree | a sibling's children do not make a key look like a directory |
| `remove01` | `removedir` on an absent directory is `Ok` | the `removedir` postcondition (this is rule `absence03`) |
| `remove02` | `removedir` on the root empties the store | `removedir` is recursive |
| `dir04` | router `is_dir` reaches a prefixed OpenDAL store | a directory's metadata is directory-shaped |
| `dir05` | directory metadata is marked as a directory | `contains` falls back to `is_dir` |

So `dir04` and `dir05` are *swapped* between the two schemes, and three `sibling` IDs name entirely
different claims.

The same applies in `liquers-core/src/store.rs`: `traitdef01` tests the `contains`→`is_dir`
fallback, which is conformance rule **`dir05`** — and rule `dir05` requires the `Directories`
capability, which the trait-defaults suite (`C4`) correctly declares `false`. So `dir05` is skipped
there and `traitdef01` is the *only* coverage of that contract for the defaults.

## Impact

`D1` (`liquers-core/tests/conformance_docs_CONF.rs`) enforces one meaning per ID across the code,
the contract and the guide — but it reads the documents and the rule registry, not unit-test
function names. So a reader who greps `sibling03` finds two tests asserting different things, and a
report naming `dir05` points at a unit test about something else.

The planned deletion pass (Phase 4 step 15 of `design/store-conformance-suite/`) was **not carried
out** because of this: the mapping table it was to execute assumed a correspondence that does not
hold, and deleting on it would have removed genuine coverage — `removedir` on the root, router
`is_dir` reach, depth scoping, and the trait defaults' `contains` fallback — while appearing to be
tidying.

## Expected behaviour

Rename the unit tests so no ID means two things. The conformance rules own these families now, since
they are cited by the contract and the guide and enforced by `D1`; the unit tests are about one
store's internals and should say so — `opendal_removedir_is_scoped_at_depth`,
`opendal_router_is_dir_reaches_a_prefixed_store`, `defaults_contains_falls_back_to_is_dir`.

Then, and only then, delete the ones that are genuinely duplicated, each against the rule that
replaces it and after checking the rule *fails* when the behaviour is broken. Several will survive
that check: the `keyabs` tests assert more than refusal — that the file outside the root is
unmodified, for instance — and `keyabs09` covers the synchronous `FileStore`, which is out of the
suite's scope entirely.

## Discovery

Found on 2026-09-02 at Phase 4 step 15 of `design/store-conformance-suite/`, by reading the test
names the mapping table proposed to delete rather than trusting the table. The table was built from
ID families, and IDs turned out not to identify contracts.
