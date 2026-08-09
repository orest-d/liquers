# Phase 3: Examples & Use-cases - Browser stores for liquers-web

## Example Type

**Conceptual code, with verified queries.** Runnable prototypes are not possible — none of the
types exist until Phase 4. The examples show intended usage, written against the Phase 2 signatures
closely enough to serve as acceptance criteria. Nothing hand-wavy is smuggled in under that licence:

- **Every query below was checked with `liquers-validate`**, including its resolved plan rather
  than only its parse status (see "Query validation").
- **Every claim about a browser API** is a claim about a documented synchronous/asynchronous
  contract, and each one that the design leans on is listed in "Corner cases" with the test that
  pins it.
- The genuinely deliverable artefact of this phase is the **test inventory**. Phase 4 implements
  against it.

## Overview Table

| # | Type | Name | Demonstrates / checks |
|---|---|---|---|
| 1 | Example | Fetch-backed data, localStorage scratch space | The primary case: a router combining a read-only `http` store with a writable `localstorage` one, configured from one YAML document |
| 2 | Example | A store implemented by the page | `JsStore` end to end — the guide's literal reading of `STORE` |
| 3 | Example | Binary round trip and quota exhaustion | The two things Phase 1 Q3 said must be reliable, exercised where they fail |
| 4 | Unit tests | Encoding and key guard | Pure functions: the round-trip corpus and `..` refusal, in the fast loop |
| 5 | Unit tests | Configuration and the factory seam | `liquers-store` native tests, including the `opendal`-off error message |
| 6 | Conformance | `STORE01`–`STORE07` × 3 stores | The prescribed inventory, per store, in the harness each one actually needs |
| 7 | Conformance | `STORE08`–`STORE11` | The four the guide is missing (`LANGUAGE-GUIDE-STORE-SCOPE-INCOMPLETE`), adopted provisionally |
| 8 | Integration | End-to-end page | `STORE07` in a real browser against a real server |
| 9 | Corner cases | Encoding, quota, concurrency, cross-tab, routing | The failure modes Phases 1-2 identified |

## Query validation

Checked with `liquers-validate` against the real registry. All five parse and plan; the interesting
column is what they *mean*.

| Query | Status | Resolved meaning (read from the plan) |
|---|---|---|
| `-R/local/in.txt/-/to_text` | Ok | `GetAsset[local, in.txt]` then `Action{to_text}` — fetch the key, then convert |
| `-R/data/input.csv/-/to_text` | Ok | `GetAsset[data, input.csv]` then `Action{to_text}` |
| `-R/local/in.txt` | Ok | `GetAsset[local, in.txt]` — the raw stored bytes |
| `-R/local/in.txt/to_text` | Ok | `GetAsset[local, in.txt, to_text]` — **a file named `to_text`**, not a command |
| `-R/local/notes/-/to_text` | Ok | `GetAsset[local, notes]` then `Action{to_text}` — an extension-less key |

The fourth row is the trap the guide warns about and it is kept deliberately: `-R/` consumes the
rest of the string as a key unless `/-/` starts a new segment. Examples below always write `/-/`.

---

## Example 1: Fetch-backed data, localStorage scratch space

**Scenario.** A dashboard reads read-only reference data published by a web server and writes
derived results into browser storage so they survive a reload. One configuration document
describes both.

```yaml
# store.yaml
stores:
  - type: http
    prefix: data
    config:
      url_prefix: https://example.org/reference/
      keys: [ input.csv, sub/report.json ]
  - type: localstorage
    prefix: local
    config:
      namespace: dashboard
      quota_bytes: 4000000
```

```js
import init, { Environment } from "./pkg/liquers_web.js";

await init();
const env = Environment.global();
await env.configureStore(storeYamlText);      // or a plain object

// Read published data through the fetch store.
const text = await env.evaluate("-R/data/input.csv/-/to_text");

// Write a derived result into browser storage.
const store = env.store();
await store.set("local/summary.json", new TextEncoder().encode(JSON.stringify(summary)));

// It is now an ordinary Liquers resource, and survives a reload.
const back = await env.evaluate("-R/local/summary.json/-/to_text");
```

