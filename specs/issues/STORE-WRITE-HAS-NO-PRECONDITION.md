---
id: STORE-WRITE-HAS-NO-PRECONDITION
kind: issue
title: A store write has no precondition, so concurrent writers clobber silently
status: draft
priority: P2
complexity: M
area: [core/store]
design: 
created: 2026-09-15
github:
---
## Problem

`AsyncStore::set(&self, key, data, metadata)` takes no precondition. There is no way to say "write
this only if the current version is still X", and no way to learn afterwards that someone else's
write was overwritten. Two writers that read the same key, modify it and write it back both
succeed, and the second silently discards the first's change.

`MetadataRecord::version` already holds a content hash of the value at save time, so the
information a precondition needs is present in the record — it is simply never consulted on the
write path. `AsyncFileStore` takes a file lock around its own write (`acquire_lock`,
`liquers-core/src/store.rs:1047`), which makes a single write atomic on that backend; it does not
make a read-modify-write sequence safe, and it is a private implementation detail of one store
rather than a contract.

`STORE_SEMANTICS.md` says nothing about concurrent writers. The only concurrency statement in it
is that `removedir` is not atomic.

## Impact

Any deployment with more than one writer can lose data, without an error and without a trace. That
covers `liquers-axum` serving several clients, two agents writing to the same store, and a single
process whose asset persistence races an API upload.

It has not bitten the project yet because the current writers are effectively serialized, which is
why this is filed at P2. The exposure grows with every multi-writer use;
`AGENT-MEMORY-SERVICE` is one, and `CORE-SESSION-AND-KEY-ACL` describes the authorization half of
the same multi-writer picture.

## Expected behaviour

The store contract states what happens when two writes race, and offers at least one way to make a
read-modify-write safe. The plausible minimum is a conditional write — `set_if_version(key, data,
metadata, expected: Option<Version>)`, returning a distinguishable "precondition failed" error —
defaulting to unconditional `set` on backends that cannot support it, declared through
`StoreCapabilities`, and checked by the conformance suite.

Whatever is chosen, `STORE_SEMANTICS.md` gains a concurrency section: last-writer-wins is an
acceptable answer, but it has to be a written one.

## Discovery

Analysis for `AGENT-MEMORY-SERVICE`, 2026-09-15. Verified at HEAD: the `AsyncStore` trait has no
conditional write, and `STORE_SEMANTICS.md` has no concurrency section.
