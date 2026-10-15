---
id: STORE-NO-READ-ONLY-ADAPTER
kind: feature
title: No read-only store adapter, so reading a file store writes to it
status: closed
priority: P2
complexity: M
area: [core/store]
design: store-adapters
created: 2026-09-16
github:
---
## Problem

Two separate things make "mount this directory read-only" impossible to express.

**1. Reading writes.** `AsyncFileStore::get` fetches the bytes, then tries `get_metadata`. When
that fails — which is exactly the case for a file with no `.__metadata__` sidecar — it builds
default metadata, logs two warnings into it, and calls `self.set_metadata(key, &metadata).await?`
before returning. So the first `get` of any file that was not written through Liquers **creates a
sidecar next to it**.

That is defensible for a store that owns its directory: metadata is repaired on demand and the
repair is recorded. It is wrong for a directory Liquers does not own. Mounting a source tree and
reading one file dirties the working copy; over a git checkout, `git status` fills with
`__metadata__` files nobody asked for.

**2. Writing is not gated.** `StoreApiBuilder` wires `PUT`, `DELETE`, `makedir` and `removedir`
for every mounted key. There is no per-mount capability flag and no wrapper that refuses writes, so
a store router mount is writable by anyone who can reach the API, regardless of what the mount is
for. `CORE-SESSION-AND-KEY-ACL` covers *who* may write; this is the simpler question of whether a
given mount accepts writes **at all**, which does not need an identity model to answer.

## Impact

Any use of Liquers to serve a corpus it does not own — documentation, a source checkout, a
reference dataset, anything version-controlled elsewhere — either mutates that corpus or exposes
it to mutation, and today it does both. There is no configuration that prevents it and no way to
declare the intent.

Surfaced on PR #70: `AGENT-MEMORY-SERVICE` mounts `specs/` and states "no `specs/` write path" as
an explicit non-goal, which a raw `AsyncFileStore` behind `StoreApiBuilder` cannot deliver.

## Expected behaviour

A `ReadOnlyStore<S>` wrapper, or an equivalent capability flag, that:

- returns the synthesized metadata from `get` **without persisting it** — the repair is still
  useful in memory, it just must not reach the disk;
- refuses `set`, `set_metadata`, `remove`, `removedir` and `makedir` with a distinguishable error
  (`ErrorType::NotSupported`, which `api_core/error.rs` already maps to 501);
- reports the refusal through `StoreCapabilities` so the conformance suite checks it rather than
  taking it on trust;
- is honoured at the API boundary, so a write to a read-only mount is refused with a documented
  status rather than reaching the store and failing there.

Open question worth settling first: whether read-only is a wrapper, a flag on the existing stores,
or a property of the router mount. The wrapper composes with any `AsyncStore` and is the smallest
change; a mount property is the one a configuration file can express, which is what a deployment
actually wants.

## Discovery

Reported by Codex review on PR #70, 2026-09-15, as a P1 against `design/agent-memory-mvp/`.
Verified at HEAD on 2026-09-16 by reading `AsyncFileStore::get` in `liquers-core/src/store.rs` and
the routes in `liquers-axum/src/store/builder.rs`. The sidecar-on-read behaviour is deliberate and
documented in its own warnings; what is missing is the ability to decline it.

## Resolution (2026-10-15)

Fixed by design `store-adapters` (PR #137), as both a wrapper and a mount property. `ReadOnlyStore` (`liquers-core/src/store/read_only.rs`, commit 5df8b02) refuses `set`, `set_metadata`, `remove`, `removedir`, `makedir` and the data-only writes with `ErrorType::NotSupported`, keeps synthesized metadata in memory and reports `is_read_only()`. A configuration mount sets it with `read_only: true` (commit dd34bb3); a pipeline under it never persists records. The conformance suite checks it (`a1_read_only_file_store`, `a3_opendal_fs_per_entry`), the API boundary returns 501 (`liquers-axum/tests/store_api_routes.rs` `read_only_store_write_is_501`), and the browser integration exposes `isReadOnly` (`store14_read_only_js_store_refuses_writes`). Other tests: `read_only_file_store_creates_nothing`, `read_only_mount_from_config`, `read_only_pipeline_never_persists`.
