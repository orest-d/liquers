# Phase 4: Implementation Plan - Browser stores for liquers-web

## Overview

**23 steps in 6 milestones.** M1 is the only milestone that touches an existing crate and is
separately gated: if `liquers-store` cannot be made to build without OpenDAL, and cannot expose a
factory seam without disturbing `liquers-axum`, the design's foundation is wrong and nothing should
be built on it. Everything after M1 is new code in `liquers-web`, a crate excluded from
`default-members`, so rollback is a file deletion.

The order is chosen so that **the tests that can catch silent data corruption run before the code
that needs a browser exists**. M2 delivers the envelope codec and the key guard as pure functions
with their full test corpus, in the fast Node loop, before either store is written.

This repository has no CI, so every gate below is a developer-run command.

## Milestones and dependency order

| Milestone | Scope | Depends on | Rollback |
|---|---|---|---|
| **M1** | `liquers-store`: `opendal` feature, factory seam | — | revert one crate; `git revert` is clean |
| **M2** | `liquers-web`: module skeleton, envelope codec, key guard | M1 (dependency edge) | delete `src/store/` |
| **M3** | `FetchStore` | M2 | delete two files |
| **M4** | `LocalStorageStore` | M2 | delete two files |
| **M5** | `JsStore`, factory, environment wiring, JS surface | M2, M3, M4 | revert `environment.rs`; the stores stay usable from Rust |
| **M6** | e2e, stubs, documentation | M5 | documentation only |

M3 and M4 are independent and may be done in either order or in parallel.

## Implementation Steps

---

### M1 — `liquers-store` groundwork

#### Step 1: Make `opendal` optional

**Files:** `liquers-store/Cargo.toml`, `liquers-store/src/lib.rs`,
`liquers-store/src/store_builder.rs`

```toml
opendal = { version = "0.55.0", optional = true }

[features]
default = ["async_store", "opendal"]
opendal = ["dep:opendal"]
```

- `lib.rs`: `#[cfg(feature = "opendal")] pub mod opendal_store;`
- `store_builder.rs`: gate `use opendal::Operator`, `use crate::opendal_store::AsyncOpenDALStore`,
  `create_opendal_store`, `create_opendal_operator`.
- The `_ if is_opendal_store_type(store_type)` arm needs **both** an enabled and a disabled form.
  Disabled must name the feature:

```rust
#[cfg(not(feature = "opendal"))]
_ if is_opendal_store_type(store_type) => Err(Error::general_error(format!(
    "Store type '{}' requires the 'opendal' feature, which is not enabled in this build",
    store_type
))),
```

- `create_filesystem_store` and its `"filesystem"` arm need
  `#[cfg(not(target_arch = "wasm32"))]`, because `AsyncFileStore` already carries that gate
  (`liquers-core/src/store.rs:816`). The wasm32 form must also name the reason, not fall into
  "unknown store type".
- `config.rs` is **not** touched. It imports only `std`, `serde` and `liquers_core`.

**Validation:**
```bash
cargo check -p liquers-store
cargo check -p liquers-store --no-default-features --features async_store
cargo check -p liquers-store --no-default-features --features async_store --target wasm32-unknown-unknown
cargo check -p liquers-axum
```

**Agent:** sonnet · skills `rust-best-practices` · knowledge: this step, `liquers-store/src/*`,
the feature-gating section of the skill (every `use`, type, arm and test needs its own `cfg`).

---

#### Step 2: The `StoreFactory` seam

**File:** `liquers-store/src/store_builder.rs`

```rust
/// Creates stores of types this crate does not know about.
///
/// Consulted **before** the built-in types, so an integration may override one — `liquers-web`
/// must override `http`, which is a built-in OpenDAL type that cannot exist on wasm.
pub trait StoreFactory {
    fn store_types(&self) -> Vec<String>;
    fn create(&self, config: &StoreConfig) -> Result<Box<dyn AsyncStore>, Error>;
}

impl StoreRouterBuilder {
    pub fn with_factory(mut self, factory: Box<dyn StoreFactory>) -> Self;
}
```

No `Send`/`Sync` bound on `StoreFactory` — the factory is transient, and `WebStoreFactory` holds
`js_sys::Object`, which is `!Send`. Adding a bound no call site needs would make the trait
unimplementable for its only real implementor.

`create_store` keeps its signature and behaviour exactly (built-ins only), so no existing caller
changes. Factory dispatch lives in `StoreRouterBuilder::build`, which consults factories first and
falls through to `create_store`.

