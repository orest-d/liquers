# Phase 1: High-Level Design — A record source's rows served as a byte stream

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — adds a `pub` trait and a defaulted `pub` method on
  `Environment` (auto-fix rules 3, 4), spans `liquers-core`, `liquers-records`, `liquers-lib` and
  `liquers-axum` (rule 6), complexity `L` (rule 1).
- **Leading issue:** **Open design - how the environment reaches the encoder** (Q1).
- **Explanation:** every piece the stream needs exists at HEAD (`RecordSource::stream`,
  `EnvResolver`, the CSV/NDJSON writers, `WriteOptions::header`); what is new is the seam through
  which a generic `liquers-core` read hands an `EnvRef` to `liquers-lib`'s value. The recommended
  seam is a registered encoder, because the issue's recommended trait method cannot build an
  `EnvResolver` inside `CombinedValue`'s generic impl (Phase 2). Phases 3-4 assume the recommended
  answers and the parent design's interfaces.
- **Open questions:**
  1. **Open design - environment seam:** (a) a `StreamEncoder<E>` trait object registered in
     `CommandRegistry<E>` by `register_records_commands!`, consulted by a new streaming read,
     **recommended** — no application wiring, the encoder is built where `E::Value` is concrete;
     (b) the issue's `serialize_stream_in<E: Environment<Value = Self>>` on the value trait —
     works only after `RecordValue` and `ExtValueInterface` are generalized from `Value` to
     `CombinedValue<B, ExtValue>`; (c) an `EnvironmentBuilder::with_stream_encoder` call — explicit,
     but every server must remember it.
  2. **Open design - which URL streams:** the source itself with a row filename
     (`…/ns-rec/to_record_source/daily.csv`), **recommended**; `ns-rec/materialize` keeps its
     meaning (build one table in memory, bounded by `max_rows`) and stays whole. Alternative:
     make `materialize/daily.csv` stream too — it changes what `materialize` means and drops its
     `max_rows` guard. The issue's example therefore changes spelling (issue tightened).
  3. **Open design - row formats versus manifest formats:** a source serializes as its manifest in
     `yaml` and `json` today. Recommended: rows only in `csv`, `csv:comma`, `tsv`, `csv:tab`,
     `ndjson`, `jsonl`; `json` stays the manifest (compatible), and the JSON-array row form is not
     offered for a source.
  4. **Open design - whole-byte callers:** the `entry` routes, `get_binary`, `liquers-web` and
     `liquers-py` read whole bytes. Recommended: they keep today's refusal (pointing at
     `ns-rec/materialize`), so no unbounded table is assembled behind a caller that did not ask for
     a stream.
  5. **Proposed resolution - NDJSON error line:** on a mid-stream error the NDJSON body ends with
     one line `{"$liquers_error": {"error_type": "…", "message": "…"}}` and then aborts. The
     `$`-prefixed key cannot collide with a field name a CSV header or `RecordSchema` produced from
     an identifier. (`record-streams` sketched `{"error": …}`, which a record could contain.)
  6. **Proposed resolution - warnings mid-stream:** logged to stderr with the query; not in-band.

## Purpose

A record source is lazy: its rows exist only while a stream is open. Today it reaches bytes only
through `ns-rec/materialize`, which builds the whole table in memory and refuses past `max_rows`.
Let a server send a source's rows as CSV, TSV or NDJSON chunk by chunk, as they are produced, so a
large export is bounded by one chunk rather than by the table. Serves VISION's data-service shape
and priority area 2 (records).

## Problem Example

```text
GET /liquer/q/-R/data/sales/daily.manifest.yaml/-/ns-rec/materialize/daily.csv
```

Today (HEAD cd6d5da): `ns-rec/materialize` drains every chunk into one `RecordBatch`
(`RecordSource::materialize`, refused past 1 000 000 rows), `write_table` encodes it whole, and the
response is sent after both — about three times the data resident at once
(`RECORD_STREAM_GUIDE.md` §4). Asking for the source itself as CSV,
`…/ns-rec/to_record_source/daily.csv`, fails: `ExtValue::as_bytes` accepts only the manifest formats
`yaml` and `json` (`liquers-lib/src/value/mod.rs`, "unsupported manifest format 'csv'").

Should be: `GET /liquer/q/-R/data/sales/daily.manifest.yaml/-/ns-rec/to_record_source/daily.csv`
answers `200 text/csv` after the first chunk is encoded, with the header once and every chunk's rows
following as each chunk is evaluated; no step holds more than one chunk.

## Scope and Acceptance Criteria

- **AC-1** CSV streams chunk by chunk
  WHEN a source of three chunks with a declared uniform schema is served as `daily.csv`
  THEN the body is the header followed by every row, equal to `materialize` then `as_bytes("csv")`,
  and it arrives in at least three body chunks, the first before the third chunk is evaluated
- **AC-2** NDJSON needs no uniform schema
  WHEN a source whose chunks have different schemas is served as `daily.ndjson`
  THEN every row is one JSON object line, each with its own chunk's fields
