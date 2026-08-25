# Phase 3: Examples & Use-cases - Liquers value type system

## High-Level Introduction

Phase 1 set out to replace two metadata fields that nothing kept consistent with an explicit type
model. Phase 2 defined it: one type axis (`type_identifier`), an encoding axis with an inward face
(`data_format`) and an outward one (`media_type`), a `TypeRegistry` that owns the facts, and
enforcement split into a hard tier that rejects and a soft tier that logs.

The scenarios below make that visible in the order a developer meets it.

**Scenario 1** is the P0 itself, end to end: a value written to a store and read back, with the
declared type and format agreeing at every step — and the same flow refused when they do not.
**Scenario 2** goes into the seeding cascade and the level-3 override, which is where the encoding
axis earns its two faces: it shows a value with no filename taking the value's own defaults, the
same value gaining an extension, and a caller deliberately overriding the media type to shape a web
response. **Scenario 3** collects the pitfalls that the current code actually exhibits — the
`"generic"`/`"i32"` mismatch, the quoted legacy `data_format`, and the extension-versus-refinement
false warning.

Every example is anchored to a check that exists in the test plan, so nothing here is aspirational
prose.

## Example Type

**Conceptual code.** Nothing in Phase 2 exists yet, so a runnable prototype would be a partial
implementation rather than an example — the design would be validated against itself. The examples
are therefore snippets showing intended usage, and the **test plan is written as concrete,
placeable test specifications** (file path, name, and the assertion each makes), so Phase 4 has
something executable to build against rather than a description of one.

*Reviewer note:* if runnable prototypes are wanted instead, the natural cut is a standalone
`liquers-core` example exercising `TypeRegistry` alone, which has no dependency on the metadata or
asset changes.

## Overview Table

| # | Type | Name | What it demonstrates / checks |
|---|---|---|---|
| 1 | Example | Round trip with an honest type | The P0 resolved: declared type and format agree on write, and the read reconstructs from `data_format` rather than the filename |
| 2 | Example | The seeding cascade and a deliberate override | Level 1 → 2 → 3; absent `data_format` meaning "use the value default"; `media_type` override surviving the reject rule |
| 3 | Example | Three pitfalls that exist at HEAD | `identifier()` vs the deserializer's expectation; the quoted legacy `data_format`; the spurious extension warning |
| 4 | Unit | `type_system.rs` — registry | Construction, duplicate rejection, realm keying, format support, unknown identifier |
| 5 | Unit | `metadata.rs` — resolution | `declared_data_format`, `effective_data_format`, `effective_media_type`, legacy `as_str()` extraction, partial documents |
| 6 | Unit | `state.rs` — seeding | Every constructor seeds level 1; an already-set field is not overwritten |
| 7 | Unit | `value.rs` — descriptions | `Value::type_descriptions()` is complete, internally consistent, and matches `identifier()` |
| 8 | Integration | `type_consistency.rs` | `set`/`set_state` hard rejections and soft warnings, through a real `AssetManager` |
| 9 | Integration | `type_round_trip.rs` | Store round trip per type; degrade-on-read for an unregistered identifier |
| 10 | Integration | Cross-crate | `ExtValue`, `CombinedValue`, `SimpleValue` all satisfy the new methods; `default_extension` delegation fixed |

## Example 1: Round trip with an honest type

### Connection to the High-Level Design

This is `CORE-METADATA-FORMAT-TYPE-CONSISTENCY` in one flow. Phase 1's purpose — "type information
serves describing, serializing, and reading back" — is exactly what a round trip exercises, and the
silent corruption in the issue is a round trip that *doesn't* close.

### Scenario

A command produces a text value. The asset layer serializes it, stores it with its metadata, the
in-memory asset is evicted, and a later query reads it back.

