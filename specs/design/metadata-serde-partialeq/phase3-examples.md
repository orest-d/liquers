# Phase 3: Examples and Tests - Serde and Equality for `Metadata`

## Use Cases

```rust
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
struct Holder { metadata: Metadata }

let mut record = MetadataRecord::new();
record.with_title("Sales".to_string());
let h = Holder { metadata: Metadata::MetadataRecord(record) };
let json = serde_json::to_string(&h)?;          // {"metadata":{ …record fields… }}
assert_eq!(serde_json::from_str::<Holder>(&json)?, h);
```

## Tests to Add (`liquers-core/src/metadata.rs`, `mod tests`)

| Test | Asserts | Criterion |
|---|---|---|
| `metadata_serialize_matches_to_json` | for a `MetadataRecord` and a legacy `{"media_type":"text/plain","custom":{"a":1}}`, `serde_json::to_value(&m)? == serde_json::from_str::<serde_json::Value>(&m.to_json()?)?` | 2, 6 |
| `metadata_deserialize_chooses_the_same_variant_as_from_json` | `serde_json::from_str::<Metadata>(s)` and `Metadata::from_json(s)` give equal values for: a full record, a partial record `{"media_type":"text/plain"}` (→ `MetadataRecord`), the legacy document above (→ `LegacyMetadata`, `custom` kept), `null`, `[1,2]` | 3, 4 |
| `metadata_record_round_trips_through_serde` | `MetadataRecord` with title, key, log entry and status round-trips to an equal `Metadata` via JSON and via `serde_yaml` | 1, 4 |
| `metadata_partial_eq_is_structural` | equal records equal; differing title unequal; `LegacyMetadata(json_of_record) != MetadataRecord(record)` | 1 |
| `struct_embedding_metadata_derives_the_traits` | the `Holder` example above compiles and round-trips | 5 |

Tests return `Result<(), Box<dyn std::error::Error>>` and use `?`, per `UNITTEST_GUIDE.md`.

## Edge Cases

- Legacy `null` → `LegacyMetadata(Value::Null)`, re-serializes as `null`.
- A record document with one unknown field → legacy, field preserved (the
  `deny_unknown_fields` rule).

## Setup

No environment, store or commands. No Liquers queries are needed.

## Coverage Review

Every Phase 1 criterion is covered; the Phase 2 compatibility risk (wire form drift) is the first
test.
