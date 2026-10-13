# Phase 1: High-Level Design — Modular store configuration and adapters

## Purpose

Make what a store does with metadata and access **composable and configurable per mount**, instead
of hard-coded in every backend. Four stages — **create** missing metadata, **enhance** it from the
data, **store** it in a layout, and enforce an **access policy** — become adapters and layouts that
wrap any backend and are chosen in the store configuration. The read-only adapter is the first one
delivered; the front-matter adapter of `agent-memory-mvp` is another.

## Problem Example

Serve this repository's `specs/` folder through Liquers, read-only, with titles taken from each
document's front-matter:

```yaml
stores:
  - type: filesystem
    prefix: specs
    config: { path: ./specs }
```

**Today:** the first `get` of any file writes a `.__metadata__` sidecar into the git checkout
(`AsyncFileStore::get` repairs missing metadata by persisting it), so `git status` fills with files
nobody asked for; the store API offers `PUT` and `DELETE` on every key; the content hash is
recomputed only when bytes are read, so an edit made by another program goes unnoticed in a listing;
the title is the file name, because nothing reads front-matter; and choosing a different metadata
layout — a `__metadata__` folder, a separate namespace, none — needs a new store type. None of this
can be said in the configuration; the `metadata` field of `StoreConfig`
(`liquers-core/src/store_config.rs`) is "reserved for future use".

**After:**

```yaml
stores:
  - type: filesystem
    prefix: specs
    config: { path: ./specs }
    metadata:
      layout: none            # sidecar | folder | namespace | none
      cache: memory           # keep derived metadata in memory, never on disk
    adapters:
      - read_only
      - enhance: { content_hash: on_change, media_type: true }
      - front_matter: { project: [title, description], write_back: false }
```

Reads write nothing, writes are refused with `NotSupported`, a changed file gets a new version on
the next listing, and `AssetInfo.title` is the document's front-matter title. (The field names are
illustrative; Phase 2 fixes the format.)

## Scope and Acceptance Criteria

- **AC-1** Read-only mount
  WHEN a mount is configured `read_only` and a client lists, reads and asks for metadata of entries
  that have no stored metadata
  THEN no file or object is created in the backend, and `set`, `set_metadata`, `remove`, `makedir`
  and `removedir` fail with `NotSupported` (501 through the HTTP APIs), and the store reports itself
  read-only through a new runtime accessor that the store API also uses (today `StoreCapabilities`
  exists only as a conformance-fixture declaration)
- **AC-2** Derived metadata without persistence
  WHEN a mount uses `layout: none` with an in-memory cache
  THEN metadata is synthesized and enhanced on read, served consistently while the data is
  unchanged, and never written to the backend
- **AC-3** Configurable layout
  WHEN the same backend is configured with the `sidecar`, `folder` and `namespace` layouts in turn
  THEN metadata is written where each layout says, and a key the layout makes unrepresentable is
  refused by `is_supported` and by `set` with the same error, whichever layout is configured
- **AC-4** Change detection
  WHEN a file is changed by another program and the mount's hash policy is `on_change`
  THEN the next metadata read — including a listing — reports a new `version` (detected from cheap
  file facts, size and modification time, then re-hashed)
- **AC-5** Front-matter enhancer with write-back
  WHEN a Markdown entry with front-matter is read, and then its `description` is set through the
  metadata API on a mount with `write_back: true`
  THEN `title` and `description` come from the front-matter, and after the write the file's
  front-matter carries the new description and both views agree
- **AC-6** No configuration, no change
  WHEN a store is configured without `metadata` or `adapters`
  THEN it behaves exactly as today: the store conformance suite passes unchanged for every backend
- **AC-7** Adapted stores are checked
  WHEN the conformance suite runs over adapted stores (read-only, `layout: none`, front-matter)
  THEN each passes the rule families its declared capabilities claim
- **AC-8** Through the router
  WHEN an adapted store is a member of `AsyncStoreRouter`
  THEN every adapter behaviour above, `get_asset_info` and directory listings included, is observed
  through the router
- **AC-9** Configuration errors
  WHEN a mount names an unknown adapter or layout, or an invalid option
  THEN building the router fails with a configuration error naming the mount and the field