```rust
// The value carries its own facts; nothing has to be told them.
let state = State::new().with_data(Value::from("hello".to_string()));
// State::sync_metadata_with_value has seeded level 1 from the value:
//   type_identifier = "Text", data_format = None (meaning: the value's default, "txt"),
//   media_type = None (meaning: derive)
assert_eq!(state.metadata.type_identifier()?, "Text");

// Write. validate_metadata_hard consults the registry:
//   is "Text" registered?               -> yes
//   is the effective format "txt" in its supported_data_formats? -> yes
asset_manager.set_state(&key, state).await?;

// Read. deserialize_stored_value dispatches on the stored data_format,
// never on the filename extension.
let back = asset_manager.get(&key).await?;
assert_eq!(back.try_into_string()?, "hello");
```

The same flow, dishonestly declared, is refused rather than stored:

```rust
let mut record = MetadataRecord::new();
record.with_type_identifier("Text".to_owned())
      .with_data_format("parquet".to_owned());     // Text cannot be written as parquet

let err = asset_manager.set(&key, b"hello", &Metadata::MetadataRecord(record)).await
    .expect_err("unsupported format must be refused");
assert_eq!(err.error_type, ErrorType::SerializationError);
// message names the type, the format, and what Text does support
```

**Checked by:** tests 8.1, 8.2, 9.1.

## Example 2: The seeding cascade and a deliberate override

### Connection to the High-Level Design

Phase 1 recorded two seeding levels plus an override, and that an absent `data_format` *means*
something. Phase 2 made `media_type` an `Option` so a level-3 override is distinguishable from a
derived value. This scenario is where those two decisions pay off — and where the reject rule has
to not break a legitimate override.

### Scenario

```rust
// Level 1 — no filename anywhere. The value's own defaults apply.
let state = State::new().with_data(Value::from("hello".to_string()));
assert_eq!(state.metadata.declared_data_format(), None);   // meaningful: "unspecified"
assert_eq!(state.metadata.effective_data_format("txt"), "txt");

// Level 2 — a filename arrives; its extension is written into data_format.
let mut record = MetadataRecord::new();
record.with_filename("notes.csv".to_owned());
assert_eq!(record.declared_data_format(), Some("csv"));

// Level 3 — a caller shapes the web response deliberately.
record.with_media_type("text/plain".to_owned());     // Some(..) = override, kept verbatim
assert_eq!(record.effective_media_type("csv"), "text/plain");   // not text/csv
```

The override is a **soft** divergence, not a hard one. It logs and stores:

```rust
asset_manager.set(&key, data, &Metadata::MetadataRecord(record)).await?;   // succeeds
// metadata log carries: warning "media_type 'text/plain' differs from expected 'text/csv' ..."
```

This is the case `liquers-web/src/store/fetch.rs:91-100` already relies on — a fetched file whose
origin server declared a `Content-Type` the extension does not imply. Promoting that warning to an
error would break it, which is why the tier split exists.

**Checked by:** tests 5.1–5.4, 6.1, 6.2, 8.3, 8.4.

## Example 3: Three pitfalls that exist at HEAD

### Connection to the High-Level Design

Each of these is a live defect the design closes. They are examples rather than tests-only because
a developer meeting the codebase will hit them, and the reference document has to say why the
behaviour changed.

**Pitfall 1 — `identifier()` and the deserializer already disagree.**

```rust
// At HEAD:
Value::I32(7).identifier()             // "generic"
// but liquers-core/src/value.rs:868 branches on:
//   "i32" => s.parse::<i32>() ...
// so an I32 written as text reads back as Value::Text("7"), silently.
```

Under this design `Value::I32(7).identifier()` is `"I32"`, the registry says `I32` supports `txt`
and `json`, and the deserializer dispatches on the same string that was written. **Backward
compatibility is deliberately not preserved** (user decision): stored identifiers change, and no
migration is provided.

**Pitfall 2 — legacy metadata returns a quoted format.**

```rust
// metadata.rs:1782, LegacyMetadata branch:
//   data_format.to_string()   on a serde_json::Value
Metadata::from_json(r#"{"data_format":"json"}"#)?.get_data_format()
// == "\"json\""   — matches no format in any registry
```

