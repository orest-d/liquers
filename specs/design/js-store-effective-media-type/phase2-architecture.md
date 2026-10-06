# Phase 2: Solution and Architecture

```rust
/// Resolves with the key's effective asset info: what the system derives from the metadata —
/// the effective `media_type`, `data_format`, `filename` — not the raw record.
#[wasm_bindgen(js_name = getAssetInfo)]
pub fn get_asset_info(&self, key: &str) -> js_sys::Promise {
    let inner = self.inner.clone();
    let key = key.to_string();
    promise(async move {
        let key = key_of(&key)?;
        asset_info_to_js(&inner.get_asset_info(&key).await?)
    })
}
```

`asset_info_to_js(info: &AssetInfo) -> Result<JsValue, Error>` lives in `store/mod.rs` next to
`metadata_to_js_value`, with the same serializer configuration.

## Known-issue preflight

None.

## Relevant commands

None.

## Documentation architecture

README store method list. `tests/stubs/valid_usage.ts` gains a `getAssetInfo` usage line (checked
by STUBS02 `tsc`).

## Risk table

| Area | Assessment |
|---|---|
| Likely files | `liquers-web/src/store/{wrapper.rs, mod.rs}`; `tests/stubs/valid_usage.ts`; a web test; README |
| Compatibility | Additive |
| Build | wasm32-only crate: follow CLAUDE.md's web loop (after `cargo clean`) |
| Certainty | High |