**What this is meant to show.** Routing is by prefix and nothing else: `data/...` reaches the fetch
store, `local/...` the localStorage one, because `AsyncStoreRouter::find_store` takes the first
store whose `key_prefix` matches *and* whose `is_supported` returns true. A write to `data/...`
fails with `KeyNotSupported` rather than falling through to the writable store — the router does
not retry the next store, and that is the behaviour to assert, because "falls through to whichever
store can write" is the plausible wrong implementation.

**Covered by:** `STORE11` (routing), `STORE09` (read-only refusal), `STORE07` (end-to-end).

---

## Example 2: A store implemented by the page

**Scenario.** A page already keeps documents in its own IndexedDB wrapper and wants Liquers to read
them without copying anything into a second store.

```js
const myStore = {
  async get(key) {
    const rec = await db.read(key);
    if (!rec) throw new liquers.LiquersError("key_not_found", `no such key: ${key}`);
    return { data: rec.bytes, metadata: { media_type: rec.type } };
  },
  async contains(key) { return (await db.read(key)) != null; },
  async listdir(key) { return await db.children(key); },
  isDir(key) { return db.isFolder(key); },        // sync is fine
  // no set / setMetadata: this store is read-only
};

env.registerStoreObject("docs", myStore);
await env.configureStore({ stores: [ { type: "js", prefix: "docs", config: { object: "docs" } } ] });

await env.evaluate("-R/docs/notes/-/to_text");
```

**What this is meant to show.** Three things, each of which is a decision from Phase 2 rather than
an incidental detail:

1. **A thrown `LiquersError` keeps its type across the boundary.** `js_error_to_liquers` preserves
   the discriminant, so `key_not_found` thrown in JavaScript arrives as `ErrorType::KeyNotFound`
   and the asset layer treats it as an absent key rather than an execution failure.
2. **`set` is simply absent, and that is a complete answer.** Calls to it return
   `KeyNotSupported` — not the permissive trait default.
3. **A sync method is allowed.** `isDir` returns a boolean, not a Promise; the adapter awaits only
   thenables.

**Covered by:** `STORE01`–`STORE07` (the `JsStore` column), `STORE09`.

---

## Example 3: Binary round trip and quota exhaustion

**Scenario.** The two things Phase 1 Q3 required to be reliable, shown where they fail rather than
where they succeed.

```js
// A PNG is not valid UTF-8, so it takes the base64 path — invisibly.
const png = new Uint8Array([0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, /* … */]);
await store.set("local/logo.png", png);
const back = await store.get("local/logo.png");
// back is byte-identical to png; the caller never learns which encoding was used.

// Metadata is inferred from the extension, not from what was written.
(await store.getMetadata("local/logo.png")).media_type === "image/png";

// Quota is the store's own budget, and it refuses rather than truncating.
try {
  await store.set("local/big.bin", new Uint8Array(5_000_000));
} catch (e) {
  e.errorType === "key_write_error";     // not key_not_supported, not a silent partial write
}
```

**What this is meant to show.** The encoding is an implementation detail with a *recorded* answer:
the caller sees bytes in and identical bytes out, and the `1u`/`1b` tag exists so the store never
has to guess on the way back. And a refused write leaves the store unchanged — the byte accounting
is checked *before* `setItem`, so a rejected write cannot leave a half-written entry or a corrupted
`used_bytes`.

**Covered by:** `STORE10`, the round-trip corpus in test group 4, corner cases C1-C4.

---

## Test harness — and why the obvious choice is wrong

The guide (§3, "Choosing a test harness") asks what each test actually needs. The answer here is
not uniform, and getting it wrong costs a suite that cannot run:

**`localStorage` does not exist under Node.** `liquers-web`'s routine loop is
`cargo test -p liquers-web --target wasm32-unknown-unknown` under Node, with no browser. `web_sys::window()`
returns `None` there, so every `LocalStorageStore` test must run in a real browser
(`wasm-pack test --headless --chrome`). This is the single most important harness fact in this
design, and it is why the pure parts are deliberately split out: the **encoding round trip** and
the **key guard** are free functions with no browser dependency, so the tests that matter most for
data integrity run in the fast loop.

