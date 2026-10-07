# Phase 5: Documentation

**Status: executed 2026-10-07; awaiting approval.**

## Summary

Implemented 2026-10-07 as Wave 3 step 21 of
`archive/2026-10-06-p2-p3-s-implementation-order-revised.md`, on the maintainer decision of
2026-10-06 (remove the field). M3 is still undecided, so the registry is regenerated for this
change alone; M3 will regenerate it again.

- `liquers-core/src/command_metadata.rs`: field, doc, both `cache: true` initialisers and the now
  unused `true_default` removed.
- `liquers-core/src/command_declaration.rs` (test assertion), `liquers-core/tests/command_declaration.rs`
  (`int04b` now checks the `volatile` default instead), `liquers-lib/src/egui/widgets.rs` (flags
  line), `liquers-py/src/command_metadata.rs` (getter).
- `specs/command_registry.yaml` regenerated: 108 `cache: true` deletions and a CHANGELOG line, no
  other change.
- Tests: `command_metadata_ignores_legacy_cache_field` (T1 and T2 in one test); the existing
  `test_command_metadata_without_payload_field_deserializes` also still loads a legacy `"cache"`
  key. `registry_export` (T3). `cargo check -p liquers-py` and the build matrix (T4).

## Conformance and deviations

As designed. Phase 2 did not list `liquers-core/tests/command_declaration.rs`, which also read the
field; its assertion now checks another default. The serialized-form test Phase 2 mentioned is a
deserialization fixture, so it was kept unchanged as legacy-input evidence.

## Documentation

`reference/COMMAND_DECLARATION.md`: `cache` removed from the key table and the defaults table,
the §2.1 example and the Stage 4 walk-through. History row, `reviewed:` bumped.

## New issues

None. Building `liquers-py` here needed `PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1`, which is the
already-filed `PY-PYO3-REJECTS-PYTHON-3-13`.

## Validation

`cargo test -p liquers-core --lib command_metadata`; `cargo test -p liquers-core --test
command_declaration`; `cargo test -p liquers-lib --test registry_export`;
`PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1 cargo check -p liquers-py`; `bash scripts/check-build-matrix.sh`;
`python3 scripts/docs_index.py --check`.
