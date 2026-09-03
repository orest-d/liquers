# Phase 2: Solution & Architecture — Sidecar-colliding keys refused by the path builders

## Overview

One type — `ReservedNames` — owns the rule "which names does a sidecar metadata layout make
unusable". Three kinds of caller consult it, and the bug is precisely that today only the first
does:

1. **`is_supported`** — the routing hint. Already correct in all three stores, filename-scoped.
2. **The path builders** — `key_to_path`, `key_to_path_metadata`, `key_to_lock_path`,
   `key_to_path_dir`. Refuse with `KeyNotSupported`, which every fallible method then inherits
   through `?`. This is the fix.
3. **The listing filters** — `listdir`, and OpenDAL's `listdir_keys_deep`. Not optional: without
   them a store containing a legacy `__metadata__` folder returns a key from `keys()` that
   `is_dir` then refuses, and `listdir_keys_deep` propagates that error, making the whole
   enumeration fail. `STORE_SEMANTICS.md` §8 already requires listings to *skip* what they cannot
   address rather than fail.

`ReservedNames` lives in `liquers-core::store` and is used by `liquers-store` too, which is legal
under the dependency flow and is what keeps one definition of the rule for both sidecar
implementations.

## Known-Issue Preflight

Open issues touching `core/store` / `store/backends`, and what each means here.

| Issue | P/C | Relation | Blocking? |
|---|---|---|---|
| `CORE-FILE-STORE-WRITES-METADATA-COLLIDING-KEYS` | P1 M | The issue being fixed. Its `M` is corrected to `L` at Phase 5, since `PathMap` puts the change in a second crate. | No |
| `STORE-METADATA-LAYOUT-HARDCODED-PER-STORE` | P2 L | Filed from the Phase 1 gate; the successor. `ReservedNames` is deliberately the *narrow* half — reserved names only, not where metadata lives — so it becomes a field of a future `MetadataLayout` rather than something that design has to unpick. | No |
| `CORE-ERROR-STORE-NAME-NOT-STRUCTURED` | P2 S | `Error::key_not_supported` needs a store name, which is why the rule is a predicate and the *store* raises the error (`opendal_store.rs:60-65`). If that issue lands, the refusal could move into the predicate. Design accommodates it; does not depend on it. | No |
| `CORE-SYNC-STORE-TRAIT-OBSOLETE` | P2 M | The sync `FileStore` is fixed here. If the trait is later removed, this code goes with it — no conflict, and leaving the bug in place is worse than fixing code that may be deleted. | No |
| `STORE-TEST-IDS-COLLIDE-WITH-CONFORMANCE-RULE-IDS` | P2 M | **Constrains this change.** New unit tests must not reuse a conformance rule family. Confirmed at HEAD the families are `absence`, `data`, `dir`, `explicit`, `keys`, `keyshape`, `nodir`, `nokeys`, `nomakedir`, `noremove`, `noremovedir`, `nowrite`, `prefix`, `remove`, `sibling`, `sidecar`. This design uses **`reserved01`–`reserved05`**, which is unused in both schemes. | No |
| `CORE-STORE-ROUTER-KEYS-FAILS-ON-AN-EMPTY-MEMBER` | P2 S | Same failure *shape* as the listing hazard in point 3 above — one member erroring kills the enumeration. Independent cause; this design must not add a second instance of it, which is why the listing filters are in scope. | No |
| `RESOURCE-NAME-ASCII-ONLY` | P2 L | `__metadata__` is `[A-Za-z0-9_.-]` and parses at HEAD and after the planned narrowing to `is_ascii_alphanumeric` plus `_ . -`. No interaction. | No |
| `STORE-ABSOLUTE-KEY-NOT-TYPE-ENFORCED` | P3 L | Same theme (a rule re-checked at each call site rather than carried by a type). A future `AbsoluteKey` would carry this refusal too; nothing here obstructs it. | No |
| `STORE-SEMANTICS-CHILDREN-RULE-CONTRADICTS-EVERY-STORE` | P2 S | Touches §8's neighbouring paragraph on directory metadata, not the sidecar rule. No overlap. | No |

**No blockers.** Nothing above must be resolved first, and no priority change is recommended.

