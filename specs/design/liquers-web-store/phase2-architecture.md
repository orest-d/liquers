# Phase 2: Solution & Architecture - Browser stores for liquers-web

## Overview

Four `AsyncStore` implementations behind the existing `AsyncStoreRouter`, built from the existing
`liquers_store::config` document. Two changes fall outside `liquers-web`: `liquers-store` gains an
`opendal` feature so it can be depended on from wasm, and its builder gains a **factory seam** so
`liquers-web` can contribute store types it does not own. Everything else is new code in
`liquers-web/src/store/`. No change to `AsyncStore`, `AsyncStoreRouter`, `Key`, `Metadata` or the
query language.

## Data Structures

### New Structs

#### `liquers-web/src/store/local_storage.rs`

```rust
/// A store backed by the browser's `localStorage`.
pub struct LocalStorageStore {
    /// Routing prefix. The prefix is *not* stripped here — see "Key mapping".
    prefix: Key,
    /// Entry-name namespace inside `localStorage`. Validated non-empty and `/`-free.
    namespace: String,
    /// Byte budget the store enforces itself. `None` is unlimited (Phase 1 Q3).
    quota_bytes: Option<u64>,
    /// Directory index and byte accounting. See "Concurrency" for why `RefCell` is sound here.
    state: RefCell<LocalState>,
}

struct LocalState {
    /// parent → child names. Mirrors `AsyncMemoryStore::dir_index` in role, not in type:
    /// single-threaded, so an ordered map beats `scc::HashMap`.
    dirs: BTreeMap<Key, BTreeSet<String>>,
    /// Directories created by `makedir` that hold no entries. Persisted as marker entries so
    /// they survive a reload.
    explicit_dirs: BTreeSet<Key>,
    /// Sum of stored envelope + metadata lengths, for quota enforcement.
    used_bytes: u64,
}
```

Ownership: everything owned. No `Arc` — the store is placed in the environment's
`Arc<dyn AsyncStore>` once and shared from there.

#### `liquers-web/src/store/fetch.rs`

```rust
/// A read-only store that fetches over HTTP.
pub struct FetchStore {
    prefix: Key,
    /// Absolute or page-relative URL prefix. Normalized at construction to end in `/`.
    url_prefix: String,
    /// The keys this store claims to have (Phase 1 Q5). Directory structure is derived from it,
    /// so `listdir`, `contains` and `is_dir` agree with each other without a network round trip.
    keys: BTreeSet<Key>,
    dirs: BTreeMap<Key, BTreeSet<String>>,
}
```

#### `liquers-web/src/store/js_store.rs`

```rust
/// Adapts a JavaScript object implementing the store protocol to `AsyncStore`.
pub struct JsStore {
    prefix: Key,
    name: String,
    /// The page's object. `!Send`/`!Sync`, which is why this crate is wasm32-only.
    obj: js_sys::Object,
    /// Which optional protocol methods the object actually provides, resolved once at
    /// construction so a missing method fails at registration rather than at first use.
    methods: JsStoreMethods,
}

struct JsStoreMethods {
    // Mandatory — construction fails without them.
    get: js_sys::Function,
    // Optional — `None` makes the corresponding `AsyncStore` method return `KeyNotSupported`.
    get_metadata: Option<js_sys::Function>,
    set: Option<js_sys::Function>,
    set_metadata: Option<js_sys::Function>,
    remove: Option<js_sys::Function>,
    removedir: Option<js_sys::Function>,
    contains: Option<js_sys::Function>,
    is_dir: Option<js_sys::Function>,
    listdir: Option<js_sys::Function>,
    makedir: Option<js_sys::Function>,
    is_supported: Option<js_sys::Function>,
}
```

#### `liquers-web/src/store/builder.rs`

```rust
/// Store types `liquers-store` does not know about: `localstorage`, `http`/`https`, `js`.
pub struct WebStoreFactory {
    /// Objects registered by name from JavaScript, so a `js` store can be named in a config
    /// document that is otherwise plain data.
    objects: HashMap<String, js_sys::Object>,
}
```

#### `liquers-web/src/store/wrapper.rs`

```rust
/// The store, visible to JavaScript. Wraps whatever the environment holds.
#[wasm_bindgen(js_name = Store)]
pub struct LiquersStore {
    inner: Arc<dyn AsyncStore>,
}
```

