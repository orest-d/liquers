---
id: STORE-METADATA-HANDLING-NOT-COMPOSABLE
kind: feature
title: Store metadata handling is not composable or configurable
status: closed
priority: P2
complexity: L
area: [core/store, store/config, store/backends]
design: store-adapters
created: 2026-10-12
github:
---
## Problem

What a store does with metadata is several independent decisions, and today each store makes all of
them in code, together:

| Stage | Question | Today |
|---|---|---|
| **Create** | What metadata does an entry that has none get? | `default_metadata` per store; `get` synthesizes and (in `AsyncFileStore`) **writes** it on first read |
| **Enhance** | What is derived from the data and added to whatever metadata exists — content hash / `version`, media type, size, title and description from Markdown front-matter, a first heading? | Hard-coded in `finalize_metadata*`; the version is re-hashed only when bytes are read (`VersionVerification::OnRead`); front-matter is not read at all |
| **Store** | Where and whether metadata is persisted — a `.__metadata__` sidecar, a `__metadata__` folder, a namespace, a database column, nowhere | Hard-coded per store (`STORE-METADATA-LAYOUT-HARDCODED-PER-STORE`) |
| **Policy** | May reads write? May the store be written at all? | No read-only mount (`STORE-NO-READ-ONLY-ADAPTER`); reading a file store writes a sidecar |
| **Configure** | How a deployment chooses the above per mount | Not expressible in `StoreRouterConfig` / the store factories |

So a deployment cannot say "mount `./specs` read-only, derive metadata from the files and their
front-matter, cache it in memory, write nothing", or "mount `./notes` writable, take title and
description from front-matter and keep the front-matter in step when metadata is set", without a new
store type per combination.

## Impact

`design/agent-memory-mvp/` needs exactly those two mounts (its Phase 1 decision 4: a front-matter
store adapter that rewrites front-matter on a metadata write; decision 2: the read-only adapter
first). Each would otherwise be one more bespoke wrapper forwarding the whole `AsyncStore` trait —
which `tiers-and-storage.md` §2 C found is most of the trait, and which the router bypasses for
`get_asset_info` (`STORE-ROUTER-DOES-NOT-FORWARD-ASSET-INFO`). The next need (a hash policy for
large remote objects, metadata in a database) would add another. Workaround: one hand-written
wrapper per combination.

## Expected behaviour

A composable metadata pipeline, configured per mount: an ordered set of **metadata handlers** — a
creator for missing metadata, enhancers that derive fields from the data (content hash, media type,
front-matter projection with optional write-back), a storage layout (sidecar, folder, namespace,
none, in-memory cache), and an access policy (read-only, no write-on-read) — expressed as adapters
around any backend, with a generic forwarding base so an adapter overrides only what it changes.
`StoreConfig` gains a per-mount section naming the handlers and their options, resolved by the
store factories. The conformance suite runs over adapted stores as well as bare ones.

Relationship to existing records: generalizes `STORE-METADATA-LAYOUT-HARDCODED-PER-STORE` (the
*store* stage), `STORE-NO-READ-ONLY-ADAPTER` (one *policy*, to be fixed first and then expressed in
this model) and the front-matter store of `agent-memory-mvp` (one *enhancer*); touches
`STORE-VERSION-BLIND-TO-CHANGES-MADE-OUTSIDE-LIQUERS` (when the hash is recomputed) and
`CORE-METADATA-NO-APPLICATION-ATTRIBUTES` (where projected fields beyond title/description go).
Needs a design folder.

## Discovery

Raised by the user on 2026-10-12 while answering `agent-memory-mvp` Phase 1 open questions 2 and 4:
the read-only adapter and the front-matter adapter are two instances of one modular metadata
mechanism, and how all of it is configured should be designed once.

## Resolution (2026-10-15)

Fixed by design `store-adapters` (PR #137). A mount's `metadata` section names a storage model, a write strategy (`direct`, `atomic`, `transactional`) and handler chains per stage (`on_default`, `on_validate`, `on_write`, `on_read`), written as queries in the realm `store_config` and compiled at build into closures run by `MetadataPipelineStore` (`liquers-core/src/store/pipeline.rs`). Built-in handlers live in `liquers-core/src/store/realm.rs`; the front-matter projection and write-back (`front_matter`, `front_matter_sync`) live in `liquers-lib/src/store_config.rs` behind `records`. Read-only is the access policy (`STORE-NO-READ-ONLY-ADAPTER`). Conformance suites `a1`-`a7` run over adapted stores. The handler mechanism is described in `specs/guides/STORE_CONFIG_GUIDE.md` and the realm in `specs/reference/REALMS.md`. Not done here and filed separately: handlers defined in another language (`LANGUAGE-STORE-CONFIG-FUNCTION-NOT-DEFINABLE`).
