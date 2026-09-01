# Phase 3: Examples and Tests

## Examples

`Error::general_error("Python command failed").with_traceback(rendered)` produces an error log
entry whose message and position are unchanged and whose `traceback` equals `rendered`. An ordinary
error produces `traceback: None`. Error JSON written before the new field still deserializes.

## Tests

1. In `liquers-core/src/error.rs`, construct with/without traceback, serialize to the existing flat
   object, deserialize legacy JSON without the field, and retain the pointer-size assertion.
2. In `liquers-core/src/metadata.rs`, test `LogEntry::from_error` copies traceback, query, and
   position together and leaves traceback absent when not supplied.
3. Round-trip `MetadataRecord::from_error` through JSON and YAML to prove the existing log-entry
   field persists the value.

Use synchronous inline unit tests returning `Result` when `?` improves parsing. Run
`cargo test -p liquers-core --lib error`, `cargo test -p liquers-core --lib metadata`, and the full
core library suite.