**A Phase 2 correction, forced by this analysis.** Phase 2 specified acquiring `fetch` from
`web_sys::Window`, falling back to `WorkerGlobalScope`. Neither exists under Node, which would have
made every `FetchStore` test browser-only for no good reason. **Corrected:** take `fetch` from
`js_sys::global()` via `Reflect::get` and call it with `apply`. That works in a window, in a
worker, *and* under Node — it is less code than two web-sys types, and it drops the `Window` and
`WorkerGlobalScope` web-sys features from `Cargo.toml`. `Request`/`Response`/`Headers` stay as
web-sys types, since the resolved value is a `Response` however the call was made.

**Pure functions are pulled out wherever a browser would only be plumbing.** This is the design
consequence of the harness analysis, not a testing trick: `encode_envelope`/`decode_envelope`, the
key guard, URL construction, and metadata inference are all expressible as free functions over
plain data, so the logic that can silently corrupt data is tested in the fast loop and only the
plumbing needs a browser. Concretely, metadata inference is specified as

```rust
pub fn infer_metadata(
    key: &Key,
    content_type: Option<&str>,
    content_length: Option<u64>,
) -> Metadata;
```

so `STORE10`'s precedence rule is a Node test, and the browser only has to prove that the store
really does read those two headers off a `Response`.

**File layout follows the crate's existing convention** (`liquers-web/tests/value_bridge_VALUE.rs`
documents it): browser-requiring tests live in *their own files* carrying
`wasm_bindgen_test_configure!(run_in_browser)`, so the Node loop is not dragged into needing a
WebDriver.

| Tier | Where | Command | What runs there |
|---|---|---|---|
| **N** native | `liquers-store/src/store_builder.rs` `#[cfg(test)]` | `cargo test -p liquers-store` | Config parsing, the factory seam, the `opendal`-off error message |
| **W-node** wasm/Node | `tests/store_encoding_STORE.rs`, `tests/store_pure_STORE.rs`, `tests/store_js_STORE.rs` | `cargo test -p liquers-web --target wasm32-unknown-unknown --test …` | Encoding corpus, key guard, URL construction, metadata precedence, `JsStore` protocol, router composition over `JsStore`s |
| **W-browser** wasm/Chromium | `tests/store_local_STORE.rs` (`run_in_browser`) | same, with `CHROMEDRIVER` set | `LocalStorageStore` full contract — the only tests that genuinely need `localStorage` |
| **P** Playwright | `tests/e2e/` | `cd liquers-web/tests/e2e && npx playwright test` | `FetchStore` against a real server, the end-to-end page, `STORE07`, the reload test |
| **B** build | `scripts/check-build-matrix.sh` | the script | Feature/target matrix |

**`FetchStore`'s network tests are Playwright's, not `wasm-bindgen-test`'s.** The browser test
runner serves only its own harness page, so fixture files would have to be smuggled into it;
Playwright already has a fixture server for the e2e page, and putting the network tests there costs
nothing and removes a fragile dependency.

Version coupling is unchanged from the parent design (`wasm-bindgen` CLI must match the crate;
WebDriver major must match Chromium) and is already recorded in `liquers-web/README.md`.

---

## Test Plan

**44 tests in six groups across five tiers.** Roll-up, and the order to run them in — cheapest and most diagnostic
first, so a broken encoding never reaches a browser run:

| # | Group | Tier | Count | Gate |
|---|---|---|---|---|
| 1 | Encoding corpus + key guard | W-node | 12 | Data integrity. Must be green before anything else is worth running. |
| 2 | Config, factory seam, feature gating | N | 4 | Runs in the native fast loop; catches a mis-gated build in seconds. |
| 3 | Pure URL construction + `infer_metadata` | W-node | 6 | `STORE10` precedence without a network. |
| 4 | `JsStore` protocol + router composition | W-node | 11 | `STORE01`–`STORE06` for `JsStore`, plus `STORE09`/`STORE11`. |
| 5 | `LocalStorageStore` full contract | W-browser | 6 | `STORE01`–`STORE06`; needs a chromedriver. |
| 6 | `FetchStore` over the wire + end-to-end | P | 5 | `STORE07` for all three stores, plus the reload test. |

