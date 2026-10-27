# Phase 2: Solution & Architecture — A record source's rows served as a byte stream

## Overview

`liquers-core` gains a `StreamEncoder<E>` trait object list, held by `CommandRegistry<E>` (as
`store_config` already is) and exposed through a defaulted `Environment::stream_encoders`. A new
`AssetRef::get_streamed` — the parent's `get_serialized` gate and wait, then the first encoder that
claims the value and format, else `get_serialized` — is what the axum data routes call.
`liquers-lib`'s `register_records_commands!` registers a `RecordSourceEncoder`, which for an
`ExtValue::RecordSource` in a row format returns `SerializedBytes::Stream` whose first poll opens
`source.stream(EnvResolver)`; `liquers-records`' new `encode_record_stream` turns the record stream
into byte chunks, checking CSV uniformity before the first byte and writing the NDJSON error line.

Rejected:
- **`serialize_stream_in<E: Environment<Value = Self>>` on the value trait** (the issue's
  recommendation, parent Q1). `CombinedValue<B, X>` implements `ValueInterface` generically; its
  override must build `EnvResolver<E>`, which requires `E::Value: RecordValue`, i.e.
  `CombinedValue<B, X>: RecordValue` — implemented only for the concrete `Value`
  (`liquers-lib/src/records/mod.rs`). It becomes feasible only by generalizing `RecordValue` and
  `ExtValueInterface` over `B`, a refactor this feature does not need.
- **A resolver captured in the value** — a manifest read from a store has none, and an `EnvRef`
  inside a cached value is a reference cycle (parent Q1).
- **`EnvironmentBuilder::with_stream_encoder`** — workable, but every server must remember the
  call; the records registration is already where the records crate plugs into an environment.
- **Branching on `ExtValue` in `liquers-axum`** — it depends on core only and cannot name it.
- **Streaming `ns-rec/materialize`** — changes the command's meaning (Phase 1 Q2).

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE` (`value-serialization-streaming`) | draft | P2 | provides `SerializedBytes`, `get_serialized`, serving functions | yes | yes, as an ordering | `requires` |
| `CORE-STORE-OPENBIN-MISSING` (`store-streaming-read-write`) | accepted | P3 | `BoxStream`, `ByteStream`, `bytes` (through the parent) | through the parent | no | none |
| `WEB-RECORD-SOURCE-RESOLVES-THROUGH-SINGLETON` | closed | — | `EnvResolver` takes an explicit `EnvRef` | — | — | none |
| `STREAMED-VALUE-IS-COLLECTED-BEFORE-IT-IS-STORED` | draft | P3 | none: sources never persist as rows | no | no | none |

## Interfaces

```rust
// liquers-core/src/value.rs (beside SerializedBytes, from the parent)
/// Encodes a value the generic serializer cannot, using the environment. Registered with the
/// commands; consulted only by streaming reads (`AssetRef::get_streamed`).
pub trait StreamEncoder<E: Environment>: MaybeSend + MaybeSync + 'static {
    /// `None`: not this encoder's value or format — the caller falls back. `Some(Err)`: refused
    /// before any byte. Synchronous; the stream does its async work when polled.
    fn encode(
        &self,
        value: Arc<E::Value>,
        data_format: &str,
        envref: EnvRef<E>,
    ) -> Option<Result<SerializedBytes, Error>>;
}

// liquers-core/src/commands.rs
pub struct CommandRegistry<E: Environment> {
    /* …existing fields… */
    /// Encoders consulted by streaming reads, in registration order. Data only, like `store_config`.
    pub stream_encoders: Vec<Arc<dyn StreamEncoder<E>>>,
}

// liquers-core/src/context.rs — defaulted, so no implementer breaks
fn stream_encoders(&self) -> &[Arc<dyn StreamEncoder<Self>>] { &[] }
// GenericEnvironment overrides: &self.command_registry.stream_encoders

// liquers-core/src/assets.rs
impl<E: Environment> AssetRef<E> {
    /// `get_serialized`'s gate and wait; then the first encoder returning `Some`, never cached;
    /// otherwise exactly `get_serialized`.
    pub async fn get_streamed(&self) -> Result<(SerializedBytes, Arc<Metadata>), Error>;
}

// liquers-records/src/formats/stream.rs (new; re-exported from formats)
/// Encodes a traversal batch by batch. CSV/TSV: refuses (first item) when `stream.schema()` is
/// `None`; header once. NDJSON: one line per row; on a batch error yields the
/// `{"$liquers_error": …}` line, then the error. Other formats: `Err` (not streamable).
pub fn encode_record_stream(
    stream: BoxRecordStream,
    format: TableFormat,
) -> BoxStream<'static, Result<Vec<u8>, Error>>;
pub fn is_streamable(format: TableFormat) -> bool;   // Csv{..} | NdJson

