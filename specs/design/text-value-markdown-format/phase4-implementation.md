# Phase 4: Implementation Plan - Markdown as a `Text` Data Format

1. **Registry.** `liquers-core/src/value.rs`, `Value::type_descriptions`: add
   `.with_data_formats(["md"])` to the `Text` entry only. Update the comment above `TEXTUAL`
   with one line saying `md` is `Text`-only. Proof: `cargo test -p liquers-core --lib value::`
   (registry tests). Containment: one line.
2. **Core serializer.** Same file: separate `"md"` arm in `as_bytes` (Text only, others
   `SerializationError` via `Error::from_error`); add `"md"` to the text arm of
   `deserialize_from_bytes`. Depends on 1 for the registry tests to agree.
   Proof: updated `scalar_identifiers_round_trip_through_the_serializer`, new
   `markdown_is_text_only`.
3. **Lib serializer.** `liquers-lib/src/value/simple.rs`: separate `"md"` arm in `as_bytes`;
   `"md"` added to the `"txt" | "html" | "toml"` read arm; test expected-value arm learns `"md"`;
   add `simple_value_text_round_trips_as_markdown`. Depends on 1 (shared `TypeInfo`).
   Proof: `cargo test -p liquers-lib --lib value::simple`.
4. **Integration test.** Add `liquers-core/tests/text_markdown_storage.rs` (Phase 3). First
   confirm it fails with steps 1-3 stashed, then passes. Proof:
   `cargo test -p liquers-core --test text_markdown_storage`.
5. **Docs.** Review `specs/reference/VALUE_TYPE_SYSTEM.md` for a per-type format list; if `Text`
   formats are enumerated, add `md`, with a `## History` row and `reviewed:` bump. Close
   `specs/issues/TEXT-VALUE-CANNOT-BE-STORED-AS-MARKDOWN.md` with a resolution naming the tests.
   Regenerate and check the index.
6. **Checks.** `cargo fmt`; `cargo test -p liquers-core --lib --tests`;
   `cargo test -p liquers-lib --lib --tests`; `bash scripts/check-build-matrix.sh` is not needed
   (no `cfg` touched) — run `cargo test -p liquers-lib --no-default-features --lib --tests` once
   as the minimal-feature check of `simple.rs`. Review the diff: `TEXTUAL` unchanged, no scalar
   gains `md`, `UNWRITABLE` unchanged, no `println!`.

## Final Review

All phases agree. The only behaviour change beyond the requested write is untyped `.md` reads
becoming `Text`, recorded in Phase 1. Rollback: revert steps 1-3 together (the registry and the
serializers must agree, which the existing registry/serializer agreement tests enforce).

## Post-Phase-4 Review (2026-10-05)

- **Problem still valid:** yes. `md` is absent from both serializers (`value.rs` ≈944/≈1010,
  `simple.rs` ≈556/≈649) and from `Text`'s `TypeInfo`.
- **Solution correct:** yes. Declaring `md` on `Text` only (not in `TEXTUAL`), with separate write
  arms, is correct and keeps `UNWRITABLE` unchanged.
- **Unnecessary:** none.
- **Detail / tests:** sufficient. The integration test proves the write path the issue is about.
  Note: `simple.rs` has a third `"txt" | "html"` match (≈1037, the test's expected-value table),
  which Phase 2 does name. Make sure the implementer edits that one and not only the two
  serializer arms.
- **Interactions:** none among the ready designs (consumer: `AGENT-MEMORY-SERVICE`).
- **Verdict:** ready.

**Resolution (2026-10-05):** the findings above are incorporated into Phases 1-4.
`phase5-documentation.md` holds the documentation plan; where a Phase 4 step names documentation
work, that plan is the authoritative list.
