# Phase 2: Solution and Architecture - Compiler-Checked `ErrorType` List for OBJECT06

## Chosen Solution

In `liquers-web/tests/objects_OBJECT.rs`, replace the hand-written `const ALL_ERROR_TYPES` with:

```rust
/// Expands one list of variants into `ALL_ERROR_TYPES` and an exhaustive `match` over the same
/// list. A variant added to `ErrorType` and not here makes `listed` non-exhaustive — a compile
/// error — so the list cannot drift silently.
macro_rules! error_types {
    ($($variant:ident),+ $(,)?) => {
        const ALL_ERROR_TYPES: &[ErrorType] = &[$(ErrorType::$variant),+];

        #[allow(dead_code)]
        fn listed(t: ErrorType) {
            match t {
                $(ErrorType::$variant => {})+
            }
        }
    };
}

error_types!(
    ArgumentMissing, ActionNotRegistered, CommandAlreadyRegistered, ParseError, ParameterError,
    TooManyParameters, ConversionError, SerializationError, General, CacheNotSupported,
    UnknownCommand, NotSupported, NotAvailable, KeyNotFound, KeyNotSupported, KeyNotAbsolute,
    KeyReadError, KeyWriteError, UnexpectedError, ExecutionError, DependencyVersionMismatch,
    DependencyCycle, StatusConflict, Cancelled,
);
```

In `object06_every_enum_variant_roundtrips`, delete the `assert_eq!(ALL_ERROR_TYPES.len(), 22, …)`
line. The distinct-names check already present catches a variant listed twice (two equal names).

## Rejected Alternatives

- **Change 22 → 23 (or 24).** Still a hand-kept number; the issue asks for derivation.
- **`strum::EnumIter` on `ErrorType`.** Adds a dependency to `liquers-core` for a test.
- **Exhaustive index function plus `assert!(index(ALL[i]) == i)`.** Catches mis-ordering but a
  newly added variant with an arm and no list entry still passes; weaker than the macro.

## Files and Symbols

`liquers-web/tests/objects_OBJECT.rs`: `ALL_ERROR_TYPES` (now macro-generated), new `listed`,
`object06_every_enum_variant_roundtrips` (count line removed). `error01_every_error_type_maps`
uses the list unchanged.

## Errors, Ownership, Sync/Async

N/A (test code). `ErrorType: Copy` (derive at `error.rs` ≈12), so the const slice is valid.

## Risk Table

| Aspect | Assessment |
|---|---|
| Likely files | `liquers-web/tests/objects_OBJECT.rs` |
| Affected workflows | `liquers-web` Node loop |
| Existing-test impact | OBJECT06 goes from red to green; ERROR01 gains a case |
| New validation | the Node loop; a scratch check that removing one macro entry fails to compile |
| Compatibility/data/security | none |
| Recovery | restore the literal list |
| Certainty | high |

## Review

Against Phase 1: criterion 1 by listing all 24; 2 by the generated match; 3 by deleting the count;
4 by running the loop. Against code: enum and both web mapping functions read at HEAD.