**Validation:** `cargo check -p liquers-store && cargo check -p liquers-axum`

**Agent:** sonnet · skills `rust-best-practices` · knowledge: Phase 2 "Function Signatures",
`store_builder.rs`, object-safety rules.

---

#### Step 3: Native tests for the seam

**File:** `liquers-store/src/store_builder.rs`, `#[cfg(test)] mod tests`

`factory01_custom_type_is_created`, `factory02_factory_precedes_builtin`,
`factory03_unclaimed_type_falls_through`, `factory04_gated_type_names_the_feature`
(the last under `#[cfg(not(feature = "opendal"))]`).

`factory02` is the load-bearing one: a test factory claiming `"http"` must win over the OpenDAL
built-in. Without it, factory-after-builtin ordering would pass every other test and break the
browser.

**Validation:**
```bash
cargo test -p liquers-store
cargo test -p liquers-store --no-default-features --features async_store
```

**Agent:** haiku · skills `liquers-unittest`, `rust-best-practices` · knowledge: Phase 3 test
group 5, `liquers-store/src/store_builder.rs` existing test module.

---

#### Step 4: Extend the build-matrix script

**File:** `scripts/check-build-matrix.sh`

Add a `liquers-store` section beside the existing `liquers-lib` one:

| Configuration | Why |
|---|---|
| `-p liquers-store` (default) | the shipped native build |
| `-p liquers-store --no-default-features --features async_store` | the OpenDAL-off build — **the one that catches missed `cfg`s** |
| the same, `--target wasm32-unknown-unknown` | proves the browser dependency edge is real |
| `-p liquers-axum` | proves the default consumer is undisturbed |

**Validation:** `bash scripts/check-build-matrix.sh`

**Agent:** haiku · no skills · knowledge: the existing script, this table.

> **M1 exit gate.** All four `cargo check`s, both `cargo test -p liquers-store` runs, and the
> matrix script pass, and `cargo test -p liquers-lib --lib --tests` shows no regression. **If this
> gate cannot be met, stop and revisit Phase 2** — everything downstream assumes it.

---

### M2 — Pure foundations in `liquers-web`

#### Step 5: Module skeleton and dependencies

**Files:** `liquers-web/Cargo.toml`, `liquers-web/src/lib.rs`, `liquers-web/src/store/mod.rs`

```toml
liquers-store = { path = "../liquers-store", default-features = false, features = ["async_store"] }
base64 = "0.22"
```
web-sys features added: `Storage`, `Request`, `RequestInit`, `Response`, `Headers`.
**Not** `Window`/`WorkerGlobalScope` — `fetch` comes from `js_sys::global()` (Phase 2, corrected in
Phase 3).

`src/lib.rs`: `pub mod store;` plus re-exports.

**Validation:** `cargo check -p liquers-web --target wasm32-unknown-unknown`

**Agent:** haiku · knowledge: Phase 2 "Integration Points".

---

#### Step 6: The envelope codec

**File:** `liquers-web/src/store/encoding.rs`

```rust
pub enum ByteEncoding { Utf8, Base64 }
pub fn encode_envelope(data: &[u8]) -> String;
pub fn decode_envelope(text: &str, key: &Key, store_name: &str) -> Result<Vec<u8>, Error>;
```

Format: one version digit, one encoding letter, then the payload — `"1u…"` or `"1b…"`.
Selection is `std::str::from_utf8(data).is_ok()`, never a metadata hint (Phase 1 Q3).
`decode_envelope` must reject a **future version digit** with `KeyReadError` rather than guessing.
Matches on `ByteEncoding` are exhaustive; no `_ =>`.

**Validation:** `cargo check -p liquers-web --target wasm32-unknown-unknown`

**Agent:** sonnet · skills `rust-best-practices` · knowledge: Phase 1 Q3, Phase 2 "The
`localStorage` layout", Phase 3 corpus.

---

#### Step 7: The key guard

**File:** `liquers-web/src/store/key_guard.rs`

```rust
pub fn check_key(key: &Key, store_name: &str) -> Result<(), Error>;
```

Refuses any segment that is `..`, `.` or empty, with `Error::key_not_supported`. It must inspect
**every** segment, not just the first — `a/../b` is the case that matters.

**Validation:** `cargo check -p liquers-web --target wasm32-unknown-unknown`

**Agent:** haiku · skills `rust-best-practices` · knowledge: Phase 2 "Key guard",
`specs/issues/STORE-FILESTORE-PATH-TRAVERSAL.md` for why the shape is refusal, not normalization.

