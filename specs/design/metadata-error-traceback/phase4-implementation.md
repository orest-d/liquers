# Phase 4: Implementation Plan

All steps done on branch `claude/metadata-error-traceback-o27ook` (2026-10-11). Two additions in
scope: `MetadataRecord::with_error` now logs through `LogEntry::from_error` (otherwise
`MetadataRecord::from_error` — Phase 3 test 3 — would not carry the traceback), and the axum
`error_to_detail` fills `error.traceback`, whose comment the new field made false.

1. Extend `ErrorPayload` in `liquers-core/src/error.rs` with serde-defaulted
   `traceback: Option<String>`, initialize it in `ErrorPayload::new`, and add a consuming
   `Error::with_traceback`. Add Phase 3 wire/size tests; rollback removes only the additive field.
2. Update `LogEntry::from_error` in `liquers-core/src/metadata.rs` to clone the optional rendered
   traceback into `LogEntry::with_traceback`; add conversion and metadata round-trip tests.
3. Review language binding error conversion call sites but do not invent frame extraction in this
   issue. Record follow-up ownership under `LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT`.
4. Update affected error/payload reference text only if it claims exhaustive serialized fields;
   update the source issue and design lifecycle during implementation.
5. Run formatting, focused and full core tests, `cargo clippy -p liquers-core --all-targets`, and
   docs-index checks. Review the final diff for a changed flat wire shape, `Error` size regression,
   unrelated binding work, debug output, and generated-file churn.
