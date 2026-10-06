# Phase 2: Solution and Architecture

In the `#[wasm_bindgen(js_class = LiquersError)] impl LiquersError` block
(`liquers-web/src/error.rs`):

```rust
/// `new LiquersError(errorType, message)`: a typed error a page can throw back at Liquers.
/// `errorType` is a name as `errorType` reports it (`"key_not_found"`); an unknown name is a
/// `TypeError`.
#[wasm_bindgen(constructor)]
pub fn construct(error_type: &str, message: String) -> Result<LiquersError, JsValue> {
    match error_type_from_name(error_type) {
        // The type is data here, so the generic constructor is the right one (CLAUDE.md's
        // typed-constructor rule is for call sites that know their type).
        Some(t) => Ok(LiquersError::new(Error::new(t, message))),
        None => Err(js_sys::TypeError::new(&format!(
            "LiquersError: unknown error type '{error_type}'")).into()),
    }
}
```

`liquers_core::error` has no constructor that takes a runtime `ErrorType` other than `Error::new`
(checked 2026-10-06), so `Error::new` is used here as the documented exception: the type is data
chosen by the page. Confirm that `LiquersError::new(inner)` leaves `jsClass`/`jsStack` unset.

## Rejected alternatives

- Accept unknown names as `general`. That silently downgrades and breaks the forward-compatibility
  policy in `error_type_from_name`'s doc.
- Accept an options object with `key`/`query` now. That waits on the decision, and is additive
  later.

## Risk Review

| Risk | Validation and recovery |
|---|---|
| Name clash with `new` | Rust name `construct`, JS name via `constructor` attribute |
| `TypeError` mapping | Test catches it in JS (`wasm_bindgen_test` with `js_sys::Reflect`/`instanceof`) |
| Stubs | `check-stubs.sh` STUBS02 compiles the usage line |
| Recovery | Remove the constructor |
