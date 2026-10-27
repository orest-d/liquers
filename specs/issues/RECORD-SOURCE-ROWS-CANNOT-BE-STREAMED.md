---
id: RECORD-SOURCE-ROWS-CANNOT-BE-STREAMED
kind: feature
title: A record source's rows cannot be served as a byte stream; only its manifest or a materialized table
status: draft
priority: P3
complexity: L
area: [core/value, lib/value, records, axum]
design: record-source-row-streaming
created: 2026-10-21
github:
---
## Problem

A `RecordSource` (`ExtValue::RecordSource`, `liquers-lib/src/value/mod.rs`) is lazy: its rows
exist only when a stream is opened and chunk queries are evaluated through a `ChunkResolver`
(`liquers-records/src/value.rs`). Its byte form is its manifest; its rows reach bytes only through
`ns-rec/materialize`, which builds the whole table in memory (capped by `max_rows`) and cannot
write a non-uniform source as NDJSON. `design/value-serialization-streaming/` gives values a pull
stream (`serialize_stream`), but its signature carries no environment, and a source needs one to
build an `EnvResolver`.

## Problem Example

```text
GET /liquer/q/-R/data/sales/daily.manifest.yaml/-/ns-rec/materialize/daily.csv
```

Today (HEAD cd6d5da): the whole table is materialized (about three times the data resident at
serving time, refused past `max_rows`), then serialized whole. The lazy spelling,
`…/ns-rec/to_record_source/daily.csv`, is refused: a source's only byte forms are its manifest's
`yaml` and `json` (`ExtValue::as_bytes`). Should be: the source served with a row filename
(`…/ns-rec/to_record_source/daily.csv` or `.ndjson`) is encoded chunk by chunk as CSV (uniform
schema checked before the header) or NDJSON (an in-band error line on failure) and sent as it is
produced; `materialize` keeps its bounded whole-table meaning (design Q2).

## Impact

Large record exports are bounded by memory and `max_rows`; no workaround beyond splitting the
query.

## Expected behaviour

An environment-aware serialization entry, recommended in `value-serialization-streaming` Phase 1
Q1: a defaulted `serialize_stream_in<E: Environment<Value = Self>>(self: Arc<Self>, fmt,
envref: &EnvRef<E>)` that defaults to `serialize_stream`; `CombinedValue` forwards through
`ExtValueInterface`; `ExtValue::RecordSource` builds an `EnvResolver` and encodes batches. The
2026-10-26 design found that override unworkable as stated — `CombinedValue<B, X>` implements the
value trait generically and cannot prove `RecordValue`, which `EnvResolver` needs — and recommends
an encoder registered with the `ns-rec` commands instead. Alternatives (a type-erased capability passed to
`serialize_stream`, a resolver captured in the value) are discussed there. Mid-stream warnings and
NDJSON error lines belong here too.

## Discovery

Scoped out of the first slice of `VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE`
(`design/value-serialization-streaming/`) during the 2026-10-20 bulk design; the need was first
recorded in `design/record-streams/` Phase 2 §"Streaming a record source over HTTP".