Left unfixed, reject-on-write turns this cosmetic bug into a refusal of every partial or foreign
metadata document. It is fixed here (`as_str()` extraction) *and* at the root: `MetadataRecord`
gains `#[serde(default)]` so a partial document no longer falls through to the legacy branch.

**Pitfall 3 — a refinement is not a mismatch.**

```rust
// assets.rs:3176 compares with a plain !=
//   extension "csv"  vs  data_format "csv:comma"   -> warns, wrongly
```

The comparison moves to the **base** format (split at the first `:`), so a legitimate refinement is
silent and a real disagreement — `data.json` holding `csv` — still warns.

**Checked by:** tests 5.5, 5.6, 7.2, 8.5.

**Pitfall 4 — error states are typed too, and inconsistently.** `Metadata::with_error` sets
`type_identifier = "error"` (`metadata.rs:1807`), while `State::from_error` calls it and *then*
`sync_metadata_with_value`, which overwrites the identifier from `V::none()`. Two paths, two
identifiers, for the same situation. Since validation runs unconditionally (`assets.rs:3226` — it
is **not** exempt today, contrary to a plausible assumption), the design must say what an error
state's type is. Phase 2 now does: `error` is a registered bare type, and the *format* check is
skipped for error states because their bytes are not a serialization of the declared type.

**Checked by:** tests 8.6, 8.8, 8.9.

## Corner Cases

| Area | Case | Expected |
|---|---|---|
| Serialization | Format supported for reading but not writing | `supported_data_formats` is one list; if the two ever diverge the registry needs two. Recorded as a Phase 4 check, not assumed |
| Serialization | Empty `type_identifier` on a non-error status | Hard error (already enforced, `assets.rs:3159`) |
| Serialization | Error status | **Not** currently exempt: `validate_required_metadata_fields` runs unconditionally (`assets.rs:3226`). See "Error states" below — the design has to say what happens, and Phase 2 now does |
| Serialization | `Metadata::with_error` sets `type_identifier = "error"` (`metadata.rs:1807`) | `error` must be a registered bare type, or every directly-errored metadata is refused |
| Serialization | An error state retains the intended output's filename | Effective format is `csv` while the identifier is `error`; the **format** check must be skipped for error states or this hard-rejects |
| Serialization | `State::from_error` ordering | `with_error` sets `"error"`, then `sync_metadata_with_value` overwrites it from `V::none()`. Two paths produce two different identifiers for the same situation |
| Registry | Two crates register `Image` | `register` returns `Err`; load order must not decide |
| Registry | Identifier absent from this build | Read degrades to `Undeserialized`, bytes and metadata verbatim, warning logged |
| Registry | Degraded value re-persisted | Bytes taken from `poll_binary` without re-serializing; metadata written back unchanged, so the hard tier never sees a value it must reject |
| Realm | Lookup in an unpopulated realm | `get_in_realm("other", ..)` → `None`; the default realm is unaffected |
| Media type | Override containing `\r\n` | Hard error — it reaches an HTTP header |
| Media type | Override that is empty string | Treated as `Some("")`, which is malformed → hard error. `None` is the way to say "derive" |
| Concurrency | Registry read during evaluation | `&TypeRegistry` behind `Environment`; built once, read-only thereafter, so no lock and no `scc` |
| Memory | `TypeInfo` cloning | `Cow<'static, str>` throughout; a statically-described type allocates nothing |
| Cross-crate | `CombinedValue` with an extended value | `default_extension` must delegate — the `"ext"` constant (`extended.rs:150`) is fixed as part of this work |
| Legacy | `Metadata::LegacyMetadata` non-object | Existing `_ => "bin"` arms; behaviour preserved, not extended |

## Test Plan

Placement follows the crate conventions: unit tests inline in the module under test, integration
tests in `tests/`. Every test returns `Result<(), Box<dyn std::error::Error>>` where it uses `?`.

### Unit — `liquers-core/src/type_system.rs`

