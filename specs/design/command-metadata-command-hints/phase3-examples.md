# Phase 3: Examples and tests

1. Build `CommandMetadata::new("export")`, set `hints["toolbar"] = true`, insert it into a
   `CommandMetadataRegistry`, and assert that a JSON/YAML round trip keeps the boolean.
2. Register a no-argument command with `register_command!(..., hint icon: "download")`, and assert
   that its metadata contains `"icon": "download"`.
3. Compile a declaration with the same command hint key twice, and assert that the macro rejects
   it (compile-fail test if `trybuild` is available, otherwise a parser unit test in
   `liquers-macro`).
4. Assert that `serde_json::to_string(&CommandMetadata::new("x"))` contains no `"hints"` key, and
   that `cargo test -p liquers-lib --test registry_export` passes **without** regenerating the
   committed registry. That proves empty maps change no signature.

Metadata tests go in `liquers-core/src/command_metadata.rs`. Macro tests go next to the existing
registration tests. Run `cargo test -p liquers-core --lib command_metadata`, `cargo test -p liquers-macro`,
and the registry export test.
