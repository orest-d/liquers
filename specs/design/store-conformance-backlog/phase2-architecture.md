# Phase 2: Solution & Architecture - Store Conformance Backlog

## Overview

Seven parked conformance findings are resolved in five work areas (A–E). The only change in shared
code is one `AsyncStore` default method body. It keeps the Phase 1 decision to populate `children`
and removes the recursive cost that decision would otherwise carry. All other changes are local to
one store, one rule, or test and document files. No trait signature, type, error kind, dependency or
feature flag changes.

Reading the code showed that two of the source issues are narrower than their text says:

- **`JsStore` absence is half built already.** `JsStore::get` maps a `null`/`undefined` result to
  `KeyNotFound` (`liquers-web/src/store/js_store.rs:185`). `absence01`/`remove03` fail only because
  the conformance stub *throws* for a missing key. The protocol table does not document the
  sentinel, and `getMetadata` returning `null` yields an **empty record** rather than
  `KeyNotFound` (`store/mod.rs:64`).
- **The `http` store is not wrong; the test expectation is stale.** `infer_metadata`
  (`store/fetch.rs:84`) seeds `data_format = "csv"` from the extension and deliberately leaves
  `media_type` unset. Since the metadata-consistency work, an absent `media_type` means "derived
  from the format" (`metadata.rs:814`). `STORE10` reads the raw field and so sees `null`. Writing
  the derived value into the record would make every filename look like a declared override, which
  is the regression that comment guards against.

Reading the code also found two gaps that the issues do not mention:

- **`LocalStorageStore` also drops metadata-only keys.** `rescan` ignores `Metadata` entries
  (`local_storage.rs:197-199`), and `set_metadata` never indexes the key. The new rule `sidecar04`
  would fail this store, so it is fixed in area D.
- **The ID collision covers more than the issue lists.** `liquers-store`'s unit test `dir03` checks
  "detection does not depend on a count". Conformance rule `dir03` checks "every `listdir` entry
  answers `is_dir`". Unit tests `dir01`, `dir02` and `sibling01` coincide with their rules in
  meaning but are still duplicate IDs. Area E renames the whole set.

## Known-Issue Preflight

| Issue | Status / priority | Relation | Blocks? |
|---|---|---|---|
| `STORE-METADATA-LAYOUT-HARDCODED-PER-STORE` | draft P2 L | Area D touches two layouts (file sidecar, localStorage namespace) without abstracting them. The fix stays inside each store's existing listing code, so a later `MetadataLayout` absorbs it unchanged. | No |
| `CORE-SYNC-STORE-TRAIT-OBSOLETE` | draft P2 M | The sync `FileStore`/`MemoryStore`/`Store` default also populate `children` and drop sidecars. The sync trait is unreachable and scheduled for deletion, so **it is not changed**. The resolution of `CORE-FILE-STORE-LISTDIR-DROPS-METADATA-ONLY-KEYS` says so. | No |
| `STORE-CONFORMANCE-VALIDATION-TOOL` | accepted P2 M | Would run the suite outside test binaries; unaffected. `sidecar04` joins the registry it would expose. | No |
| `WEB-LIQUERSERROR-NOT-CONSTRUCTIBLE` | accepted P3 S | Would make the rejected "recognised thrown shape" option feasible later. The `null` sentinel does not need it. | No |
| `CORE-ERROR-STORE-NAME-NOT-STRUCTURED` | rejected | Cited by the JsStore issue as a reason not to match on message text. The design uses only `ErrorType`. | No |
| `WEB-NATIVE-IO-TIER2`, `LANGUAGE-STORE-TYPE-NOT-DEFINABLE` | accepted/draft | Future stores; they inherit the corrected contract and rules. | No |

### Blocking and Priority Decision

No blockers. All seven source issues stay P2; none meets the P0/P1 criteria.

## Data Structures

**None new.** No struct, enum or `ExtValue` change. `RuleOutcome`, `AllowedFailure`,
`StoreCapabilities` and `Capability` are reused as they are. `MetadataRecord.children:
Vec<AssetInfo>` keeps its type and meaning. `AssetInfo` has no `children` field, and area A relies on
that.