### New Enums

```rust
/// How a value's bytes are represented inside a `localStorage` string (Phase 1 Q3).
///
/// Recorded in the envelope at write time, never inferred at read time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ByteEncoding {
    /// The bytes are valid UTF-8 and are stored as text.
    Utf8,
    /// Anything else, base64-encoded.
    Base64,
}
```

Matches on `ByteEncoding` are exhaustive with no `_ =>` arm, per project rule, so a third encoding
is a compile error at every decode site rather than a silent misread.

### ExtValue Extensions

None. Stores traffic in `Vec<u8>` + `Metadata`; no value type is involved.

## Key mapping

Phase 1 said "each store strips its own `key_prefix()` before addressing its backend". That is
right for one store and wrong for the other two, so it is stated per store here:

| Store | Prefix stripped? | Why |
|---|---|---|
| `FetchStore` | **yes** | Required: the URL is `url_prefix` + key-minus-prefix, so a store at prefix `data` maps `data/x.csv` to `{url_prefix}x.csv`. |
| `LocalStorageStore` | **no** | Unnecessary: the `namespace` already separates this store's entries from everything else in `localStorage`. Keeping the whole key makes the entry name reversible, which is what lets the index be derived by scanning. |
| `JsStore` | **no** | The page object receives the key it would see in a query. Stripping would silently rewrite the addresses a page reasons about, and the page can strip its own prefix if it wants to. |

## The `localStorage` layout

Three entry kinds under one namespace. `namespace` is validated `/`-free at construction, so the
name parses back unambiguously by stripping `"{ns}/"` and taking the first `/`-delimited field.

| Entry name | Holds |
|---|---|
| `{ns}/d/{key}` | the data envelope |
| `{ns}/m/{key}` | `Metadata::to_json()` |
| `{ns}/D/{key}` | an empty-directory marker (empty value) |

**The envelope is a two-character prefix, not JSON:** `"1u"` + the text, or `"1b"` + base64.
Digit = format version, letter = `ByteEncoding`. JSON was rejected because it would escape the
whole payload a second time (a JSON string of text is larger *and* costs a parse on every read),
and because the envelope has exactly two fields, neither of which will grow — a version digit is
enough to add a third encoding later.

**The index is derived, never stored separately.** At construction the store enumerates
`localStorage` (`length()` / `key(i)`), selects entries under its namespace, and builds `dirs`,
`explicit_dirs` and `used_bytes`. There is deliberately no persisted index entry: a second source
of truth can disagree with the entries after a partial write, and reconciling it is a bug farm.
Empty directories are the one thing not derivable from data entries, which is exactly why they get
their own marker entry. Cost is one O(n) scan per store construction.

## The `fetch` URL and metadata inference

**URL** = `url_prefix` + the key with the store's `key_prefix` removed, segments joined by `/`.
Per Phase 1 Q6 this is a *verbatim* join, with no percent-encoding: the reachable URL space is
narrower than HTTP's, which is the accepted limitation. Segments that would change the URL's
meaning are refused by the key guard rather than escaped.

**Metadata precedence — extension first, response second.** For a key ending `.csv`, the metadata
says `text/csv` even when the server said `text/plain`.

| Source | Used when |
|---|---|
| `file_extension_to_media_type(key.extension())` | it returns something other than `application/octet-stream` |
| response `Content-Type` (parameters stripped) | the extension is absent or unrecognised |
| `application/octet-stream` | neither |

This inverts the usual web precedence on purpose. Liquers dispatches deserialization on
`data_format`, which derives from the extension; a static file server that labels everything
`text/plain` or `application/octet-stream` would otherwise break every command downstream of a
fetched asset. `file_extension_to_media_type` falls back to `application/octet-stream`
(`liquers-core/src/media_type.rs:138`), so "unrecognised" is detectable without a second table —
at the cost that a genuine `.bin` lets the response header win, which is harmless because the
answer is the same either way.

`get_metadata` issues **HEAD**, falling back to GET on 405 or 501, so metadata does not download
the body. Size comes from `Content-Length`, or from the body length when GET was used.

## The JavaScript store protocol

One vocabulary serves both directions: these are the methods `JsStore` calls on a page object, and
the methods `LiquersStore` exposes to a page. Keys cross as strings, data as `Uint8Array`, metadata
as a plain object (`Metadata` JSON). Every method may return a value or a Promise; the adapter
awaits thenables, reusing `is_thenable` and the `JsFuture` path already in
`liquers-web/src/command/adapter.rs:233`.

