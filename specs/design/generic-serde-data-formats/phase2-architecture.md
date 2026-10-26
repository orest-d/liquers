# Phase 2: Solution & Architecture — generic serde data formats

## Overview

A new module, `liquers-core/src/serde_format.rs`, holds a `SerdeFormat` enum (`Json`, `Yaml`,
`Cbor`, `MessagePack`) and three operations: map a data format to it, encode a `Serialize`
payload, and decode a `DeserializeOwned` payload. The same module lists the generic formats
enabled in the build, and that list is used twice: by the `TypeInfo` builder
`with_serde_formats()`, so the declaration comes from it, and by the codec arms, so the
implementation does too.

Core `Value` and `SimpleValue` each get one new branch in `as_bytes` and one in
`deserialize_from_bytes`. The branch matches every variant or identifier explicitly and hands the
payload to the helper. JSON keeps its existing arms.

Rejected:
- A blanket `impl<T: Serialize> DefaultValueSerializer for T`. It conflicts with the existing
  impls and cannot opt out per variant.
- Serializing the whole enum tagged in the binary formats. That would be a second wire shape
  beside JSON's identifier-dispatched payload.
- Schema-dependent formats (Phase 1, question 2).
- TOML in the generic set (question 1).
- `cfg`-gated `SerdeFormat` variants. They would make every match feature-dependent, so this
  follows `TableFormat`'s precedent instead: the variants always exist, and a disabled one refuses
  at run time, naming its feature.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `DATA-FORMAT-CONSTANTS-AND-TOOLING` (`data-format-vocabulary`) | draft | P2 | must list `cbor`, `msgpack` | yes | no | Add the two rows there or here |
| `CORE-VALUE-JSON-READ-IGNORES-STRUCTURED-IDENTIFIERS` (`core-value-json-identifier-dispatch`) | draft | P2 | same identifier table for JSON | preferably | no | Reuse its table |
| `SERIALIZATION-CANNOT-REPORT-A-LOSSY-WRITE` | draft | P2 | binary formats are not lossy | no | no | None |
| `VALUE-SERIALIZATION-HAS-NO-INCREMENTAL-WRITER` | draft | P2 | same `as_bytes` functions | no | no | Rebase |

## Interfaces

```rust
// liquers-core/src/serde_format.rs   (pub mod serde_format; in lib.rs) — all synchronous
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SerdeFormat { Json, Yaml, Cbor, MessagePack }

impl SerdeFormat {
    /// "json"; "yaml" | "yml"; "cbor"; "msgpack". Anything else: None (refinements never match).
    pub fn from_data_format(data_format: &str) -> Option<SerdeFormat>;
    pub fn data_format(self) -> &'static str;       // canonical spelling
    pub fn is_enabled(self) -> bool;                // Cbor: cfg!(feature = "cbor"), MessagePack: "msgpack"
    pub fn is_binary(self) -> bool;                 // Cbor, MessagePack
}

/// The generic formats (JSON excluded: every type writes JSON its own way) enabled in this
/// build, with their declared aliases: ["yaml", "yml", "cbor"?, "msgpack"?].
pub fn generic_serde_formats() -> Vec<&'static str>;
/// `from_data_format` restricted to the generic set and enabled.
pub fn generic_serde_format(data_format: &str) -> Option<SerdeFormat>;

pub fn to_bytes<T: Serialize + ?Sized>(value: &T, format: SerdeFormat) -> Result<Vec<u8>, Error>;
pub fn from_bytes<T: DeserializeOwned>(b: &[u8], format: SerdeFormat, what: &str) -> Result<T, Error>;
/// Bytes as a byte string where the format has one (CBOR, MessagePack), else as a sequence.
pub fn bytes_to_bytes(b: &[u8], format: SerdeFormat) -> Result<Vec<u8>, Error>;
pub fn bytes_from_bytes(b: &[u8], format: SerdeFormat) -> Result<Vec<u8>, Error>;
```

Codecs: `serde_json::to_vec` / `from_slice`; `serde_yaml::to_string` / `from_slice`;
`ciborium::into_writer` / `from_reader`; `rmp_serde::to_vec_named` / `from_slice`. `to_vec_named`
writes structs as maps, so `#[serde(default)]` and `skip_serializing_if` keep working. A disabled
format returns `SerializationError` "data format 'cbor' needs liquers-core's 'cbor' feature,
which this build does not enable".

`liquers-core/src/type_system.rs`:

```rust
impl TypeInfo {
    /// Appends `generic_serde_formats()` to `supported_data_formats`.
    pub fn with_serde_formats(self) -> Self;
}
```

`liquers-core/src/value.rs`, `json_f64::serialize`: `if !serializer.is_human_readable() {
serializer.serialize_f64(*n) } else { as today }`. The visitor already accepts a float.

Core `Value`, `DefaultValueSerializer`:
- `as_bytes`: before the existing `match`,
  `if let Some(sf) = serde_format::generic_serde_format(data_format) { return self.serde_payload(sf); }`
- the private `Value::serde_payload(&self, sf)` has one arm per variant: `None` → `&()`, scalars,
  `Text`, `Array`, `Object`, `Metadata`, `AssetInfo`, `Recipe`, `CommandMetadata`, `Query`, `Key`
  → their payload, `Bytes` → `bytes_to_bytes`. There is no `_` arm.
