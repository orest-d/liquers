# Phase 1: High-Level Design - store-conformance-suite

## Feature Name

`AsyncStore` conformance: an implemented suite, a completed contract, and a store implementation
guide

## Purpose

`AsyncStore` has seven in-tree implementations across three crates, each tested only against
itself; eleven ways they disagree were found one at a time by people tripping over them, one of
them a P0 that destroyed data. This project gives the trait an **executable** contract — a shared,
parameterized suite that any store implementation can be run against, in-tree or out — completes
`specs/reference/STORE_SEMANTICS.md`, which today records three questions as unsettled, and adds
`specs/guides/STORE_IMPLEMENTATION_GUIDE.md`, the operational counterpart that tells someone
writing the eighth store what decisions they have to make and how to check that they made them
consistently.

## Core Interactions

### Query System
None directly. The suite builds its keys with `parse_key` and never evaluates a query.

### Store System
The entire feature. In scope: `AsyncMemoryStore`, `AsyncFileStore`, `AsyncStoreRouter`
(`liquers-core`), `AsyncOpenDALStore` (`liquers-store`), `FetchStore`, `LocalStorageStore`,
`JsStore` (`liquers-web`), and the `AsyncStore` trait defaults themselves — **all seven are run
against the suite**, which is what "validate the existing implementations against the contract"
means concretely. The rules to be checked are the sections of `STORE_SEMANTICS.md`: the sibling
rule, directory derivation, derived versus explicit directories, absence versus failure, removal,
prefixes and routing, key shape, metadata sidecars, and `keys()`.

### Command System
None. No new commands, no namespace.

### Asset System
Indirectly: `get_asset_info` and `listdir_asset_info` are built on `get_metadata`, so directory
metadata is contract surface the suite must check.

### Web/API (if applicable)
No endpoint changes. `liquers-web`'s three stores must run the suite under `wasm32`, which is what
forbids a `#[tokio::test]`-shaped suite and makes this an `L`. `liquers-axum`'s store handlers are
the exposure that made the sibling rule a P0, but they are unchanged.

### Value Types
None.

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

**Reference:** **Extend** `specs/reference/STORE_SEMANTICS.md` — the normative *what*. It already
answers most of the contract; this project resolves its three ⚠ rows (the trait-default
`removedir`, `AsyncMemoryStore::is_supported`, and what `keys()` returns), and replaces each
section's *Enforced by* line with the suite's rule IDs. A second reference would split one contract
across two files.

**Guide:** **Create** `specs/guides/STORE_IMPLEMENTATION_GUIDE.md` — the operational *how*, playing
the role for a store that `LANGUAGE-INTEGRATION_GUIDE.md` plays for a language integration, and
following its structure: independently selectable capabilities with stable IDs, requirement levels
(Essential / Profile / Optional), implementation states (`NA`, `NS`, `PARTIAL`, `COMPLETE`,
`BLOCKED`, `CONFORMANT`), a per-store status matrix, per-capability design questions, and the rule
that `NA` must be argued and must carry a reversing condition.

**One deliberate divergence from that guide, and it is the point:** where
`LANGUAGE-INTEGRATION_GUIDE.md` fixes its contract in Appendix A as Python pseudocode — because no
one Rust suite could run in every integrated language — every store here is a Rust `AsyncStore`, so
the suite is **implemented once and applied to any implementation**. The guide names rule IDs that
resolve to real functions in `liquers_core::store_conformance`; there is no appendix of pseudocode
to drift from the code.

### What the guide must answer

The questions a store author has to settle before writing anything, which the guide poses per
capability and the suite then checks:

- **What plays the role of an internal key?** A path, an object name, a row key, a URL, a
  `localStorage` string key.
- **How do Liquers keys map onto internal keys?** Does the mapping round-trip? Which Liquers keys
  are *unrepresentable*, and are they refused (`is_supported`, path builders) rather than silently
  colliding — the `.__metadata__` collision is the existing instance.
- **Does the backend address by string prefix?** If so, the sibling rule (`data` versus `database`)
  is the first thing to get right; it is the class of bug that destroyed data here.
- **Can the backend store metadata as well as data, or is a fallback needed?** A sidecar, a native
  metadata facility, or derivation on the fly — and **which source wins when they disagree**.
- **Is the store read-only or writable?** How does it refuse writes, and with which error?
- **Are all keys supported, or only a subset?** A view of a database table with predefined
  projections supports a fixed key set; `is_supported` is where that is expressed, and it is what
  makes layering work.
- **Does the backend meaningfully support directories, and can children be found?** Which of the
  three sources of directory truth does it offer — `stat`, a bounded listing, or neither (then
  `DirectoryIndex`)? Can an explicitly created empty directory be distinguished from a derived one?
