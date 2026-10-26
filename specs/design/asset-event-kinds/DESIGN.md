---
id: ASSET-EVENT-KINDS
kind: design
title: The manager event stream announces new versions and removals, not only expirations
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/assets]
issues: [ASSET-EVENT-STREAM-CARRIES-ONLY-EXPIRATIONS]
affects_docs: [reference/ASSETS.md, reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md, guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md]
created: 2026-10-26
---
# The manager event stream announces new versions and removals, not only expirations

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written, not
approved, nothing implemented. A follow-up of `external-index-sync`, which designs the stream and
its `Expired` events (Phase 4 written, not implemented); this design adds two event kinds to it
and cannot land before it.

## Phase 1: High-Level Design

### Purpose

An index feeder, cache warmer or mirror (VISION: external views of a record source) subscribed to
the manager event stream of `external-index-sync` learns that something *became stale*, but not
that a key *got new content* or *went away*. Announce those two facts from the primitives that
already decide them, so a push-fed view stays current between reconciliations.

### Problem Example

Checked at HEAD (`cd6d5da`); the stream itself is not implemented yet, so this is the behaviour
`external-index-sync` Phase 2 specifies plus what HEAD's write paths do.

```rust
let m = envref.get_asset_manager();
let mut events = m.subscribe_asset_events(AssetEventScope::Under(parse_key("docs")?));
m.set_state(&parse_key("docs/a.txt")?, State::from("first draft")).await?;   // new document
m.remove(&parse_key("docs/b.txt")?).await?;                                    // a Source deleted
```

- **Today (with `external-index-sync` landed):** `events.try_recv()` returns `None` twice. The write
  goes through `publish_version` (`liquers-core/src/assets.rs`, `AssetManager::publish_version`)
  and `refresh_listing_version(docs)`; the removal through `AssetManager::remove`
  (`RemoveAction::Delete`). Neither sends anything; only a dependent's `Expired` would, and nothing
  depends on `docs/` (the listing version is refreshed only when registered).
- **Should:** `Updated { subject: Key(docs/a.txt), version: v1, origin: Written }`, then
  `Removed { subject: Key(docs/b.txt) }`, in that order. A query that later evaluates
  `-R/docs/a.txt/-/to_text` under a recipe `docs/summary.txt` adds
  `Updated { subject: Key(docs/summary.txt), version, origin: Evaluated }` when it finishes.

### Scope and Acceptance Criteria

- **AC-1** A write is announced
  WHEN `set_state` or `set_binary` stores a `Source`, `Override` or `Ready` value with a version under a subscribed scope
  THEN the subscriber receives `Updated { subject: Key, version, origin: Written }` once per write, carrying the written version
- **AC-2** An evaluated keyed asset is announced
  WHEN a keyed, non-volatile asset finishes evaluation and enters the dependency graph
  THEN the subscriber receives `Updated { origin: Evaluated }` with the version the graph registered
- **AC-3** A load is not a new version
  WHEN a stored keyed value is served by the fast track (loaded, not recomputed)
  THEN no `Updated` event is sent
- **AC-4** Deletion is announced; dropping a computed value is not
  WHEN `remove` deletes a user value (`RemoveAction::Delete`) and succeeds, or a corrupted stored value is deleted by `apply_external_change`
  THEN the subscriber receives `Removed { subject: Key }`; WHEN `remove` only drops a computed value (the recipe and version remain) or fails, THEN no `Removed` is sent
- **AC-5** An outside change adopted from the store is announced
  WHEN `apply_external_change` adopts a changed stored value (`AcceptAsInput` / `ConvertToOverride`)
  THEN `Updated { origin: Adopted, version: actual }` is sent
- **AC-6** Root before dependents
  WHEN a write or adoption cascades to dependents
  THEN the root's `Updated` precedes the dependents' `Expired` events
- **AC-7** Scope and managers as for expirations
  WHEN the scenarios above run on the queued, the inline and the external test manager, with `Under(docs)` and `Under(other)` subscribers
  THEN every manager delivers the same events, only to the matching scope, with no change to the external manager's code
- **AC-8** Queries and volatile assets are silent
  WHEN a non-keyed query asset or a volatile keyed asset finishes evaluation
  THEN no `Updated` event is sent

Non-goals: a separate listing-change event (question 1); events for metadata-only edits
(`set_description`), for `removedir`'s recipe files, or for the in-memory map evictions of the
expiration monitor; delivery out of process (`asset-events-over-websocket`); any change to the
per-asset `watch` channel.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — adds `pub` enum variants and a new `pub` enum to
  `liquers-core` (rules 3, 4), and the event type it extends does not exist yet.