// liquers-lib/src/records/stream.rs (new, feature `records`)
pub struct RecordSourceEncoder;
impl<E: Environment> StreamEncoder<E> for RecordSourceEncoder where E::Value: RecordValue { … }
```

`RecordSourceEncoder::encode`: `value.as_record_source()` is `None` → `None`; the format is a
manifest format (`yaml`, `json`) or not streamable → `None` (the generic path answers, AC-6);
otherwise `Some(Ok(SerializedBytes::Stream { chunks, length: None }))`, where `chunks` is
`futures::stream::once(source.stream(Arc::new(EnvResolver::new(envref))))` flattened into
`encode_record_stream(…)` and mapped `Vec<u8>` → `Bytes`. Opening the stream is therefore inside
the first poll, which the parent's `first_chunk` performs before headers (AC-3, AC-4).

`BoxStream` is the parent's `maybe_send::BoxStream` (`Send` on native only), so the wasm build
compiles; `liquers-web` never calls `get_streamed` (AC-8). `liquers-records` already depends on
`futures`; it gains no dependency (chunks are `Vec<u8>`; `liquers-lib` already has `bytes`).

## Integration Points

- `liquers-core/src/value.rs`: `StreamEncoder`. `commands.rs`: the field, initialized empty in
  `CommandRegistry::new`. `context.rs`: `Environment::stream_encoders` (defaulted) and the
  `GenericEnvironment` override. `assets.rs`: `get_streamed`.
- `liquers-records/src/formats/{mod.rs,stream.rs}`: `encode_record_stream`, `is_streamable`;
  reuses `csv::write_csv` (with `WriteOptions { header }`) and `ndjson::write_ndjson` per batch.
- `liquers-lib/src/records/{mod.rs,stream.rs}`: `RecordSourceEncoder`; `register_records_commands!`
  pushes it onto `registry.stream_encoders` beside the `store_config` registration.
- `liquers-axum`: the parent's `serve_query` and `assets/common.rs` `asset_bytes` call
  `get_streamed` instead of `get_serialized`; `entry` routes keep `get_serialized` + `collect`
  (AC-6, Phase 1 Q4).
- Not touched: `liquers-web`, `liquers-py`, persistence, `ns-rec/materialize`, the type registry
  (`RecordSource` still declares only `yaml`/`json`, so the write path never tries rows).

## Error Handling

| Failure | Result |
|---|---|
| CSV/TSV and `stream.schema()` is `None` | first item `Error::general_error("… declare uniform_schema in the manifest, or request .ndjson")` → error answer (AC-3) |
| `source.stream(…)` or the first batch fails | first item that error → error answer with its status (AC-4) |
| later batch fails, CSV | item `Err` → body aborted by hyper; `eprintln!` with the query (parent AC-4) |
| later batch fails, NDJSON | one `$liquers_error` line (`error_type`, `message`), then `Err` → aborted |
| a cell fails to format (`format_value`) | as a failed batch |
| client disconnects | the body, the record stream and the resolver drop; no further chunk is evaluated |

No `unwrap`; matches on `TableFormat` are explicit (no `_ =>`).

## Relevant Commands

None added or changed; `specs/command_registry.yaml` is unchanged (encoders are not commands and
are not exported).

## Documentation Architecture

As Phase 1 §Documentation Intent: `RECORD_STREAMS.md`, `VALUE_TYPE_SYSTEM.md`,
`WEB_API_SPECIFICATION.md` §2.3, `RECORD_STREAM_GUIDE.md` §3.4 and §7, `SERIALIZATION_GUIDE.md` —
History rows and `reviewed:` bumps. `affects_docs` lists them.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | core `value.rs`, `commands.rs`, `context.rs`, `assets.rs`; records `formats/{mod,stream}.rs`; lib `records/{mod,stream,commands}.rs`; axum `query/handlers.rs`, `assets/common.rs` |
| Crates and workflows affected | core, records, lib (`records`), axum; Query and Assets API `data` routes |
| Existing tests likely to change | none: every value except a source in a row format takes the parent's path |
| New validation | Phase 3: encoder unit tests in records, lib integration through `get_streamed`, axum route tests |
| Compatibility / data / concurrency / performance / security | additive; a `CommandRegistry` field is `pub` (struct literal construction outside core would break — none found, constructed by `new`); each request re-evaluates nothing cached but re-encodes; chunk evaluations run as the client reads (back-pressure); a long export holds one asset-manager job at a time |
| Recovery | drop the encoder registration; every route returns to the parent's behaviour |
| Certainty and open questions | high on records encoding and the seam; medium on the effective data format of a source asset given `daily.csv` (the type default `yaml` must not win over the filename — `MetadataRecord::set_filename` seeds `data_format` only when unset; re-check at Step 3); Q1-Q4 are the open choices |
