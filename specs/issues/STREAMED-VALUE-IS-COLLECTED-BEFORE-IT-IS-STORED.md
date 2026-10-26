---
id: STREAMED-VALUE-IS-COLLECTED-BEFORE-IT-IS-STORED
kind: issue
title: Persisting a keyed asset serializes its value whole, even when the value can stream and the store can take a stream
status: draft
priority: P3
complexity: L
area: [core/assets, core/store]
design: streamed-keyed-persistence
created: 2026-10-21
github:
---
## Problem

A keyed, non-volatile asset is serialized whole at finalization, because its content version is
`Version::from_content(&bytes)` over the whole encoding (`AssetRef::prepare_version`,
`liquers-core/src/assets.rs`), and the bytes are then written with `AsyncStore::set`
(`save_to_store`). Once `design/value-serialization-streaming/` lands a value can produce its
encoding as a stream, and once `design/store-streaming-read-write/` lands a store can take one
(`set_stream`); neither exists at HEAD (cd6d5da). Even then the persistence path would still collect: the version must be known before the asset becomes readable,
and the write must not expose a partial object.

## Problem Example

A keyed recipe `exports/sales.csv` evaluating to a 2 GB DataFrame: today the full CSV is built in
memory to hash it and is kept as the cached binary until written (and, without a disposal policy,
afterwards). Should be: the encoding is hashed incrementally (blake3 is incremental) while it is
streamed to `set_stream`, and the version and final metadata are installed when the stream ends.

## Impact

Peak memory of keyed persistence is the value plus its whole encoding. Unkeyed and volatile
results are not affected.

## Expected behaviour

Options, not chosen here: incremental hashing during a streamed `set_stream` with the asset in
`Storing` until it completes (changes when the version becomes visible relative to readability —
see the single-transaction rule in `prepare_version`'s doc); or a non-content version for streamed
values; or keep whole serialization for keyed assets and document it. Interacts with
`SERIALIZED-BINARY-RETAINED-WITH-NO-DISPOSAL-POLICY`.

## Discovery

Scoped out of `design/value-serialization-streaming/` (Phase 1 Q5) during the 2026-10-20 bulk
design, after reading `prepare_version` and `finalize_status_with_version` at HEAD 18732dd.

Designed 2026-10-26 in `design/streamed-keyed-persistence/` (bulk design; recommends the first
option: the asset becomes readable when the streamed write and its hash complete).