- **AC-3** CSV refuses a non-uniform source before any byte
  WHEN a source without a declared schema (`RecordSource::schema()` is `None` on the stream) is
  served as CSV or TSV
  THEN the response is the error answer, with a message naming `ndjson` and `uniform_schema`
- **AC-4** A failing first chunk is a status
  WHEN the first chunk's query fails
  THEN the response is that error's answer, as a failed `get_binary` is
- **AC-5** A later failing chunk
  WHEN chunk 2 of 3 fails
  THEN a CSV body is aborted (incomplete transfer); an NDJSON body ends with one
  `{"$liquers_error": …}` line and is then aborted; both log the error to stderr
- **AC-6** Manifest formats and other readers are unchanged
  WHEN a source is serialized as `yaml` or `json`, or any value is read through `get_binary`,
  `entry` routes, `liquers-web` or `liquers-py`
  THEN the bytes or errors are exactly those of HEAD
- **AC-7** Nothing is cached or stored
  WHEN a source is served as CSV twice
  THEN chunks are encoded twice, the asset holds no cached binary, and the store is not written
  (the chunks' own assets are cached as usual)
- **AC-8** Feature boundaries
  WHEN `records` is off, or the build is wasm32
  THEN the build succeeds, and a source cannot exist (no `records`) or is served whole as today
  (wasm: `liquers-web` does not use the streaming read)

Non-goals: Arrow IPC or Parquet streaming (needs a streaming writer per format); a streamed
`ns-rec/materialize`; streamed persistence (`STREAMED-VALUE-IS-COLLECTED-BEFORE-IT-IS-STORED`);
HTTP trailers; content negotiation.

## Core Interactions

- Value types / serialization: a registered encoder beside `serialize_stream`.
- Assets: a streaming read beside the parent's `get_serialized`.
- Records: a batch-by-batch encoder over `BoxRecordStream`.
- Web/API: the Query API and Assets API `data` routes call the streaming read.

## Crate Placement

`liquers-core`: the encoder trait, its slot in `CommandRegistry`, `Environment::stream_encoders`,
`AssetRef::get_streamed` (generic, no value types). `liquers-records`: `encode_record_stream`
(core-only, wasm-clean, no new dependency). `liquers-lib` (feature `records`): the encoder for
`ExtValue::RecordSource`, registered by `register_records_commands!`. `liquers-axum`: one call
changed in each data route.

## Documentation Intent

- Reference: `RECORD_STREAMS.md` (a source's byte forms: manifest, or streamed rows over HTTP),
  `VALUE_TYPE_SYSTEM.md` (stream encoders), `WEB_API_SPECIFICATION.md` §2.3 (which reads stream;
  the NDJSON error line).
- Guide: `RECORD_STREAM_GUIDE.md` §3.4 (serving a manifest as CSV/NDJSON) and §7 (pitfalls: CSV
  needs `uniform_schema`; truncated CSV is undetectable); `SERIALIZATION_GUIDE.md` (encoders).
- Other documents: `record-streams` Phase 2 is frozen (complete); no edit.

## Design Dependencies

- `value-serialization-streaming` — **requires**: `SerializedBytes`, `AssetRef::get_serialized`,
  `first_chunk`/`respond`, `ByteStream`. This design's Q1 answers that design's deferred Q1.
- `store-streaming-read-write` — **requires** (transitively, through the parent): `BoxStream`,
  `bytes`, `StreamResponse`.
- `record-streams` (complete) — origin; its HTTP section's behaviour is implemented here.
- `external-index-reconciliation` — **overlaps** (weak): both open `RecordSource::stream` with an
  `EnvResolver` outside a command; no shared contract.
- `STREAMED-VALUE-IS-COLLECTED-BEFORE-IT-IS-STORED` (issue, no design) — **overlaps** (weak):
  persistence of streams; sources are never persisted as rows.

## Consolidated Findings

- **Seam:** the parent's deferred `serialize_stream_in` cannot be overridden usefully — inside
  `impl ValueInterface for CombinedValue<B, X>` the bound `CombinedValue<B, X>: RecordValue` that
  `EnvResolver<E>` needs is unprovable (`RecordValue` is implemented for the concrete `Value`
  only). A registered encoder is built where `E` is concrete, as commands are; `CommandRegistry`
  already carries non-command data (`store_config`), so it is the slot that needs no new wiring.
- **URL:** streaming is reached by serving the *source* with a row filename; `materialize` stays a
  bounded whole build. `json` stays the manifest.
- **Whole readers keep refusing** a source in row formats, so nothing collects an unbounded stream.
- **Ordering:** implement after the parent's Steps 1-5; this design's core step is small and
  additive.
- **Error contract:** pull the first batch before headers; CSV checks the declared schema there;
  NDJSON carries one `$liquers_error` line on a later failure, then aborts.
- **Issue tightened:** its example now names the streaming spelling; its recommended mechanism is
  recorded as rejected alternative (b) with the reason.