## Trait Implementations

### Area A — §2 `children`: keep eager, make it one level (`STORE-SEMANTICS-CHILDREN-…`)

**Contract decision.** Directory metadata **does** populate `children` with one `AssetInfo` per
direct child, in `listdir` order. It is one level only: a child directory's `AssetInfo` is
directory-shaped and does not describe its own children.

**Where the recursion came from.** The `AsyncStore` default `get_asset_info` calls `get_metadata`,
and a directory's `get_metadata` calls `listdir_asset_info`, which calls `get_asset_info` per child.
`AssetInfo` has no `children` field, so all that nested work is thrown away. The fix is to the
default body only (`liquers-core/src/store.rs:409`):

```rust
/// Get asset info. A directory's info is built from `default_metadata(key, true)` without
/// reading its metadata: `AssetInfo` carries no children, so populating them only to discard
/// them made one directory read walk the whole subtree (STORE_SEMANTICS §2).
async fn get_asset_info(&self, key: &Key) -> Result<metadata::AssetInfo, Error>;
// body: if self.is_dir(key).await? { let mut info = self.default_metadata(key, true).get_asset_info();
//        info.with_key(key.to_owned()); info.is_dir = true; return Ok(info); }
//       then the existing file path, with `info.is_dir = false` (the is_dir answer is already known).
```

The signature is unchanged. No implementor overrides `get_asset_info` (checked: `liquers-core`,
`liquers-store`, `liquers-web`; `liquers-py` wraps metadata but does not implement `AsyncStore`).
Afterwards, reading a directory's metadata costs one `listdir` plus one `is_dir`, and one
`get_metadata` for each *data* child. That matches a directory listing, so the cost the old sentence
warned about goes away. A side effect is one fewer `is_dir` call per file `get_asset_info` (today it
calls `is_dir` after `get_metadata`).

**`AsyncOpenDALStore`** is changed to match everyone else. Its directory branch
(`opendal_store.rs:307-312`) sets `metadata.children = self.listdir_asset_info(key).await?`, and
the comment explaining the omission is removed. This is now affordable because of the fix above.

**Rule `dir07`** is inverted and made live. It now says "directory metadata populates `children`
with the direct children only". It passes when `record.children` holds exactly the `listdir` names
of the parent (compared as a set, by the key filename of each `AssetInfo`). It fails when the list
is empty or differs from the listing. The `Blocked` branch is deleted. Registry label in
`rules/mod.rs` is updated, capabilities `[Directories, Write]` unchanged.

### Area B — `JsStore` directory metadata (`WEB-JS-STORE-HAS-NO-DIRECTORY-METADATA`)

`JsStore::get_metadata` (`js_store.rs:198`) becomes:

1. Ask the data path first (`getMetadata`, or `get` when absent) — the common case pays nothing
   extra.
2. On `Err(e)` with `e.error_type == ErrorType::KeyNotFound` (a public field; `ErrorType: PartialEq`), and **only** then: if the delegate has
   `isDir` and it answers truthy, return `default_metadata(key, true)` with `children =
   self.listdir_asset_info(key).await?` (area A contract). Otherwise re-return the `KeyNotFound`.
3. Any other error is returned unchanged — a thrown value is a failure, not a hint to try the
   directory branch.

Data-first and not `is_dir`-first, because `isDir` is optional in the protocol. With `is_dir` first,
every metadata read on a store without `isDir` would hit `KeyNotSupported`. Data-first also avoids a
second JS round trip on every file read. This depends on area C: a delegate must be able to say "no
data here", and that is exactly the sentinel. `ErrorType` is compared with `==`; there is no `match`
with a default arm.

### Area C — `JsStore` absence sentinel (`WEB-JS-STORE-CANNOT-EXPRESS-KEY-NOT-FOUND`)

- **Protocol (documented):** `get(key)` or `getMetadata(key)` returning `null`/`undefined` means
  **absent** (`KeyNotFound`). A thrown value means **failure** (`KeyReadError`, as today). Only
  this form is recognised; the thrown-`{kind:"not-found"}` alternative is rejected.