- **AC-10** Extensible
  WHEN a crate above `liquers-core` registers its own adapter (the front-matter adapter in
  `liquers-lib`) in a new adapter registry of the factory model (today the factory chain resolves store
  types only; the form is Phase 2's)
  THEN configurations can name it like a built-in one

Non-goals: layouts for `liquers-web`'s stores (a later milestone); identity, users and per-key ACL (`CORE-SESSION-AND-KEY-ACL`); conditional writes
(`STORE-WRITE-HAS-NO-PRECONDITION`); new backends; migrating existing stored metadata between
layouts (reading the legacy folder layout is enough).

## Core Interactions

- **Store:** an adapter wraps any `AsyncStore` and forwards what it does not change; a layout decides
  where a file-like backend keeps metadata and which keys it reserves (`STORE_SEMANTICS.md` §8).
- **Configuration:** `StoreConfig` gains its per-mount metadata and adapter sections; the store
  factories resolve them (`STORE_CONFIG_FSD.md`).
- **Assets:** unaffected, except that versions change when the enhancer detects outside edits.
- **Web/API:** read-only mounts answer writes with 501; nothing new to wire.
- **Bindings:** `liquers-web`'s stores take the policy and enhancer adapters (they are backend
  independent); adopting layouts there is a later milestone, not an acceptance criterion here.

## Crate Placement

`liquers-core`: the forwarding base, the layout abstraction, the built-in adapters (read-only,
create, content-hash and media-type enhancers, in-memory cache) and the configuration fields, because
configuration and construction already live there (`store_factory.rs`, `store_config.rs`).
`liquers-store`: `PathMap` and the OpenDAL store consult the layout. `liquers-web`: its stores adopt
layouts where they have paths. `liquers-lib`: the front-matter adapter, registered through the
factory chain, since front-matter is a document convention rather than a storage one.

## Documentation Intent

- Reference: extend `specs/reference/STORE_CONFIG_FSD.md` (the configuration) and
  `specs/reference/STORE_SEMANTICS.md` (layouts, reserved names, read-only capability).
- Guide: extend `specs/guides/STORE_IMPLEMENTATION_GUIDE.md` with writing an adapter or a layout and
  running the conformance suite over it.
- Other documents: none.
- Documents to update: `specs/guides/STORE_FACTORY_GUIDE.md`, `specs/README.md` capability map.

## Design Dependencies

- **Requires** `STORE-ROUTER-DOES-NOT-FORWARD-ASSET-INFO` (AC-8); small, fixed first or inside this
  design.
- **Required by** `agent-memory-mvp` (its decisions 2 and 4: the read-only mount and the
  front-matter adapter).
- **Overlaps** `STORE-VERSION-BLIND-TO-CHANGES-MADE-OUTSIDE-LIQUERS` (AC-4),
  `CORE-METADATA-NO-APPLICATION-ATTRIBUTES` (where projected fields beyond title and description
  go), `CORE-SESSION-AND-KEY-ACL` (excluded).

## Open Questions

Tiers: **[open design]** needs the user's decision; **[proposed]** has a recommendation to accept or
amend.

1. **[open design] Wrapper or pipeline?** Adapters as decorators around a store (each overrides some methods,
   forwards the rest), or one generic store driven by an ordered list of handlers? *Recommended:
   decorators over a forwarding base for create / enhance / policy, and a layout trait used inside
   file-like backends for the store stage* — a layout needs the backend's paths, which a decorator
   cannot see.
2. **[proposed] Configuration shape.** Use the reserved `metadata` field for the layout and cache, plus an
   ordered `adapters:` list; or a single `adapters:` list for everything? *Recommended: both, as in
   the problem example* — the layout is a property of the backend, adapters are layered on top.
3. **[proposed] Delivery order.** The user decided (2026-10-12, `agent-memory-mvp` decision 2) that the
   read-only adapter comes first. *Recommended:* ship it as the first milestone of this design,
   before layouts and enhancers, so `agent-memory-mvp` is unblocked early.
4. **[proposed] Where the hash policy lives.** Today `VersionVerification` is an environment setting
   (`environment_builder.rs`). Per mount, per environment, or both with the mount winning?
   *Recommended: both, mount wins.*
5. **[open design] Size.** Three issues merged make this `XL`. Split the layouts (store stage) from the adapters
   (create, enhance, policy) if Phase 2 confirms they are independent? *Recommended: decide at the
   Phase 2 gate.*

## References

Sources `STORE-METADATA-LAYOUT-HARDCODED-PER-STORE`, `STORE-NO-READ-ONLY-ADAPTER`,
`STORE-METADATA-HANDLING-NOT-COMPOSABLE`; `design/agent-memory-mvp/tiers-and-storage.md` §2 C (the
front-matter adapter and the forwarding problem); `design/sidecar-colliding-keys/` (reserved names).
