//! The JavaScript-facing `Store` wrapper's `getAssetInfo`: the effective asset info, as opposed to
//! the raw record `getMetadata` returns (`JS-STORE-WRAPPER-HAS-NO-EFFECTIVE-MEDIA-TYPE`, design
//! `specs/design/js-store-effective-media-type/`).
//!
//! Pure ECMAScript over a memory store, so these run under Node with no WebDriver:
//!
//! ```text
//! cargo test -p liquers-web --target wasm32-unknown-unknown --test store_wrapper_STORE
//! ```

#![cfg(target_arch = "wasm32")]

use std::sync::Arc;

use liquers_core::metadata::Metadata;
use liquers_core::parse::parse_key;
use liquers_core::query::Key;
use liquers_core::store::{AsyncMemoryStore, AsyncStore};
use liquers_web::store::wrapper::LiquersStore;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::*;

fn get(object: &JsValue, property: &str) -> JsValue {
    js_sys::Reflect::get(object, &JsValue::from_str(property)).expect("property read")
}

/// A wrapper over a memory store holding `data/input.csv`, written with empty metadata.
async fn wrapper() -> LiquersStore {
    let store = AsyncMemoryStore::new(&Key::new());
    store
        .set(
            &parse_key("data/input.csv").expect("key"),
            b"a,b\n1,2\n",
            &Metadata::new(),
        )
        .await
        .expect("set");
    LiquersStore::new(Arc::new(store))
}

/// The raw record has no `media_type` (no override was declared); the asset info has the
/// effective one.
#[wasm_bindgen_test]
async fn get_asset_info_reports_the_effective_media_type() {
    let store = wrapper().await;
    let info = JsFuture::from(store.get_asset_info("data/input.csv"))
        .await
        .expect("asset info");
    assert_eq!(
        get(&info, "media_type").as_string().as_deref(),
        Some("text/csv")
    );
    assert_eq!(get(&info, "is_dir").as_bool(), Some(false));

    // `getMetadata` is unchanged: the raw record, nothing derived written into it.
    let metadata = JsFuture::from(store.get_metadata("data/input.csv"))
        .await
        .expect("metadata");
    assert_ne!(
        get(&metadata, "media_type").as_string().as_deref(),
        Some("text/csv"),
        "getMetadata must stay the raw record"
    );
}

#[wasm_bindgen_test]
async fn get_asset_info_of_a_directory_is_a_directory() {
    let store = wrapper().await;
    let info = JsFuture::from(store.get_asset_info("data"))
        .await
        .expect("directory asset info");
    assert_eq!(get(&info, "is_dir").as_bool(), Some(true));
}

#[wasm_bindgen_test]
async fn get_asset_info_of_an_absent_key_rejects_with_key_not_found() {
    let store = wrapper().await;
    match JsFuture::from(store.get_asset_info("data/absent.csv")).await {
        Ok(info) => panic!("an absent key must reject, got {info:?}"),
        Err(rejection) => assert_eq!(
            get(&rejection, "errorType").as_string().as_deref(),
            Some("key_not_found")
        ),
    }
}