- **Code:** `get` is unchanged (it already implements this). In `get_metadata`, the `getMetadata`
  branch checks `value.is_null() || value.is_undefined()` *before* `metadata_from_js` and returns
  `Error::key_not_found(key)`. `metadata_from_js_value` keeps its null→empty-record behaviour,
  because it also serves the `set`/`setMetadata` direction, where absent metadata legitimately means
  "none supplied".
- **Compatibility:** a delegate whose `getMetadata` returned `null` for an *existing* key used to get
  an empty record and now gets `KeyNotFound`. That delegate was violating the protocol's own
  description ("→ object"). The change is recorded in the module docs, the TypeScript declaration
  (`typescript.rs:160` gains `| null | undefined` on `get`/`getMetadata` returns with a doc comment),
  and `liquers-web/README.md`.
- **Stub:** the conformance stub's `get` returns `null` for a missing key instead of throwing, so the
  test uses the protocol as documented.

### Area D — metadata-only keys are listed (`CORE-FILE-STORE-LISTDIR-DROPS-METADATA-ONLY-KEYS`)

- **`AsyncFileStore::listdir`** (`store.rs:1312-1325`): for each entry name, if it ends with
  `.__metadata__` (the `METADATA` suffix constant), strip the suffix and push the *implied* name. The
  implied name must itself pass `!RESERVED.is_reserved_name` (for example, `x.__lock__.__metadata__`
  implies a reserved name and is skipped). Other reserved names (the `.__lock__` suffix, the bare
  `__metadata__` folder) are skipped as today. The result is de-duplicated preserving first
  occurrence (data file and sidecar both imply `foo`). A `BTreeSet` or a `HashSet` guard is fine;
  order is not part of the contract.
- **`LocalStorageStore`**: `rescan`'s `EntryKind::Metadata` arm does what the `Data` arm does
  (`fresh.keys.insert(key)` and index its ancestors). `set_metadata` updates `state.keys` and
  `index_key` like `set` does. `remove` already drops both entries and rescans, so a metadata-only
  key disappears correctly.
- **Sync `FileStore`**: not changed (see preflight).
- **New rule `sidecar04`** in `rules/sidecar.rs`, registered in `rules/mod.rs` as
  `rule!("sidecar04", "a key holding only metadata is listed by its parent", "STORE_SEMANTICS.md
  §8", [StoredMetadata, Write], CreateOnly, sidecar::sidecar04)`:
  1. Take a `FreshNested { depth: 1 }` key and `require_absent`.
  2. Call `set_metadata(key, record)`. If that returns `Err(KeyNotFound)`, the rule **passes**:
     §8 gains the sentence "a store may refuse metadata for a key with no data, with
     `KeyNotFound`; a store that accepts it must list the key". Any other error goes through
     `e.into()`.
  3. `record_created(key)`, then assert `contains(key) == true` and that `key` appears in
     `listdir_keys(parent)`. `keys()` is not asserted here; `keys02` owns that.

### Area D′ — `LocalStorageStore` §4/§7/§9 (`LOCAL-STORAGE-STORE-FAILS-CONFORMANCE-IN-A-BROWSER`)

| Rule | Change in `liquers-web/src/store/local_storage.rs` |
|---|---|
| `absence03` | `removedir`: when `!is_dir`, return `Ok(())` (was `Err(key_not_found)`); `self.check(key)?` stays first so key-shape refusals remain errors (§4 last paragraph). |
| `keyshape01` | `contains`, `is_dir` and `listdir` call `self.check(key)?` first. Today only the mutating and reading methods do, so the three index lookups accept `data/../../escape.txt`. |
| `keys02` | `keys()` returns the union of `state.keys` and every `state.dirs` key that `has_key_prefix(&self.prefix)`, plus `self.prefix`, de-duplicated. That is data keys, the directories above them, and the prefix (§9). |

