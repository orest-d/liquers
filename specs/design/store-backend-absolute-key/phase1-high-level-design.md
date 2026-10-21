# Phase 1: High-Level Design — store-backend-absolute-key

## Purpose

A store must never act on a relative key (`.` or `..` in any segment). Today that is a convention:
every fallible key-taking method of every `AsyncStore` implementation must remember to check, so a
new backend or a new method can silently reintroduce `STORE-FILESTORE-PATH-TRAVERSAL`. Make the
check structural: a backend receives a key type that can only exist once the check has passed, and
the trait callers use performs that check in exactly one place.

## Problem Example

Verified at HEAD (18732dd). A backend author writes a store and forgets the check in one method:

```rust
struct NaiveStore { root: PathBuf }
#[async_trait]
impl AsyncStore for NaiveStore {
    async fn get_bytes(&self, key: &Key) -> Result<Vec<u8>, Error> {
        tokio::fs::read(self.root.join(key.encode())).await
            .map_err(|e| Error::key_read_error(key, "naive", &e))
    }
    /* … */
}
// Compiles, and reads outside `root`:
store.get_bytes(&parse_key("a/../../etc/passwd")?).await
```

Nothing fails until a conformance run (`keyshape01`) or a security report. The rule is enforced by
roughly sixty `key.as_absolute()?` lines in `liquers-core/src/store.rs`, plus `check_key` in
`liquers-web/src/store/key_guard.rs`, fallible path builders in the file and OpenDAL stores, and a
`check_key` in `store/pipeline.rs`.

What should happen: the backend implements `StoreBackend`, whose methods take `AbsoluteKey<'_>`.
`get_bytes(&self, key: AbsoluteKey<'_>)` can only be called with a checked key. The same call through
`AsyncStore` returns `ErrorType::KeyNotAbsolute`, and the backend method is never entered.

## Scope and Acceptance Criteria

- **AC-1** A backend without its own check is still protected
  WHEN a `StoreBackend` whose methods contain no absoluteness check is called through `AsyncStore`, directly (no router), with `a/../../etc`
  THEN the call fails with `KeyNotAbsolute` and the backend method is not entered
- **AC-2** Every fallible key-taking method refuses
  WHEN each fallible key-taking `AsyncStore` method (both keys of `rename_data`) is called with a relative key
  THEN each returns `KeyNotAbsolute`
- **AC-3** Routing hint
  WHEN `AsyncStore::is_supported` is called with a relative key
  THEN it returns `false` without consulting the backend
- **AC-4** Absolute keys pass unchanged
  WHEN the existing store unit tests, the conformance suites (`store_conformance_CONF.rs`) and the `liquers-lib` and `liquers-web` store tests run
  THEN they pass with their assertions unchanged
- **AC-5** The checked key cannot be forged
  WHEN code outside `liquers-core` tries to build an `AbsoluteKey` from a relative key, or without the constructor
  THEN `AbsoluteKey::new` returns `KeyNotAbsolute`, and there is no other way to make one (a `compile_fail` doc test)
- **AC-6** Backends cannot bypass the facade
  WHEN a crate other than `liquers-core` implements `AsyncStore` directly
  THEN it does not compile; it must implement `StoreBackend` (a `compile_fail` doc test)
- **AC-7** Callers are unchanged
  WHEN `liquers-lib`, `liquers-axum`, `liquers-py`, `liquers-records`, `liquers-mcp` and `liquers-agent` are built
  THEN no call site of an `AsyncStore` method or `dyn AsyncStore` changes

Non-goals: changing what an absolute key is (`Key::is_relative` is unchanged); key-*returning*
methods (`keys`, `listdir_keys`) keep returning `Key`; empty segments, reserved names and
prefix checks stay store-specific (`KeyNotSupported`); `AssetManager`, recipes and the routers keep
carrying `Key`.

## Core Interactions

- Store: `AsyncStore` becomes the caller-facing facade over a new implementor trait `StoreBackend`.
- Query: a borrowed proof type `AbsoluteKey<'a>` beside `Key`.
- Web: three stores and `key_guard` migrate; JavaScript and Python see nothing.

## Crate Placement

`liquers-core` (`query.rs` for `AbsoluteKey`, `store.rs` for the traits). Backends in
`liquers-store`, `liquers-web`, and test stores in `liquers-records` and `liquers-core/tests` migrate.

## Documentation Intent

- Reference: extend `specs/reference/STORE_SEMANTICS.md` §7 (enforced by the type, not by
  convention); `specs/reference/PROJECT_OVERVIEW.md` §5 Storage (two traits);
  `DOC_02` §Relative keys (`AbsoluteKey`).
