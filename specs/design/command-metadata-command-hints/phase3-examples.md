# Phase 3: Examples and tests

1. Build `CommandMetadata::new("export")`, set `hints["toolbar"] = true`, insert it in `CommandMetadataRegistry`, and assert JSON/YAML round-trip preserves the boolean.
2. Register a no-argument command with `register_command!(..., hint icon: "download")`; assert the resulting metadata contains `"icon": "download"` and has no argument workaround.
3. Compile a declaration with the same command hint key twice and assert the macro diagnostic rejects it.
4. Deserialize and reserialize `specs/command_registry.yaml`; retain the existing byte-identical assertion proving empty maps are omitted.

Place metadata tests in `liquers-core/src/command_metadata.rs` and macro integration/compile tests alongside existing registration tests. Use descriptive focused test names and one behaviour per test. Run `cargo test -p liquers-core command_metadata` and the relevant `liquers-macro` test target, then the registry export check.
