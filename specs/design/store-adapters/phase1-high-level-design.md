# Phase 1: High-Level Design — Modular store configuration and adapters

## Purpose

Make what a store does with **access**, **metadata** and **writes** configurable per mount,
instead of hard-coded in every backend. Four concerns become separate, composable parts chosen in
the store configuration:

1. **Access policy** — any store, of any type, can be made read-only.
2. **Metadata storage model** — *where* metadata is kept: one record per entry under a configurable
   key, records grouped per folder, centrally for a whole store or router, or not at all.
3. **Metadata handlers** — *what* metadata is: an ordered list of pluggable handlers that create
   default metadata, validate what a caller supplies, and fill or overwrite fields on write and on
   read.
4. **Write strategy** — *how safely* data and metadata are written: directly in place (today), each
   object atomically, or as one all-or-nothing transaction with recovery after a crash.

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
nothing reads front-matter; a crash between the three writes of `set` (metadata marked `Storing`,
then data in place, then the final metadata) leaves a truncated file or data and metadata that
disagree; and choosing a different metadata layout — a `__metadata__` folder, one
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
`storage: { model: grouped, file: __metadata__.json }` and `writes: transactional`. (Field names are illustrative; Phase 2 fixes
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
storage, **the router decides the metadata**: the central record is authoritative, and a member
either stores none or keeps its own copy as a backup.

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
  template, grouped, central, front-matter, each write strategy)
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
  THEN metadata written through the router goes to the central storage, and reads are answered
  from it; a member with no storage model of its own stores nothing, while a member configured with
  one keeps a backup copy written with every central write, from which a record missing centrally
  is restored (with a warning); the members' data is unaffected
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
- **AC-17** Write strategy is configurable
  WHEN a mount configures `writes: direct`, `atomic` or `transactional`
  THEN `set`, `set_metadata` and `remove` follow that strategy; `direct` is the default and is
  today's behaviour (AC-6), and a strategy whose guarantee the backend cannot give (for example
  `atomic` on a backend with neither atomic object writes nor rename) is a configuration error
  naming the mount
- **AC-18** Atomic objects
  WHEN a write under `atomic` fails or is interrupted part-way (the test injects a failure into the
  backend at each step)
  THEN the object being written is either wholly its old content or wholly its new content, never
  truncated, and no staging key appears in a listing
- **AC-19** All-or-nothing data and metadata
  WHEN a `set` or `remove` under `transactional` is interrupted after any one of its steps, under
  each storage model (per entry, grouped, central)
  THEN after recovery the key reads either the old data with the old metadata or the new data with
  the new metadata, never a mix — and for the grouped model the other records of the group are
  unchanged
- **AC-20** Recovery
  WHEN a key is read or its folder listed, or the store is synced, with the staged objects or the
  journal record of an interrupted transactional write left in the backend
  THEN the write is completed or rolled back, the staged objects are removed, the outcome is
  recorded as a warning in the key's metadata log, and a sync of the whole store reports what it
  repaired
- **AC-21** One configuration mechanism
  WHEN a handler chain for a stage is configured — built-in handlers, a registered one, and two of
  them stacked so that the second receives the first one's output
  THEN all are named in the same form, with no configuration syntax specific to any handler, and
  the chain applies them in order (the form is a Phase 2 decision between commands and a query
  compiled to a function, question 2)
- **AC-22** Presets
  WHEN a mount selects a preset and overrides one of its fields, and an application defines a
  preset of its own in the configuration
  THEN the mount behaves as the preset with that one field changed, the application's preset is
  usable like a built-in one, and an unknown preset is a configuration error naming the mount