- Guide: `specs/guides/STORE_IMPLEMENTATION_GUIDE.md` (implement `StoreBackend`; drop the "call
  `as_absolute` first" instruction); `LANGUAGE-INTEGRATION_GUIDE.md` STORE05 (still required, now
  passes by construction for Rust stores).
- Other documents: `CLAUDE.md` §Adding a Store Backend step 1 ("Implement `AsyncStore`").
- Documents to update: the five in `affects_docs`, plus `CLAUDE.md`.

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — adds a trait and a type and changes every store
  implementation's signatures (rules 3 and 4), `L`.
- **Leading issue:** **Open design question — where the check lives.** The issue proposes that
  `AsyncStore` itself take `&AbsoluteKey`, which moves the conversion to every caller (hundreds of
  call sites in `assets.rs`, `store.rs`, `liquers-axum`, `liquers-records`, `liquers-lib`) and to the
  bindings. Recommended: **split the trait** — backends implement `StoreBackend` (takes
  `AbsoluteKey<'_>`); callers keep `AsyncStore` (takes `&Key`), implemented once by a blanket impl
  that checks and delegates. The conversion then happens in exactly one place, which answers three of
  the issue's four questions (boundary, routers and `AssetManager`, bindings).
- **Explanation:** a working plan exists under the recommended answers; the remaining questions
  change the API backend authors see, so they need the maintainer.
- **Open questions:**
  1. **Open design question — seal `AsyncStore`.** Recommended: yes, so no crate can implement the
     facade directly and skip the check (AC-6). Consequence: every external `impl AsyncStore`
     becomes `impl StoreBackend` (none known outside this repository).
  2. **Proposed resolution — borrowed proof type.** `AbsoluteKey<'a>` wraps `&'a Key`, is `Copy`,
     and derefs to `Key`. Recommended over an owned `AbsoluteKey(Key)`: no clone per store call and
     no `unsafe` reference cast. `Deref` does not weaken the guarantee, because it yields `&Key`,
     which cannot be mutated, and the only constructor checks (the issue's fourth question).
  3. **Proposed resolution — key-returning methods.** Keep `Vec<Key>`. Returning `AbsoluteKey`
     would need an owned form and buys nothing a caller cannot get by calling a store again.

## Design Dependencies

- `store-key-guard` (complete, E4): **origin**. It established the rule, `as_absolute` and
  `KeyNotAbsolute`, and rejected this newtype on scope; this design reuses all three.
- `sync-store-removal` (complete): **requires** — satisfied. Only `AsyncStore` remains.
- `store-conditional-write` (`in_review`, not implemented): **overlaps** (T3, the `AsyncStore`
  trait). It adds `set_if` / `write_atomicity`. Whichever lands second adds them to both traits;
  no merge (E2: the combined scope is unrelated work).
- `resource-name-unicode` (this run): **overlaps** (T3 — both change each backend's key-to-path
  mapping). Recommended order: this design first, so that the name-validity check that design adds
  can live in `AbsoluteKey::new` and reach every backend at once. Merging would exceed `L` (E2).
- `store-streaming-read-write`, `central-storage-model`, `store-config-key-scoped-handlers` (open,
  written today or `in_review`): **overlaps** — each adds or edits store methods; whatever lands
  after this design writes them against `StoreBackend`.

## Consolidated Findings

- The check runs once, in the blanket `impl<T: StoreBackend + ?Sized> AsyncStore for T`. Backends
  can neither forget it nor skip it, and callers, `dyn AsyncStore` and the bindings do not change.
- `AbsoluteKey<'a>` is a borrowed, `Copy` proof with a private field and one checking constructor.
  It needs no `unsafe` and no clone. `async_trait` accepts its elided lifetime in arguments.
- Decorators (`SharedStore`, `ReadOnlyStore`, `MetadataPipelineStore`, `AsyncStoreRouter`) become
  `StoreBackend`s and forward `&*key` to their inner `dyn AsyncStore`, which checks again. That
  costs one segment scan, accepted for simplicity.
- The width is mechanical but large: about 45 `impl AsyncStore` blocks (11 in library code, the rest
  test stores) across `liquers-core`, `liquers-store`, `liquers-web`, `liquers-records` and the core
  integration tests. One PR is required, because the sealed facade makes a half-migrated tree
  uncompilable. Steps are ordered so each crate compiles before the next.
- The ~60 `as_absolute()?` lines and `key_guard::check_key`'s absoluteness half become redundant
  and are removed. The path builders keep their store-specific checks (reserved names).
- Conformance rules `keyshape01/02` stay. They now test the facade and keep guarding non-Rust
  stores (STORE05).
- Three decisions are open (split vs. issue's form, sealing, borrowed type). The plan assumes the
  recommended answer to each.
