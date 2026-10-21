# Phase 2: Solution & Architecture — store-backend-absolute-key

## Overview

Split the store trait in two. **`StoreBackend`** is what a backend implements. Each fallible
key-taking method receives an `AbsoluteKey<'_>`, a borrowed proof that the key was checked.
**`AsyncStore`** keeps its current signatures (`&Key`) and is what every caller and every
`dyn AsyncStore` uses. It is implemented once, by a blanket impl over `StoreBackend` that converts
`&Key` to `AbsoluteKey` (or returns `KeyNotAbsolute`) and delegates. `AsyncStore` is sealed, so the
blanket impl is the only implementation outside `liquers-core`.

Rejected:

- **`AsyncStore` takes `&AbsoluteKey` (the issue's proposal).** Every caller converts, including
  roughly 100 call sites in `assets.rs` and the axum handlers, and the routers and `AssetManager`
  would have to decide whether to carry the new type. The guarantee is the same, at several times
  the change.
- **An owned `AbsoluteKey(Key)` with a `&Key → &AbsoluteKey` cast.** The cast needs
  `#[repr(transparent)]` plus `unsafe` (or a new dependency), and the alternative is a clone per
  store call.
- **A checking decorator applied by the factory**: a directly held store bypasses it. **Status
  quo plus conformance tests**: the convention the issue replaces.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `STORE-WRITE-HAS-NO-PRECONDITION` (`store-conditional-write`) | draft (design in_review) | P2 | Adds `set_if`, `write_atomicity` to `AsyncStore` | no | no | The second to land mirrors them in both traits |
| `CORE-STORE-OPENBIN-MISSING` | accepted | P3 | A future key-taking method; under this design it is written once on `StoreBackend` with `AbsoluteKey` | no | no | None |
| `LANGUAGE-STORE-TYPE-NOT-DEFINABLE` | draft | P2 | A language-defined store adapter would implement `StoreBackend` | no | no | None |
| `RESOURCE-NAME-ASCII-ONLY` (`resource-name-unicode`) | draft | P2 | Changes each backend's path mapping | no | no | Land this first (Phase 1 dependencies) |

## Interfaces

`liquers-core/src/query.rs`:

```rust
/// A key a store may address: every segment checked by `Key::is_relative`. The only way to
/// make one is `new`, which is the single absoluteness check behind `AsyncStore`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AbsoluteKey<'a>(&'a Key);          // field private
impl<'a> AbsoluteKey<'a> {
    pub fn new(key: &'a Key) -> Result<Self, Error>;   // Error::key_not_absolute
    pub fn key(self) -> &'a Key;
}
impl std::ops::Deref for AbsoluteKey<'_> { type Target = Key; }
impl std::fmt::Display for AbsoluteKey<'_>;            // as Key
```

`Key::as_absolute` and `try_into_absolute` stay (other callers use them). `as_absolute` is
reimplemented as `AbsoluteKey::new(self).map(AbsoluteKey::key)`.

`liquers-core/src/store.rs`:

```rust
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait StoreBackend: MaybeSend + MaybeSync {
    // non-key and infallible methods: unchanged signatures
    fn store_name(&self) -> String;  fn key_prefix(&self) -> Key;
    fn default_metadata(&self, key: &Key, is_dir: bool) -> MetadataRecord;
    fn finalize_metadata(&self, …, key: &Key, …);  fn finalize_metadata_empty(…);
    fn data_capabilities(&self) -> DataCapabilities;  fn is_read_only(&self) -> bool;
    fn version_verification(&self, key: &Key) -> Option<VersionVerification>;
    async fn keys(&self) -> Result<Vec<Key>, Error>;  async fn sync(&self) -> Result<SyncReport, Error>;
    // fallible key-taking methods: `key: AbsoluteKey<'_>`
    async fn get(&self, key: AbsoluteKey<'_>) -> Result<(Vec<u8>, Metadata), Error>;
    async fn set_metadata(&self, key: AbsoluteKey<'_>, metadata: &Metadata) -> Result<(), Error>;
    async fn rename_data(&self, from: AbsoluteKey<'_>, to: AbsoluteKey<'_>) -> Result<(), Error>;
    fn is_supported(&self, key: AbsoluteKey<'_>) -> bool;
    // … and get_bytes, get_metadata, get_asset_info, set, remove, removedir, contains, is_dir,
    // listdir, listdir_keys, listdir_asset_info, listdir_keys_deep, makedir, get_data, set_data,
    // remove_data, listdir_data, data_facts, get_stored_metadata — same defaults as today
}

mod sealed { pub trait Sealed {} impl<T: super::StoreBackend + ?Sized> Sealed for T {} }

pub trait AsyncStore: sealed::Sealed + MaybeSend + MaybeSync { /* today's signatures, no defaults */ }

impl<T: StoreBackend + ?Sized> AsyncStore for T { /* each fallible method:
    StoreBackend::get(self, AbsoluteKey::new(key)?).await ;
    is_supported: AbsoluteKey::new(key).is_ok_and(|k| StoreBackend::is_supported(self, k)) */ }
```

The defaults move from `AsyncStore` to `StoreBackend` unchanged in behaviour. Default bodies that
call sibling methods (`get_bytes` → `get`, `listdir_keys` → `listdir`, `listdir_keys_deep` →
`is_dir` on joined keys) call the `StoreBackend` methods. A key built by `join` inside a default
is checked with `AbsoluteKey::new` before use, so a listed name `..` is refused exactly as today.
`dyn AsyncStore` stays object-safe: the trait has no generic methods.

## Integration Points

- `liquers-core/src/store.rs`: traits, blanket impl. `SharedStore`, `NoAsyncStore`,
  `AsyncMemoryStore`, `AsyncFileStore`, `AsyncStoreRouter` → `impl StoreBackend`. Their
  `key.as_absolute()?` lines are deleted. `AsyncFileStore::key_to_path`, `key_to_path_metadata` and
  `key_to_lock_path` take `AbsoluteKey<'_>` and keep `reject_reserved`. The router keeps its
  `find_store(&Key)` and calls members through `AsyncStore`.
- `liquers-core/src/store/read_only.rs`, `store/pipeline.rs` (`check_key` loses its absoluteness
  half), `store/test_support.rs` (the `forward_with_hook!` macro), `store/writes.rs` callers if they
  take `&Key` from a backend method.
- `liquers-core/src/store_conformance/mod.rs` test stores, and every `impl AsyncStore` in
  `liquers-core/src/{assets,recipes}.rs` test modules and `liquers-core/tests/*.rs`.
- `liquers-store/src/opendal_store.rs`: `impl StoreBackend for AsyncOpenDALStore`; path builders
  take `AbsoluteKey<'_>`.
- `liquers-web/src/store/{local_storage,fetch,js_store}.rs` → `StoreBackend`. `key_guard::check_key`
  keeps only the empty-segment check, taking `AbsoluteKey<'_>`.
- `liquers-records/src/provider.rs` test stores.
- Callers (`assets.rs`, `liquers-axum`, `liquers-lib`, `liquers-py`, `liquers-records` sources,
  `liquers-mcp`, `liquers-agent`): no change (AC-7).

## Error Handling

`AbsoluteKey::new` returns `Error::key_not_absolute(key)`, which is what every method returns today.
When both keys of `rename_data` are relative, `from` is reported first, matching the current
`AsyncFileStore` order. `is_supported` stays infallible and returns `false`. No new `ErrorType`.

## Relevant Commands

None. No command signature changes; `specs/command_registry.yaml` is untouched.

## Documentation Architecture

| Document | Change |
|---|---|
| `specs/reference/STORE_SEMANTICS.md` §7 | "enforced by the type": backends receive `AbsoluteKey`; remove the convention sentence and the `STORE-ABSOLUTE-KEY-NOT-TYPE-ENFORCED` pointer |
| `specs/guides/STORE_IMPLEMENTATION_GUIDE.md` | implement `StoreBackend`; the "twenty defaults" text applies to it; drop "call `as_absolute` first" |
| `specs/reference/PROJECT_OVERVIEW.md` §5 | the two traits, one line each |
| `specs/reference/api/DOC_02_QUERY_LANGUAGE_REFERENCE.md` §Relative keys | `AbsoluteKey` as the store-facing form of `as_absolute` |
| `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` STORE05 | note that a Rust backend passes by construction; the test stays for language-defined stores |
| `CLAUDE.md` §Adding a Store Backend | step 1: implement `StoreBackend` |
| `liquers-core/src/store.rs` module docs `# A store key is absolute` | rewritten around the facade |

Each reference and guide change adds a `## History` row and bumps `reviewed:`.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | those under Integration Points: about 25 files in five crates |
| Crates and workflows affected | every store backend; no query, asset or command behaviour |
| Existing tests likely to change | only the `impl` headers and parameter types of test stores; assertions unchanged (AC-4) |
| New validation | facade tests (Phase 3), `compile_fail` doc tests, the feature matrix and wasm32 build (`liquers-web`) |
| Compatibility | API break for backend authors: `impl AsyncStore` → `impl StoreBackend`. No caller, wire, serde or binding change |
| Concurrency / performance | one `is_relative` scan per call, plus one more per decorator layer; negligible beside I/O |
| Security | strictly stronger. The file store's path builders keep their own checks via the `AbsoluteKey` parameter, so the dangerous conversion still cannot be reached unchecked |
| Recovery | one PR; revert as a unit. A partly migrated tree does not compile, so there is no half-landed state |
| Certainty and open questions | high on feasibility (`async_trait` supports elided lifetimes in argument types; sealed blanket traits are standard). Open: the three Phase 1 decisions |
