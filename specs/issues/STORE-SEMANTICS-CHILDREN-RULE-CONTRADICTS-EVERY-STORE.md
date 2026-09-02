---
id: STORE-SEMANTICS-CHILDREN-RULE-CONTRADICTS-EVERY-STORE
kind: issue
title: STORE_SEMANTICS says directory metadata does not populate children, and every store populates it
status: draft
priority: P2
complexity: S
area: [core/store, store/backends, web, docs]
design: store-conformance-suite
created: 2026-09-02
github:
---
## Problem

`specs/reference/STORE_SEMANTICS.md` §2 states:

> Directory metadata does **not** populate `children`. `listdir_asset_info` calls `get_asset_info`
> per child, which calls `get_metadata` per child directory: a full recursive walk of the subtree
> for one directory read. The `AsyncStore` default still does this; stores that care override it.

The paragraph contradicts itself — the first sentence forbids what the last says the default does —
and **no implementation follows the first sentence.** Every store populates `children` in
`get_metadata` for a directory:

| Implementation | Site |
|---|---|
| `AsyncStore` trait default | `liquers-core/src/store.rs:401` |
| `AsyncMemoryStore` | `store.rs:666` |
| `AsyncFileStore` | `store.rs:1011`, `:1043` |
| `Store` trait default (sync) | `store.rs:84` |
| `FileStore` | `store.rs:1285`, `:1310` |
| `MemoryStore` | `store.rs:1488` |
| `LocalStorageStore` | `liquers-web/src/store/local_storage.rs:404` |

All of them do `metadata.children = self.listdir_asset_info(key)…`. The claim is unanimous in the
other direction, so this is a defect in the *contract*, not in eight stores.

## Impact

Two costs, and they point opposite ways, which is why this needs deciding rather than patching.

- **The performance concern behind the sentence is real.** One `get_metadata` on a directory
  triggers `listdir_asset_info`, which calls `get_asset_info` per child, which calls `get_metadata`
  per child *directory* — so reading one directory's metadata walks its whole subtree. On a remote
  backend that is a round trip per node.
- **Something depends on the current behaviour.** `liquers-py` reads `.children`
  (`liquers-py/src/metadata.rs:406`), and directory listings in the UI are the obvious consumer.
  Simply deleting the assignment would empty a field callers read.

## Expected behaviour

Decide, and make the contract and the code agree:

1. **Keep the behaviour, fix the sentence** — say directory metadata *does* carry `children`, and
   record the subtree-walk cost as a known characteristic with a separate issue for making it
   bounded (a depth limit, or a lazy variant).
2. **Keep the sentence, change eight stores** — `get_metadata` stops populating `children`, and
   whatever needs children calls `listdir_asset_info` explicitly. A breaking change for
   `liquers-py` and for any UI reading the field.

Option 1 looks right on the evidence — a rule no implementation has ever followed is a rule that was
never agreed — but the cost the sentence was written to warn about should not be lost with it.

## Discovery

Found on 2026-09-02 by the conformance suite's first census run, at Phase 4 step 6 of
`design/store-conformance-suite/`. The rule `dir07` encodes the contract sentence, failed against
`AsyncMemoryStore`, and checking the other implementations showed all eight agree with each other
and disagree with the document. `dir07` reports `Blocked` citing this issue until it is settled —
the outcome that exists for "the rule is right or the contract is, and someone must say which".
