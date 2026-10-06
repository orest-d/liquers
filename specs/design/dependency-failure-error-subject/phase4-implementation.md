# Phase 4: Implementation Plan

1. Add `AssetRef::expiry_subject` (Phase 2 §1). Proof: `cargo check -p liquers-core`.
2. Change both messages in `DefaultAssetManager::wait_for_dependency` (Phase 2 §2). Proof: T1, T2.
   Agent: haiku tier; skills rust-best-practices.
3. Add T1, T2. Run `cargo test -p liquers-core --lib dependency` and `--lib assets`.
4. Close the source issue with a resolution note, set this design's lifecycle, regenerate and
   check the index.
5. Diff review: two message sites, one helper, tests.