| Method | Required | Absent ⇒ |
|---|---|---|
| `get(key)` → `{data, metadata}` | **yes** | construction fails |
| `getMetadata(key)` → object | no | derived from `get` |
| `set(key, data, metadata)` | no | `set` → `KeyNotSupported` |
| `setMetadata(key, metadata)` | no | `set_metadata` → `KeyNotSupported` |
| `remove(key)` / `removedir(key)` | no | → `KeyNotSupported` |
| `contains(key)` / `isDir(key)` | no | → `KeyNotSupported` |
| `listdir(key)` → array of names | no | → `KeyNotSupported` |
| `makedir(key)` | no | → `KeyNotSupported` |
| `isSupported(key)` → bool (**sync**) | no | every key under `prefix` is supported |

**Absent optional methods error rather than take the trait default.** `AsyncStore`'s defaults for
`contains` and `listdir` are `false` and `[]` — permissive values that make a half-written store
look like an empty one, so `STORE03`'s listing invariants would pass vacuously. Failing loudly is
worth more than a default that lies. `isSupported` is the exception: it is sync (the trait method
is not `async`), and a store that answers "no" for everything is invisible to the router, so its
absence defaults to *supported* instead.

## Key guard (`STORE05`)

```rust
// liquers-web/src/store/key_guard.rs
/// Refuses key shapes that would escape a URL prefix or a storage namespace.
pub fn check_key(key: &Key, store_name: &str) -> Result<(), Error>;
```

Refuses any key with a segment that is `..`, `.`, or empty, with `Error::key_not_supported`.
Called from every `is_supported` **and** from each fallible method, because `is_supported` gates
*routing*, not direct calls: a store used without the router would otherwise skip the check.

Rejection, not normalization — a key is an address, and silently equating `a/../b` with `b` would
make two distinct addresses alias one asset.

The same hole exists in `AsyncFileStore` today and is exploitable there, which is a separate,
larger problem: filed as `specs/issues/STORE-FILESTORE-PATH-TRAVERSAL.md` (P1). This design does
not fix it — the helper lives in `liquers-web` for now, and that issue proposes hoisting a shared
version into `liquers_core::store` so every backend gets it.

## Trait Implementations

`AsyncStore` for `LocalStorageStore`, `FetchStore`, `JsStore`. Object-safe already; each impl is
attributed exactly as the existing ones are:

```rust
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl AsyncStore for LocalStorageStore { … }
```

**Every store must override `is_supported`** — it defaults to `false`
(`liquers-core/src/store.rs:461`), and `AsyncStoreRouter::find_store` requires it, so a store that
forgets is silently never selected. **Every store must implement `set_metadata`** — it is the one
method with no default (`liquers-core/src/store.rs:358`).

| Method | `LocalStorageStore` | `FetchStore` | `JsStore` |
|---|---|---|---|
| `get` | envelope + metadata entry | GET | delegate |
| `get_metadata` | metadata entry; dir → `default_metadata` | HEAD (GET fallback) | delegate or derive |
| `set` / `set_metadata` | write, quota-checked | `KeyNotSupported` | delegate or `KeyNotSupported` |
| `remove` / `removedir` | write + index update | `KeyNotSupported` | delegate or `KeyNotSupported` |
| `makedir` | marker entry | `KeyNotSupported` | delegate or `KeyNotSupported` |
| `contains` / `is_dir` / `listdir` | index | derived from `keys` | delegate or `KeyNotSupported` |
| `is_supported` | prefix + key guard | prefix + key guard | prefix + guard + optional `isSupported` |

`listdir_keys`, `listdir_asset_info`, `listdir_keys_deep`, `keys`, `get_bytes`, `get_asset_info`,
`default_metadata`, `finalize_metadata*` keep their trait defaults — they are all expressed in
terms of the methods above.

## Generic Parameters & Bounds

None of the three stores is generic: `AsyncStore` is used as `dyn AsyncStore`, and a generic store
could not be. `StoreFactory` (below) is likewise a plain object-safe trait with **no** `Send`/`Sync`
bound — the factory is transient and consumed during construction; only its product needs the
`MaybeSend + MaybeSync` that `AsyncStore` already requires. Adding bounds the call sites do not
need would make `WebStoreFactory` (which holds `js_sys::Object`, `!Send`) unable to implement it.

