# Phase 2: Solution and Architecture

## Test (`liquers-lib/tests/registry_export.rs`)

```rust
/// The committed registry's implementation versions match the code.
///
/// Separate from `committed_registry_is_fresh` (signatures) so the message says which kind of
/// change needs a regeneration: an edited command body changes `impl_version` only.
#[cfg(all(feature = "egui", feature = "image-support", feature = "polars", feature = "records"))]
#[tokio::test]
async fn committed_registry_impl_versions_are_fresh() -> Result<(), Error>
```

Body:

1. Load the committed registry (reuse `committed_registry_path` and `from_json_or_yaml`).
2. Build `current = full_registry()?` twice. Any command whose `impl_version` differs between the
   two builds is time-based (`version: now`), and the test fails with
   "`<cmd>` uses `version: now`, which cannot be committed; use `auto` or a fixed version".
3. For each command present in both registries (key sets are already checked by the existing
   test), collect those with differing `impl_version` and fail with the list and the regenerate
   command (the same text as the existing test's `regenerate`).

## CLAUDE.md

"Regenerate whenever a `register_command!` signature changes, a command is added or removed, **or
the body of a command with `version: auto` changes** (its implementation version is a hash of the
function)."

## Rejected alternatives

- Fold `impl_version` into `signature_of`. That loses the distinction in the message.
- Byte-compare the files. That fails on YAML formatting.

## Risk Review

| Risk | Validation and recovery |
|---|---|
| Contributor friction (comment edits) | The decided cost. The message gives the exact command. |
| False positive from `now` | Detected and explained (step 2) |
| Feature gating | Same `cfg` as the existing test |
| Recovery | Remove the test |
