# Phase 2: Solution and Architecture

In the `#[wasm_bindgen(js_class = LiquersError)] impl LiquersError` block
(`liquers-web/src/error.rs`):

```rust
/// `new LiquersError(errorType, message, query?)`: a typed error a page can throw back at
/// Liquers. `errorType` is a name as `errorType` reports it (`"key_not_found"`); an unknown name is
/// a `TypeError`. A key is given as a query (`"-R/data/a.txt"`); one that does not parse is a
/// `TypeError`.
#[wasm_bindgen(constructor)]
pub fn construct(
    error_type: &str,
    message: String,
    query: Option<String>,
) -> Result<LiquersError, JsValue> {
    let Some(t) = error_type_from_name(error_type) else {
        return Err(js_sys::TypeError::new(&format!(
            "LiquersError: unknown error type '{error_type}'")).into());
    };
    // The type is data here, so the generic constructor is the right one (CLAUDE.md's
    // typed-constructor rule is for call sites that know their type).
    let mut error = Error::new(t, message);
    if let Some(text) = query {
        let parsed = parse_query(&text).map_err(|e| {
            JsValue::from(js_sys::TypeError::new(&format!(
                "LiquersError: invalid query '{text}': {e}")))
        })?;
        error = error.with_query(&parsed);
    }
    Ok(LiquersError::new(error))
}
```

`liquers_core::error` has no constructor that takes a runtime `ErrorType` other than `Error::new`
(checked 2026-10-06), so `Error::new` is used here as the documented exception: the type is data
chosen by the page. Confirm that `LiquersError::new(inner)` leaves `jsClass`/`jsStack` unset.

## Rejected alternatives

- Accept unknown names as `general`. That silently downgrades and breaks the forward-compatibility
  policy in `error_type_from_name`'s doc.
- A separate `key` argument: rejected by the maintainer; a key is represented as a query.
- An options object: one optional positional `query` is enough and maps directly to an optional
  TypeScript parameter.

## Risk Review

| Risk | Validation and recovery |
|---|---|
| Name clash with `new` | Rust name `construct`, JS name via `constructor` attribute |
| `TypeError` mapping | Test catches it in JS (`wasm_bindgen_test` with `js_sys::Reflect`/`instanceof`) |
| Stubs | `check-stubs.sh` STUBS02 compiles the usage line |
| Recovery | Remove the constructor |