## Sync vs Async Decisions

| Operation | Choice | Rationale |
|---|---|---|
| `LocalStorageStore` `AsyncStore` methods | `async`, but contain no `.await` | The Web Storage API is **synchronous**. `async` only satisfies the trait. |
| `FetchStore` methods | genuinely async | `JsFuture` over `fetch`. |
| `JsStore` methods | genuinely async | Protocol methods may return Promises. |
| `is_supported`, `store_name`, `key_prefix` | sync | Trait says so; all three are pure. |
| Store construction | sync | `LocalStorageStore::new` performs its index scan synchronously — the API is sync, so there is nothing to await. |
| `Environment.configureStore` | returns a `Promise` | Nothing to await today, but a future IndexedDB store will; changing a sync API to async later breaks every caller. Same reasoning as `init()`. |

That `LocalStorageStore` never awaits is load-bearing, not trivia: it is why `RefCell` is sound
(below) and why `STORE06` has a trivial answer.

## Function Signatures

```rust
// liquers-web/src/store/local_storage.rs
impl LocalStorageStore {
    pub fn new(prefix: &Key, namespace: &str, quota_bytes: Option<u64>) -> Result<Self, Error>;
    pub fn used_bytes(&self) -> u64;
}

// liquers-web/src/store/fetch.rs
impl FetchStore {
    pub fn new(prefix: &Key, url_prefix: &str, keys: Vec<Key>) -> Result<Self, Error>;
}

// liquers-web/src/store/js_store.rs
impl JsStore {
    pub fn new(prefix: &Key, name: &str, obj: js_sys::Object) -> Result<Self, Error>;
}

// liquers-web/src/store/key_guard.rs
pub fn check_key(key: &Key, store_name: &str) -> Result<(), Error>;

// liquers-web/src/store/encoding.rs
pub fn encode_envelope(data: &[u8]) -> String;
pub fn decode_envelope(text: &str, key: &Key, store_name: &str) -> Result<Vec<u8>, Error>;

// liquers-web/src/store/builder.rs
impl WebStoreFactory {
    pub fn new() -> Self;
    pub fn register_object(&mut self, name: &str, obj: js_sys::Object);
}
impl liquers_store::store_builder::StoreFactory for WebStoreFactory { … }

/// Builds the router for a browser page from a configuration document.
pub fn build_router(
    config: &StoreRouterConfig,
    factory: WebStoreFactory,
) -> Result<AsyncStoreRouter, Error>;
```

```rust
// liquers-store/src/store_builder.rs — the factory seam (new, additive)
pub trait StoreFactory {
    /// Type strings this factory claims, e.g. `["localstorage"]`.
    fn store_types(&self) -> Vec<String>;
    fn create(&self, config: &StoreConfig) -> Result<Box<dyn AsyncStore>, Error>;
}

impl StoreRouterBuilder {
    /// Adds a factory consulted **before** the built-in types, so an integration may also
    /// override `http` with a target-appropriate implementation.
    pub fn with_factory(self, factory: Box<dyn StoreFactory>) -> Self;
}
```

```rust
// liquers-web/src/store/wrapper.rs — the JavaScript surface
#[wasm_bindgen(js_class = Store)]
impl LiquersStore {
    pub fn get(&self, key: &str) -> js_sys::Promise;              // → Uint8Array
    #[wasm_bindgen(js_name = getMetadata)]
    pub fn get_metadata(&self, key: &str) -> js_sys::Promise;     // → object
    pub fn set(&self, key: &str, data: &[u8], metadata: JsValue) -> js_sys::Promise;
    #[wasm_bindgen(js_name = setMetadata)]
    pub fn set_metadata(&self, key: &str, metadata: JsValue) -> js_sys::Promise;
    pub fn remove(&self, key: &str) -> js_sys::Promise;
    pub fn removedir(&self, key: &str) -> js_sys::Promise;
    pub fn contains(&self, key: &str) -> js_sys::Promise;         // → boolean
    #[wasm_bindgen(js_name = isDir)]
    pub fn is_dir(&self, key: &str) -> js_sys::Promise;           // → boolean
    pub fn listdir(&self, key: &str) -> js_sys::Promise;          // → array of strings
    pub fn makedir(&self, key: &str) -> js_sys::Promise;
}

// liquers-web/src/environment.rs — additions
#[wasm_bindgen(js_class = Environment)]
impl LiquersEnvironment {
    /// Accepts an object, or a YAML/JSON string.
    #[wasm_bindgen(js_name = configureStore)]
    pub fn configure_store(&self, config: JsValue) -> js_sys::Promise;
    /// Names a page object so a `js` store entry in the configuration can refer to it.
    #[wasm_bindgen(js_name = registerStoreObject)]
    pub fn register_store_object(&self, name: &str, obj: js_sys::Object) -> Result<(), JsValue>;
    pub fn store(&self) -> Result<LiquersStore, JsValue>;
}
```