Non-goals: database (sqlite, sled, fjall) storage models and a production central storage beyond
the interface and the test implementation of AC-12; migrating stored metadata between models
(reading today's sidecars and the legacy folder layout is enough); models for `liquers-web`'s stores
(a later milestone); identity, users and per-key ACL (`CORE-SESSION-AND-KEY-ACL`); conditional
writes and isolation between concurrent writers (`STORE-WRITE-HAS-NO-PRECONDITION`) — the write
strategy is about crashes and failures, not about two writers racing; transactions spanning more
than one key; new data backends.

## Core Interactions

- **Store:** the policy wraps any `AsyncStore` and forwards what it does not refuse. The storage
  model and the handlers form a **metadata layer** between the caller and the backend. **A
  decorator alone cannot make reads non-writing:** `AsyncFileStore::get` and `get_metadata` repair
  missing metadata by calling their own `set_metadata` inside the backend, which a wrapper never
  sees. So the backends need **data-only access** — reading and writing data and listing keys
  without touching metadata — on which the layer keeps metadata through its storage model. Today's
  behaviour becomes the default model (the `{key}.__metadata__` template) and the default handler
  chain (today's `default_metadata` and `finalize_metadata`), which is how AC-6 holds.
  The write strategy sits under the layer: it turns one logical write (data plus its metadata
  record, or a group file) into backend operations — in place, staged then renamed, or staged with a
  journal record that recovery reads.
- **Router:** forwards to the member's layer as today, or, with central storage (AC-12), runs the
  layer itself over data-only members, writing a member's backup copy where one is configured.
  A transaction then spans the central storage and a member; AC-19 covers that case.
- **Configuration:** `StoreConfig` gains `read_only` and its `metadata` section, the router
  configuration the same; the store factories resolve them (`STORE_CONFIG_FSD.md`).
- **Assets:** unaffected, except that versions change when a handler detects outside edits.
- **Web/API:** read-only mounts answer writes with 501; nothing new to wire.
- **Bindings:** `liquers-web`'s stores take the policy and the handlers (backend independent);
  storage models there are a later milestone, not an acceptance criterion here.

## Crate Placement

`liquers-core`: the policy wrapper, the metadata layer, the storage-model and handler abstractions,
the built-in models (per-entry templates, grouped, none, the test central storage) and handlers
(defaults, validation, media type, file facts, content hash, in-memory cache), the write strategies
and recovery, and the
configuration fields, because configuration and construction already live there
(`store_factory.rs`, `store_config.rs`). `liquers-store`: `PathMap` and the OpenDAL store provide
data-only access, consult the model's reserved names, and declare which write guarantees they can
give (OpenDAL's `fs` service already has an `atomic_write_dir` option; object stores write single
objects atomically but have no rename). `liquers-web`: its stores adopt the policy
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
  `CORE-SESSION-AND-KEY-ACL` (excluded); the `Storing` status (`ASSETS.md`), which today is the
  only trace an interrupted write leaves.

## Scope Changes

- **2026-10-15, at the user's request, before approval.** Three requirements made explicit:
  read-only for *any* store, set in its configuration (AC-1 widened); a storage-model interface
  covering per-entry key templates, grouped records (a folder file or a database) and central
  storage on a store or router, with per-entry templates and the grouped `__metadata__.json` built
  (AC-3 rewritten for templates; AC-11, AC-12 added); and a composable handler system with four
  stages — create defaults (stored or not), validate supplied metadata, fill on write, fill on read
  (AC-13 to AC-16 added). "Adapters" for metadata became *storage models* and *handlers*; open
  question 6 folded into question 1. Size stays XL (question 5).
- **2026-10-15, second revision, at the user's request.** A member's own storage under a central
  router is allowed as a backup, not an error (AC-12 rewritten, question 7). A fourth concern added:
  configurable write strategies — direct, atomic per object, transactional with recovery — so that
  data and metadata change together or not at all (AC-17 to AC-20, questions 11 to 13; question 11 decided: the journal, with versioned objects as a
  possible later strategy). Timestamps
  reconsidered (question 8). The user decided to keep everything in one design (question 5).
- **2026-10-15, third revision: the user's answers.** Question 1 decided (C), with data-only access
  as default methods implemented by three backends only. Question 2: a per-adapter configuration
  format rejected; a single stackable, composable mechanism required (AC-21), with two alternatives
  — the command system, or a query compiled to a metadata function — to be compared in Phase 2.
  Question 4: the hash policy is a handler. Question 8: the field is `created`, meaning when the
  data was written; `Partial` and `openbin` raised for discussion. Question 13: defaults come from
  presets (AC-22). Questions 3 and 9 accepted; 10 and 12 explained further.
- **2026-10-15, Phase 1 approved** (`proceed`), with questions 8, 10 and 12 decided as recorded
  below. Data-only access as default `AsyncStore` methods (question 1) stood unopposed at approval.

## Open Questions

Tiers: **[open design]** needs the user's decision; **[proposed]** has a recommendation to accept or
amend.

1. **Decided 2026-10-15 by the user: (C).** The options as presented:
   - *(A) Decorators.* Every concern is a struct that implements `AsyncStore` around an inner one
     (`ReadOnly<S>`, `FrontMatter<S>`, …) and the configuration stacks them. Each is small and
     independent, and it works over any store, the router and third-party stores included. But the
     order of a stack is implicit in every method; a wrapper never sees what the backend does
     inside (the sidecar written on read); a listing pays one `get_asset_info` per entry per layer,
     so a grouped model cannot read its folder file once; and no single layer sees the data and the
     metadata write together, which `transactional` needs.
   - *(B) Pipeline.* One generic store — the metadata layer — built from four parts: a data-only
     backend, a storage model, an ordered list of handlers (trait objects with a default no-op for
     each of the four stages) and a write strategy. The stage order is defined once; handlers stay
     small; the storage model gets whole folders (AC-11); the write strategy sees data and metadata
     as one write (AC-19); the configuration maps onto the parts one to one. The cost is a larger
     new abstraction and data-only access in every backend that takes part.
   - *(C) Both.* *Recommended:* (B) for metadata and writes; (A) for the access policy, a pure filter
     that must also work over the router and over stores without data-only access. With no
     `metadata` section a store is not wrapped in the layer at all, so AC-6 holds without relying on
     the default parts reproducing today's behaviour; a `metadata` section on a backend without
     data-only access is a configuration error (AC-9). A per-backend "do not write on read" flag
     (the former question 6) is rejected: every backend would reimplement it, and it leaves the
     storage model inside each backend.

   The user's concern, recorded with the decision: a new storage abstraction that every store must
   implement is too much. So data-only access is **an extension of `AsyncStore` with default
   methods that refuse** (`NotSupported`), implemented only by the backends that offer the metadata
   pipeline (`AsyncFileStore`, `AsyncMemoryStore`, the OpenDAL store); no other store changes, and a
   store without it can still be read-only, just not take a `metadata` section. Phase 2 confirms that
   this is a handful of methods and not a second store trait.
2. **[open design] Configuration — moved to Phase 2 as alternatives for discussion.** The user
   rejected a configuration format designed per adapter: every new adapter or handler would need its
   own configuration design. Requirements: adapters are **stacked**; metadata components are both
   **stackable** (on write, a modification and a validation pass the record through a chain of
   functions) and **composable**; one generic mechanism configures all of them, built-in and
   registered alike (AC-10, AC-21). Phase 2 presents two alternatives:
   (a) *the command system* — handlers are registered commands (`register_command!`, in a metadata
   namespace) and a chain is a query over them, evaluated through the existing machinery;
   (b) *a query as a function* — a query "compiles" into a function of one or two arguments,
   `MetadataRecord` or `MetadataRecord` and the data, returning `Result<MetadataRecord, Error>`; its
   actions are metadata functions, not commands over states, and the chain is the query's action
   sequence. Phase 2 compares them on extensibility, error positions, async, the cost per read (a
   listing runs the read chain once per entry) and what a configuration file looks like.
3. **Accepted 2026-10-15.** Delivery order. The user decided (2026-10-12, `agent-memory-mvp` decision 2) that
   the read-only policy comes first. *Recommended:* M1 read-only with data-only reads; M2 the
   metadata layer with the default model and chain, then per-entry templates and grouped; M3 the
   handlers (media type, file facts, content hash, validation, front-matter); M4 write strategies
   and recovery; M5 central storage on the router.
4. **Decided 2026-10-15 by the user: the hash policy is configurable, as a metadata handler.**
   `VersionVerification` (`environment_builder.rs`) remains the environment default; a mount that
   configures a content-hash handler overrides it.
5. ~~**Size.**~~ **Decided 2026-10-15 by the user: one design**, delivered in milestones
   (question 3). It stays `XL`, and Phase 1 is over the one-page target because of it.
6. ~~**The non-persisting read.**~~ Folded into question 1 on 2026-10-15: data-only access is the
   non-persisting read.
7. **[proposed] Central storage and member storage together.** Decided by the user: allowed, as a
   backup. *Recommended:* the central record is authoritative; the member copy is written with every
   central write (in the same transaction under `transactional`), is never read while the central
   record exists, and restores a record missing centrally (AC-12). A member copy that disagrees
   with the central one is overwritten, not merged.
8. **Decided 2026-10-15 by the user: `created`, from the backend's time.** `created` means *when
   the current bytes were written*: it differs from `updated`, which `set_updated_now` moves on
   every metadata change (status transitions, progress, `set_metadata`). It is the **backend's**
   modification time where the backend has one — the Liquers clock only for a backend without one,
   such as `AsyncMemoryStore` — written in one standardized form (RFC 3339, UTC, at the backend's
   own precision), so that the value read back compares exactly with the backend's current one and
   the content-hash handler can use it for `on_change` detection (AC-4); an ETag or generation id,
   where present, is kept beside it. A `Partial` checkpoint and the final write each move it;
   under a future streaming `openbin` it is set when the writer is closed.
9. **Accepted 2026-10-15.** Handler failure. A read-stage handler that fails (malformed front-matter):
   *recommended* — the entry stays readable, the failure is a warning in the metadata log;
   validators on write fail the write (AC-14).
10. **Decided 2026-10-15 by the user: grouped storage caches and locks, and sibling writes are
    safe.** The `AsyncStore` API is unchanged, but the contract grows (`STORE_SEMANTICS.md`): a
    write to one key of a group is a locked read-modify-write of the group file, so concurrent
    writes to siblings never lose a record; the group file is cached, keyed by its own version; the
    group file name is reserved in every folder; a record with no data object is a metadata-only key
    and is listed (§8); and because an interrupted group write can damage the whole folder, grouped
    mounts are recommended `atomic` or `transactional`. Safety across processes waits for
    `STORE-WRITE-HAS-NO-PRECONDITION`.
11. **Decided 2026-10-15 by the user: the journal.** `transactional` stages the data and the metadata
    under reserved staging keys, writes a journal record naming them and the targets, moves both into
    place (rename where the backend has one, copy-then-delete where it has not) and deletes the
    journal; recovery rolls a write forward when its journal is complete and back otherwise. The
    write strategy is a pluggable part chosen in the configuration, so a **versioned-object**
    strategy (objects written under new names, a pointer record switched last) can be added later
    without changing the other parts; it is not built in this design.
12. **Decided 2026-10-15 by the user: on read by default, and on sync for a whole store.** An
    interrupted `transactional` write is completed or rolled back when an affected key is read or
    its folder listed, and by a whole-store sync operation; no scan when the store is built (AC-20).
13. **Decided 2026-10-15 by the user: defaults depend on the application, so they come from
    presets.** A preset is a named bundle of policy, storage model, handlers and write strategy that
    a mount selects and overrides field by field (AC-22). *Proposed built-in presets:*
    `default` (today's behaviour: sidecar, today's handlers, `direct`; what a mount without a
    `metadata` section gets, so AC-6 holds), `source` (read-only, no stored metadata, media type,
    file facts, hash on change, front-matter), `generated` (sidecar, `atomic`), `memory` (grouped,
    `transactional`, for `agent-memory-mvp`). An application can define its own presets in its
    configuration, and a crate can register them (AC-10).

## References

Sources `STORE-METADATA-LAYOUT-HARDCODED-PER-STORE`, `STORE-NO-READ-ONLY-ADAPTER`,
`STORE-METADATA-HANDLING-NOT-COMPOSABLE`; `design/agent-memory-mvp/tiers-and-storage.md` §2 C (the
front-matter adapter and the forwarding problem); `design/sidecar-colliding-keys/` (reserved names);
`liquers-core/src/store.rs` (`METADATA_SUFFIX`, `METADATA_FOLDER`, `AsyncStore::default_metadata`,
`finalize_metadata`).
