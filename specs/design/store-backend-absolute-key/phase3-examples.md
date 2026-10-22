# Phase 3: Examples & Use-cases — store-backend-absolute-key

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | A backend with no checks of its own | the facade refuses before the backend runs | AC-1 |
| 2 | Example | A decorator | forwards `&*key` to an inner `dyn AsyncStore` | AC-4 |
| 3 | Test | `store::tests::facade_refuses_relative_key_before_backend` | every fallible method, direct call | AC-1, AC-2 |
| 4 | Test | `store::tests::facade_is_supported_is_false_for_relative_key` | routing hint | AC-3 |
| 5 | Test | `store::tests::facade_passes_absolute_key_unchanged` | the backend sees the caller's key | AC-4 |
| 6 | Doc test | `AbsoluteKey` (`query.rs`) | constructor refuses; no other constructor | AC-5 |
| 7 | Doc test | `AsyncStore` (`store.rs`) | sealed: a direct impl does not compile | AC-6 |
| 8 | Suite | existing store, conformance, web and records tests | unchanged assertions | AC-4 |
| 9 | Check | caller crates build with no diff | | AC-7 |

## Example 1: a backend with no checks of its own

```rust
use liquers_core::store::{StoreBackend, AsyncStore};
use liquers_core::query::AbsoluteKey;

struct DirStore { root: PathBuf }

#[async_trait]
impl StoreBackend for DirStore {
    fn key_prefix(&self) -> Key { Key::new() }
    async fn get(&self, key: AbsoluteKey<'_>) -> Result<(Vec<u8>, Metadata), Error> {
        let bytes = tokio::fs::read(self.root.join(key.encode())).await
            .map_err(|e| Error::key_read_error(&key, "dir", &e))?;
        Ok((bytes, Metadata::new()))
    }
    async fn set_metadata(&self, key: AbsoluteKey<'_>, _m: &Metadata) -> Result<(), Error> {
        Err(Error::key_not_supported(&key, "dir"))
    }
}

let store: Arc<dyn AsyncStore> = Arc::new(DirStore { root });
let err = store.get_bytes(&parse_key("a/../../etc/passwd")?).await.unwrap_err();
assert_eq!(err.error_type, ErrorType::KeyNotAbsolute);   // DirStore::get never ran
```

The backend cannot call its own `get` with an unchecked key either: `AbsoluteKey` has no public
constructor other than `AbsoluteKey::new`.

## Example 2: a decorator

`ReadOnlyStore` becomes:

```rust
impl StoreBackend for ReadOnlyStore {
    async fn get(&self, key: AbsoluteKey<'_>) -> Result<(Vec<u8>, Metadata), Error> {
        self.inner.get(&key).await          // inner: Arc<dyn AsyncStore>; &key derefs to &Key
    }
    async fn set(&self, key: AbsoluteKey<'_>, _: &[u8], _: &Metadata) -> Result<(), Error> {
        Err(read_only_error(&key))
    }
}
```

The inner store checks again, which costs one scan of the segments. Router members are called the
same way.

## Edge and Error Cases

| Case | Expected | Test |
|---|---|---|
| `rename_data("a/../b", "c")` and `("c", "../d")` | `KeyNotAbsolute` naming the relative key, `from` first | Test 3 |
| interior `..` (`a/../b`), which needs no CWD | refused, as today (`is_relative` checks every segment) | Test 3 |
| `.hidden`, `a..b` | accepted, as today | Test 5 |
| empty key (root) | accepted by the facade; the backend decides as today | Test 5 |
| empty segment (`liquers-web`) | still `KeyNotSupported` from the reduced `check_key` | existing web tests |
| a listed name `..` reaching the `listdir_keys_deep` default | refused by `AbsoluteKey::new` on the joined key, as today | existing `keyshape` rules |
| wasm32 (`async_trait(?Send)`) | the blanket impl compiles for both attributes | `liquers-web` loop |

## Test Plan

- Unit: `liquers-core/src/store.rs` `mod tests` — a private `Probe` backend implementing
  `StoreBackend` that counts entries into each method and records the last key:
  - `facade_refuses_relative_key_before_backend` (AC-1, AC-2): for each fallible key-taking
    method, called on `&dyn AsyncStore` with `a/../../etc`, assert `KeyNotAbsolute` and an entry
    count of 0; `rename_data` with each argument relative in turn.
  - `facade_is_supported_is_false_for_relative_key` (AC-3): `false`, entry count 0.
  - `facade_passes_absolute_key_unchanged` (AC-4): `data/x.txt`, root and `a/.hidden` reach the
    probe equal to the caller's key.
- Doc tests: `liquers-core/src/query.rs` on `AbsoluteKey` (AC-5): `new` on `../x` returns
  `KeyNotAbsolute`; a `compile_fail` block constructing `AbsoluteKey(&key)` from outside the crate.
  `liquers-core/src/store.rs` on `AsyncStore` (AC-6): a `compile_fail,E0277` block implementing
  `AsyncStore` for a local type.
- Existing suites (AC-4): `cargo test -p liquers-core --lib store`,
  `cargo test -p liquers-core --features store-conformance --test store_conformance_CONF`,
  `cargo test -p liquers-store`, `cargo test -p liquers-records --lib provider`, and the
  `liquers-web` Node loop.
- Callers (AC-7): `cargo check -p liquers-lib -p liquers-axum -p liquers-py -p liquers-mcp -p
  liquers-agent`, and `git diff --stat -- liquers-lib liquers-axum liquers-py liquers-mcp
  liquers-agent` is empty.
- Command to run the new tests: `cargo test -p liquers-core --lib store::tests::facade && cargo test -p liquers-core --doc AbsoluteKey AsyncStore`.

## Learning Log

- `async_trait` rewrites elided lifetimes in argument types, so `key: AbsoluteKey<'_>` needs no
  named lifetime on the trait methods; verify on the first method converted (Phase 4 Step 2).
- `Key::as_absolute` stays as the non-store form of the same check; the facade and it share
  `AbsoluteKey::new`, so there is still exactly one predicate (`Key::is_relative`).