**Test placement:** `c9_local_storage_store` moves out of `store_conformance_CONF.rs` into a new
`liquers-web/tests/store_conformance_browser_CONF.rs`. The new file carries
`#![cfg(all(target_arch = "wasm32", feature = "browser-tests"))]` and
`wasm_bindgen_test_configure!(run_in_browser);`, matching `store_local_STORE.rs`. `C8`/`C10` stay in
the Node file. The shared `report` helper is duplicated, because it is three lines and the test files
have no common module for it.

### Area E — rule-ID ownership (`STORE-TEST-IDS-COLLIDE-WITH-CONFORMANCE-RULE-IDS`)

**Convention:** every ID family used by the conformance rule registry is **owned by conformance
rules**. Today that is `absence`, `data`, `dir`, `explicit`, `keys`, `keyshape`, `nodir`, `nokeys`,
`nomakedir`, `noremove`, `noremovedir`, `nowrite`, `prefix`, `remove`, `sibling` and `sidecar`. The
set is derived from the registry, not hard-coded, so a new family is owned as soon as its first rule
is registered. A
unit test never uses an ID from these families; it takes a descriptive name prefixed by its subject.
Store-internal families (`keyabs`, `reserved`, `pathmap`, `diridx`, `traitdef`, `STOREnn`) stay.

Renames (behaviour unchanged):

| Old | New |
|---|---|
| `liquers-store` `dir01_directory_key_is_addressable_without_directory_objects` | `opendal_directory_key_is_addressable_without_directory_objects` |
| `dir02_is_dir_on_an_absent_key_is_false_not_an_error` | `opendal_is_dir_on_an_absent_key_is_false` |
| `dir03_directory_detection_does_not_depend_on_a_count` | `opendal_directory_detection_does_not_depend_on_a_count` |
| `dir04_router_is_dir_reaches_a_prefixed_opendal_store` | `opendal_router_is_dir_reaches_a_prefixed_store` |
| `dir05_directory_metadata_is_marked_as_a_directory` | `opendal_directory_metadata_is_marked_as_a_directory` |
| `sibling01_removedir_leaves_a_prefix_sharing_sibling` | `opendal_removedir_leaves_a_prefix_sharing_sibling` |
| `sibling02_removedir_is_scoped_at_depth` | `opendal_removedir_is_scoped_at_depth` |
| `sibling03_listdir_keys_deep_excludes_siblings` | `opendal_listdir_keys_deep_excludes_siblings` |
| `sibling04_a_prefixed_store_enumerates_only_its_own_subtree` | `opendal_prefixed_store_enumerates_only_its_own_subtree` |
| `remove01_removedir_on_an_absent_directory_is_ok` | `opendal_removedir_on_an_absent_directory_is_ok` |
| `remove02_removedir_on_the_root_empties_the_store` | `opendal_removedir_on_the_root_empties_the_store` |

`traitdef01` keeps its ID: it is not in a rule family, and it is the only coverage of the default
`contains` fallback, because `C4` declares `directories: false`. The doc comment gains "same
contract as rule `dir05`, which C4 cannot run".

**Deletion pass**, per the approved decision. A renamed test is deleted only if **(a)** a conformance
rule runs against the same store with the same claim, and **(b)** breaking the behaviour on a
scratch branch makes that rule fail. Phase 3 lists the candidates; on current reading only
`opendal_is_dir_on_an_absent_key_is_false` (rule `dir02`) and
`opendal_removedir_on_an_absent_directory_is_ok` (rule `absence03`) are plausible. Everything else is
kept.

**Enforcement:** conformance test `D1` (`liquers-core/tests/conformance_docs_CONF.rs`) gains a check,
with the owned families computed from the rule registry by stripping trailing digits from each rule
ID, that no `fn` in `liquers-core/src`, `liquers-store/src`, `liquers-web/src` or their `tests/`
directories is named `<family><digits>_…` for an owned family. The check scans the source as text,
the way D1 already scans documents. Test files that *invoke* rules by ID in strings are unaffected,
because the check matches `fn` names only.