**Prescribed-inventory disposition:** of the 21 cells (`STORE01`–`STORE07` × 3 stores), **19 are
required** and **2 are `NA`** — both `FetchStore`, both because it has no write path, both with the
reversing condition recorded below. That ratio is what the guide asks for; a feature excusing half
its inventory would be a warning sign, and this one excuses a tenth of it for one structural reason.

**Coverage of Phase 1 and 2 decisions.** Every decision either has a test or is explicitly a
documented limitation:

| Decision | Test |
|---|---|
| Recorded, never inferred, encoding (P1 Q3) | `encoding02` |
| UTF-8 round trip is lossless (P1 Q3) | `encoding01`, `encoding04` |
| Configurable quota, unlimited default (P1 Q3) | C3, C4 tests |
| Directory index, `makedir` survives reload (P1 Q4) | `STORE03`, `STORE07` reload |
| Known-key listing (P1 Q5) | `STORE03` on `FetchStore` |
| `..` refused (P1 Q6) | `keyguard01`–`keyguard03`, `STORE05` |
| No `${VAR}` expansion (P1 Q7) | C15 |
| Factory precedes built-ins (P2) | `factory02` |
| Extension beats `Content-Type` (P2) | `STORE10` |
| Absent `JsStore` methods error (P2) | `STORE09`, C11 |
| Rebuild on reconfigure (P2) | covered by the existing `ENVIRON`/`COMMAND` rebuild tests; no new test |
| Cross-tab staleness (P2) | C6 — asserted as a *known* behaviour, not fixed |

## Conformance inventory

Naming per the guide: `fn store01_set_get_data_and_metadata()` in Rust, `test("STORE07 …")` in
Playwright. Files carry the feature ID (`tests/store_conformance.rs`).

**The prescribed suite runs against three stores, not one.** `STORE01`–`STORE07` describe *a*
store; this design ships three, and a suite that exercised only one would leave the other two
unasserted. The suite is therefore parameterised, and where a store cannot satisfy a test the
disposition is per store and stated.

| ID | Contract | `LocalStorageStore` | `FetchStore` | `JsStore` | Tier |
|---|---|---|---|---|---|
| `STORE01` | set/get data and metadata | required · W-browser | **adapted**: get only; the fixture is served, not written · P | required · W-node | |
| `STORE02` | missing key → `KeyNotFound` | required · W-browser | required (HTTP 404) · P | required (thrown `LiquersError` keeps its type) · W-node | |
| `STORE03` | directory listing invariants | required · W-browser | required (derived from `keys`) · W-node | required · W-node | |
| `STORE04` | remove / removedir | required · W-browser | **`NA`** — read-only store; `STORE09` asserts the refusal | required · W-node | |
| `STORE05` | unsupported key → `KeyNotSupported` | required (`..`) | required (`..`) | required (`..`) | W-node (pure guard) |
| `STORE06` | concurrent update policy | required · W-browser | **`NA`** — no writes to race; `GET` is idempotent | required · W-node | |
| `STORE07` | works in end-to-end evaluation | required · P | required · P | required · P | P |

Two `NA`s, both on `FetchStore`, both for the same reason — the store has no write path — and both
with the same **reversing condition: if `FetchStore` ever gains a write path (a `PUT`-capable
variant), `STORE04` and `STORE06` become required for it immediately.** `STORE09` covers the
refusal itself, so nothing about the read-only decision goes unasserted.

`STORE01` on `FetchStore` is marked *adapted*, not `NA`: the contract "data and metadata come back
together and agree" is exactly what the store must do, and the only part that cannot apply is the
`set` that seeds it. The fixture is served by the test server instead.

### The four the guide does not have

`LANGUAGE-GUIDE-STORE-SCOPE-INCOMPLETE` records that §5 `STORE` asks nothing about
integration-provided stores or store configuration, and proposes these IDs. This design adopts them
provisionally — if the guide lands different numbers, these get renamed to match; they are listed
here so the coverage is not lost while the guide question is open.