| # | Test | Asserts |
|---|---|---|
| 4.1 | `registry_from_value_type_is_complete` | `TypeRegistry::from_value_type::<Value>()` contains one entry per `Value` variant identifier |
| 4.2 | `duplicate_registration_is_an_error` | Registering the same `(realm, identifier)` twice returns `Err`, naming the identifier |
| 4.3 | `supports_data_format_matches_the_list` | True for a listed format, false for an unlisted one, false for an unknown type |
| 4.4 | `realm_keying_isolates_lookups` | A type registered in realm `"web"` is not found by the default-realm `get`, and is found by `get_in_realm` |
| 4.5 | `builder_produces_consistent_defaults` | `TypeInfo::new(..).with_defaults(..)` yields matching extension / media type / format |
| 4.6 | `identifier_naming_rule_holds` | Every registered identifier is either bare-and-on-the-enumerated-list, or `provider.LocalName` with exactly one dot and no other non-alphanumerics |

### Unit — `liquers-core/src/metadata.rs`

| # | Test | Asserts |
|---|---|---|
| 5.1 | `declared_data_format_distinguishes_none` | `None` when unset; `Some(f)` when set. No extension fallback |
| 5.2 | `effective_data_format_uses_value_default` | `None` + default `"txt"` → `"txt"`; `Some("csv")` + default `"txt"` → `"csv"` |
| 5.3 | `effective_media_type_prefers_the_override` | `Some("text/plain")` survives verbatim against a `csv` format |
| 5.4 | `effective_media_type_derives_when_absent` | `None` + `"csv"` → `"text/csv"` via `media_type::file_extension_to_media_type` |
| 5.5 | `legacy_accessors_return_unquoted_strings` | `Metadata::from_json(r#"{"data_format":"json"}"#)` → `"json"`, not `"\"json\""`. Same for `media_type`, `type_identifier`, `type_name` |
| 5.6 | `partial_document_deserializes_into_a_record` | `{"media_type":"text/plain"}` produces `Metadata::MetadataRecord`, not `LegacyMetadata` |
| 5.7 | `malformed_media_type_is_rejected` | CR, LF, and a value without `/` each fail validation |

### Unit — `liquers-core/src/state.rs`

| # | Test | Asserts |
|---|---|---|
| 6.1 | `every_constructor_seeds_level_one` | `new`, `from_value_and_metadata`, `with_metadata`, `with_data`, `from_error` all leave `type_identifier` and the level-1 defaults set |
| 6.2 | `seeding_does_not_overwrite_a_declared_value` | A metadata carrying `data_format: Some("csv")` keeps it when attached to a `Text` value |
| 6.3 | `from_parts_does_not_seed` | The documented low-level escape hatch stays unsynced — a behaviour change here would be silent |

### Unit — `liquers-core/src/value.rs`

| # | Test | Asserts |
|---|---|---|
| 7.1 | `type_descriptions_match_identifier` | For every `Value` variant, `v.identifier()` has an entry in `Value::type_descriptions()` |
| 7.2 | `scalar_identifiers_round_trip_through_the_serializer` | For each variant, `as_bytes(f)` then `deserialize_from_bytes(.., identifier, f)` yields the same variant — the assertion `Value::I32` fails at HEAD |
| 7.3 | `every_default_is_in_supported_formats` | `default_data_format()` appears in that type's `supported_data_formats` |
| 7.4 | `supports_data_format_agrees_with_the_registry` | Instance method and registry lookup give the same answer for every variant |

### Integration — `liquers-core/tests/type_consistency.rs`

| # | Test | Asserts |
|---|---|---|
| 8.1 | `set_state_accepts_a_consistent_value` | Round trip through `AsyncMemoryStore` succeeds and the stored metadata names the format actually used |
| 8.2 | `set_rejects_an_unsupported_format` | `ErrorType::SerializationError`, message naming type, format and the supported set |
| 8.3 | `set_rejects_an_unregistered_identifier` | `ErrorType::General`, message naming the identifier |
| 8.4 | `a_declared_media_type_override_survives` | `set` succeeds; the stored `media_type` is the override; a warning is logged |
| 8.5 | `extension_refinement_does_not_warn` | `notes.csv` + `csv:comma` produces no warning; `notes.json` + `csv` does |
| 8.6 | `error_state_with_a_mismatched_filename_is_storable` | A `Status::Error` metadata whose filename is `report.csv` stores despite the `error` identifier not supporting `csv` — the format check is skipped for error states |
| 8.8 | `error_identifier_is_registered` | `Metadata::with_error` produces metadata that passes the identifier check |
| 8.9 | `state_from_error_and_direct_with_error_agree` | Both paths yield the same `type_identifier`; the ordering hazard in `State::from_error` is pinned by a test |
| 8.7 | `malformed_media_type_is_refused` | An override containing `\r\n` fails before reaching the store |