### Area F — `http` store media type (`HTTP-STORE-METADATA-DROPS-THE-EXTENSION-MEDIA-TYPE`)

**No store change.** `STORE10` (`liquers-web/tests/e2e/store.spec.ts:151`) is corrected to assert
what the level model guarantees: `csv.data_format === 'csv'` (the extension decided) and
`csv.media_type` is absent (nothing overrode it), while `blob.media_type` is a non-empty string (the
header filled a gap). The issue is closed as "expected behaviour was stale", citing the
metadata-consistency change. If a page needs the effective media type, that is a new
`getAssetInfo`-style API on `LiquersStore`, and it is filed as an issue rather than built here.

## Generic Parameters & Bounds

None added or changed.

## Sync vs Async Decisions

All changed methods are existing `async fn`s of `AsyncStore`; they stay async. No new sync code. The
sync `Store` trait is deliberately untouched. `LocalStorageStore` keeps its `RefCell` borrows scoped
to non-`await` blocks, as today: the new `keys()` and `set_metadata` indexing borrow, compute and
drop before returning.

## Function Signatures

No signature changes. New functions:

```rust
// liquers-core/src/store_conformance/rules/sidecar.rs
/// `sidecar04` — a key holding only metadata is listed by its parent.
pub async fn sidecar04(f: &dyn Fixture) -> RuleOutcome;
```

Changed bodies: `AsyncStore::get_asset_info` (default), `AsyncOpenDALStore::get_metadata`,
`AsyncFileStore::listdir`, `JsStore::get_metadata`, `LocalStorageStore::{removedir, contains,
is_dir, listdir, keys, set_metadata, rescan}`, rule `dir07`.

## Integration Points

| Crate | Files |
|---|---|
| `liquers-core` | `src/store.rs` (default `get_asset_info`, `AsyncFileStore::listdir`, `traitdef01` doc), `src/store_conformance/rules/{directories.rs, sidecar.rs, mod.rs}`, `tests/conformance_docs_CONF.rs` (D1 check) |
| `liquers-store` | `src/opendal_store.rs` (directory `children`, test renames) |
| `liquers-web` | `src/store/js_store.rs`, `src/store/local_storage.rs`, `src/typescript.rs`, `tests/store_conformance_CONF.rs` (stub, allowed failures removed, C9 moved out), new `tests/store_conformance_browser_CONF.rs`, `tests/e2e/store.spec.ts` (STORE10), `README.md` |
| `liquers-lib`, `-axum`, `-py`, `-macro`, `-records` | no change; `liquers-py` still reads `.children`, whose meaning is kept |

**Dependencies:** none added.

## Documentation Architecture

### Reference Plan

Extend `specs/reference/STORE_SEMANTICS.md` (audience: store implementers and callers):

- **§2:** replace the ⚠ unsettled bullet with the one-level `children` contract and its cost; add
  `dir07`'s new wording.
- **§4:** add a short note that a delegate store's protocol must be able to express absence, citing
  `JsStore`'s `null` sentinel as the example.
- **§8:** replace the "file stores do not yet report…" sentence with the rule and the permitted
  `KeyNotFound` refusal, and add `sidecar04` to *Enforced by*.
- **Why this document exists:** update the line saying two questions remain.
- Add a `## History` row and bump `reviewed:`.

### Guide Plan

Extend `specs/guides/STORE_IMPLEMENTATION_GUIDE.md`: a short section on **naming tests** (the owned
rule families; name unit tests by subject; D1 enforces this) and one paragraph in its delegate/page
store part (or in §5 if none exists) on signalling absence. Add a History row and bump `reviewed:`.

### Other Documents to Create

None. One issue will be filed in Phase 5 if nobody objects: `JS-STORE-WRAPPER-HAS-NO-EFFECTIVE-MEDIA-TYPE`
(P3 S, web): a page can read declared metadata but not the effective media type.

### New Reference or Guide Documents

None.

### Existing Documents to Review or Update