## Data Structures

One new type, in `liquers-core/src/store.rs`. No fields on any store change; `ReservedNames` is a
zero-sized-in-practice `Copy` value holding a `'static` slice, so a store keeps it as a constant
rather than as state.

```rust
/// The names a metadata layout reserves, and therefore the keys a store using it must refuse.
///
/// A sidecar layout keeps the metadata for `foo` at `foo.__metadata__`, which makes two shapes
/// unaddressable: the key `foo.__metadata__`, whose *data* path is that same byte string, and —
/// because earlier Liquers versions kept metadata in a `__metadata__` *folder*
/// (`parent/__metadata__/filename.json`) — the bare name itself, which stays reserved so that
/// layout can be supported again (`STORE-METADATA-LAYOUT-HARDCODED-PER-STORE`).
///
/// A predicate rather than a fallible function because [`AsyncStore::is_supported`] returns
/// `bool` and cannot carry an error; keeping the rule in one place is what lets `is_supported`,
/// the path builders and the listing filters ask the same question. The *store* raises the error,
/// because `Error::key_not_supported` needs a store name this type cannot reach
/// (`CORE-ERROR-STORE-NAME-NOT-STRUCTURED`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReservedNames {
    /// Each entry is a dotted suffix, e.g. `".__metadata__"`. Both the suffix form
    /// (`x.__metadata__`) and the bare form (`__metadata__`) are reserved by one entry.
    suffixes: &'static [&'static str],
}
```

### Provenance of the bare `__metadata__` reservation

The widening to the bare folder name is not speculative. It restores a rule the **predecessor
Python implementation** (`github.com/orest-d/liquer`, `liquer/store.py` at `2eb4e64`) has and the
Rust port dropped:

| Python Liquer | Line | Rule |
|---|---|---|
| `METADATA = "__metadata__"` | 401, 1303, 1499 | the bare name, in three store classes |
| `p.parent / self.METADATA / (p.name + ".json")` | 426 | the layout itself — metadata for `sub/foo.txt` at `sub/__metadata__/foo.txt.json` |
| `assert p.name != self.METADATA` | 420, 425, 1316, 1321, 1326 | refuses the bare name as a filename |
| `assert key_name(key) != self.METADATA and ("/" + self.METADATA + "/") not in key` | 1513, 1526 | refuses it as a filename **and in any interior position** |
| `if d.name != self.METADATA` | 543, 1468-1469 | filters it out of listings |

The last two rows are exactly the two things this design adds: the interior-segment rule, and the
listing filter. The Rust port kept the refusal but narrowed it to the filename, and moved the
layout from a folder to a `.__metadata__` sidecar without carrying the folder name forward as
reserved. So the two reserved forms have different origins and both are load-bearing — the bare
name is the predecessor's, the dotted suffix is the Rust era's — and a store that may one day read
either layout must refuse both.

Recorded here because there is **no evidence of this layout in the `liquers` repository at all**:
no archived spec, no git history, no code. A future reader finding a reservation with no visible
reason is the one most likely to remove it, so the citation belongs in the `ReservedNames` doc
comment as well as here.

There are **no named preset constants** — settled at the Phase 2 gate, replacing an earlier
`SIDECAR` / `SIDECAR_AND_LOCK` pair. "Sidecar" names a layout *style*, which is not what a reader
of the constant needs to know; and a shared preset forces two stores to agree that one label
describes them both, when the design's own rule is that each store reserves what its own layout
uses. Instead each store spells out its set, which is self-documenting at the point it matters:

```rust
/// The metadata sidecar suffix — the metadata for `foo` lives at `foo.__metadata__`.
/// One definition, replacing the three copies of this string literal at HEAD
/// (`store.rs:883`, `store.rs:1242`, `opendal_store.rs:71`).
pub const METADATA_SUFFIX: &str = ".__metadata__";
/// The lock-file suffix the file stores take while writing (`store.rs:884`).
pub const LOCK_SUFFIX: &str = ".__lock__";

impl ReservedNames {
    /// Declares the suffixes a layout uses. `const` so a store can hold the result as an
    /// associated constant rather than building it per call.
    pub const fn new(suffixes: &'static [&'static str]) -> Self { Self { suffixes } }
}
```

