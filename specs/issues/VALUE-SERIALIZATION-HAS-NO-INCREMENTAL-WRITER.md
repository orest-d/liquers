---
id: VALUE-SERIALIZATION-HAS-NO-INCREMENTAL-WRITER
kind: feature
title: Value serialization has no incremental writer
status: draft
priority: P2
complexity: M
area: [core/value]
design: record-streams
created: 2026-09-17
github:
---
## Problem

`DefaultValueSerializer` has one serialization method and it returns the whole encoding in memory:

```rust
fn as_bytes(&self, data_format: &str) -> Result<Vec<u8>, Error>;
```

(`liquers-core/src/value.rs:926`.) There is no writer- or sink-based counterpart, so encoding a
value always allocates the complete output before anything can be written, sent or stored. The
deserialization side is symmetric: `deserialize_from_bytes(b: &[u8], …)` requires the entire input
in memory first.

The capability exists one layer down and is not exposed. `liquers-lib`'s polars module has
`serialize_dataframe_to_writer<W: Write>(df, data_format, writer)`
(`liquers-lib/src/polars/serde.rs:94`), which writes CSV, parquet and NDJSON incrementally — and
`liquers-lib/src/value/mod.rs:302` calls it with a `Vec<u8>` as the writer, immediately discarding
the benefit to satisfy the `as_bytes` signature.

## Impact

Peak memory for any serialization is the value plus its full encoding. For a large DataFrame written
to CSV that is roughly twice the data, at the moment of writing, for no reason other than the
signature.

It also forecloses three things that are otherwise natural:

- **Streaming an HTTP response.** A large export is buffered whole before the first byte is sent.
- **Writing directly to a store.** The persistence path could hand a store's writer to the
  serializer instead of building a `Vec` and passing it on.
- **Emitting a record stream as a table.** `design/store-and-asset-search/record-model.md` §6 makes
  a record stream serializable as NDJSON, CSV or parquet. NDJSON in particular is line-per-record
  and needs no buffering at all — but through `as_bytes` the whole stream materializes, which
  defeats the batching that model introduces specifically to bound memory.

It is P2 rather than higher because every current use is of a size where it does not matter, and the
workaround — accept the allocation — is always available.

## Expected behaviour

A writer-based serialization path beside the existing one, with `as_bytes` becoming a thin
convenience over it rather than the only way in. Roughly:

```rust
fn write_to<W: std::io::Write>(&self, data_format: &str, writer: W) -> Result<(), Error>;
```

Questions the design must answer:

- **Async.** Stores and HTTP responses are async, and `std::io::Write` is not. Either an async
  writer trait, or a sync writer plus an adapter, or both. `liquers-web` is wasm32-only, which
  constrains the choice.
- **Object safety.** `DefaultValueSerializer` is used behind trait objects in places; a generic
  method is not object-safe, so the writer may need to be `&mut dyn Write`.
- **Whether deserialization gets the same treatment.** A reader-based path pairs naturally with
  `openbin` (`CORE-STORE-OPENBIN-MISSING`), and the two are most valuable together: reading a large
  file incrementally is pointless if it must be encoded whole afterwards.
- **A default implementation** that routes through `as_bytes`, so no existing value type has to
  change.

## Discovery

Found while designing the record model for `store-and-asset-search`, 2026-09-17, checking whether a
record stream could be emitted as a table without materializing it. Verified at HEAD: `as_bytes` is
the only serialization method on the trait (`value.rs:926`), and the writer-based DataFrame path at
`polars/serde.rs:94` is called with an in-memory `Vec<u8>` at `value/mod.rs:302`.

## Update 2026-09-20 — the shape may be wrong, and the motivating case is HTTP

`record-streams` Phase 2 revision 7 designs streaming a `RecordSource` to CSV or NDJSON over HTTP
(`liquers-axum`), which is this issue's clearest motivating case: a multi-gigabyte export must not
build the whole document in memory, and `as_bytes` guarantees that it does.

**A finding that changes what this issue should ask for.** The problem statement above proposes a
*writer*-based counterpart modelled on `serialize_dataframe_to_writer<W: Write>`. That is **push**-
based — the serializer drives and writes when it chooses. An HTTP body is **pull**-based: hyper polls
the body and the producer must yield a chunk per poll. Bridging push to pull requires a bounded
channel or a duplex pipe plus a task to run the writer, which is real machinery rather than an
adapter, and it reintroduces a buffer whose size has to be chosen.

So a writer-based API alone would **not** straightforwardly serve the HTTP case. The design should
consider:

```rust
/// Pull-based: the consumer drives. Serves an HTTP body directly.
fn serialize_to_stream(&self, data_format: &str) -> Result<BoxStream<'_, Result<Bytes, Error>>, Error>;

/// Push-based: convenient for files and for anything already `Write`-shaped.
fn serialize_to_writer<W: Write>(&self, data_format: &str, w: &mut W) -> Result<(), Error>;
```

with the **writer form built on the stream form** rather than the reverse, since stream → writer is a
loop and writer → stream is a channel.

Meanwhile `liquers-axum` takes an ad-hoc path for record sources only, confined to that crate, which
a general mechanism can replace without changing any URL or caller.

Two constraints that any general mechanism inherits, discovered in the same work and worth stating
here so they are not rediscovered:

- **A mid-stream error cannot change an HTTP status that has already been sent.** NDJSON can carry an
  error in band; CSV cannot, and truncates indistinguishably from success. Pulling the first chunk
  before sending headers converts most failures into a proper status and is the highest-value
  mitigation.
- **Streaming bypasses result caching**, since nothing materializes. Acceptable where the *input*
  value is itself cacheable — as a `RecordSource` is — so only the encoding repeats.