| Path | Change |
|---|---|
| `specs/reference/STORE_SEMANTICS.md` | as above |
| `specs/guides/STORE_IMPLEMENTATION_GUIDE.md` | as above |
| `liquers-web/README.md` | the `JsStore` absence protocol; the new browser conformance file in the browser loop |
| `specs/issues/*.md` (seven) | `status: closed` with a resolution note (Phase 5) |
| `specs/design/{store-directory-metadata-children, js-store-directory-metadata, js-store-not-found-sentinel}/DESIGN.md` | `status: superseded`, `superseded_by: store-conformance-backlog` |
| `specs/README.md` | the capability-map line for this design (regenerated plus the hand-written entry) |

`affects_docs`: `STORE_SEMANTICS.md`, `STORE_IMPLEMENTATION_GUIDE.md`.

### Design and Capability Links

The three superseded designs point here through `superseded_by`. `STORE_SEMANTICS.md` History cites
this design slug. `store-conformance-suite` (complete) is not edited.

### Evidence to Collect During Implementation

The conformance report of every suite before and after, the D1 output, and the scratch-branch
break-and-fail results for each deletion candidate.

## Relevant Commands

### New Commands

None. This is store-layer work.

### Relevant Existing Namespaces

None directly. Stores are reached through `-R/` resources; the `ns-store` command namespace does not
exist (`STORE-COMMAND-NAMESPACE-MISSING`). No command namespace needs review. The user approved
Phase 1 with this scope, so no separate namespace question was raised.

## Error Handling

### Error Constructors

Only existing ones: `Error::key_not_found(key)` (JsStore sentinel, JsStore directory fallback),
`self.check(key)?` / `key.as_absolute()?` for key-shape refusal. No `Error::new`, no new
`ErrorType`.

### Error Scenarios

| Scenario | Result |
|---|---|
| `JsStore` delegate throws in `get` | `KeyReadError` (unchanged), and no directory fallback |
| `JsStore` `get` → `null`, `isDir` absent | `KeyNotFound` |
| `JsStore` `get` → `null`, `isDir` → true | directory metadata with children |
| `LocalStorageStore::removedir` on an absent key | `Ok(())` |
| `LocalStorageStore::contains` on a relative key | `Err(KeyNotAbsolute)` |
| `sidecar04` against a store refusing metadata-only | `set_metadata` → `KeyNotFound` → pass |
| Directory `get_metadata` when a child's `get_metadata` fails | propagates, as today |

## Serialization Strategy

None changed. `MetadataRecord.children` serialization is unchanged; it is populated by one more
store (OpenDAL).

## Risk Review

| Risk | Mitigation |
|---|---|
| `get_asset_info` default change alters a caller's result | It returned `is_dir: true` plus directory-shaped fields before as well. Only discarded work goes away. Existing `dir04`, `dir06` and `dir07` cover this, and area A adds a count-of-calls unit test in Phase 3. |
| OpenDAL directory reads become slower | One listing plus per-child stat, bounded by one level. Phase 3 adds a test that a nested tree is not walked. |
| JS delegates relying on `getMetadata → null` meaning "empty" | Documented break; TypeScript declaration updated; no in-tree user (checked: e2e, examples). |
| Renames lose a test | Renames only change names; the deletion pass is gated on break-and-fail evidence. |

## Review Outcome

Two independent reviews were run (Phase 1 conformity; codebase alignment). There were **no blocking
findings**, so no fixer pass was needed. One correction was made inline: `Error::error_type` is a
public field, not a method (area B). The advisory findings are recorded here as accepted deviations
from the source issues' own "Expected behaviour":

1. **`http` media type (F):** the test expectation is corrected instead of the store. Writing the
   derived media type would regress the metadata level model.
2. **Sync `FileStore` (D):** not fixed. The sync trait is obsolete and unreachable
   (`CORE-SYNC-STORE-TRAIT-OBSOLETE`), which is stated in the issue's closing note.
3. **`getMetadata` → `null` (C):** the source issue called the sentinel fully backward-compatible. It
   is for `get`, but not for a `getMetadata` that returned `null` for an existing key. That case is
   a documented break (see area C, *Compatibility*).