## Integration Points

### `liquers-store` (two additive changes)

| File | Change |
|---|---|
| `Cargo.toml` | `opendal = { version = "0.55.0", optional = true }`; features `opendal = ["dep:opendal"]`, added to `default`. |
| `src/lib.rs` | `#[cfg(feature = "opendal")] pub mod opendal_store;` |
| `src/store_builder.rs` | Gate the OpenDAL arm and `create_opendal_store`; gate `create_filesystem_store` on `not(target_arch = "wasm32")` — `AsyncFileStore` already carries that gate (`liquers-core/src/store.rs:816`). Add `StoreFactory` and `with_factory`. |
| `src/config.rs` | **Unchanged.** It imports only `std`, `serde` and `liquers_core`. |

Both gated-off arms must return a *named* error — `"store type 's3' requires the opendal feature"`
— not "unknown store type", or a wasm user gets a misleading message about a type that exists.

`liquers-axum` keeps default features and is unaffected. `liquers-lib` uses `liquers-store` only as
a **dev**-dependency, so it was never in `liquers-web`'s tree.

### `liquers-web`

| File | Change |
|---|---|
| `Cargo.toml` | `liquers-store` with `default-features = false, features = ["async_store"]`; `base64`; web-sys features `Storage`, `Request`, `RequestInit`, `Response`, `Headers`. |
| `src/store/` | New: `mod.rs`, `local_storage.rs`, `fetch.rs`, `js_store.rs`, `builder.rs`, `encoding.rs`, `key_guard.rs`, `wrapper.rs`. |
| `src/environment.rs` | `configure_store`, `register_store_object`, `store`; store config joins the rebuild replay. |
| `src/lib.rs` | `pub mod store;` and re-exports. |
| `src/typescript.rs` | Declarations for `Store` and the three new `Environment` methods. |

**`fetch` is taken from `js_sys::global()` via `Reflect::get` and called with `apply`** — not from
`web_sys::Window`. *(Corrected in Phase 3: the original `Window` → `WorkerGlobalScope` fallback
works in neither Node nor a bare global scope, which would have forced every `FetchStore` test into
a browser for no reason. The global lookup works in a window, a worker **and** under Node, is less
code than two web-sys types, and drops two web-sys features.)* `Request`/`Response`/`Headers`
remain web-sys types, since the resolved value is a `Response` however the call was made.

**Two wasm-bindgen mechanics the implementation must get right**, both of which bite at compile
time rather than in review:

- `future_to_promise` requires a `'static` future, so every `LiquersStore` method clones the
  `Arc<dyn AsyncStore>` and copies its `&[u8]` / `&str` arguments into owned values *before*
  entering the async block. This is the pattern `asset.rs:161` already uses.
- `register_store_object` takes `&self`, but the objects it names must survive until
  `configure_store` builds the factory. They live in a thread-local beside `REGISTERED_SPECS`, and
  like it, they are replayed on a rebuild — otherwise a rebuild silently loses every `js` store.

### Environment wiring — the store joins the rebuild path

`DefaultEnvironment::with_async_store` needs `&mut self`, and `Environment::to_ref` consumes the
environment into an `Arc`. This is precisely the problem `register_command_on`
(`liquers-web/src/environment.rs:90`) already solved: configure directly while `PENDING_ENV` holds
an un-shared environment, and rebuild-and-replay afterwards. The store configuration is retained
in a new thread-local alongside `REGISTERED_SPECS` and re-applied on every rebuild.

