# Phase 3: Examples and Tests

## Examples

`{"name":"greet", ...}` deserializes with conventional `state`; the same object with
`"state_argument": null` is a source command. Serializing the source form writes the null back so
deserialization cannot reinterpret it as the newly documented conventional default.

## Tests

1. Add JSON and YAML omission tests in `command_metadata.rs` comparing with
   `CommandMetadata::new`; add explicit-null tests asserting `None`.
2. Round-trip the committed registry and prove its explicit state arguments and bytes are stable.
3. In declaration tests, cover convention-derived transforming commands and explicit source
   commands so the declaration layer continues writing intent before raw serde conversion.
4. Add or extend a `plan.rs` unit test showing the omitted form consumes predecessor state and the
   explicit-null form does not.

Run `cargo test -p liquers-core --lib`, the registry freshness test, and
`bash scripts/check-build-matrix.sh`. Tests use `Result` where parsing uses `?`; no async environment
is required unless the existing plan harness already uses one.
