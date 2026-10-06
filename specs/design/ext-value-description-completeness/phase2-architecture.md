# Phase 2: Solution and Architecture

## Test structure (`liquers-lib/tests/value_type_system.rs`)

```rust
/// Every `ExtValue` variant must appear here. A new variant fails to compile until it is added,
/// which is the point.
fn sampled(value: &ExtValue) -> bool {
    match value {
        ExtValue::Image { .. } => true,
        #[cfg(feature = "polars")]
        ExtValue::PolarsDataFrame { .. } => true,
        #[cfg(feature = "egui")]
        ExtValue::UiCommand { .. } | ExtValue::Widget { .. } => true,
        ExtValue::UIElement { .. } => true,
        // Integration-owned identifier, registered by the integrating crate, not by `type_descriptions()`.
        ExtValue::Foreign { .. } => false,
        #[cfg(feature = "records")]
        ExtValue::RecordView { .. } | ExtValue::RecordSource { .. } => true,
    }
}

fn samples() -> Vec<ExtValue> { /* one per variant, gated */ }
```

The test body:
1. for each sample with `sampled(&v)`: assert that `identifier(v)` is in `described`;
2. collect the sample identifiers, and assert that every `described` identifier is in that set
   (reverse check).

If `UIElement` / `Widget` samples cannot be built cheaply, `samples()` omits them. The reverse
check then needs an allowance list naming those identifiers, with the reason. Prefer real samples.

The file's `#![cfg(...)]`: `Image` requires `image-support`. Check the file's current gating and
keep it.

## Known-issue preflight

None.

## Relevant commands

None.

## Documentation architecture

TYPE_SYSTEM_GUIDE sentence, with History and `reviewed:`.

## Risk table

| Area | Assessment |
|---|---|
| Likely files | `liquers-lib/tests/value_type_system.rs`; the guide |
| Risk | Test might reveal a real missing `TypeInfo`. Then fix the `TypeInfo`, which is in scope as the test's purpose, or file an issue if it is non-trivial. |
| Certainty | High |