**A swappable-store indirection was considered and rejected.** A `SwappableStore` holding
`RefCell<Arc<dyn AsyncStore>>` would let the store change without a rebuild and without discarding
the asset cache. But that cache is the problem, not the cost: assets computed against the old
store are stale the moment it is replaced, and there is no invalidation path for "everything
derived from the store". Discarding them is the correct outcome, so the rebuild is a feature.
Reusing one mechanism for both services also keeps a single lifecycle to document.

`POST-INIT-COMMAND-REGISTRATION` already records the underlying limitation; this design inherits
it rather than adding a second one.

## Relevant Commands

### New Commands

**None.** A store is a service, not a command. It is reached through `-R/` resource queries, which
the planner turns into `GetAsset` steps, and through recipes.

### Relevant Existing Namespaces

Nothing in `liquers-lib` reads or writes the store through a *command* today — there is no `store`
namespace, and `commands.rs` registers none. The store is consumed by the evaluation machinery
(`GetAsset`) rather than by command code, so no namespace interacts with this feature.

> **Open question for the user (Q8), below:** whether store-manipulation *commands* should be in
> scope at all.

## Web Endpoints

Not applicable — no HTTP server. The `#[wasm_bindgen]` surface above is the equivalent, and it is
specified in "Function Signatures".

## Error Handling

All errors are `liquers_core::error::Error` via typed constructors; no `Error::new`, no new error
type, no `unwrap`/`expect`.

| Condition | Constructor | `ErrorType` |
|---|---|---|
| key absent (any store) | `Error::key_not_found(key)` | `KeyNotFound` |
| HTTP 404 | `Error::key_not_found(key)` | `KeyNotFound` |
| `..`/`.`/empty segment | `Error::key_not_supported(key, store)` | `KeyNotSupported` |
| write to `FetchStore`, or an absent optional `JsStore` method | `Error::key_not_supported(key, store)` | `KeyNotSupported` |
| HTTP non-2xx other than 404; network failure; `localStorage` read denied | `Error::key_read_error(key, store, msg)` | `KeyReadError` |
| quota exceeded, browser `QuotaExceededError`, `setItem` failure | `Error::key_write_error(key, store, msg)` | `KeyWriteError` |
| undecodable envelope, bad base64, malformed metadata JSON | `Error::key_read_error(key, store, msg)` | `KeyReadError` |
| a `JsStore` method throws | `js_error_to_liquers(e, ErrorType::ExecutionError)` | preserved if structured |

`js_error_to_liquers` already exists in `liquers-web/src/error.rs` and preserves a thrown
`LiquersError`'s type, so an error raised inside a page's store keeps its identity across the
boundary — that is what `STORE02` asserts for `JsStore`.

Corrupt envelope is `KeyReadError`, deliberately not `KeyNotFound`: the entry exists and cannot be
read, and reporting "absent" would invite a caller to overwrite data it could not interpret.

## Serialization Strategy

- **Metadata** — `Metadata::to_json()` / `Metadata::from_json()`
  (`liquers-core/src/metadata.rs:1524`, `:1567`), the same pair `AsyncFileStore` uses. No new
  serde types, and `LegacyMetadata` round-trips because `from_json` already falls back to it.
- **Data** — the two-character envelope above. Versioned by its leading digit.
- **Configuration** — the existing `StoreRouterConfig` derives, unchanged. New per-type keys:

```yaml
stores:
  - type: localstorage
    prefix: local
    config: { namespace: liquers, quota_bytes: 4000000 }
  - type: http
    prefix: data
    config:
      url_prefix: https://example.org/data/
      keys: [ input.csv, sub/report.json ]
  - type: js
    prefix: custom
    config: { object: myStore }        # a name passed to registerStoreObject
```

`quota_bytes` absent = unlimited (Phase 1 Q3). `keys` absent = an empty store that fetches nothing,
which is a configuration mistake worth a `console.warn` rather than an error, since crawling will
later populate it.

**No `${VAR}` expansion** (Phase 1 Q7): the wasm builder calls `build_without_env_expansion`, which
`StoreRouterBuilder` already provides. A config containing `${…}` is left verbatim rather than
silently emptied, and the builder warns.

## Concurrency Considerations

- **`RefCell` in `LocalStorageStore` is sound because nothing awaits.** The crate's borrow rule
  (`environment.rs` module docs) is "no `RefCell` borrow across an `.await` or a call into
  JavaScript". Web Storage is synchronous, so every borrow begins and ends inside one method body
  with no suspension point. The `localStorage` calls themselves *are* calls into JavaScript, so the
  rule still bites: each method computes what it needs, drops the borrow, then touches storage, or
  holds the borrow only across pure computation. This is a discipline the implementation must
  follow method by method, and it is the single most likely place to get this wrong.
