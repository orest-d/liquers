# Phase 1: High-Level Design — Modular store configuration and adapters

## Purpose

Make what a store does with **access** and **metadata** configurable per mount, instead of
hard-coded in every backend. Three concerns become separate, composable parts chosen in the store
configuration:

1. **Access policy** — any store, of any type, can be made read-only.
2. **Metadata storage model** — *where* metadata is kept: one record per entry under a configurable
   key, records grouped per folder, centrally for a whole store or router, or not at all.
3. **Metadata handlers** — *what* metadata is: an ordered list of pluggable handlers that create
   default metadata, validate what a caller supplies, and fill or overwrite fields on write and on
   read.

The read-only policy is delivered first; the front-matter handler of `agent-memory-mvp` is one of
the handlers.

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
the media type is whatever was recorded, even after a rename; the title is the file name, because
nothing reads front-matter; and choosing a different metadata layout — a `__metadata__` folder, one
file per folder, a database, none — needs a new store type. None of this can be said in the
configuration; the `metadata` field of `StoreConfig` (`liquers-core/src/store_config.rs`) is
"reserved for future use".

**After:**

```yaml
stores:
  - type: filesystem
    prefix: specs
    config: { path: ./specs }
    read_only: true
    metadata:
      storage: none                  # per_entry | grouped | central | none
      persist_defaults: false        # derived metadata is cached in memory, never written
      handlers:
        - media_type: { from: extension }
        - file_facts                 # size and the backend's modification time
        - content_hash: { when: on_change }
        - front_matter: { fields: [title, description], write_back: false }
```

Reads write nothing, writes are refused with `NotSupported`, a changed file gets a new version on
the next listing, the media type follows the extension, and `AssetInfo.title` is the document's
front-matter title. A writable mount of generated data would instead choose, for example,
`storage: { model: grouped, file: __metadata__.json }`. (Field names are illustrative; Phase 2 fixes
the format.)

### Metadata storage models

The metadata of `docs/a.md` under each model. The interface must support all of them; the MVP
implements the rows marked **built**.

| Model | Where it lives | MVP |
|---|---|---|
| (a) per entry, key template `{key}.__metadata__` | `docs/a.md.__metadata__` (today's sidecar) | **built** (existing) |
| (a) per entry, `__metadata__/{key}` | `__metadata__/docs/a.md` | **built** (same template mechanism) |
| (a) per entry, `{folder}/__metadata__{name}.json` | `docs/__metadata__a.md.json` | **built** (same template mechanism) |
| (a) per entry, `{folder}/__metadata__/{name}.json` | `docs/__metadata__/a.md.json` (the Python predecessor) | **built** (same template mechanism) |
| (b) grouped, one file per folder | the `a.md` entry of `docs/__metadata__.json` | **built** |
| (b) grouped, a database (sqlite, sled, fjall) | a row or value keyed by folder and name | interface only |
| (c) central, for a store or a router | one metadata storage for every member; members keep data only | interface, plus a test implementation |
| none | synthesized by the handlers, optionally cached in memory | **built** |

Models (a) and (b) keep their records **in the same backend** as the data, under keys the model
reserves; a template's reserved names follow from the template (`STORE_SEMANTICS.md` §8). Model (c)
removes the duplication today's contract implies when a router fronts its members: with central
storage, **the router decides the metadata**, and the members store none.

## Scope and Acceptance Criteria

- **AC-1** Read-only, for any store
  WHEN a mount of any store type (built-in, OpenDAL or registered by another crate), or a whole
  router, is configured `read_only: true` — or any `AsyncStore` is wrapped as read-only in code —
  and a client lists, reads and asks for metadata of entries that have no stored metadata
  THEN no file, object or record is created in the backend, and `set`, `set_metadata`, `remove`,
  `makedir` and `removedir` fail with `NotSupported` (501 through the HTTP APIs), and the store
  reports itself read-only through a new runtime accessor that the store API also uses (today
  `StoreCapabilities` exists only as a conformance-fixture declaration)
- **AC-2** Derived metadata without persistence
  WHEN a mount uses `storage: none`, or `persist_defaults: false` with any model
  THEN metadata is synthesized and enhanced on read, served consistently while the data is
  unchanged, and never written to the backend
- **AC-3** Configurable per-entry key template
  WHEN the same backend is configured in turn with each per-entry template of the table above
  THEN metadata is written under the key that template gives, and a key the template makes
  unrepresentable is refused by `is_supported` and by `set` with the same error, whichever template
  is configured
- **AC-4** Change detection
  WHEN a file is changed by another program and the mount's hash policy is `always`
  THEN the next metadata read — including a listing — re-hashes the bytes and reports a new `version`;
  and with the cheaper policy `on_change`, a change visible in the backend's change facts (size and
  modification time, or a generation id / ETag where the backend has one) that no longer match the
  facts recorded in the metadata does the same, while a rewrite those facts cannot see (same size,
  preserved timestamp, `cp -p`) is documented as missed by that policy and caught by `always` or by
  `verify_stored_versions`
