---
id: WEB-API-SPEC-READ-ONLY-MOUNT
kind: design
title: The web API specification names the read-only mount as the way to protect a store
form: compact
status: in_review
phase: implementation
readiness: ready
autofix: eligible
area: [docs, axum]
issues: [WEB-API-SPEC-SAYS-A-DIRECTORY-MOUNT-CANNOT-BE-READ-ONLY]
created: 2026-10-26
---
# The web API specification names the read-only mount as the way to protect a store

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written, not
approved, nothing implemented.

## Phase 1: High-Level Design

### Purpose

`specs/reference/WEB_API_SPECIFICATION.md` must be true at HEAD. Its §9, on access control, still
says that a store mounted from a directory can be protected only by leaving routes out. In fact the
store configuration can make a mount refuse every write. A reader who is securing a deployment
should be pointed at that setting.

### Problem Example

Verified at HEAD (`cd6d5da`). §9 (lines 642-646) ends:

> A store mounted from a directory stays writable through the Store API's `PUT`/`DELETE` routes and
> the Assets API's mutations unless those are left out (`STORE-NO-READ-ONLY-ADAPTER`).

`STORE-NO-READ-ONLY-ADAPTER` is `closed`. The evidence:

- `liquers-core/src/store/read_only.rs`: `ReadOnlyStore::new(inner)`. Its `set`, `set_metadata`,
  `remove` and `removedir` return `Error::not_supported("Store '…' is read-only: … refused")`, and it
  never writes when reading.
- `liquers-core/src/store_config.rs`: `StoreConfig::read_only` (per mount) and
  `StoreRouterConfig::read_only` (every mount). `STORE_CONFIG_FSD.md` documents both: the
  `read_only` field, and the build that wraps a mount in `ReadOnlyStore`.
- `liquers-axum/tests/store_api_routes.rs` `read_only_store_write_is_501`: under a `ReadOnlyStore`,
  `GET data/…` answers 200 and `PUT` / `DELETE data/…` answer 501 with
  `error.type = "NotSupported"`. This matches §3.1, where `NotSupported` maps to 501.

Today a reader follows §9 and removes the Store API's write routes, which also removes them for
every other mount. The section should name `read_only: true` on that mount, and state that a refused
write answers 501 `NotSupported`.

### Scope and Acceptance Criteria

- **AC-1** §9 names the read-only mount
  WHEN a reader looks in §9 for how to keep a mounted store from being written
  THEN it names `read_only: true` on the mount, or on the router for every mount, with a link to
  `STORE_CONFIG_FSD.md`, and names `ReadOnlyStore` for code that assembles stores by hand
- **AC-2** The observable answer is stated
  WHEN §9 describes a write to a read-only mount
  THEN it says that the store refuses the write with `NotSupported`, that the Store API answers 501
  (§3.1), that reads are unaffected, and it cites `read_only_store_write_is_501`
- **AC-3** No stale reference remains
  WHEN `specs/reference/` and `specs/guides/` are searched for `STORE-NO-READ-ONLY-ADAPTER` or
  "stays writable"
  THEN nothing remains that presents the adapter as missing
- **AC-4** The document's record is kept
  WHEN the change lands
  THEN `WEB_API_SPECIFICATION.md` has a new `## History` row and a bumped `reviewed:` date (§9.2)

Non-goals: rewriting §9 for principals and policies (`access-policy-at-entry-points`); claims about
which Assets API mutations reach the store, which are not tested at HEAD (see Phase 2).

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible. This is a documentation fix to one reference document, of size
  `S`, with no interface change.
- **Leading issue:** None
- **Explanation:** the facts are in code and in an existing route test. The fix replaces one
  sentence and adds a History row.
- **Open questions:** None.

### Design Dependencies

- `access-policy-at-entry-points` (in review, finished Phase 4): **overlaps**. Its Phase 4 rewrites
  §9 (principal, policy, 403) and noted this stale sentence, which led to this issue being filed. The
  two do not merge: E1 applies, because this item is eligible for automatic fixing and that design is
  not. Whichever lands second keeps the read-only paragraph when it rebases §9.