---

#### Step 8: Tests — encoding corpus and key guard (12)

**Files:** `liquers-web/tests/store_encoding_STORE.rs`, `liquers-web/tests/store_pure_STORE.rs`

`encoding01`–`encoding04`, `keyguard01`–`keyguard04`, plus the `STORE05` cell for each store's
guard path. Node harness — **no** `wasm_bindgen_test_configure!(run_in_browser)`, per the
convention documented in `liquers-web/tests/value_bridge_VALUE.rs`.

The corpus is fixed in Phase 3 and must be transcribed exactly; `[0xED, 0xA0, 0x80]` and
`[0xFF, 0xFE]` are the entries that fail a naive implementation.

**Validation:**
```bash
cargo test -p liquers-web --target wasm32-unknown-unknown --test store_encoding_STORE
cargo test -p liquers-web --target wasm32-unknown-unknown --test store_pure_STORE
```

**Agent:** sonnet · skills `liquers-unittest`, `rust-best-practices` · knowledge: Phase 3 test
group 4 verbatim, `liquers-web/tests/value_bridge_VALUE.rs` for the file conventions.

> **M2 exit gate.** Both test files green under Node. This is the point at which data integrity is
> established; the stores that follow only have to route bytes through these functions.

---

### M3 — `FetchStore`

#### Step 9: Pure URL construction and metadata inference

**File:** `liquers-web/src/store/fetch.rs`

```rust
pub(crate) fn key_to_url(url_prefix: &str, prefix: &Key, key: &Key) -> Result<String, Error>;
pub fn infer_metadata(
    key: &Key,
    content_type: Option<&str>,
    content_length: Option<u64>,
) -> Metadata;
```

`key_to_url` strips `prefix` (this is the store that strips — see Phase 2 "Key mapping") and joins
verbatim. `infer_metadata` implements the precedence table: extension wins unless
`file_extension_to_media_type` returns `application/octet-stream`, in which case the response
`Content-Type` (parameters stripped) fills in.

Free functions on plain data, deliberately: this is what keeps `STORE10` out of the browser.

**Validation:** `cargo check -p liquers-web --target wasm32-unknown-unknown`

**Agent:** sonnet · skills `rust-best-practices` · knowledge: Phase 2 "The `fetch` URL and metadata
inference", `liquers-core/src/media_type.rs`.

---

#### Step 10: `FetchStore` and the `AsyncStore` impl

**File:** `liquers-web/src/store/fetch.rs`

Construction normalizes `url_prefix` to end in `/` and derives `dirs` from `keys`.
`fetch` is obtained from `js_sys::global()` via `Reflect::get`, called with `apply`; the resolved
value is `dyn_into::<web_sys::Response>()`.

- `get` → GET; 404 → `key_not_found`; other non-2xx and network failure → `key_read_error`.
- `get_metadata` → HEAD, falling back to GET on 405/501.
- `set`/`set_metadata`/`remove`/`removedir`/`makedir` → `key_not_supported`.
  **`set_metadata` must be written** — it is the one trait method with no default.
- `contains`/`is_dir`/`listdir` from `keys`/`dirs`, no network.
- **`is_supported` must be overridden** — it defaults to `false` and the router depends on it.

**Validation:** `cargo check -p liquers-web --target wasm32-unknown-unknown`

**Agent:** sonnet · skills `rust-best-practices` · knowledge: Phase 2 trait table and error table,
`liquers-core/src/store.rs:268-465` for the trait defaults.

---

#### Step 11: Tests — URL and metadata (6)