- **`STORE06`, last write wins, trivially.** `localStorage` writes are synchronous and the store
  never yields, so ten concurrent `set`s serialize and the last one is intact. There is no
  interleaving point at which a torn value could be produced — which is what the test should
  assert, rather than accommodating two outcomes.
- **`FetchStore` is immutable after construction**, so concurrent reads need no synchronization.
- **`JsStore` concurrency is the page's problem**, and the adapter does not serialize calls. The
  documented contract is that the page object must tolerate overlapping calls, since Liquers
  evaluates dependencies concurrently.
- **wasm is single-threaded**, so no `Send`/`Sync` is required of anything here; the `?Send`
  `async_trait` attribute is what permits it.
- **Cross-tab mutation is not observed.** A second tab writing the same namespace invalidates this
  store's derived index until reload. Documented limitation; a `storage`-event listener is the
  future fix and is deliberately not in this design.

## Compilation Validation

Checked by inspection against the real signatures:

- `AsyncStore` is object-safe and already used as `Box<dyn AsyncStore>` by `AsyncStoreRouter`;
  the new stores add no generic methods.
- `AsyncStoreRouter::add_store(Box<dyn AsyncStore>)` accepts all three.
- On wasm, `MaybeSend`/`MaybeSync` are blanket-implemented no-ops, so `js_sys::Object` inside a
  store is fine — this is the same reason `liquers-web`'s command closures compile.
- `StoreFactory` has no `Self`-by-value method and no generics, so `Box<dyn StoreFactory>` is legal.
- `DefaultEnvironment::with_async_store(&mut self, Box<dyn AsyncStore>)` matches what the builder
  produces.
- Feature matrix to verify in Phase 4: `liquers-store` default; `--no-default-features --features
  async_store`; the same for `wasm32-unknown-unknown`; `liquers-axum` unchanged; and
  `liquers-web` on wasm32. The `opendal`-off build is the one that will actually catch missed
  `#[cfg]`s.

## References to liquers-patterns.md

| Pattern | Conformance |
|---|---|
| Async-only stores | Yes — no sync `Store` impl added. |
| Typed error constructors | Yes — see the error table; no `Error::new`. |
| No default match arm | `ByteEncoding` and every `ErrorType`/`Metadata` match enumerate variants. |
| No `unwrap`/`expect` in library code | Every fallible boundary returns `Result`; `JsValue` errors go through `js_error_to_liquers`. |
| `eprintln!`, never `println!` | Diagnostics (replaced store, empty `keys`, unexpanded `${…}`) go to `console.warn` in wasm; no stdout. |
| Crate dependency flow | `liquers-web → liquers-store → liquers-core`. No backward edge; nothing moves into core. |
| Extend, don't mutate, traits | `AsyncStore` untouched. `StoreFactory` and `with_factory` are additive; `create_store` keeps its signature. |
| Builders named `…Builder` | Reuses `StoreRouterBuilder`. |

## Resolved Questions

**Q8 — store-manipulation commands are out of scope (user decision).** A `store` namespace
(`store_get`, `store_set`, `store_list`) would make store contents reachable from a query rather
than only from JavaScript. It belongs in `liquers-lib`, so every target gets it rather than one
host at a time, and folding it in here would widen this design from "the browser can have a store"
to "queries can mutate stores" — which needs its own security discussion alongside
`CORE-SESSION-AND-KEY-ACL`, and write commands marked `volatile`. Filed as
`specs/issues/STORE-COMMAND-NAMESPACE-MISSING.md`.

## Open Questions

None.

## Phase 3 corrections applied to this document

Phase 3 changed two things here rather than leaving the documents to disagree:

1. **`fetch` acquisition** — `js_sys::global()` + `Reflect`, not `web_sys::Window` with a
   `WorkerGlobalScope` fallback. See "Integration Points".
2. **Pure-function seams** — `infer_metadata(key, content_type, content_length)` and the URL
   builder are specified as free functions over plain data, so the logic that can silently corrupt
   or misroute is testable without a browser. This is a testability requirement on the
   implementation, not merely a test-plan detail.
