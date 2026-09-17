# Phase 1: High-Level Design — Store and asset search

## Feature Name

Store and asset search

## Purpose

Let a caller ask a Liquers environment **which keys match** instead of listing a subtree and
filtering in their own code. Stores and the asset layer can enumerate and fetch but cannot select
(`STORE-NO-CONTENT-OR-METADATA-SEARCH`), so every consumer that needs a subset pays O(corpus) reads
and reimplements retrieval. The users are people browsing a store and agents retrieving from a
corpus; both need to judge a hit without opening the document.

## Scope

**In:** selection over keys, metadata fields and stored text content, rooted at a key; store-level
and asset-level, unioned; boolean matching (substring, field equality, key glob); results as asset
infos with a match excerpt; a command surface so the capability is reachable as a query.

**Out:** ranked full-text and relevance scoring; semantic/embedding search; a dedicated index
engine or crate; a new HTTP endpoint; identity- or ACL-filtered results. Each is named in the
options analysis with what keeps it cheap to add later.

**Hard invariant:** a search never evaluates. It reads what already exists, so a content search
over a corpus of unevaluated recipes cannot recompute the corpus.

## Core Interactions

**Query system** — no grammar change; a search is an ordinary action whose arguments carry the
predicate, and `ActionRequest::encode` already escapes arbitrary terms.

**Store system** — a selection method on `AsyncStore` with a default scan implementation over
`listdir_keys_deep`, so no existing store breaks and a capable backend can override. Needs a
`StoreCapabilities` flag and conformance rules. `AsyncStoreRouter` must fan out across mounts below
the search root.

**Command system** — a new namespace in `liquers-lib` building the predicate from action arguments
and returning results; further filtering is ordinary commands over the returned list.

**Asset system** — search unions store entries, live assets and recipe-declared keys, described the
way `AssetManager::get_asset_info` already resolves them, without triggering evaluation.

**Value types** — none new: `Value::AssetInfo(Vec<AssetInfo>)` already exists and already
serializes; a per-hit match record (matched field, excerpt, reserved score) sits beside it.

**Web/API and UI** — nothing new. The command is reachable through `/q`; an MCP tool and any UI
affordance construct a query rather than a private search path.

## Crate Placement

`liquers-core` — the predicate type, the store selection method with its default implementation,
the asset-level union, capability and conformance rules. `liquers-lib` — the command namespace.
No change to `liquers-store` beyond an optional push-down override, none to `liquers-axum`.

## Documentation Intent

**Reference:** create `specs/reference/SEARCH.md` — the predicate, the match contract, the
non-evaluation and boundedness invariants, and what a store must guarantee when it overrides the
default. New rather than an extension: no existing reference owns selection, and splitting it
between `STORE_SEMANTICS.md` and `ASSETS.md` would leave the union unowned.

**Guide:** extend `specs/guides/STORE_IMPLEMENTATION_GUIDE.md` with the selection capability, since
that is where a store author already learns which capabilities to declare and how to run the
conformance suite. A separate how-to-search guide is not yet justified; reconsider if the command
surface grows beyond one namespace.

**Other documents to create:** none.

**Specific documents to update:** `specs/reference/STORE_SEMANTICS.md` (the selection contract and
the new capability), `specs/reference/CONFORMANCE_TERMS.md` (new rule terms), `specs/README.md`
(capability map), `specs/command_registry.yaml` (regenerated, not edited).

Audience: a contributor implementing or overriding selection, and an agent or user composing a
search query. Both should work from the reference and the guide without opening this folder.

## Open Questions

1. Does the union deduplicate a key present both as a live asset and in a store by preferring the
   asset, and is that the same precedence `get_asset_info` already uses?
2. Is a partially pushed-down predicate re-checked by the caller, or must push-down be
   all-or-nothing? A dropped clause is a correctness bug either way.
3. Is truncation a hard limit with a flag, or a cursor — and if a cursor, stable against what?
4. Does search subsume `listdir_asset_info` (an empty predicate at depth 1) or stay separate?
5. How does a hit decide a value is text before extracting an excerpt, and what does a binary hit
   return instead?
6. Is the router's fan-out across mounts in scope for the first version, or is a search confined to
   one mount until a later change?

## References

- [Options analysis](./options-analysis.md) — the eight decisions, their alternatives and costs
- `specs/issues/STORE-NO-CONTENT-OR-METADATA-SEARCH.md` — the gap this closes
- `specs/design/agent-memory-mvp/phase1-high-level-design.md` — open question 3 asks how far an MVP
  goes on search; this design answers it
- `specs/issues/DIRECTORY-LISTING-DEPENDENCY-IS-NEVER-REGISTERED-OR-CHECKED.md` — filed while
  writing the analysis; it is what makes a maintained index an unverified option rather than a
  ready one
- `specs/issues/CORE-METADATA-NO-APPLICATION-ATTRIBUTES.md` — adjacent gap; predicate field tests
  are defined over metadata generally so they extend to attributes when those exist
- `specs/reference/STORE_SEMANTICS.md`, `specs/guides/STORE_IMPLEMENTATION_GUIDE.md` — the contract
  and the author-facing procedure this extends