**File:** `liquers-web/tests/store_pure_STORE.rs` (extends Step 8's file)

`STORE10` precedence cases, `key_to_url` including the prefix-stripping case and a refused key,
`STORE03` listing derived from `keys`.

**Validation:** `cargo test -p liquers-web --target wasm32-unknown-unknown --test store_pure_STORE`

**Agent:** haiku · skills `liquers-unittest` · knowledge: Phase 3 groups 3 and 6.

---

### M4 — `LocalStorageStore`

#### Step 12: Layout, index scan, get/set

**File:** `liquers-web/src/store/local_storage.rs`

Entry names `{ns}/d/{key}`, `{ns}/m/{key}`, `{ns}/D/{key}`. `new` validates `namespace` non-empty
and `/`-free, then scans `localStorage` (`length()`/`key(i)`) to build `dirs`, `explicit_dirs` and
`used_bytes`.

**The `RefCell` discipline is the risk in this step.** Every method must drop its borrow before
calling into `localStorage`. Reviewer instruction: read each method and name the point at which the
borrow ends.

**Validation:** `cargo check -p liquers-web --target wasm32-unknown-unknown`

**Agent:** sonnet · skills `rust-best-practices` · knowledge: Phase 2 layout + "Concurrency
Considerations", `liquers-web/src/environment.rs` module docs (the borrow rule),
`AsyncMemoryStore` (`liquers-core/src/store.rs:503`) for the index shape.

---

#### Step 13: Directories

**File:** `liquers-web/src/store/local_storage.rs`

`makedir` writes a marker; `removedir`, `listdir`, `is_dir`, `contains` read the index.
`is_dir` and `contains` may **both** be true for the same key (Phase 3 C7).

**Validation:** `cargo check -p liquers-web --target wasm32-unknown-unknown`

**Agent:** sonnet · skills `rust-best-practices` · knowledge: Step 12's output, Phase 3 C7/C8.

---

#### Step 14: Quota and error mapping

**File:** `liquers-web/src/store/local_storage.rs`

Budget checked **before** `setItem`, so a refused write leaves nothing behind; data entry written
last, so a failure cannot leave a readable half-write. `setItem`'s `Err(JsValue)` maps to
`key_write_error` regardless of the budget (a browser may refuse below it).

**Validation:** `cargo check -p liquers-web --target wasm32-unknown-unknown`

**Agent:** sonnet · skills `rust-best-practices` · knowledge: Phase 1 Q3, Phase 3 C3/C4.

---

#### Step 15: Browser tests (6)

**File:** `liquers-web/tests/store_local_STORE.rs` — with
`wasm_bindgen_test_configure!(run_in_browser);`

`STORE01`–`STORE06` for `LocalStorageStore`. `STORE06` asserts the value equals the **last** write
and is one byte long — not "one of them won".

Each test must clear its namespace first; `localStorage` persists across tests in one browser
session, and a suite that passes only in a fresh profile is a suite that fails in CI later.

**Validation:**
```bash
CHROMEDRIVER=$(which chromedriver) \
  cargo test -p liquers-web --target wasm32-unknown-unknown --test store_local_STORE
```

**Agent:** sonnet · skills `liquers-unittest` · knowledge: Phase 3 conformance table and the
non-vacuous-assertion table, the browser-test note in `liquers-web/tests/value_bridge_VALUE.rs`.

---

### M5 — `JsStore`, composition and the JavaScript surface

#### Step 16: `JsStore`

**File:** `liquers-web/src/store/js_store.rs`

Resolve every protocol method at construction into `JsStoreMethods`; fail immediately if `get` is
absent, naming what is missing. Absent optional methods produce `key_not_supported` — **not** the
permissive trait default. `isSupported` is the exception: absent means supported.

Thenables are awaited with the `is_thenable` + `JsFuture` pattern already in
`liquers-web/src/command/adapter.rs:220-240`; reuse it rather than reimplementing.
Thrown values go through `js_error_to_liquers`, preserving a `LiquersError`'s discriminant.

**Validation:** `cargo check -p liquers-web --target wasm32-unknown-unknown`

**Agent:** sonnet · skills `rust-best-practices` · knowledge: Phase 2 protocol table,
`command/adapter.rs`, `error.rs`.

---

#### Step 17: `WebStoreFactory` and `build_router`

**File:** `liquers-web/src/store/builder.rs`

Claims `localstorage`, `http`, `https`, `js`. Uses `StoreRouterBuilder::with_factory` and
`build_without_env_expansion` (no `${VAR}` expansion — Phase 1 Q7; warn if a `${` is present).
A `js` entry whose named object was never registered fails **here**, naming the object (Phase 3 C12).

**Validation:** `cargo check -p liquers-web --target wasm32-unknown-unknown`

**Agent:** sonnet · skills `rust-best-practices` · knowledge: Phase 2 "Serialization Strategy",
Step 2's trait.

---

#### Step 18: Environment wiring

**File:** `liquers-web/src/environment.rs`

A thread-local for the store configuration and one for registered objects, beside
`REGISTERED_SPECS`. `configure_store` follows the `register_command_on` shape exactly: configure
`PENDING_ENV` directly when un-shared, otherwise rebuild and replay. **Both new thread-locals must
be replayed on rebuild**, or a rebuild silently drops every `js` store — this is the specific bug
to watch for.

**Validation:** `cargo test -p liquers-web --target wasm32-unknown-unknown --test environment_ENVIRON`
(no regression in the existing rebuild tests)

**Agent:** sonnet · skills `rust-best-practices` · knowledge: `environment.rs:60-140` in full,
Phase 2 "Environment wiring".

---

#### Step 19: The `LiquersStore` wasm wrapper

**File:** `liquers-web/src/store/wrapper.rs`

Ten `#[wasm_bindgen]` methods returning `js_sys::Promise`. Every one clones the
`Arc<dyn AsyncStore>` and copies its `&[u8]`/`&str` arguments **before** entering the async block —
`future_to_promise` needs `'static`. Follow `liquers-web/src/asset.rs:161`.

**Validation:** `cargo check -p liquers-web --target wasm32-unknown-unknown`

**Agent:** sonnet · skills `rust-best-practices` · knowledge: `asset.rs`, Phase 2 signatures.

---

#### Step 20: Tests — `JsStore` and composition (11)

**File:** `liquers-web/tests/store_js_STORE.rs` (Node)

`STORE01`–`STORE06` against a JavaScript fixture store, plus `STORE09` (read-only refusal *and*
the writable store at another prefix unchanged) and `STORE11` (routing, first match, unmatched key
→ `KeyNotFound`), plus C11 and C12.

`STORE08` is satisfied by this step together with Step 15: it is the claim that the parameterised
suite covers *every* store, so it has no separate test body — its failure mode is a store added to
the design without being added to the matrix, which a reviewer catches, not a runtime assertion.
Record it as covered here so it is not mistaken for an omission.

**Validation:** `cargo test -p liquers-web --target wasm32-unknown-unknown --test store_js_STORE`

**Agent:** sonnet · skills `liquers-unittest` · knowledge: Phase 3 groups 4 and 6, existing
`tests/common/`.

---

### M6 — Delivery

#### Step 21: Playwright — network and end-to-end (5)

**Files:** `liquers-web/tests/e2e/` — fixture directory, page, spec

Fixture server serves `reference/input.txt` and `reference/input.csv`. `STORE07` for all three
stores, `STORE02` over a real 404, `STORE10`'s header half, and the **reload test** — the only
test proving the derived index is rebuilt from storage rather than living in the first page's
memory.

**Validation:** `cd liquers-web/tests/e2e && npm install && npx playwright test`

**Agent:** sonnet · skills none · knowledge: Phase 3 group 8, existing `tests/e2e/` setup.

---

#### Step 22: TypeScript declarations

**Files:** `liquers-web/src/typescript.rs`, `liquers-web/scripts/check-stubs.sh`

Declarations for `Store` and for `configureStore` / `registerStoreObject` / `store`.

**Validation:** `./liquers-web/scripts/check-stubs.sh`

**Agent:** haiku · knowledge: `typescript.rs`, Phase 2 signatures.

---

#### Step 23: Documentation

**Files:**

| File | Change |
|---|---|
| `specs/reference/STORE_CONFIG_FSD.md` | Document `localstorage`, `js`, and the browser reading of `http`/`https`; document the `opendal` feature and the `StoreFactory` seam. **Add a `## History` row and bump `reviewed:` in the same commit** (`DOCS_STRUCTURE_GUIDE.md` §9.2). |
| `liquers-web/README.md` | The store section, the browser-test command, and the `CHROMEDRIVER` requirement. |
| `CLAUDE.md` | Add the browser store test command to the `liquers-web` loop list. |
| `specs/README.md` | Move the capability line from *designing* to *built*. |
| `specs/design/liquers-web-store/DESIGN.md` | `status: complete`, drop `phase`. |
| `specs/issues/WEB-NATIVE-IO-TIER2.md` | It asked for IndexedDB and a JS command backend; this delivers `localStorage`, `fetch` and a JS *store*. **Do not close it** — record what remains. |

**Validation:** `python3 scripts/docs_index.py --check`

**Agent:** sonnet · skills none · knowledge: `DOCS_STRUCTURE_GUIDE.md` §8.1 and §9.2, all four
phase documents.

---

## Testing Plan

| Gate | Command | Expects |
|---|---|---|
| M1 exit | `cargo test -p liquers-store` (×2 feature sets); `cargo check -p liquers-axum`; `bash scripts/check-build-matrix.sh`; `cargo test -p liquers-lib --lib --tests` | 4 factory tests; no regression anywhere |
| M2 exit | `cargo test -p liquers-web --target wasm32-unknown-unknown --test store_encoding_STORE --test store_pure_STORE` | 12 tests |
| M3 exit | as above, `store_pure_STORE` | +6 tests |
| M4 exit | `CHROMEDRIVER=… cargo test … --test store_local_STORE` | 6 tests, in Chromium |
| M5 exit | `cargo test … --test store_js_STORE`; `--test environment_ENVIRON` | 11 tests; no rebuild regression |
| M6 exit | `npx playwright test`; `./liquers-web/scripts/check-stubs.sh`; `python3 scripts/docs_index.py --check` | 5 e2e; stubs fresh; 0 doc errors |
| Full | every command above | **44 new tests** (4 native + 29 Node + 6 Chromium + 5 Playwright), all existing suites green |

**Disk.** `cargo clean` between the native loop and the wasm loop, per `CLAUDE.md` — the two
targets together exceed the 30 GB allowance.

**Not automated:** nothing. Every gate is a command. There is no CI, so they are developer-run.

## Agent Assignment

| Step | Model | Skills | Critical knowledge |
|---|---|---|---|
| 1 | sonnet | `rust-best-practices` | feature gating: every `use`, type, arm and test needs its own `cfg` |
| 2 | sonnet | `rust-best-practices` | object safety; no `Send`/`Sync` bound |
| 3 | haiku | `liquers-unittest`, `rust-best-practices` | Phase 3 group 5 |
| 4 | haiku | — | the existing script |
| 5 | haiku | — | Phase 2 "Integration Points" |
| 6 | sonnet | `rust-best-practices` | Phase 1 Q3 — check, never hint |
| 7 | haiku | `rust-best-practices` | every segment, not just the first |
| 8 | sonnet | `liquers-unittest`, `rust-best-practices` | the corpus, transcribed exactly |
| 9 | sonnet | `rust-best-practices` | precedence table; pure functions |
| 10 | sonnet | `rust-best-practices` | trait defaults: `is_supported`, `set_metadata` |
| 11 | haiku | `liquers-unittest` | Phase 3 groups 3, 6 |
| 12 | sonnet | `rust-best-practices` | the `RefCell` borrow rule |
| 13 | sonnet | `rust-best-practices` | C7, C8 |
| 14 | sonnet | `rust-best-practices` | check before write |
| 15 | sonnet | `liquers-unittest` | non-vacuous `STORE06`; clear the namespace |
| 16 | sonnet | `rust-best-practices` | reuse `adapter.rs`'s thenable path |
| 17 | sonnet | `rust-best-practices` | fail early on a missing named object |
| 18 | sonnet | `rust-best-practices` | replay **both** new thread-locals |
| 19 | sonnet | `rust-best-practices` | `'static` futures; own before the async block |
| 20 | sonnet | `liquers-unittest` | `STORE09`'s second assertion |
| 21 | sonnet | — | the reload test is the point |
| 22 | haiku | — | `typescript.rs` |
| 23 | sonnet | — | §9.2 History row; do not close the issue |

**Steps needing the most care**, in order: 18 (a missed replay silently drops stores), 12 (the
`RefCell` rule), 1 (cfg cross-product), 16 (a permissive default would make `STORE03` vacuous).

## Rollback Plan

| Milestone | Rollback | Cost |
|---|---|---|
| M1 | `git revert`. The `opendal` feature is in `default`, so reverting restores the previous build exactly; the `StoreFactory` trait is additive and unused by any existing caller. | none |
| M2-M4 | Delete `liquers-web/src/store/` and its test files; remove `pub mod store;`. Nothing else references them. | none |
| M5 | Revert `environment.rs` alone. The stores remain constructible from Rust; only the JavaScript surface disappears. | the feature is unreachable from a page, but the crate still builds |
| M6 | Documentation and tests only. | none |

**The one irreversible-ish step is Step 1**, because it changes a published crate's feature set. It
is mitigated by putting `opendal` in `default`: a consumer who does nothing sees no change. A
consumer using `default-features = false` today would newly lose OpenDAL — `liquers-axum` does not,
and `liquers-lib` has it only as a dev-dependency, so within this workspace the set of affected
consumers is empty.

**Partial-delivery position.** If M5 is abandoned, M1-M4 still leave `liquers-store` usable from
wasm and two working stores — a real improvement, and the remainder becomes an issue rather than a
half-finished design (`DOCS_STRUCTURE_GUIDE.md` §5.6).

## Open Questions

None.