| ID | Contract | Tier |
|---|---|---|
| `STORE08` | An integration-provided store satisfies the same contract as a language-defined one — the parameterised suite above *is* this test, and it fails if a store is added without being added to the matrix | W-browser + W-node |
| `STORE09` | A read-only store refuses every write with `KeyNotSupported`, and the router does not fall through to a writable store | W-node |
| `STORE10` | Metadata inference: extension wins over response `Content-Type`; `Content-Type` fills in when the extension is unknown; size from `Content-Length` | W-node (`infer_metadata`) + P (headers really are read) |
| `STORE11` | A router built from a configuration document routes by prefix, first match wins, and an unmatched key is `KeyNotFound` | W-node |

### Making the assertions non-vacuous

The guide singles out two shapes that pass whatever the code does. Applied here:

| Test | The vacuous version | What is asserted instead |
|---|---|---|
| `STORE06` | "either the last write won or one of them did" | `LocalStorageStore` never awaits, so there is *no* interleaving point: assert the value equals the **last** write specifically, and that its length is 1 byte (not a torn concatenation). Fails the day someone makes a storage call async. |
| `STORE05` | asserting `..` is absent from a listing it was never in | Call `get`/`set` with `parse_key("../escape")` and assert the *error type* is `KeyNotSupported`. Fails if the guard is removed. |
| `STORE09` | "the write did not succeed" | Assert the specific `ErrorType`, and separately assert the writable store at another prefix is **unchanged** — which is what catches a router that falls through. |
| `STORE02` (`JsStore`) | "it threw something" | Assert `error_type == KeyNotFound`, which fails if `js_error_to_liquers` stops preserving the discriminant. |
| Encoding round trip | round-tripping only ASCII | A fixed corpus that includes inputs which *must* take each path, plus an assertion on which path was taken (the stored envelope's first two characters). Fails if the selector inverts. |

---

## Test group 4: encoding and key guard (tier W-node, and native where possible)

The highest-value tests in the design, because they are where silent data corruption would live.

```rust
// liquers-web/tests/store_encoding.rs
const CORPUS: &[(&str, &[u8], ByteEncoding)] = &[
    ("empty",            b"",                          ByteEncoding::Utf8),
    ("ascii",            b"hello",                     ByteEncoding::Utf8),
    ("utf8 multibyte",   "héllo — ok".as_bytes(),      ByteEncoding::Utf8),
    ("emoji",            "🦀".as_bytes(),               ByteEncoding::Utf8),
    ("embedded nul",     b"a\0b",                      ByteEncoding::Utf8),
    ("invalid utf8",     &[0xFF, 0xFE],                ByteEncoding::Base64),
    ("lone surrogate",   &[0xED, 0xA0, 0x80],          ByteEncoding::Base64),
    ("png header",       &[0x89, 0x50, 0x4E, 0x47],    ByteEncoding::Base64),
];

fn encoding01_corpus_round_trips_byte_identical();   // decode(encode(x)) == x, for every entry
fn encoding02_selector_picks_the_declared_path();    // the envelope tag matches the third column
fn encoding03_corrupt_envelope_is_key_read_error();  // "1b!!!!", "2u…", "", "x" → KeyReadError
fn encoding04_large_binary_round_trips();            // 1 MiB of pseudorandom bytes
```

`ED A0 80` earns its place: it is the UTF-8 encoding of a lone surrogate, the one byte sequence
where "valid-looking text" and "valid UTF-8" diverge, and the one most likely to survive a naive
implementation and corrupt on the way back. `encoding03`'s `"2u…"` case covers a *future* format
version arriving at *today's* decoder — it must be a clean `KeyReadError`, not a misread.

```rust
// liquers-web/tests/store_key_guard.rs
fn keyguard01_parent_segment_refused();      // "../escape"      → KeyNotSupported
fn keyguard02_interior_parent_refused();     // "a/../b"         → KeyNotSupported
fn keyguard03_current_segment_refused();     // "a/./b"          → KeyNotSupported
fn keyguard04_ordinary_keys_accepted();      // "a/b.txt", "a.b/c"
```

`keyguard02` matters more than `keyguard01`: a guard that only inspects the first segment passes
`keyguard01` and lets `a/../../etc` through.

## Test group 5: configuration and the factory seam (tier N, native — fast loop)

```rust
// liquers-store/src/store_builder.rs  #[cfg(test)] mod tests
fn factory01_custom_type_is_created();          // a test factory claiming "testtype"
fn factory02_factory_precedes_builtin();        // a factory claiming "http" wins over OpenDAL
fn factory03_unclaimed_type_falls_through();    // built-ins still reachable
#[cfg(not(feature = "opendal"))]
fn factory04_gated_type_names_the_feature();    // "requires the opendal feature", not "unknown"
```

`factory02` is the one that must not be skipped: `http` is already an OpenDAL built-in, so if
factories were consulted *after* built-ins the browser could never override it, and Example 1 would
silently construct an OpenDAL store that cannot exist on wasm.

`factory04` is why the gated arm must produce a distinct message — a wasm user asking for `s3`
otherwise gets "unknown store type" for a type that plainly exists in the documentation.

## Test group 8: end-to-end (tier P)

```js
test("STORE07 a stored resource evaluates end to end", async ({ page }) => {
  // fixture server serves reference/input.txt = "hello"
  const out = await page.evaluate(() => window.env.evaluate("-R/data/input.txt/-/to_text"));
  expect(out).toBe("hello");
});

test("STORE07 a localStorage resource survives a reload", async ({ page }) => {
  await page.evaluate(() => window.env.store().set("local/in.txt", new TextEncoder().encode("hello")));
  await page.reload();
  const out = await page.evaluate(() => window.env.evaluate("-R/local/in.txt/-/to_text"));
  expect(out).toBe("hello");
});
```

The reload half is the one worth having: it is the only test that proves the derived index is
actually rebuilt from storage rather than living only in the first page's memory.

---

## Corner Cases

| # | Corner case | Risk | Handling | Test |
|---|---|---|---|---|
| C1 | Bytes that look like text but are not valid UTF-8 | Silent corruption | Encoding chosen by `from_utf8`, recorded in the envelope | `encoding01`, `encoding02` |
| C2 | A future envelope version read by today's decoder | Misread as data | Version digit checked; unknown → `KeyReadError` | `encoding03` |
| C3 | Quota exceeded mid-write | Half-written entry, corrupted accounting | Budget checked *before* `setItem`; data and metadata written in a fixed order with the data entry last, so a failure leaves no readable partial | `STORE10`, C3 test |
| C4 | Browser `QuotaExceededError` below the configured budget | Unhandled `JsValue` error | `set_item`'s `Err` is mapped to `key_write_error` regardless of the budget | C4 test |
| C5 | `RefCell` borrow held across a call into JavaScript | `already borrowed` panic | Every method drops the borrow before touching `localStorage`; reviewed method by method | `STORE06`, and a nested-call test |
| C6 | A second tab writes the same namespace | Stale derived index | **Accepted limitation**, documented. Asserted as a *known* behaviour so it cannot regress silently into something worse | C6 test |
| C7 | A key that is a prefix of a directory (`a` and `a/b` both exist) | `is_dir` and `contains` disagree | Index holds both facts independently; both may be true | `STORE03` |
| C8 | `makedir` then reload | Empty directory vanishes | Marker entry `{ns}/D/{key}`; this is why markers exist | `STORE07` reload test |
| C9 | Write to a read-only prefix in a router | Falls through to a writable store | Router does not retry; assert the other store is unchanged | `STORE09` |
| C10 | A `JsStore` method returns a rejected Promise | Opaque failure | Awaited thenable, rejection through `js_error_to_liquers` | `STORE02` |
| C11 | A `JsStore` object loses a method after registration | Late `TypeError` | Methods resolved and held at construction, so removal after registration does not affect the store | `STORE08` |
| C12 | A `js` store named in config but never registered | Confusing failure at first use | `configure_store` fails immediately, naming the missing object | `STORE11` |
| C13 | Server returns 200 with an unexpected `Content-Type` for a known extension | Wrong deserialization | Extension wins (Phase 2 precedence) | `STORE10` |
| C14 | Server does not support HEAD | `get_metadata` fails | 405/501 falls back to GET | `STORE10` |
| C15 | `${VAR}` in a browser config | Silently empty value | Not expanded; builder warns and leaves it verbatim | `STORE11` |

## Open Questions

None. The one Phase 2 question (Q8) was closed by the user — store-manipulation commands belong in
`liquers-lib`, filed as `STORE-COMMAND-NAMESPACE-MISSING`.
