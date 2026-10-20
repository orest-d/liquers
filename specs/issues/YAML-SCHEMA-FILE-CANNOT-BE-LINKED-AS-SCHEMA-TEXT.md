---
id: YAML-SCHEMA-FILE-CANNOT-BE-LINKED-AS-SCHEMA-TEXT
kind: issue
title: A plain .yaml schema file cannot be linked into to_record's schema argument
status: draft
priority: P2
complexity: S
area: [records, lib/value]
design:
created: 2026-10-20
github:
---
# A plain .yaml schema file cannot be linked into to_record's schema argument

## Problem

`ns-rec/to_record`'s `schema` argument is a `String`, and the documented way to pass a schema is a
link to a schema file: `ns-rec/to_record-csv-t-~X~-R/data/orders.schema.yaml~E`. A `.yaml` file in
the store is read back as a `Value::Object`, so the link resolves to an object, and binding it to the
`String` argument fails:

```
ConversionError: Can't convert 'object' to string
```

`liquers-lib/tests/records_table_ops.rs` (`schema_round_trips_through_the_schema_argument`) passes
only by accident: its schema uses YAML tags (`!FullText`), which the object reader rejects, so that
file falls back to bytes and binds as text. Its comment ("A .yaml file arrives as bytes") is not the
general rule. Found while writing `to_record_parquet_with_schema_option_is_strict`
(`design/parquet-read-declared-schema/`), which uses `.txt` schema files to get around it.

## Expected

A schema file linked into `schema` works whatever its content parses as: either `to_record` accepts
an `Object` value for `schema` (deserializing it as a `RecordSchema`), or the argument binding turns
an object into its YAML/JSON text.