Read at each store, the declaration says exactly what that store reserves and why:

```rust
impl AsyncFileStore {
    const RESERVED: ReservedNames = ReservedNames::new(&[METADATA_SUFFIX, LOCK_SUFFIX]);
}
impl FileStore {                        // no lock files
    const RESERVED: ReservedNames = ReservedNames::new(&[METADATA_SUFFIX]);
}
impl PathMap {                          // no lock files
    pub const RESERVED: ReservedNames = ReservedNames::new(&[METADATA_SUFFIX]);
}
```

`&'static [&'static str]` rather than `Vec<String>`: the set is fixed at compile time for every
store in the tree, so this keeps `ReservedNames` `Copy` and usable in a `const`. When
`STORE-METADATA-LAYOUT-HARDCODED-PER-STORE` makes layouts configurable, that design builds a
`ReservedNames` from its configured suffixes and the field becomes owned — a change inside this
type, not at any call site, and one `new` already anticipates.

## Trait Implementations

`ReservedNames` implements nothing — deliberately. It derives `Debug, Clone, Copy, PartialEq, Eq`
(all semantically sound: it is a small value type) and exposes two inherent predicates. Making it a
trait now would prejudge `STORE-METADATA-LAYOUT-HARDCODED-PER-STORE`, whose trait has a wider job
(where metadata is written, whether it is stored at all, whether derivation is cached).

No `AsyncStore` or `Store` trait method is added, removed or re-signed, so no implementor outside
this change — `liquers-py` included — is affected. The three stores gain **inherent** methods and
one associated constant each.

## Sync vs Async

Everything added is synchronous and pure: string comparison over key segments, no I/O. The
predicates are called from both async (`AsyncFileStore`, `AsyncOpenDALStore`) and sync (`FileStore`)
code and from `is_supported`, which is a non-async trait method — so a sync predicate is the only
shape that works in all three. This does not violate "async is the default": there is no I/O to
make async.

## Function Signatures

### New, in `liquers-core/src/store.rs`

```rust
impl ReservedNames {
    /// True when one directory-entry name is reserved.
    ///
    /// Both forms of every suffix: `foo.__metadata__` (the sidecar of `foo`) and `__metadata__`
    /// (the legacy metadata folder). Takes `&str` because the listing filters have a name and no
    /// key, and building a key per entry to ask would allocate for nothing.
    pub fn is_reserved_name(&self, name: &str) -> bool;

    /// True when **any** segment of the key is reserved.
    ///
    /// Any, not the last: `dir.__metadata__/child` needs `dir.__metadata__` to be a directory,
    /// while the metadata of `dir` needs it to be a file, so the key is unaddressable even though
    /// its filename is innocent. `Key::filename()` is the last segment only, which is why the
    /// filename-scoped check missed this.
    pub fn is_reserved_key(&self, key: &Key) -> bool;
}
```

Implementation sketch (Phase 4 writes it; this pins the semantics):

```rust
pub fn is_reserved_name(&self, name: &str) -> bool {
    self.suffixes.iter().any(|suffix| {
        name.ends_with(suffix) || suffix.strip_prefix('.').is_some_and(|bare| name == bare)
    })
}

pub fn is_reserved_key(&self, key: &Key) -> bool {
    key.iter().any(|segment| self.is_reserved_name(&segment.name))
}
```

`Key::iter()` and `ResourceName::name` are both public at HEAD (`query.rs:1444`, `query.rs:698`),
so no `Key` API changes.

### Changed, in `liquers-core/src/store.rs`

```rust
impl AsyncFileStore {
    const RESERVED: ReservedNames = ReservedNames::new(&[METADATA_SUFFIX, LOCK_SUFFIX]);

    /// Raises the refusal, because `Error::key_not_supported` needs this store's name.
    fn reject_reserved(&self, key: &Key) -> Result<(), Error>;         // new, private

    pub fn key_to_path(&self, key: &Key) -> Result<PathBuf, Error>;          // + reject_reserved
    pub fn key_to_path_metadata(&self, key: &Key) -> Result<PathBuf, Error>; // + reject_reserved
    fn key_to_lock_path(&self, key: &Key) -> Result<PathBuf, Error>;         // + reject_reserved
}

impl FileStore {           // identical, minus the lock file it does not have
    const RESERVED: ReservedNames = ReservedNames::new(&[METADATA_SUFFIX]);
    fn reject_reserved(&self, key: &Key) -> Result<(), Error>;
    pub fn key_to_path(&self, key: &Key) -> Result<PathBuf, Error>;
    pub fn key_to_path_metadata(&self, key: &Key) -> Result<PathBuf, Error>;
}
```