- **Is the backend authoritative and writable by others?** If yes, no write-side index may be kept,
  because it goes stale.
- **What does absence look like, and can it be told apart from a failure?** An S3 403 reported as
  `Ok(false)` is a lie about existence.
- **What is the prefix, and is it part of the backend path or stripped?** Every store but
  `FetchStore` keeps it; the router selects on `key_prefix()` alone.
- **What are the atomicity, concurrency and limit behaviours?** Are data and metadata written
  together? What happens on a concurrent `set`? Is a partial write possible at a quota boundary?
- **What does enumeration cost?** Is `keys()` a full backend scan, and is it acceptable at all?
- **How do backend errors map onto `ErrorType`?**
- **Is the store constructible from a configuration document?** That is a `StoreTypeInfo` and a
  place in a `StoreFactory` chain — `guides/STORE_FACTORY_GUIDE.md` owns the procedure and is
  linked, not repeated.

**Keeping the contract and the guide synchronized** is a mechanism, not an intention: the suite's
rule IDs are the shared spine, every contract section and every guide capability cites them, and a
test asserts that the IDs registered in code, cited in `STORE_SEMANTICS.md`, and cited in the guide
are the same set. The precedent is `registry_export`, which holds `specs/command_registry.yaml` to
the registered commands.

**Other documents to create:** None.

**Specific documents to update:**
- `specs/README.md` — link the new guide in the capability map.
- `CLAUDE.md` §"Adding a Store Backend" — replace the five steps with a pointer to the guide plus
  running the suite.
- `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §STORE — its direction-2 questions ("which host
  storage, is it read-only", "where does metadata come from", "directory semantics on a backend
  that has none") are the new guide's subject; cross-link rather than answer twice.
- `specs/guides/STORE_FACTORY_GUIDE.md`, `specs/reference/STORE_CONFIG_FSD.md` — cross-links, so
  the three store guides form one path: implement (new) → declare a type → configure.
- `specs/index.csv`; and the issue files closed or depended on below.

**Audience:** whoever writes the eighth `AsyncStore`, in this repository or against it. They should
be able to read the guide, answer its questions, run the suite, and find out where they disagree
with the contract, without ever opening this design folder.

## Open Questions

1. **What does `keys()` enumerate** — data keys only, or data keys plus directories plus the
   prefix? The suite cannot accept both, so this must be decided, not deferred. (Row 10;
   `CORE-STORE-KEYS-MEANS-TWO-DIFFERENT-THINGS` records the candidates.)
2. **Does the trait-default `removedir` become `Ok(())`**, matching all three overrides, or do the
   overrides become an error? (Row 6, this issue's own.)
3. **Are the guide's capability IDs and the suite's capability declaration the same vocabulary?**
   They should be — a store declaring `WRITE` in code is the same claim as `WRITE: COMPLETE` in its
   status row — but that couples a document's ID scheme to a Rust type, and Phase 2 must decide how
   tightly.
4. **How does a rule get a fresh store?** A factory closure the harness supplies, or a store handed
   in already-empty and cleaned up by the rule? `LocalStorageStore` persists across tests in one
   browser session, so this is not cosmetic.
5. **How does one rule map to one reported failure** under two harnesses that disagree about what a
   test is — a macro generating a `#[tokio::test]` / `#[wasm_bindgen_test]` per rule, or one test
   per store collecting a report?
6. **Is the synchronous `Store` trait in scope?** `MemoryStore`, `FileStore` and `StoreRouter`
   implement it and have the same rules. The issue says `AsyncStore`; including them roughly
   doubles the suite.
7. **What happens when an existing store fails its own suite?** Several will — the three ⚠ rows are
   live divergences. Fix inside this project, or land the suite with named expected failures and
   an issue each? The guide's `BLOCKED` state exists for exactly this and suggests the latter.

## References

- `specs/issues/STORE-ASYNC-STORE-NO-BEHAVIOURAL-CONFORMANCE-SUITE.md` — the issue, with the
  eleven-row divergence table this project answers.
- `specs/reference/STORE_SEMANTICS.md` — the contract as far as it goes today.
- `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` — the model for the new guide, and §STORE is the
  half of it that moves.
- `specs/design/opendal-path-mapping/` — wrote the contract, and rejected building the suite inside
  a P0 fix.
- `liquers-core/tests/store_key_absolute.rs` and the `keyabs` family — the one existing
  cross-implementation suite, and the shape the issue asks this one to follow.
- `liquers-core/src/store_dir_index.rs` — `DirectoryIndex`, the shared directory source of truth.
- `liquers-lib/tests/registry_export.rs` — the precedent for holding a document to the code.
