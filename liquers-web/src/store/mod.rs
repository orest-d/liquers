//! `STORE` — browser stores and declarative store composition.
//!
//! Implements the `STORE` feature of `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` for the browser,
//! which `specs/design/liquers-web/` deferred. Design: `specs/design/liquers-web-store/`.
//!
//! Four stores behind the existing [`liquers_core::store::AsyncStoreRouter`]:
//!
//! | Store | Config `type` | Writes |
//! |---|---|---|
//! | `LocalStorageStore` | `localstorage` | yes — the full `AsyncStore` contract |
//! | `FetchStore` | `http` / `https` | no — read-only over HTTP |
//! | `JsStore` | `js` | whatever the page object implements |
//!
//! The router itself is reused unchanged: it is already `?Send` on wasm, and routing is "the first
//! store whose `key_prefix` matches *and* whose `is_supported` returns true".
//!
//! # Two things every store here must do
//!
//! - **Override `is_supported`.** It defaults to `false`, and the router consults it, so a store
//!   that forgets is silently never selected.
//! - **Implement `set_metadata`.** It is the one `AsyncStore` method with no default.
//!
//! # Why parts of this module are free functions
//!
//! `localStorage` does not exist under Node, so anything touching it can only be tested in a real
//! browser. The logic that can *silently corrupt or misroute data* is therefore deliberately
//! expressed as pure functions over plain data — [`encoding`], [`key_guard`], and later the URL
//! builder and metadata inference — so it is covered by the fast Node loop rather than by the
//! browser suite. That is a design requirement, not a testing convenience; see
//! `specs/design/liquers-web-store/phase3-examples.md`.

pub mod encoding;
pub mod key_guard;

pub use encoding::{decode_envelope, encode_envelope, ByteEncoding};
pub use key_guard::check_key;
