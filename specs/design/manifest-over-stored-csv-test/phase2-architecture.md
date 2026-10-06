# Phase 2: Solution and Architecture

## Test file

`liquers-lib/tests/records_manifest_over_csv_files.rs`, `#![cfg(feature = "records")]`, modelled
on `record_manifest_resource_key.rs` (environment construction with `AsyncMemoryStore`,
`register_records_commands!`, a `ManifestRecipeProvider` if that file installs one; copy its
`build_env`).

Manifest document:

```yaml
manifest: record-stream
uniform_schema:
  fields:
    - name: month
      data_type: Text
    - name: amount
      data_type: Int
chunks:
  - query: -R/data/raw/jan.csv/-/ns-rec/to_record-csv
  - query: -R/data/raw/feb.csv/-/ns-rec/to_record-csv
```

Check the exact `RecordSchema` YAML field names (`data_type` vs `type`, nullable default) against
an existing manifest test with `uniform_schema` before writing it.

## Known-issue preflight

None blocking.

## Relevant commands

`ns-rec/to_record`, `ns-rec/materialize`.

## Documentation architecture

Guide §3.2 paragraph rewritten with the test's manifest and a `<sub>` source line, as other
snippets in that guide do. History row, `reviewed:`.

## Risk table

| Area | Assessment |
|---|---|
| Likely files | New test file; the guide |
| Production risk | None |
| Test risk | It may expose a defect. File it rather than work around it. |
| Certainty | High |