### Integration — `liquers-core/tests/type_round_trip.rs`

| # | Test | Asserts |
|---|---|---|
| 9.1 | `round_trip_every_core_type` | For each `Value` variant and each of its supported formats: write, evict, read, compare |
| 9.2 | `read_dispatches_on_data_format_not_extension` | A file named `.txt` whose `data_format` is `json` deserializes as JSON — the asymmetry named in `FINDINGS.md` §2/§3 |
| 9.3 | `unregistered_identifier_degrades` | A store entry declaring an unknown identifier yields `Undeserialized`, a warning, and no error until a value is requested |
| 9.4 | `degraded_value_re_persists_verbatim` | Re-storing a degraded asset writes the original bytes and metadata unchanged |

### Integration — cross-crate

| # | Test | Location | Asserts |
|---|---|---|---|
| 10.1 | `ext_value_type_descriptions_complete` | `liquers-lib/tests/value_type_system.rs` | Every `ExtValue` variant appears in `type_descriptions()`, under every feature combination |
| 10.2 | `combined_value_delegates_all_defaults` | same | `default_extension` delegates rather than returning `"ext"` — the `COMBINED-VALUE-DEFAULT-EXTENSION-NOT-DELEGATED` regression test |
| 10.3 | `simple_value_satisfies_the_new_methods` | same | `SimpleValue` implements the three additions consistently with `Value` |
| 10.4 | `registry_export_still_matches` | existing `registry_export.rs` | The command registry export is unaffected — a guard against accidental coupling |

### Build matrix

Feature interactions are where cfg bugs hide, so the suites above run under each configuration this
project can affect:

```bash
cargo test -p liquers-core --lib --tests
cargo test -p liquers-lib --lib --tests                              # default features
cargo test -p liquers-lib --no-default-features --lib --tests        # no polars, no egui, no image
cargo test -p liquers-lib --no-default-features --features polars --lib --tests
```

`liquers-web` and `liquers-py` compile-only checks are listed in Phase 4; they have no new
behaviour, only the three trait methods.

## Documentation and Learning Log

Guide-worthy material identified while writing these examples, for
`specs/guides/TYPE_SYSTEM_GUIDE.md` in Phase 5:

| Candidate | Answers | Evidence to link |
|---|---|---|
| Adding a value type, start to finish | "How do I add a type?" | Test 10.1 as the executable check that it was done completely |
| Choosing an identifier | "Bare or prefixed? What does the dot mean?" | Test 4.6, which enforces the rule mechanically |
| Declaring supported formats | "Why is my value refused on write?" | Example 1's rejection case; test 8.2 |
| Understanding the seeding cascade | "Why did my value serialize as CSV?" | Example 2; the `Info` provenance log entry |
| Overriding a media type deliberately | "How do I control the HTTP Content-Type?" | Example 2's level-3 case; `fetch.rs:91-100` as the in-tree precedent |

Learning worth carrying to Phase 5 regardless of where it lands:

- The four duplicated copies of `add_soft_consistency_warnings` are the reason the checks drifted;
  hoisting them is a prerequisite, not a tidy-up.
- `size_of::<Value>()` is 704 bytes (`CORE-VALUE-ENUM-OVERSIZED`) — the payload discipline this
  design documents has an immediate, measured target.
- The orphan rule forced `TypeIdentifiedIn<V>` over a bare `TypeIdentified`; that constraint is
  invisible in the signature and needs saying in the reference, or someone will "simplify" it back.