`is_supported` in both keeps its signature and its meaning, and its body becomes
`!key.is_relative() && key.has_key_prefix(&self.prefix) && !Self::RESERVED.is_reserved_key(key)` —
which is where it *widens*: today it asks `key.filename()`, so it accepts `dir.__metadata__/child`.

`listdir` in both keeps its signature; its filter becomes
`!Self::RESERVED.is_reserved_name(&name)`, replacing the two hand-written `ends_with` calls in
`AsyncFileStore` (`store.rs:1209`) and the single one in `FileStore` (`store.rs:1444`) — which is
where it *widens* to the bare folder name.

### Changed, in `liquers-store/src/opendal_store.rs`

```rust
impl PathMap {
    /// What this store's layout reserves. Replaces the private `const METADATA` at
    /// `opendal_store.rs:71`.
    pub const RESERVED: ReservedNames = ReservedNames::new(&[METADATA_SUFFIX]);

    // pub fn is_suffix_ambiguous(key: &Key) -> bool;   ← REMOVED
}
```

**`PathMap::is_suffix_ambiguous` is removed**, settled at the Phase 2 gate. After the widening the
name is wrong twice over: the rule is no longer about a suffix (the bare `__metadata__` folder
carries none) and no longer about the filename. Keeping it as a wrapper would leave two names for
one rule across two crates, which is what the conformance design spent its effort eliminating. Its
two production call sites — `reject_ambiguous` (`opendal_store.rs:144`) and `is_supported`
(`:521`) — call `PathMap::RESERVED.is_reserved_key(key)` instead, so the rule has one name
everywhere.

This removes a `pub` function from `liquers-store`. Nothing in-tree outside those call sites and
two unit tests uses it, and `liquers-py` does not depend on `liquers-store`.

Two test names lose the wrong word while **keeping their IDs**, so cross-references and `D1` still
resolve:

| At HEAD | After |
|---|---|
| `pathmap03_suffix_ambiguous_keys_are_refused_everywhere` | `pathmap03_reserved_keys_are_refused_everywhere` |
| `pathmap07_directory_form_refuses_suffix_ambiguous_keys` | `pathmap07_directory_form_refuses_reserved_keys` |

`reject_ambiguous`, `key_to_path`, `key_to_path_metadata` and `key_to_path_dir` keep their
signatures and their structure — they already consult the predicate, which is the whole point of
copying this store's shape. `listdir` and `listdir_keys_deep` gain one `continue` on a decoded key
that is reserved.

## Integration Points

| Crate / file | Change |
|---|---|
| `liquers-core/src/store.rs` | `ReservedNames`, `METADATA_SUFFIX`, `LOCK_SUFFIX`; `AsyncFileStore` and `FileStore` path builders, `is_supported`, `listdir`; unit tests `reserved01`-`reserved05` |
| `liquers-core/tests/store_conformance_CONF.rs` | `C2` drops its `AllowedFailure` for `sidecar03` and gains `.with_unsupported_shape(collide.__metadata__)`, which makes `prefix03` and `sibling05` runnable there for the first time |
| `liquers-store/src/opendal_store.rs` | `PathMap::is_suffix_ambiguous` removed in favour of `PathMap::RESERVED`; `listdir` and `listdir_keys_deep` skip reserved decoded keys; `pathmap03`/`pathmap07` renamed (IDs kept) |
| `liquers-store/tests/store_conformance_CONF.rs` | No change needed — its fixture already declares both shapes; re-run confirms no regression |
| `liquers-axum` | None. Handlers call the store directly (`store/handlers.rs:80`) and now surface `KeyNotSupported` instead of writing through |
| `liquers-web`, `liquers-py`, `liquers-lib` | None |

