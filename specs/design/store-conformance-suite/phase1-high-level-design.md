# Phase 1: High-Level Design - store-conformance-suite

## Feature Name

Shared `AsyncStore` behavioural conformance suite

## Purpose

`AsyncStore` has seven in-tree implementations across three crates, each tested only against
itself; eleven ways they disagree were found one at a time by people tripping over them, one of
them a P0 that destroyed data. This project gives the trait an **executable** contract: a shared,
parameterized suite that every implementation — in-tree and in a language integration — runs, and
it completes `specs/reference/STORE_SEMANTICS.md`, which today records three questions as
unsettled rather than answering them.

## Core Interactions

### Query System
None directly. The suite builds its keys with `parse_key` and never evaluates a query.

### Store System
The entire feature. In scope: `AsyncMemoryStore`, `AsyncFileStore`, `AsyncStoreRouter`
(`liquers-core`), `AsyncOpenDALStore` (`liquers-store`), `FetchStore`, `LocalStorageStore`,
`JsStore` (`liquers-web`), and the `AsyncStore` trait defaults themselves. The rules to be checked
are the sections of `STORE_SEMANTICS.md`: the sibling rule, directory derivation, derived versus
explicit directories, absence versus failure, removal, prefixes and routing, key shape, metadata
sidecars, and `keys()`.

### Command System
None. No new commands, no namespace.

### Asset System
Indirectly: `get_asset_info` and `listdir_asset_info` are built on `get_metadata`, so directory
metadata is contract surface the suite must check.

### Value Types
None.

### Web/API (if applicable)
No endpoint changes. `liquers-web`'s three stores must run the suite under `wasm32`, which is what
forbids a `#[tokio::test]`-shaped suite and makes this an `L`. `liquers-axum`'s store handlers are
the exposure that made the sibling rule a P0, but they are unchanged.

### UI (if applicable)
None.

## Crate Placement

**`liquers-core`** — a public module (`store_conformance`) behind a non-default feature, beside the
trait it specifies. It must be *shipped*, not test-only: `liquers-store` and `liquers-web` consume
it as a dependency, and so does an out-of-tree store written through a language integration. It
must be runtime-agnostic — plain `async fn`s taking a store, no `tokio`, no test attributes — so
each crate drives it with its own harness (`#[tokio::test]` natively, `#[wasm_bindgen_test]` on
wasm).

Not `liquers-lib`: it sits above `liquers-store`, so a suite there could not reach either.

## Documentation Intent

**Reference:** **Extend** `specs/reference/STORE_SEMANTICS.md`. It already answers most of the
contract; this project resolves its three ⚠ rows (the trait-default `removedir`,
`AsyncMemoryStore::is_supported`, and what `keys()` returns), adds the capability model that lets a
read-only or delegating store conform, and replaces each section's *Enforced by* line with the
suite's rule IDs. A second reference would split one contract across two files.

**Guide:** **Create** `specs/guides/STORE_CONFORMANCE_GUIDE.md`. Running the suite against a new
store, declaring which capabilities it has, and reading a failure back to the rule it broke are
repeatable tasks with a procedure — exactly what a guide is for, and the reason the suite is worth
anything to a store written outside this repository.

**Other documents to create:** None.

**Specific documents to update:**
- `specs/README.md` — link the new guide in the capability map.
- `CLAUDE.md` §"Adding a Store Backend" — add running the suite as a numbered step.
- `specs/index.csv` — the design, the guide, and the status changes below.
- `specs/issues/CORE-STORE-KEYS-MEANS-TWO-DIFFERENT-THINGS.md` (P2) — closed by the `keys()`
  decision; `specs/issues/STORE-ASYNC-STORE-NO-BEHAVIOURAL-CONFORMANCE-SUITE.md` — closed by this
  design; `.../CORE-ASYNC-MEMORY-STORE-IS-SUPPORTED-IGNORES-PREFIX.md` (P1) — its own design folder
  exists, so this one records the dependency rather than fixing it.

**Audience:** whoever writes the eighth `AsyncStore`, in this repository or against it. They should
be able to read the contract, run the suite, and find out where they disagree with it, without ever
opening this design folder.

## Open Questions

1. **What does `keys()` enumerate** — data keys only, or data keys plus directories plus the
   prefix? The suite cannot accept both, so this must be decided, not deferred. (Row 10;
   `CORE-STORE-KEYS-MEANS-TWO-DIFFERENT-THINGS` records the candidates.)
2. **Does the trait-default `removedir` become `Ok(())`**, matching all three overrides, or do the
   overrides become an error? (Row 6, this issue's own.)
3. **How does a store declare what it can do?** `FetchStore` is read-only and `JsStore` delegates
   almost everything to JavaScript; a suite that fails them for that is useless. A capability
   struct, or per-rule opt-in?
4. **How does a rule get a fresh store?** A factory closure the harness supplies, or a store handed
   in already-empty and cleaned up by the rule? `LocalStorageStore` persists across tests in one
   browser session, so this is not cosmetic.
5. **How does one rule map to one reported failure** under two harnesses that disagree about what a
   test is — a macro generating a `#[tokio::test]` / `#[wasm_bindgen_test]` per rule, or one test
   per store collecting a report?
6. **Is the synchronous `Store` trait in scope?** `MemoryStore`, `FileStore` and `StoreRouter`
   implement it and have the same rules. The issue says `AsyncStore`; including them doubles the
   suite.

## References

- `specs/issues/STORE-ASYNC-STORE-NO-BEHAVIOURAL-CONFORMANCE-SUITE.md` — the issue, with the
  eleven-row divergence table this project answers.
- `specs/reference/STORE_SEMANTICS.md` — the contract as far as it goes today.
- `specs/design/opendal-path-mapping/` — wrote that contract, and rejected building the suite
  inside a P0 fix.
- `liquers-core/tests/store_key_absolute.rs` and the `keyabs` family — the one existing
  cross-implementation suite, and the shape the issue asks this one to follow.
- `liquers-core/src/store_dir_index.rs` — `DirectoryIndex`, the shared directory source of truth.
- `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` — the out-of-tree consumer this suite must serve.
