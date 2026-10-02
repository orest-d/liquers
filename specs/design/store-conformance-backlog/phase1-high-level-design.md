# Phase 1: High-Level Design - Store Conformance Backlog

## Feature Name

Store conformance backlog — make every store, the contract and the test IDs agree

## Purpose

The store conformance suite (`design/store-conformance-suite/`) found seven disagreements between
`STORE_SEMANTICS.md` and in-tree stores. They are parked as `Blocked` rules, allowed failures, a test
that never runs in a browser, or IDs that mean two things. This project settles the contract where it
is ambiguous, brings every store up to it, and deletes each parking mechanism so the suite's report is
clean and can go red again when something breaks.

## Core Interactions

### Query System
None directly. `-R-dir/` inspection of page-defined (`JsStore`) directories starts working.

### Store System
The whole change. By contract area:
- **Directory metadata (§2):** decide whether `children` is populated (7 stores yes, `AsyncOpenDALStore`
  no), make the contract and all stores agree, and unblock rule `dir07`. `JsStore::get_metadata`
  returns directory-shaped metadata for a directory instead of calling `get` (fixes `dir04`/`dir07`).
- **Absence (§4, §5):** a `JsStore` delegate can signal "not found" (`null`/`undefined` from `get`), so
  `absence01`/`remove03` pass; `LocalStorageStore::removedir` on an absent directory is `Ok`.
- **Key shape and enumeration (§7, §9):** `LocalStorageStore` refuses relative keys in `contains` and
  includes ancestor directories in `keys()`.
- **Sidecars (§8):** `AsyncFileStore`/`FileStore::listdir` report a metadata-only key instead of
  dropping it; a new `sidecar` rule checks this.
- **Media type:** liquers-web's `http` store applies the extension's media type before the response
  header, so `input.csv` reports `text/csv`.

### Command System / Asset System / Value Types / Web API / UI
None. Assets see stores only through `AsyncStore`. No commands, value types, axum routes or widgets
change. The one visible effect is that asset resolution sees `KeyNotFound` from a page store.

### Test infrastructure
The `LocalStorageStore` conformance test moves to a file that runs in a browser. Store unit tests
that reuse conformance rule IDs (`dir04`, `dir05`, `sibling02-04`, `remove01-02`, `traitdef01`) are
renamed to descriptive names. Confirmed duplicates are deleted only after checking that the
replacing rule fails when the behaviour is broken.

## Crate Placement

`liquers-core` (file/memory stores, conformance rules), `liquers-store` (OpenDAL, if §2 needs it),
`liquers-web` (`JsStore`, `LocalStorageStore`, `http`/`fetch` store, test layout). No new crate or
dependency; nothing moves between crates.

## Documentation Intent

**Reference:** Extend `specs/reference/STORE_SEMANTICS.md` (§2 `children`, §4 absence for delegate
stores, §8 metadata-only listing). No new reference is needed: this is the existing contract made
consistent.
**Guide:** Extend `specs/guides/STORE_IMPLEMENTATION_GUIDE.md`, covering the rule-ID ownership
convention (conformance rules own the IDs, unit tests get descriptive names) and the
`JsStore` not-found protocol for page authors.
**Other documents to create:** None. A new issue is filed only if §2 chooses a bounded or lazy
`children`, or if a fix is left out.
**Specific documents to update:** `liquers-web/README.md` (the browser test loop and `JsStore`
protocol), the seven issue files (closed in Phase 5), the three stub designs (→ `superseded`), and
`specs/README.md` (the capability-map line for this design).

## Open Questions

1. §2 `children`: keep them populated eagerly and document the cost (the stub design's lean), or
   populate none? Either way, `AsyncOpenDALStore` either aligns or has its exception written into the
   contract.
2. `JsStore` absence: is `null`/`undefined` from `get`/`getMetadata` enough, or should a thrown
   `{kind:"not-found"}` also be recognised?
3. Scope: is the `http` media-type issue in, or deferred? It shares only the web test loop.
4. Unit-test cleanup: rename only, or rename and delete verified duplicates in this project?

**Resolved at approval (2026-09-29), all as recommended:** (1) `children` stays populated;
the cost is documented. (2) Only `null`/`undefined` signals absence. (3) The `http` media-type issue
is in scope. (4) Rename the tests, then delete only duplicates whose rule is shown to fail when the
behaviour breaks. Phase 2 records how each decision is implemented.

## References

- `specs/design/store-conformance-suite/` (complete); `specs/reference/STORE_SEMANTICS.md`
- Issues listed in `DESIGN.md`; stub designs `store-directory-metadata-children`,
  `js-store-directory-metadata`, `js-store-not-found-sentinel`