**Caller survey (Phase 1 open question 4).** `grep` for `__metadata__` across the tree finds it only
in `store.rs`, `opendal_store.rs`, the conformance fixture and rules, and the two conformance test
files. **Nothing in-tree constructs a reserved key.** The only route to one is user input:
`liquers-axum`'s `PUT /api/store/data/{key}`, its bulk-upload handler (`handlers.rs:575`, `:700`),
and any application passing a filename through to `set`. Those are exactly the callers meant to
start failing.

## Documentation Architecture

| Path | Kind | Audience | Change | Links |
|---|---|---|---|---|
| `specs/reference/STORE_SEMANTICS.md` | reference | internal | **Extend §8.** Restate the rule as *reserved names, in any segment, declared by the store's metadata layout*, replacing the filename-scoped wording. Name the three instances (`*.__metadata__`, the bare `__metadata__` folder, the file stores' `*.__lock__`), state that each store reserves what its own layout uses, and that listings skip reserved names rather than failing — tying the existing "a path a store cannot decode is skipped" sentence to this rule. `## History` row, `reviewed: 2026-09-03`. | → `STORE_IMPLEMENTATION_GUIDE.md`, → `STORE-METADATA-LAYOUT-HARDCODED-PER-STORE` |
| `specs/guides/STORE_IMPLEMENTATION_GUIDE.md` | guide | internal | **Extend §"The key space".** The existing bullet says to refuse from `is_supported` *and* the path builders; add *how* — one predicate (`ReservedNames`), because `is_supported` returns `bool` and cannot carry an error — the listing filter as the third caller, and the failure mode: `is_supported` is a routing hint, so a store that refuses only there corrupts data through any direct caller. `## History` row, `reviewed: 2026-09-03`. | → `STORE_SEMANTICS.md` §8 |
| `specs/issues/CORE-FILE-STORE-WRITES-METADATA-COLLIDING-KEYS.md` | issue | — | `status: closed` at Phase 5, with a resolution note and `complexity: L` corrected | → this design |
| `specs/index.csv`, `specs/README.md` | — | — | Regenerated by `scripts/docs_index.py` | — |

**Authoritative `affects_docs`:** `[STORE_SEMANTICS, STORE_IMPLEMENTATION_GUIDE]`. Candidates
generated by area (`core/store`, `store/backends`) and **rejected**: `STORE_CONFIG_FSD`,
`STORE_FACTORY_GUIDE`, `ENVIRONMENT_CONFIG`, `ENVIRONMENT_CONSTRUCTION_GUIDE` (construction and
configuration, untouched — no store type, argument or factory changes); `CONFORMANCE_TERMS` (the
vocabulary is unchanged; no rule is added or renamed); `PROJECT_OVERVIEW` (no core concept, Query
or Key encoding change); `LANGUAGE-INTEGRATION_GUIDE` (no integration-owned store is affected).

No new documents. `STORE-METADATA-LAYOUT-HARDCODED-PER-STORE` carries the subsystem question and
this design does not pre-empt it.

## Relevant Commands

**No new commands, and no existing command is involved.** This is a store-layer refusal; nothing
reaches the command registry, no `register_command!` invocation changes, and
`specs/command_registry.yaml` does not need regenerating.

No existing namespace is relevant either — not `lui`/`egui` (no UI), not `pl` (no DataFrames), not
`ns-img`. The one namespace that would touch stores, `STORE-COMMAND-NAMESPACE-MISSING` (P3, M), is
an unimplemented feature; when it exists, its commands will inherit the refusal through the store
rather than needing to know about it.

*User confirmation requested at this gate: that "no commands are relevant" is the right answer, and
no store-facing command namespace should be considered in scope.*

## Error Handling

`Error::key_not_supported(key, &self.store_name())` → `ErrorType::KeyNotSupported`, matching
`AsyncOpenDALStore::reject_ambiguous` exactly. Never `Error::new`.

