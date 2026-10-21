# Phase 3: Examples & Use-cases — resource-name-unicode

All queries below are **rejected at HEAD** (checked with `liquers-validate --no-registry`:
"Can't parse query completely") except `-R/data/report-2024.csv`, which parses today and must keep
its encoding. They state the behaviour after implementation.

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | Fetch a file created outside Liquers | literal Unicode, store reads `café.csv` | AC-1, AC-6 |
| 2 | Example | Spell a name with entities | canonical encoding | AC-2 |
| 3 | Test | `parse::tests::resource_names_accept_unicode_and_entities` | parser | AC-1, AC-2, AC-5 |
| 4 | Test | `parse::tests::resource_name_entity_cannot_decode_to_separator_or_control` | AC-4 errors and position | AC-4 |
| 5 | Test | `query_backward_compatibility.rs` `resource_name_encodings_unchanged` | corpus of HEAD keys | AC-3 |
| 6 | Test | `query::tests::checked_constructors_refuse_invalid_names` | `try_new`, `try_join` | AC-4 |
| 7 | Test | `store_conformance` rule `keyshape03` (new): Unicode names round-trip | all conforming stores | AC-6 |
| 8 | Test | `store::tests::file_store_refuses_non_component_name` | backend refusal | AC-7 |
| 9 | Test | `query_builder::tests::unicode_key_segment` / `liquers-template` placeholder test | followers | AC-8 |

## Example 1: a file created outside Liquers

A directory store over `/srv/data` contains `reports/Łódź 2024.csv`.

```text
-R/reports/Łódź~.2024.csv/-/to_text
```

The key is `["reports", "Łódź 2024.csv"]`. `AsyncFileStore` reads `/srv/data/reports/Łódź 2024.csv`.
`Key::encode()` is `reports/~U141~~UF3~d~U17A~~.2024.csv` (letters encode numerically, as in
parameters: `encode_token("café")` is `caf~UE9~`). That text parses back to the same key. `listdir_keys(reports)` returns the same key.

## Example 2: entities and the canonical form

| Written | Name | `encode()` |
|---|---|---|
| `café.csv` | `café.csv` | `caf~UE9~.csv` |
| `caf~UE9~.csv`, `caf~D233~.csv` | `café.csv` | same as above |
| `my~.file.csv` | `my file.csv` | `my~.file.csv` |
| `~_x.csv` | `-x.csv` | `~_x.csv` |
| `a~~b` | `a~b` | `a~~b` |
| `report-2024.csv` | `report-2024.csv` | `report-2024.csv` (unchanged) |

The exact named-or-numeric spelling follows `entities::curated_name`, the same rule as parameters.
The test asserts `parse(encode(k)) == k` and pure ASCII, not a hand-written spelling.

## Edge and Error Cases

| Case | Expected | Test |
|---|---|---|
| `a~/b`, `a~U2F~b`, `~H` in a name | parse error at the `~`, naming `/` | 4 |
| `~U0~`, `~U7F~`, `~U1F~` in a name | parse error naming the control character | 4 |
| `~U2E~~U2E~` as a segment | key is relative (`..`) | 3 |
| NFD `cafe~U301~.csv` | a different key from NFC `café.csv`; both round-trip | 3 |
| `Key::new().join("a/b")` (unchecked) handed to `AsyncFileStore` | `KeyNotSupported` | 8 |
| `ResourceName::try_new("")` | `ParameterError` | 6 |
| name with `:` on Linux file store | accepted; on Windows `KeyNotSupported` (component check) | 8 (platform-gated row) |
| `liquers-web` `FetchStore` with `café.csv` | URL percent-encodes the decoded name | web test `fetch_key_url_percent_encodes_unicode` |

## Test Plan

- Unit, `liquers-core/src/parse.rs`: `resource_names_accept_unicode_and_entities` (AC-1, AC-2,
  AC-5); `resource_name_entity_cannot_decode_to_separator_or_control` (AC-4: error type
  `ParseError`, `position.offset` at the `~`).
- Unit, `liquers-core/src/escape/tests.rs`: `encode_resource_name_is_ascii_and_round_trips` —
  property over random strings that pass `is_valid_resource_name` (AC-2).
- Unit, `liquers-core/src/query.rs`: `checked_constructors_refuse_invalid_names` (AC-4);
  `key_path_is_decoded` (`Key::path()` vs `encode()`).
- Integration, `liquers-core/tests/query_backward_compatibility.rs`: replace
  `non_ascii_resource_names_are_rejected_coherently` by `non_ascii_resource_names_parse` and add
  `resource_name_encodings_unchanged` over every key in the file's existing corpus (AC-3).
- Conformance, `liquers-core/src/store_conformance/rules/keyshape.rs`: `keyshape03` writes, lists
  and reads `["u", "café.csv"]` and `["u", "my file.csv"]` (AC-6); run by
  `cargo test -p liquers-core --features store-conformance --test store_conformance_CONF` and
  `cargo test -p liquers-store`.
- Unit, `liquers-core/src/store.rs`: `file_store_refuses_non_component_name` (AC-7).
- `liquers-web`: `fetch_key_url_percent_encodes_unicode`, `local_storage_unicode_key_round_trips`.
- `liquers-template` and (if landed) `query_builder`: `café.csv` accepted as a key segment (AC-8).
- Commands: `cargo test -p liquers-core --lib parse escape query store`, the conformance command
  above, `cargo test -p liquers-store`, `cargo test -p liquers-template`,
  `cargo test -p liquers-lib --lib --tests`, the `liquers-web` loop after `cargo clean`.

## Learning Log

- The parser already accepts Unicode alphanumerics literally in parameters (D6), so doing the same
  for names keeps the two grammars parallel. Only the encoder's literal class differs between them
  (names keep inner `-` literal).