- **AC-5** Front-matter handler with write-back
  WHEN a Markdown entry with front-matter is read, and then its `description` is set through the
  metadata API on a mount with `write_back: true`
  THEN `title` and `description` come from the front-matter, and after the write the file's
  front-matter carries the new description and both views agree; a Markdown file without
  front-matter is read with the other handlers' metadata and no error
- **AC-6** No configuration, no change
  WHEN a store is configured without `read_only` or `metadata`
  THEN it behaves exactly as today: the store conformance suite passes unchanged for every backend
- **AC-7** Adapted stores are checked
  WHEN the conformance suite runs over adapted stores (read-only, `storage: none`, each per-entry
  template, grouped, central, front-matter)
  THEN each passes the rule families its declared capabilities claim
- **AC-8** Through the router
  WHEN an adapted store is a member of `AsyncStoreRouter`
  THEN every behaviour above, `get_asset_info` and directory listings included, is observed
  through the router
- **AC-9** Configuration errors
  WHEN a mount names an unknown storage model or handler, or an invalid option
  THEN building the router fails with a configuration error naming the mount and the field
- **AC-10** Extensible
  WHEN a crate above `liquers-core` registers its own handler (the front-matter handler in
  `liquers-lib`) or its own storage model in a registry of the factory model (today the factory
  chain resolves store types only; the form is Phase 2's)
  THEN configurations can name it like a built-in one
- **AC-11** Grouped metadata
  WHEN a mount uses `storage: { model: grouped, file: __metadata__.json }` and several entries of a
  folder are written, one entry's metadata is changed, and one entry is removed
  THEN the folder holds one `__metadata__.json` with a record per remaining entry, changing one
  record leaves the others intact, a folder listing reads the group file once rather than once per
  entry, and the key `__metadata__.json` is reserved in every folder
- **AC-12** Central metadata on a router
  WHEN a router is configured with central metadata storage (in the test, an in-memory
  implementation standing in for a database) over two members
  THEN metadata written and read through the router goes to the central storage only, no member
  stores a record of its own, a member configured with its own storage model under such a router is
  a configuration error naming that member, and the members' data is unaffected
- **AC-13** Default metadata, stored or not
  WHEN an entry that has no stored metadata is read on a writable mount with `persist_defaults:
  true`, and on another with `persist_defaults: false`
  THEN the first stores the created default record, which later reads return without re-creating
  it; the second writes nothing and returns the same record each time
- **AC-14** Validation of supplied metadata
  WHEN `set` or `set_metadata` is called with metadata a configured validator rejects (for example a
  required field missing, or a media type contradicting the extension under a strict policy)
  THEN the call fails with an error naming the field and nothing is written; a validator configured
  to normalize corrects the field instead and the write succeeds
- **AC-15** Filling on write and on read
  WHEN data is written with caller metadata, and later a file written by another program is read
  THEN on write the handlers fill or overwrite their fields as configured (content hash, file size,
  media type from the extension, timestamps), whatever the caller supplied for an overwritten field,
  and on read the read-stage handlers do the same; a field a handler only *fills* keeps a value
  already present
- **AC-16** Composition order
  WHEN two handlers in a mount's list both set the same field
  THEN the later handler in the list wins, on write and on read alike, and reordering the list in
  the configuration reverses the outcome

Non-goals: database (sqlite, sled, fjall) storage models and a production central storage beyond
the interface and the test implementation of AC-12; migrating stored metadata between models
(reading today's sidecars and the legacy folder layout is enough); models for `liquers-web`'s stores
(a later milestone); identity, users and per-key ACL (`CORE-SESSION-AND-KEY-ACL`); conditional
writes (`STORE-WRITE-HAS-NO-PRECONDITION`); new data backends.

## Core Interactions

- **Store:** the policy wraps any `AsyncStore` and forwards what it does not refuse. The storage
  model and the handlers form a **metadata layer** between the caller and the backend. **A
  decorator alone cannot make reads non-writing:** `AsyncFileStore::get` and `get_metadata` repair
  missing metadata by calling their own `set_metadata` inside the backend, which a wrapper never
  sees. So the backends need **data-only access** — reading and writing data and listing keys
  without touching metadata — on which the layer keeps metadata through its storage model. Today's
  behaviour becomes the default model (the `{key}.__metadata__` template) and the default handler
  chain (today's `default_metadata` and `finalize_metadata`), which is how AC-6 holds.
- **Router:** forwards to the member's layer as today, or, with central storage (AC-12), runs the
  layer itself over data-only members.
- **Configuration:** `StoreConfig` gains `read_only` and its `metadata` section, the router
  configuration the same; the store factories resolve them (`STORE_CONFIG_FSD.md`).
- **Assets:** unaffected, except that versions change when a handler detects outside edits.
- **Web/API:** read-only mounts answer writes with 501; nothing new to wire.
- **Bindings:** `liquers-web`'s stores take the policy and the handlers (backend independent);
  storage models there are a later milestone, not an acceptance criterion here.

## Crate Placement

`liquers-core`: the policy wrapper, the metadata layer, the storage-model and handler abstractions,
the built-in models (per-entry templates, grouped, none, the test central storage) and handlers
(defaults, validation, media type, file facts, content hash, in-memory cache), and the
configuration fields, because configuration and construction already live there
(`store_factory.rs`, `store_config.rs`). `liquers-store`: `PathMap` and the OpenDAL store provide
data-only access and consult the model's reserved names. `liquers-web`: its stores adopt the policy
and the handlers. `liquers-lib`: the front-matter handler, registered through the factory chain,
since front-matter is a document convention rather than a storage one. Database models, when built,
go in `liquers-store` behind features.

## Documentation Intent

- Reference: extend `specs/reference/STORE_CONFIG_FSD.md` (the configuration) and
  `specs/reference/STORE_SEMANTICS.md` (storage models, reserved names, read-only capability, the
  handler stages).
- Guide: extend `specs/guides/STORE_IMPLEMENTATION_GUIDE.md` with writing a handler or a storage
  model and running the conformance suite over it.
- Other documents: none.
- Documents to update: `specs/guides/STORE_FACTORY_GUIDE.md`, `specs/README.md` capability map.

## Design Dependencies

- **Requires** `STORE-ROUTER-DOES-NOT-FORWARD-ASSET-INFO` (AC-8) — closed 2026-10-13.
- **Required by** `agent-memory-mvp` (its decisions 2 and 4: the read-only mount and the
  front-matter handler).
- **Overlaps** `STORE-VERSION-BLIND-TO-CHANGES-MADE-OUTSIDE-LIQUERS` (AC-4, closed),
  `CORE-METADATA-NO-APPLICATION-ATTRIBUTES` (where projected fields beyond title and description
  go), `STORE-WRITE-HAS-NO-PRECONDITION` (a grouped file is read-modify-write),
  `CORE-SESSION-AND-KEY-ACL` (excluded).

## Scope Changes

- **2026-10-15, at the user's request, before approval.** Three requirements made explicit:
  read-only for *any* store, set in its configuration (AC-1 widened); a storage-model interface
  covering per-entry key templates, grouped records (a folder file or a database) and central
  storage on a store or router, with per-entry templates and the grouped `__metadata__.json` built
  (AC-3 rewritten for templates; AC-11, AC-12 added); and a composable handler system with four
  stages — create defaults (stored or not), validate supplied metadata, fill on write, fill on read
  (AC-13 to AC-16 added). "Adapters" for metadata became *storage models* and *handlers*; open
  question 6 folded into question 1. Size stays XL (question 5).

## Open Questions

Tiers: **[open design]** needs the user's decision; **[proposed]** has a recommendation to accept or
amend.

1. **[open design] Wrapper or pipeline?** *Recommended:* a decorator for the access policy; for
   metadata, one **metadata layer** — a storage model plus an ordered list of handlers (trait
   objects, each implementing any of the four stages) — over **data-only access** that every
   backend gains, with a default preserving today's behaviour. A per-backend "do not write on read"
   flag (the alternative of the former question 6) is rejected: every backend would reimplement it,
   and it leaves the storage model inside each backend.
2. **[proposed] Configuration shape.** `read_only: bool` beside `type`, and the reserved `metadata`
   field holding `storage`, `persist_defaults` and `handlers`, on a mount and on the router.
   *Recommended: as in the problem example.*
3. **[proposed] Delivery order.** The user decided (2026-10-12, `agent-memory-mvp` decision 2) that
   the read-only policy comes first. *Recommended:* M1 read-only with data-only reads; M2 the
   metadata layer with the default model and chain, then per-entry templates and grouped; M3 the
   handlers (media type, file facts, content hash, validation, front-matter); M4 central storage on
   the router.
4. **[proposed] Where the hash policy lives.** Today `VersionVerification` is an environment setting
   (`environment_builder.rs`). Per mount, per environment, or both with the mount winning?
   *Recommended: both, mount wins* — the mount's setting is the `content_hash` handler's option.
5. **[open design] Size.** Now clearly `XL`. Split into (i) read-only with data-only access, (ii) the
   metadata layer and storage models, (iii) the handlers? *Recommended: decide at the Phase 2
   gate*, once the interfaces show how independent they are.
6. ~~**The non-persisting read.**~~ Folded into question 1 on 2026-10-15: data-only access is the
   non-persisting read.
7. **[proposed] Central storage and member storage together.** A member that configures its own
   model under a router with central storage: a configuration error, or ignored with a warning?
   *Recommended: an error* (AC-12) — silently ignoring it would hide where metadata goes.
8. **[proposed] Timestamps.** `MetadataRecord` has `updated` and `file_size` but no creation time
   and no record of the backend's modification time, which `on_change` detection (AC-4) and the
   `file_facts` handler need. *Recommended:* add both as optional fields in Phase 2.
9. **[proposed] Handler failure.** A read-stage handler that fails (malformed front-matter):
   *recommended* — the entry stays readable, the failure is a warning in the metadata log;
   validators on write fail the write (AC-14).
10. **[proposed] Grouped writes.** A grouped file is read-modify-write. *Recommended:* serialize
    writes per group inside one process; atomicity across processes waits for
    `STORE-WRITE-HAS-NO-PRECONDITION`, as for the rest of the MVP.

## References

Sources `STORE-METADATA-LAYOUT-HARDCODED-PER-STORE`, `STORE-NO-READ-ONLY-ADAPTER`,
`STORE-METADATA-HANDLING-NOT-COMPOSABLE`; `design/agent-memory-mvp/tiers-and-storage.md` §2 C (the
front-matter adapter and the forwarding problem); `design/sidecar-colliding-keys/` (reserved names);
`liquers-core/src/store.rs` (`METADATA_SUFFIX`, `METADATA_FOLDER`, `AsyncStore::default_metadata`,
`finalize_metadata`).