Not `KeyNotAbsolute`: the same path builders raise that for a traversal (`../SECRET.txt`), and it
means "this is not a store address at all". A reserved key *is* a well-formed address — this store
cannot represent it. `keyabs08`/`keyabs09` assert `KeyNotAbsolute` for traversal shapes and must
keep passing unchanged, which they do: `as_absolute()?` runs first in every builder, so a key that
is both relative and reserved still reports `KeyNotAbsolute`. That ordering is deliberate and is
asserted by `reserved05`.

Where each refusal surfaces, and why nothing is left half-done:

| Method | Refused by | Side effects before the refusal |
|---|---|---|
| `get`, `get_bytes`, `get_metadata`, `contains`, `is_dir`, `listdir`, `makedir` | `key_to_path` / `key_to_path_metadata` | none |
| `set`, `set_metadata`, `remove`, `removedir` | `acquire_lock` → `key_to_lock_path`, which runs **first** | none — the lock is taken before `create_dir_all` and before any write |
| OpenDAL equivalents | `reject_ambiguous`, unchanged | none |

No `unwrap`/`expect`, no new error type, no `_ =>` match arm (nothing new matches an enum).

## Open Questions Resolved

- **Where the predicate lives** (Phase 1 Q1) → `liquers-core::store`, used by `liquers-store` too.
  One rule, one definition, forward-dependency only, and one *name* — see the gate decision below.
- **Interior-segment coverage** (Phase 1 Q2) → unit tests `reserved01`-`reserved05` in `store.rs`,
  not a new conformance rule. `GenericFixture` holds one `unsupported_shape` key, and adding a
  second shape to the fixture would change the suite's API for one store's benefit. The suite's job
  is the *contract*; the segment-level rule is checked where it is implemented. `C2` still gains
  the filename shape, which turns on `prefix03` and `sibling05`.
- **Listing filters** (Phase 1 Q3) → in scope and mandatory, per Overview point 3.
- **Caller survey** (Phase 1 Q4) → see Integration Points. Nothing in-tree writes a reserved key.

## Settled at the Phase 2 Gate

Both remaining naming questions were decided by the user, and one confirmation given:

1. **`PathMap::is_suffix_ambiguous` is removed**, not renamed — see Function Signatures. One rule,
   one name (`ReservedNames::is_reserved_key`), across both crates. This supersedes the Phase 1
   Crate Placement sentence that had the function *widening*; Phase 1 is left as approved, and
   Phase 2 is the authority on mechanism.
2. **The `SIDECAR` / `SIDECAR_AND_LOCK` presets are dropped** in favour of
   `ReservedNames::new(&[...])` declared per store — see Data Structures. "Sidecar" named a layout
   style rather than what is reserved, and a shared preset contradicted the design's own rule that
   each store reserves what its own layout uses.
3. **No commands are relevant** — confirmed. `specs/command_registry.yaml` needs no regeneration.

## Remaining Open Questions

None. Phase 3 can proceed on the architecture as written.

## Review Record

Two independent reviewers (focused tier), run in parallel per the workflow. No fixer pass was
needed: neither returned a blocking or advisory finding.

**Reviewer A — Phase 1 conformity.** No findings. Confirmed all five decisions settled at the
Phase 1 gate are honoured, store by store against Phase 1's reserved-name table; that
`ReservedNames` builds no part of the out-of-scope layout subsystem; that all four Phase 1 open
questions are resolved rather than restated; and that the `affects_docs` rejection list is
defensible.

**Reviewer B — codebase alignment at HEAD.** No findings. Verified every line reference cited
here; that `Key::iter()` and `ResourceName::name` are public with the assumed types; that
`acquire_lock` runs before any `key_to_path` call or directory creation in `set` (1095),
`set_metadata` (1116), `remove` (1121) and `removedir` (1144), so a lock-path refusal leaves no
side effects; that both listing filters match by dotted suffix only and so miss the bare folder
name; that `PathMap::decode` strips the suffix exactly once (opendal_store.rs:120-126) and still
collapses `x.__metadata__` to `x` after the widening; that the 16 conformance families leave
`reserved01`-`reserved05` free; and that nothing in-tree constructs a reserved key. It also
confirmed the design reuses `Error::key_not_supported`, the `reject_ambiguous` shape and the
existing `with_unsupported_shape` fixture builder rather than reinventing them.
