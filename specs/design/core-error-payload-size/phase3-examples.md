# Phase 3: Examples and Tests

1. `size_of::<Error>() == size_of::<usize>()` and the containing `Result` stays below the lint
   threshold.
2. A fully populated error serializes to the same flat JSON object and deserializes identically.
3. `command_key` remains skipped; direct reads and assignments through deref compile and persist.
4. `ErrorPayload -> Error -> ErrorPayload` is lossless.
5. The same clippy command reports 715 to zero `result_large_err` warnings, with no new warnings.

Run core tests, dependent crate suites, Python tests, and the web wasm conformance loop as available.