- `store-adapters` (complete): **requires**. It introduced `ReadOnlyStore` and `read_only`.

## Phase 2: Architecture

### Solution

In `specs/reference/WEB_API_SPECIFICATION.md` §9, replace the last sentence, "A store mounted from a
directory stays writable … (`STORE-NO-READ-ONLY-ADAPTER`).", with:

> A mount that must not be written is made read-only where the store is configured, not by leaving
> routes out. Set `read_only: true` on the mount (or on the router, for every mount;
> `[STORE_CONFIG_FSD.md](STORE_CONFIG_FSD.md)`), or wrap the store in `ReadOnlyStore` when
> assembling it in code. Every write that reaches such a store is refused with `NotSupported`,
> whichever route issued it, and the Store API answers 501 (§3.1). Reads are served as before, and
> nothing is written on read (`liquers-axum/tests/store_api_routes.rs`,
> `read_only_store_write_is_501`). Leaving out write routes remains a way to narrow what a
> deployment exposes, but it applies to every mount behind the router.

Add the History row `| 2026-MM-DD | §9: a mount is protected by the store configuration's
`read_only` (or `ReadOnlyStore`), with writes answered 501 `NotSupported`; the stale reference to the
closed `STORE-NO-READ-ONLY-ADAPTER` is removed. | `design/web-api-spec-read-only-mount/` |`, and set
`reviewed:` to the landing date.

Rejected: stating what each Assets API mutation answers on a read-only mount. Those routes go
through `AssetManager::set_binary` and the related methods, and no test covers that outcome. The new
sentence claims only what holds for every route, namely that the store refuses the write, plus the
Store API status that is tested.

### Changes

`specs/reference/WEB_API_SPECIFICATION.md` (§9, History, `reviewed:`). No code changes.
`liquers-axum/README.md` and `WEB_API_GUIDE.md` do not repeat the stale claim (checked with
`grep`).

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| A conflict with `access-policy-at-entry-points` §9 | It is one paragraph; the read-only paragraph survives the rewrite | high |
| Over-claiming Assets API behaviour | The wording is limited to the store's refusal and the tested Store API status | high |

Recovery: revert the document.

## Phase 3: Examples and Tests

### Examples

The configuration a reader is pointed to:

```yaml
stores:
  - type: filesystem
    prefix: data
    read_only: true
    config:
      path: ./data
```

`PUT /liquer/api/store/data/data/report.csv` answers `501` with `{"error": {"type": "NotSupported",
…}}`. `GET` of the same path answers `200`.

### Tests

No new test is needed. The behaviour the text describes is already covered:

- `cargo test -p liquers-axum --test store_api_routes read_only_store_write_is_501`, which must pass
  unchanged: AC-2
- `grep -rn "STORE-NO-READ-ONLY-ADAPTER\|stays writable" specs/reference specs/guides` prints
  nothing: AC-3
- `python3 scripts/docs_index.py --check`, plus reading the diff for the History row and
  `reviewed:`: AC-1, AC-4

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `specs/reference/WEB_API_SPECIFICATION.md` §9: replace the sentence with the Phase 2 paragraph. Proof: the `grep` of Phase 3 (AC-1, AC-2, AC-3)
- [ ] 2. The same file: add the History row and bump `reviewed:`. Proof: diff review (AC-4)
- [ ] 3. `specs/issues/WEB-API-SPEC-SAYS-A-DIRECTORY-MOUNT-CANNOT-BE-READ-ONLY.md` `status: closed` with a resolution note; `python3 scripts/docs_index.py`. Proof: `python3 scripts/docs_index.py --check`

### Validation

`cargo test -p liquers-axum --test store_api_routes read_only_store_write_is_501` (it is unchanged
and checks the claim), then `python3 scripts/docs_index.py --check`. Review the diff: one reference
document, the issue file and the index.