- **Leading issue:** Open design question — whether a listing change is its own event kind.
- **Explanation:** every new-version and deletion decision already passes through a handful of
  provided `AssetManager` methods, and the event hub of `external-index-sync` lives in the
  `DependencyManager` every manager owns, so emission needs no required trait method. What remains
  open are contract choices with recommended answers.
- **Open questions:**
  1. **Open design - listing changes:** no `ListingChanged` kind (recommended). A folder's
     membership changes only through a write or a removal of a key in it, which this design
     announces with that key as subject, so `Under(dir)` already sees every change; and
     `refresh_listing_version` computes a listing version only when something depends on it, so
     a `ListingChanged` would be silent exactly for the folders the issue cites. Alternative: send
     `ListingChanged { dir, version }` from `refresh_one_listing` when the registered version moves
     (cheap, but only for registered listings).
  2. **Proposed resolution - origin field:** `Updated` carries `origin: UpdateOrigin` (`Written`,
     `Evaluated`, `Adopted`), so a feeder can ignore its own writes or recomputations. Without it
     every consumer re-derives the cause from timing.
  3. **Proposed resolution - removal semantics:** `Removed` means "the key no longer holds a
     stored value of its own": sent for `Delete` and for the corrupted-file deletion; not for
     `DropComputed`, which keeps the version, so an index of that key stays correct.
  4. **Proposed resolution - release order:** land in the same release as `external-index-sync`,
     so the `AssetEvent` enum (not `#[non_exhaustive]`, that design's question 5) gains its
     variants before any caller can match it exhaustively.
  5. **Implementation details (resolved in Phase 2):** a written value without a version sends no
     `Updated` (as `publish_version` registers nothing); `track_installed` sends nothing, because
     the write already announced through `publish_version`; the stale-dependency finalization
     (`Expired` with a new version) sends `Updated { Evaluated }`, because the key's content did
     change.

### Design Dependencies

- `external-index-sync` — **requires**: defines `AssetEvent`, `AssetEventSubject`,
  `AssetEventScope`, the hub (`DependencyManager::announce`) and the subscription. This design
  adds variants and emission sites only.
- `asset-events-over-websocket` — **required-by** (softly): it forwards whatever `AssetEvent`
  kinds exist; its wire mapping gains two message types when this lands, in either order.
- `assets-api-admin-operations` — **overlaps** (same change site, T3): it moves the body of
  `AssetManager::remove` into `remove_keyed`; the `Removed` emission goes wherever that body is
  at implementation time. That design has a finished Phase 4, so it is not extended.
- `remote-asset-manager` — **required-by**: invalidating a client cache needs new-version events.
- `EXTERNAL-INDEX-HAS-NO-RECONCILIATION-CONTRACT` (issue) — **overlaps**: its push path consumes
  these events; reconciliation stays the correctness guarantee.

Overlap with `ASSET-EVENTS-ARE-NOT-DELIVERED-OUT-OF-PROCESS` was judged weak: T2 (shared contract)
was considered and does not hold, because the WebSocket design forwards event kinds generically
behind a `type` tag that clients must tolerate unknown values of, so neither design's decision
constrains the other. Different crates and change sites; kept separate.

### Consolidated Findings

- Emission sites are exactly: `AssetManager::publish_version` (every write of every manager,
  including external ones, per `ASSET_MANAGER_IMPLEMENTATION_GUIDE.md` §Primitives), the two
  evaluation branches of `AssetRef` that register a finished keyed asset (`track_asset` and the
  stale-dependency `track_keyed_asset`), `AssetManager::remove`'s `Delete` arm after a successful
  store removal, and `apply_external_change` (adoption and corrupted deletion). Fast-track loads
  (`register_version` in `try_fast_track`) and `refresh_one_listing` are deliberately not sites.
- Emitting from `DependencyManager::register_version` instead was rejected: it cannot tell a load
  from a new version (both are a first registration), and it also sees command and listing keys.
- Ordering: every `Updated` is sent before the `expire_dependencies_result` call that follows it,
  which keeps `external-index-sync`'s "root before dependents" rule. `Removed` is the exception: it
  is sent after the store removal succeeds, so after the cascade's `Expired` events, because
  announcing a removal that then fails would be false. Phase 2 states this in the reference.
- No new lock or await: `announce` is a synchronous `broadcast::send`.

## Phase 2: Architecture

### Solution

Extend `AssetEvent` (from `external-index-sync`) with two variants and send them beside the
existing calls that decide a new version or a deletion, through the same private
`DependencyManager::announce`. The scope filter, lag handling and subscription are unchanged:
both new variants carry an `AssetEventSubject::Key`, which `Under(k)` already matches.

Rejected: emitting inside `DependencyManager::register_version` / `register_written_version`
(cannot distinguish load from change; sees non-asset keys); emitting in `track_installed` (double
announcement of every built-in `set_state`, which calls `publish_version` and
`track_replacing_asset`); a `ListingChanged` kind (Phase 1 question 1).

Known issues: `UNCACHED-STORED-COPY-EXPIRY-RACES-AN-INFLIGHT-EVALUATION` (draft) and
`RECIPE-ONLY-FOLDER-NAMES-HIDE-A-CREATED-FOLDER-FROM-ITS-PARENT` (draft) do not affect these
events: neither alters which write or removal happens, and no listing event is sent.

### Changes

`liquers-core/src/assets.rs` (types from `external-index-sync`):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateOrigin {
    /// `set_state` / `set_binary` (any manager, through `publish_version`).
    Written,
    /// A keyed asset finished evaluation and entered the dependency graph.
    Evaluated,
    /// An outside change to the stored value was adopted (`apply_external_change`).
    Adopted,
}

pub enum AssetEvent {
    Expired { /* unchanged */ },
    /// A key holds new content. `version` is the version now registered.
    Updated { subject: AssetEventSubject, version: Version, origin: UpdateOrigin },
    /// A key no longer holds a stored value of its own (deleted; a recipe may recompute it).
    Removed { subject: AssetEventSubject },
}
```

Emission (each a synchronous `self.dependency_manager().announce(..)`, no new await):

| Site | Condition | Event | Placed |
|---|---|---|---|
| `AssetManager::publish_version` | `dep_key.key()` (existing `DependencyKey::key`, `metadata.rs`) is `Ok(Some(key))`, i.e. a `-R/` key | `Updated { Key, version, Written }` | after `register_written_version`, before `expire_dependencies_result` |
| `AssetRef` evaluation finalization, `track_asset` branch | `bound_owner_key()` is `Some` and the asset was tracked (status `Ready`/`Source`/`Override`) | `Updated { Key, version, Evaluated }` | after the version is read, before `expire_dependencies_result` |
| same, stale-dependency branch (`track_keyed_asset`) | owner key `Some` | `Updated { Key, version, Evaluated }` | same |
| `AssetManager::remove` | `RemoveAction::Delete` and `result.is_ok()` | `Removed { Key }` | after the store removal, before `refresh_listing_version` |
| `apply_external_change` | `AcceptAsInput` / `ConvertToOverride` | `Updated { Key, actual, Adopted }` | before `expire_dependencies_result` |
| `apply_external_change` | `Delete`, store removal `Ok` | `Removed { Key }` | after the removal |

A private helper `fn announce_key_event(&self, event)` is not needed; each site builds the event
inline. `publish_version` takes a `DependencyKey`; the key is recovered with the existing
`DependencyKey::key()`, which answers `Ok(None)` for listing, recipe and command keys (no event)
and `Err` only for a malformed `-R/` string (no event; a `publish_version` caller never builds
one). The evaluation branches already hold the key.

`external-index-sync`'s `AssetEventSubscription` needs no change. Its match over `AssetEvent`
for the scope test (subject extraction) gains the two arms, explicitly (CLAUDE.md match rule).

Documents: `reference/ASSETS.md` §Communication Channels (event kinds, `UpdateOrigin`, the
`Removed` ordering exception); `reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md`; the guide's
§Primitives rows for `publish_version` ("also announces `Updated`") and the §8 note that events are
sent by the provided methods, so an external manager gets them by calling them.

### Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `liquers-core/src/assets.rs` (types, six sites) |
| Crates and workflows | `liquers-core`; every write, keyed evaluation and removal gains one sync send |
| Existing tests | none change; no assertion observes the stream except `external-index-sync`'s tests, which match `Expired` and must use explicit arms (an `Updated` now precedes a cascade's `Expired` in some of its scenarios, so tests that `recv` the first event after a write must skip `Updated`) |
| New validation | `liquers-core/tests/asset_events.rs` events09-events15 on three managers; `check-build-matrix.sh` wasm rows |
| Compatibility | Breaking for exhaustive matches of `AssetEvent` if released after `external-index-sync` (question 4) |
| Performance | One `broadcast::send` per keyed evaluation; a failed send with no receivers |
| Security | In-process only; reveals key names to in-process code that already holds the manager |
| Recovery | Revert; nothing else depends on the variants |
| Certainty | High for the sites (all inspected at HEAD); medium for exact line placement, because `external-index-sync` and `assets-api-admin-operations` will move code first |

## Phase 3: Examples and Tests

### Examples

```rust
// After external-index-sync and this design:
let mut ev = m.subscribe_asset_events(AssetEventScope::Under(parse_key("docs")?));
m.set_state(&parse_key("docs/a.txt")?, State::from("v1")).await?;
assert!(matches!(ev.recv().await, AssetEventReceive::Event(AssetEvent::Updated {
    origin: UpdateOrigin::Written, .. })));
m.remove(&parse_key("docs/a.txt")?).await?;           // a Source with no recipe: Delete
assert!(matches!(ev.recv().await, AssetEventReceive::Event(AssetEvent::Removed { .. })));
```

Evaluated: recipe `docs/summary.txt` with query `-R/docs/a.txt/-/to_text` (validated with
`liquers-validate`: `GetAsset[docs, a.txt]`, `Action{to_text}`); `m.get(&docs/summary.txt)` then
waiting for `Ready` yields `Updated { Key(docs/summary.txt), origin: Evaluated }`; a second `get`
in a fresh environment over the same store (fast track) yields nothing (AC-3).

### Tests

In `liquers-core/tests/asset_events.rs` (created by `external-index-sync`), each run on
`DefaultAssetManager`, `ImmediateAssetManager` and the external test manager of
`tests/external_asset_manager.rs` through the file's existing per-manager harness:

- `events09_write_announces_updated` — AC-1, AC-7
- `events10_evaluation_announces_updated_once` — AC-2, AC-7
- `events11_fast_track_load_is_silent` — AC-3
- `events12_remove_delete_announces_removed_drop_computed_does_not` — AC-4
- `events13_adopted_external_change_announces_updated` — AC-5 (uses the store-sync fixture that
  rewrites stored bytes, as `apply_external_change`'s unit tests do)
- `events14_updated_precedes_dependents_expired` — AC-6 (`docs/b.txt` recipe reading
  `-R/docs/a.txt`; write `a`, expect `Updated(a)` then `Expired(b)`)
- `events15_query_and_volatile_evaluations_are_silent` — AC-8; plus an `Under(other)` subscriber
  asserting `try_recv()` is `None` throughout (AC-7 scope half)
- unit test in `assets.rs`: `corrupted_external_change_announces_removed` — AC-4 second half

Run: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --test asset_events --test external_asset_manager`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-core/src/assets.rs` `UpdateOrigin`, `AssetEvent::{Updated, Removed}`, the scope
  test's subject arms; re-exports beside `AssetEvent` — no emission yet — proof:
  `cargo test -p liquers-core --lib` (requires `external-index-sync` merged)
- [ ] 2. `AssetManager::publish_version` emission, keyed through `DependencyKey::key()` — proof:
  `events09`
- [ ] 3. Evaluation finalization (both tracking branches of `AssetRef`'s finish path) — proof:
  `events10`, `events11`, `events15`
- [ ] 4. `AssetManager::remove` (or `remove_keyed`, if `assets-api-admin-operations` has landed)
  `Delete` arm — proof: `events12`
- [ ] 5. `apply_external_change` both arms — proof: `events13`,
  `corrupted_external_change_announces_removed`
- [ ] 6. Ordering test and three-manager run — proof: `events14`; whole
  `cargo test -p liquers-core --lib --tests`
- [ ] 7. Documents: `reference/ASSETS.md`, `reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md`,
  `guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md` (History rows, `reviewed:` bumps); close
  `ASSET-EVENT-STREAM-CARRIES-ONLY-EXPIRATIONS` — proof: `python3 scripts/docs_index.py --check`

Rollback: each step reverts alone; steps 2-5 are independent of one another.

### Validation

`cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-lib --lib --tests` (no change
expected); `bash scripts/check-build-matrix.sh` wasm rows; `python3 scripts/docs_index.py --check`.
No `command_registry.yaml` change. Final diff review: no emission inside `DependencyManager`, no
`_ =>` arm over `AssetEvent`.
