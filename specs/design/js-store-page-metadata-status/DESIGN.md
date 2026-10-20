---
id: JS-STORE-PAGE-METADATA-STATUS
kind: design
title: Stored data without a status is loadable
form: compact
workflow: liquers-project
status: in_review
phase: architecture
area: [core/assets, web]
issues: [JS-STORE-RESOURCE-NOT-FOUND-WITHOUT-A-RECIPE]
created: 2026-10-20
---
# Stored data without a status is loadable

## Phase 1: High-Level Design

### Purpose

`STORE07` for the `js` store fails: a resource that a page-implemented store serves cannot be
evaluated. The guide requires every selected store to work in end-to-end evaluation, so the
README's `STORE` row is marked complete while one of its three stores does not.

### Problem Example

The e2e test `STORE07 a JavaScript store evaluates end to end`
(`liquers-web/tests/e2e/store.spec.ts`) registers this object, routes the `mem` prefix to it and
evaluates `-R/mem/greeting.txt/-/to_text`:

```js
{
  async get(key) {
    if (key !== 'mem/greeting.txt') return undefined;
    return { data: new TextEncoder().encode('from the page'),
             metadata: { media_type: 'text/plain' } };
  },
  async contains(key) { return key === 'mem/greeting.txt'; },
}
```

**Today** it fails with `No recipe found for key mem/greeting.txt`. **It should** return
`'from the page'`.

**Cause.** A keyed resource is loaded by `AssetData::try_fast_track`
(`liquers-core/src/assets.rs`). It calls `contains`, which answers `true`, and then `get`. Then it
reuses the bytes only when the stored status is `Ready`, `Source` or `Override`. Otherwise it
treats them as a value that must be recomputed and falls through to the recipe provider. There is
no recipe for this key, hence the error. The page's metadata has no `status`, so it reads as
`Status::None`.

**This is not specific to JavaScript** (checked 2026-10-20 with a throwaway native test).
`AsyncMemoryStore::set` and `AsyncFileStore::set` store the caller's record as given, with only the
filename seeded. So `store.set(a/b.txt, b"hello", <record with no status>)` followed by evaluating
`-R/a/b.txt` fails natively in the same way ("No recipes defined…"). With status `Source`, the same
test returns `hello`. The `http` and `localstorage` cases of `STORE07` pass only because those
stores write `Source` themselves.

The rule the fast track should apply is already documented, but the status check comes before the
code that applies it. `ASSETS.md` "Content changed outside Liquers" recognises **a file with no
metadata** as a stored `Source` or `None` with recorded version 0. It treats that file as `Source`
when there is no recipe and as `Ready` when there is one (`external_change_status`). But
`try_fast_track` refuses `None` at the status gate, before `decide_external_change` reaches that
mapping.

**A second, `liquers-web`-only gap.** `MetadataRecord` is `#[serde(default, deny_unknown_fields)]`,
so `{ media_type: 'text/plain' }` parses as a *complete* record on `metadata_from_js_value`'s first
branch (`liquers-web/src/store/mod.rs`). Its `key` and `filename` stay unset, so its data format
falls back to `bin`. Only objects that carry unknown fields reach the overlay branch, which is the
one that seeds the key.

### Scope and Acceptance Criteria

In scope: the status that core's fast-track and store fallback read for stored bytes, and the key
and filename seeding in `liquers-web`'s `metadata_from_js_value`. Out of scope: rewriting the
stored records themselves.

- **AC-1** A page store with only `get` and `contains` serves a resource query
  - WHEN a `js` store's `get` returns data with metadata that has no `status`, and
    `-R/<prefix>/<key>/-/to_text` is evaluated
  - THEN the result is the store's text, and no recipe is consulted
- **AC-2** Missing metadata is treated the same as partial metadata
  - WHEN `get` returns `{ data }` with no `metadata` at all
  - THEN the key evaluates the same way, and the record has the key, the filename and a data format
    derived from the extension
- **AC-3** A status the page declares is kept
  - WHEN `get` returns metadata with `status: 'error'` (or any status other than `None`)
  - THEN that status is unchanged, and fast-track refuses it as it does today
- **AC-4** A page write evaluates afterwards
  - WHEN `env.store().set(key, bytes, {})` writes to a `js` store with a `set` method, and the key
    is then evaluated
  - THEN the bytes are served
- **AC-5** The existing e2e test passes unchanged
  - WHEN `npx playwright test` runs `liquers-web/tests/e2e/store.spec.ts`
  - THEN `STORE07 a JavaScript store evaluates end to end` passes

- **AC-6** Core stores behave the same way
  - WHEN bytes are written to `AsyncMemoryStore` or `AsyncFileStore` with a record that has no
    status and no version, and the key is then evaluated
  - THEN the bytes are served as `Source` when the key has no recipe, and as `Ready` when it has one
- **AC-7** A dependency with no status does not block its dependent
  - WHEN a stored dependency of a fast-tracked asset has status `None`
  - THEN the dependency check treats it as inconclusive, the same as an unreadable one, and does not
    refuse the fast track

### Open Questions