- `deserialize_from_bytes`: the same early branch, calling the private
  `serde_payload_read(b, type_identifier, sf)`. It matches identifiers like the JSON table of
  `core-value-json-identifier-dispatch`. `""` and unknown identifiers read `Value` by shape.
- `type_descriptions`: `.with_serde_formats()` on every core identifier.

`liquers-lib/src/value/simple.rs`: the same pair of branches over `SimpleValue`'s variants. In
YAML, `Array` and `Object` are written through a private `ShapeWrite(&SimpleValue)` that
implements `Serialize` as plain data (Phase 1, question 4). In CBOR and MessagePack, their payload
keeps `SimpleValue`'s tagged elements. Reading `Array` or `Object` tries the payload type first
and then a shape read, as the JSON arm already does. The `yaml`/`yml` arm, which goes through
`serde_json::Value`, is removed. A shape read (`""`, or the fallback) uses a
private `ShapeRead(SimpleValue)` with a `deserialize_any` visitor: unit/none → `None`, bool, i64 /
u64 within range → `I64`, f64 → `F64`, str → `Text`, bytes → `Bytes`, seq → `Array`, map with
string keys → `Object`. That works for every self-describing format, and NaN survives it. The
`toml` text read is unchanged. `type_descriptions` is shared with core, so it inherits the formats.

`liquers-lib/src/value/mod.rs`: `RecordSource`'s manifest arms call `serde_format` for `yaml` and
`json`, with no behaviour change. No other `ExtValue` variant opts in.

Cargo:
- `liquers-core`: `ciborium = { version = "0.2", optional = true }`,
  `rmp-serde = { version = "1.3", optional = true }`, `serde_bytes = "0.11"`; features
  `cbor = ["dep:ciborium"]`, `msgpack = ["dep:rmp-serde"]`.
- `liquers-lib`: `serde-binary = ["liquers-core/cbor", "liquers-core/msgpack"]`, added to
  `default`.
- `liquers-web`: no change. It can opt in with `liquers-core/cbor`.

## Integration Points

- `liquers-core/src/lib.rs`: `pub mod serde_format;`.
- `liquers-core/src/data_format.rs` (after `data-format-vocabulary`): table rows `cbor` and
  `msgpack`, and constants `CBOR`, `MSGPACK`. `serde_format` uses the constants.
- `liquers-core/src/media_type.rs`: already maps both, so no change.
- The asset write path checks `supported_data_formats`, so the new formats become writable through
  the declarations alone. The read path dispatches by `data_format` into the new branches.
- `scripts/check-build-matrix.sh`: `CORE_CONFIGS` gains `--features cbor,msgpack --tests`.
  `LIB_CONFIGS`' `--no-default-features` rows already build without `serde-binary`. Add a wasm32
  row, `--features webui,serde-binary`, to prove both crates build for wasm.

## Error Handling

Encode and decode failures are `Error::from_error(ErrorType::SerializationError, …)`, naming the
format and the identifier. A disabled format gets the feature message. A payload that does not
match its identifier fails to deserialize, and the asset layer then treats it as corrupted and
recomputes it, as it already does for JSON. No `Error::new` and no `unwrap`.

## Relevant Commands

None. `specs/command_registry.yaml` is unchanged.

## Documentation Architecture

| Path | Change |
|---|---|
| `specs/guides/SERIALIZATION_GUIDE.md` | the fidelity table gains `yaml`, `cbor`, `msgpack` rows; a "Compact and lossless: CBOR" subsection; History, `reviewed:` |
| `specs/reference/VALUE_TYPE_SYSTEM.md` | §Reading: generic formats, payload by identifier, empty identifier by shape; History, `reviewed:` |
| `specs/guides/TYPE_SYSTEM_GUIDE.md` | step 4: `with_serde_formats()` and the matching codec branch; History, `reviewed:` |

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `liquers-core/{Cargo.toml, src/lib.rs, src/serde_format.rs, src/value.rs, src/type_system.rs, src/data_format.rs}`; `liquers-lib/{Cargo.toml, src/value/simple.rs, src/value/mod.rs}`; `scripts/check-build-matrix.sh` |
| Crates and workflows affected | core, lib; any store write or read with a `.yaml`, `.yml`, `.cbor` or `.msgpack` key; HTTP serving of those formats |
| Existing tests likely to change | the `SimpleValue` YAML tests (`simple.rs` ~970-1080). Their values do not change, but they now go through the payload rule. Format-list snapshots, if any compare declared formats |
| New validation | Phase 3 tests; build-matrix rows; wasm32 check |
| Compatibility / data | New formats only. JSON bytes are unchanged (`json_f64` changes only in non-human-readable serializers). A `.yaml` file that read as `Object` still does. A `.yaml` file with a declared identifier now reads as that identifier's payload |
| Concurrency / performance / security | All synchronous and pure. CBOR decoding of hostile input: `ciborium` bounds recursion (default 256), and `rmp-serde` has a depth limit. Both read only into the declared payload type |
| Dependencies | `ciborium`, `rmp-serde` and `serde_bytes` are already in `Cargo.lock`. `serde_yaml` is archived upstream but already used throughout the workspace, so this adds no new exposure |
| Recovery | Turn off `serde-binary`. Revert the module and the branches; nothing persisted depends on them before release |
| Certainty | High for the codec mechanics. The untagged core `Value` shape read was checked against serde's `Content` buffering: an f64 NaN never matches `None`, and an integer does not match `F64` first. Medium for exact size ratios (AC-7 asserts only an ordering and a bound) |