None open. Decided:

1. **Where the default lives:** core's fast track, chosen by the user on 2026-10-20, superseding
   the adapter-only answer this phase first recommended.
2. **`getMetadata` without data:** `metadata_from_js_value` seeds the key and the filename, and the
   status is left as the page gives it.
3. **The shared helper:** fixed for every caller, so it seeds the key on both branches.

### Scope Changes

2026-10-20, after Phase 1 was approved: the native check showed that the core stores fail the same
way. The fix moves from `JsStore` into `try_fast_track`, `AC-4`'s THEN no longer prescribes the
record passed to the page's `set`, and `AC-6` and `AC-7` are added.

Readiness: not assessed. Complexity is still `M`, mostly in tests.

## Phase 2: Architecture

### Solution

Give the asset layer one rule for the status of stored bytes, and apply it everywhere it reads a
stored record together with its data. A record whose status is `None` and whose recorded version
is unknown is the documented **file with no metadata**. It reads as `Source` when the key has no
recipe and as `Ready` when it has one, which is exactly what `external_change_status` already
returns. Every other status is read as stored.

When a status is mapped, the asset's in-memory metadata gets the mapped status **and a warning in
its log**, requested by the user on 2026-10-20. The warning names the key and the status it was
read as, for example:

```text
no status was stored for mem/greeting.txt; read as Source
```

Nothing is written back to the store. The asset's metadata is persisted only by the paths that
already persist it, so a read never writes. That keeps `read_only` mounts and the data-only
contract intact.

Rejected alternatives:
- **Normalise in every store's `set`** (memory, file, `JsStore`, plus `JsStore::get`). That is
  four places to keep in step, and data placed in a backend from outside Liquers would still
  fail.
- **Write the mapped status back on read.** A read would then write, which `ReadOnlyStore` would
  refuse.
- **Map every no-version record, as `external_change_status` does.** That would turn an existing
  `Source` into `Ready` whenever a recipe exists, changing what the fast track reports today.
  Only `None` is mapped.

Known-issue preflight: `EXTENDED-FAST-TRACK`, `ASSETS-FIX1` and
`UNCACHED-STORED-COPY-EXPIRY-RACES-AN-INFLIGHT-EVALUATION` touch the same code. None of them blocks
this change or has to go first. No command namespace is involved.

### Changes

`liquers-core/src/assets.rs`:

```rust
/// The status stored bytes are reused under: `stored`, except that a record with no status and
/// no recorded version (a file with no metadata) reads as `external_change_status` maps it.
pub fn reusable_stored_status(stored: Status, recorded: Version, has_recipe: bool) -> Status;

/// Applies `reusable_stored_status` to `metadata` in place. When the status changes, it adds a
/// warning naming `key` and the new status. The recipe provider is asked (`can_make`) only when
/// the stored status is `None`, so the common path costs nothing extra.
async fn adopt_stored_status<E: Environment>(
    manager: &E::AssetManager,
    key: &Key,
    metadata: &mut Metadata,
) -> Result<(), Error>;
```

Call sites, each before its status check:
- `AssetData::try_fast_track`, before the `Ready | Source | Override` gate.
- The store branches of `get_any_status` and `get_binary_any_status` in both
  `DefaultAssetManager` and `ImmediateAssetManager`, before `has_data()`.
- `to_override` keeps its `has_data()` gate unchanged, since it changes a status deliberately and
  is not a read.
- `dependency_blocks_fast_track` gets a `Status::None` arm that answers `false` (inconclusive),
  without asking for the recipe (AC-7).

`liquers-web/src/store/mod.rs` `metadata_from_js_value`: on the complete-record branch, set `key`
when the record has none and call `liquers_core::store::seed_filename_from_key`. That makes both
branches seed the same fields, so `JsStore::get`, `getMetadata` and the `LiquersStore` wrapper all
pick it up.

There are no new errors, and `can_make` errors propagate as they already do in
`decide_external_change`. Everything is async, as today. No command or registry changes, so
`specs/command_registry.yaml` is untouched.

Documents:
- `specs/reference/ASSETS.md`: "Content changed outside Liquers" and the fast-track paragraph say
  the no-metadata rule now also governs reuse, with the warning.
- `specs/reference/STORE_SEMANTICS.md` §10: a note that a store may return data with no status.
- `liquers-web/README.md`: no change. The `STORE` row becomes true.

Each reference document gets a History row and a `reviewed:` bump.

### Risks

- **Behaviour change:** a key stored with status `None` that has a recipe now reuses its bytes as
  `Ready` instead of recomputing them. That matches what the external-change table already
  promises. A test that relied on recomputation would show it, and the Phase 3 suite searches for
  such tests.
- **Tests likely to change:** the fast-track tests in `assets.rs`, plus
  `liquers-core/tests/metadata_only_entry_reload.rs`. A metadata-only entry has no data, so `get`
  answers `KeyNotFound` before the new rule runs, and nothing should change there.
- **Recovery:** revert the call sites, since the helper is pure. Certainty is high: the native
  check already showed `Source` loading and `None` failing.
