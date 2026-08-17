# Phase 2: Solution & Architecture — Absolute Store Keys

## Overview

Three additions to `Key` carry the rule (`is_relative`, `as_absolute`, `try_into_absolute`), one new
`ErrorType` variant names the violation, and every store calls the check before the key is used. The
two file stores get it structurally as well, by making their path builders fallible so the
filesystem cannot be reached without passing. No new types, no new dependencies, no query-language
change.

## Known-Issue Preflight

Searched: the issue linked from Phase 1; `specs/index.csv` filtered to open (`draft`, `accepted`,
`in_progress`) issues in `core/store`, `store/backends`, `core/error`, `core/query`, `axum`, `web`;
and the integration points this design touches (`store.rs` traits and stores, `opendal_store.rs`,
`error.rs` and its four name/mapping tables, `liquers-web/src/store/`, `liquers-axum` status mapping).

| Issue | Status | Priority | Relevance and solution impact | First? | Blocking? | Required action | Priority action |
|---|---|---|---|---|---|---|---|
| `STORE-FILESTORE-PATH-TRAVERSAL` | accepted | P0 | The issue this design resolves. | n/a | no | Close in Phase 5. | **Contradiction to settle — see below** |
| `STORE-OPENDAL-SLASH-HANDLING` | accepted | P1 | Same file, different cause: `AsyncOpenDALStore::key_to_path` is `key.encode()` and slash handling is broken (`opendal_store.rs:335` FIXME). Our change adds the guard to the same methods but does not touch path mapping. | no | no | Monitor. If WP-5's `path_map.rs` lands first, the guard moves into it rather than into each method. | Keep P1 |
| `CORE-STORE-OPENBIN-MISSING` | accepted | P3 | `openbin` is unimplemented in every store. When implemented it is another key-taking method that must carry the check. | no | no | Note the requirement in the trait rustdoc so the implementor sees it. | Keep P3 |
| `CORE-SESSION-AND-KEY-ACL` | accepted | P2 | Wants per-key write authorization at the store. This guard is a *well-formedness* check, not authorization, and must not be mistaken for one. | no | no | Say so explicitly in the trait rustdoc; the ACL will be a separate, orthogonal check. | Keep P2 |
| `RESOURCE-NAME-ASCII-ONLY` | draft | P2 | Also about which key shapes are addressable. Independent: it widens the accepted character set, this refuses two specific segment values. | no | no | None. | Keep P2 |
| `CORE-ERROR-PAYLOAD-SIZE` | accepted | P2 | `Error` is large enough to bloat every `Result`. A new `ErrorType` variant is a discriminant only and does not grow `Error`. | no | no | None. | Keep P2 |
| `LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT` | accepted | P3 | Error transport across the language boundary. The new variant must be added to the `liquers-web` and `liquers-py` name tables, which this design does. | no | no | None beyond the table updates already planned. | Keep P3 |
| `WEB-LIQUERSERROR-NOT-CONSTRUCTIBLE` | accepted | P3 | Same file (`liquers-web/src/error.rs`), unrelated defect. | no | no | None. | Keep P3 |
| `AXUM-HANDLER-TEST-COVERAGE` | accepted | P2 | No handler test scaffolding, so the new `400` mapping cannot be asserted through a handler. Shapes Phase 3: the status mapping is tested at `error_to_status_code`, not end-to-end. | no | no | Constrain the Phase 3 test plan; do not build scaffolding here. | Keep P2 |

### Blocking and Priority Decision

**No blockers.** Nothing must be resolved before this work, and the architecture does not depend on
any open issue.

**One contradiction needs the user's decision.** `STORE-FILESTORE-PATH-TRAVERSAL` carries
`priority: P0` in its front matter and `index.csv`, while its own body closes with *"Marked P1
rather than P0 because exploitation requires an exposed query endpoint reachable by an untrusted
caller, which is a deployment posture rather than the default."* One of the two is wrong.

Assessment against `DOCS_STRUCTURE_GUIDE.md` §4.4 (P0 = incorrect results, data loss, a panic on a
supported path, or a documented feature that does not work): unauthorized **writes** outside the
store root are data loss on a supported path, and `liquers-axum` serving the query API is a
documented, supported deployment rather than an exotic one. That reads as P0. **Recommendation: keep
P0 and delete the trailing paragraph**, which is the stale half. Confirm before Phase 5 closes the
issue.

## Data Structures

**No new structs and no new enums**, apart from one variant on an existing enum.

### `ErrorType` — one new variant

```rust
// liquers-core/src/error.rs
pub enum ErrorType {
    // … 22 existing variants, unchanged
    /// A store was given a key that is not absolute — some segment is `.` or `..`.
    KeyNotAbsolute,
}
```

**Name.** `KeyNotAbsolute`, not `RelativeKey`, for consistency with the existing key family
(`KeyNotFound`, `KeyNotSupported`, `KeyReadError`, `KeyWriteError`): each reads *Key* + condition.

**Why a variant rather than a new constructor over `KeyNotSupported`.** A caller must be able to
tell "this address is malformed" from "this store does not serve this prefix". They differ in
audience (the caller's bug versus routing), in HTTP status (400 versus 404), and in what a test can
assert. A distinct message on a shared variant would be assertable only by string matching.

**Cost.** `ErrorType` has no `_ =>` arms anywhere, so the compiler enumerates the work:

| Site | Change |
|---|---|
| `liquers-axum/src/api_core/error.rs:8` | `KeyNotAbsolute => StatusCode::BAD_REQUEST` |
| `liquers-core/src/assets.rs:1456` | add to the `NotPersisted` arm list |
| `liquers-py/src/error.rs:7,32,65` | enum variant + both `From` directions |
| `liquers-web/src/error.rs:15,46` | `error_type_name` → `"key_not_absolute"`, `error_type_from_name` ← same |
| `liquers-web/src/error.rs:3` | module doc says "22 variants"; becomes 23 |

**Serialization note.** `ErrorType` derives `Serialize, Deserialize`, and an `Error` can be stored
inside a `.__metadata__` record. Metadata written by a build that knows `KeyNotAbsolute` will fail to
deserialize on one that does not. This is a forward-compatibility direction the project already
accepts for new variants (`liquers-web/src/error.rs:44` states the policy for names), and no such
metadata can exist before this change ships.

## Function Signatures

### Module: `liquers_core::query` — the rule lives on `Key`

```rust
impl Key {
    /// Returns `true` when **any** segment is `.` or `..`.
    ///
    /// Distinct from `CwdCursor::needs_cwd`, which tests only the *first* segment because at query
    /// level "relative" means "needs a CWD to resolve". `a/../../etc` needs no CWD and is still
    /// relative in this sense: it is not a store address.
    pub fn is_relative(&self) -> bool;

    /// Returns the key unchanged when it is absolute, otherwise [`Error::key_not_absolute`].
    ///
    /// **Asserts; does not resolve.** [`Key::to_absolute`] resolves `.` and `..` against a working
    /// directory — one word apart from this method and the opposite operation.
    pub fn as_absolute(&self) -> Result<&Key, Error>;

    /// Consuming form of [`Self::as_absolute`], for call sites that already own the key.
    pub fn try_into_absolute(self) -> Result<Key, Error>;
}
```

`query.rs` already imports `crate::error::Error` (`query.rs:133`), so no new module coupling.

**Why both fallible forms** — see the naming table in `DESIGN.md`. Short version: RFC 430 gives
`as_` to a free borrowed→borrowed operation and `into_` to a consuming one, so the consuming method
must be `try_into_absolute`; but `AsyncStore`'s methods take `&Key`, and a consuming check there
would force a `key.clone()` on every store operation purely to validate. `as_absolute` avoids the
clone and keeps the property that motivated the consuming form: `let key = key.as_absolute()?;`
shadows the parameter, so the unchecked key cannot be used afterwards by accident.

**Complexity.** `is_relative` is O(segments) with two `&str` comparisons each and no allocation —
the same shape as `has_key_prefix`, which every routed call already runs. Store operations are I/O
bound; this is not measurable.

### Module: `liquers_core::error`

```rust
impl Error {
    /// The key is not a store address: some segment is `.` or `..`.
    pub fn key_not_absolute(key: &Key) -> Self;
}
```

Message: ``Key '<key>' is not absolute; a store requires a key without '.' or '..' segments``.
Sets `key: Some(key.encode())`, `error_type: ErrorType::KeyNotAbsolute`, `position: unknown`.

**No `store_name` parameter**, unlike `key_not_supported(key, store_name)`. A relative key is
invalid for *every* store, so naming one adds no information and would force `Key::as_absolute` to
take a parameter it has no business knowing.

### Renamed: `CwdCursor::is_relative` → `CwdCursor::needs_cwd`

`pub(crate)`, three call sites (`query.rs:2202`, `:2489`, `:2504`). Renamed in the same change so
that two methods named `is_relative` with different meanings never coexist. Behaviour unchanged.

### `Query::absolute` — documented against, not renamed (Phase 1 open question 2)

`Query::absolute` means "the textual form had a leading `/`" and is documented as independent of
`.`/`..` resolution and as currently having no semantic meaning (`query.rs:67`, `:2148`). It is now
the third use of "absolute" in the crate, alongside `Key::to_absolute` (resolves) and
`Key::as_absolute` (asserts).

**Decision: leave it, and add one disambiguating sentence to each of the three doc sites.** A rename
would touch a public field that is part of `Query` equality, hashing and encoding, and appears in
serialized queries — a wide, wire-visible change to fix a readability problem, in a design whose
subject is a P0 security fix. It does not belong in the same change.

**Filed rather than dropped:** Phase 5 files a `docs`/`core/query` issue proposing that
`Query::absolute` be renamed to something naming what it holds (`had_leading_slash`, `rooted`),
carrying this analysis. That keeps the observation from evaporating without widening this fix.

## Trait Implementations

### `Store` and `AsyncStore` — precondition, stated and enforced

No new trait method. The precondition is documented on the traits and enforced in each
implementation, for a reason worth recording: a defaulted `fn check_key` would be *overridable*,
which is precisely the wrong affordance for a security precondition — a backend could weaken it
silently. `Key::as_absolute` cannot be overridden.

Every fallible, key-taking method in every store begins:

```rust
let key = key.as_absolute()?;
```

Methods covered, per store: `get`, `get_bytes`, `get_metadata`, `get_asset_info`, `set`,
`set_metadata`, `remove`, `removedir`, `contains`, `is_dir`, `listdir`, `listdir_keys`,
`listdir_asset_info`, `listdir_keys_deep`, `makedir`. Methods that only delegate to a checked method
(`get_bytes`, `listdir_keys`, `listdir_asset_info`, `listdir_keys_deep`, `get_asset_info` in their
trait-default forms) inherit the check and are not double-guarded.

`is_supported` gains `&& !key.is_relative()`, so the routers stop selecting a store for a relative
key. It is *not* the enforcement point: only `StoreRouter::find_store` and
`AsyncStoreRouter::find_store` consult it (`store.rs:1579`, `:1588`, `:1793`), so a directly held
store never runs it.

The infallible metadata helpers (`default_metadata`, `finalize_metadata`, `finalize_metadata_empty`)
are left alone — they cannot report a refusal, and every path that reaches them has already passed a
checked method.

### `FileStore` / `AsyncFileStore` — the structural check

```rust
// liquers-core/src/store.rs — both stores
pub fn key_to_path(&self, key: &Key) -> Result<PathBuf, Error>;
pub fn key_to_path_metadata(&self, key: &Key) -> Result<PathBuf, Error>;
fn key_to_lock_path(&self, key: &Key) -> Result<PathBuf, Error>;   // AsyncFileStore only
```

Each begins with `key.as_absolute()?` before `PathBuf::push`. This is the one place where a relative
key becomes an actual filesystem escape, and making it fallible means no future method can touch the
filesystem without passing.

**Breaking change**: `key_to_path` and `key_to_path_metadata` are `pub`. Accepted — the alternative
is leaving the dangerous conversion infallible.

**Call sites**: 15 in `AsyncFileStore`, 13 in `FileStore`, all inside `Result`-returning methods, so
each takes a `?`. One needs restructuring rather than a bare `?` — `store.rs:1019` calls
`.display()` inside a `format!` argument, so the path must be bound first.

### `AsyncOpenDALStore`

```rust
pub fn key_to_path(&self, key: &Key) -> Result<String, Error>;
pub fn key_to_path_metadata(&self, key: &Key) -> Result<String, Error>;
```

Same treatment, 15 call sites. The sync `OpenDALStore` (`opendal_store.rs:16-218`) is commented out
and is not touched. `is_supported` (`:509`) gains the predicate.

### `AsyncMemoryStore` / `MemoryStore`

Guarded too, for uniformity: a key that one store refuses and another serves is a worse rule than
one that holds everywhere, and a memory store is what tests are written against. `AsyncMemoryStore::is_supported`
(`store.rs:809`) currently returns bare `true`, ignoring even its prefix; this change makes it
`!key.is_relative()`. The prefix omission is pre-existing and **out of scope** — noted here so it is
not mistaken for an oversight.

### `StoreRouter` / `AsyncStoreRouter`

Fallible methods call `key.as_absolute()?` **before** `find_store`, so a relative key reports
`KeyNotAbsolute` rather than the misleading `key_not_supported(key, "store router")` that
"no store matched" would otherwise produce. `is_supported` becomes `!key.is_relative() && find_store(key).is_some_and(…)`.

### `liquers-web` — the local guard collapses onto the shared rule

```rust
// liquers-web/src/store/key_guard.rs
pub fn check_key(key: &Key, store_name: &str) -> Result<(), Error>;
```

Signature unchanged; body delegates. `.`/`..` now yield `KeyNotAbsolute` from `Key::as_absolute`;
the empty-segment case keeps `Error::key_not_supported(key, store_name)`, since an empty segment is
malformed rather than relative and the store name *is* informative there. The module docs lose the
"browser's copy until that lands" paragraph.

**This changes an already-shipped conformance assertion**, which is the one visible behaviour change
outside the fix itself:

| Site | Was | Becomes |
|---|---|---|
| `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` `STORE05` (`:1767`) | `../escape`, `a/../../etc`, `a/./b` → `KeyNotSupported` | → `KeyNotAbsolute` |
| `liquers-web/tests/store_pure_STORE.rs:284`, `store_local_STORE.rs:178`, `store_js_STORE.rs:216` | assert `KeyNotSupported` / not routed | assert `KeyNotAbsolute` |

`liquers-web/tests/e2e/store.spec.ts:265,375` assert `key_not_supported` for a *read-only write
refusal* — a different case, unchanged.

## Generic Parameters & Bounds

None added. `Key` is concrete; no method gains a bound; `Store` and `AsyncStore` stay object-safe
(no new methods at all, so trivially).

## Sync vs Async Decisions

| Function | Async? | Rationale |
|---|---|---|
| `Key::is_relative` / `as_absolute` / `try_into_absolute` | No | Pure, no I/O, no allocation |
| `Error::key_not_absolute` | No | Constructor |
| Store method guards | Inherit | The check is sync inside both sync and async methods |

The rule is identical in `Store` and `AsyncStore`; no wrapper or duplication is needed because it
lives on `Key`, not on either trait.

## Integration Points

| Crate | File | Change |
|---|---|---|
| `liquers-core` | `src/query.rs` | 3 `Key` methods; rename `CwdCursor::is_relative` → `needs_cwd` (3 call sites) |
| `liquers-core` | `src/error.rs` | `ErrorType::KeyNotAbsolute`; `Error::key_not_absolute` |
| `liquers-core` | `src/store.rs` | `Store`/`AsyncStore` rustdoc; guards in 6 impls; 3 path builders fallible (28 call sites); 2 routers |
| `liquers-core` | `src/assets.rs` | one match arm |
| `liquers-store` | `src/opendal_store.rs` | guards + 2 fallible path builders (15 call sites) |
| `liquers-web` | `src/store/key_guard.rs` | delegate; docs |
| `liquers-web` | `src/error.rs` | 2 name-table entries + module doc count |
| `liquers-axum` | `src/api_core/error.rs` | one match arm → 400 |
| `liquers-py` | `src/error.rs` | enum variant + 2 `From` arms |

**Dependencies:** none added. **Dependency flow:** unchanged, all edges point forward.

## Web Endpoints

**No route changes.** `liquers-axum` store and recipe handlers already `parse_key` and hand the key
to the store (`store/handlers.rs`, 15 sites; `recipes/handlers.rs`, 4), so the traversal closes with
no handler edit. The only observable change is status: a query or store path carrying `..` returns
**400** with `error_type: "KeyNotAbsolute"` instead of reading a file it should not have.

## Error Handling

| Scenario | Constructor | Type | HTTP |
|---|---|---|---|
| Key has a `.` or `..` segment, any store, any method | `Error::key_not_absolute(key)` | `KeyNotAbsolute` | 400 |
| Key has an empty segment (browser stores) | `Error::key_not_supported(key, store)` | `KeyNotSupported` | 404 |
| No store serves the prefix | `Error::key_not_supported(key, "store router")` | `KeyNotSupported` | 404 |

All constructors typed; no `Error::new`; propagation by `?`.

## Concurrency Considerations

None. The check is a pure function of an immutable `&Key`, holds no lock, and adds no shared state.

## Relevant Commands

**No new commands, and no command changes.** This is a store-layer precondition; no command in
`liquers-lib` constructs a store key that could carry a `.` or `..` segment (Phase 1 open question 5,
confirmed below). `specs/command_registry.yaml` does not change and `registry_export` stays green.

**Namespaces relevant only as consumers**, none modified: `pl` (Polars, reads/writes store keys),
`img`, `lui`/`egui`. **Question for the user: is that the right set to have checked, or is there a
namespace that builds store keys programmatically that I should look at?**

### Phase 1 open question 5 — resolved, and Phase 1's preliminary answer was incomplete

Phase 1 said every dot-segment key in the tree is pre-store. That holds for the *evaluation* paths —
CWD resolution in `context.rs` and `interpreter.rs` is consumed by `resolve_key_from_cwd` before any
store call, and the remaining occurrences are parser and `to_absolute` unit tests that never reach a
store. It does not hold for recipes, and the review pass found the gap:

- **`Recipe.cwd` is an unvalidated deserialized string.** It is `Option<String>`
  (`recipes.rs:82`), and `recipes.rs:48` records deliberately that deserialization does not validate;
  `get_cwd` just calls `parse_key` on it (`:284`). A `recipes.yaml` carrying `cwd: ../../etc`
  therefore produces a relative `Key` that flows into `Step::SetCwd` (`:239`) and into
  `store_to_key` via `cwd.join(filename.name)` (`:298`).
- **`Recipe::key_to_absolute` calls `key.to_absolute(&cwd)` with that same cwd** (`:198`), and
  `to_absolute`'s own docs state that `cwd_key` is *assumed* absolute and the condition is not
  checked (`query.rs:1525`).

So `Key::join` is **not** only ever called with a `listdir` name — `recipes.rs:298` joins a query
filename onto a recipe cwd, and `:566`/`:646` join the literal `recipes.yaml`.

This is a second source of a relative store key, distinct from the query path in the issue. It is
much less severe — authoring `recipes.yaml` already requires store-write access — but it is exactly
what the guard is for, and it means the guard's blast radius is wider than "queries only". **No
in-tree caller passes a relative key to a store today**, so nothing that currently works breaks;
Phase 3 pins both the evaluation paths and this recipe path with tests rather than assertions.

## Documentation Architecture

### Reference Plan

**No new `specs/reference/` document.** `specs/reference/api/DOC_07_STORES_PERSISTENCE.md` is the
reference that will carry this rule and does not exist (P1, *Not started*). Recorded instead in
`specs/reference/api/API_DOCS_GAP_ANALYSIS.md` §7 and its progress tracker — **already committed**
(`30f2f3e`), since the requirement stands regardless of how this fix lands.

**Primary home is rustdoc**, which is where a backend author reads the contract:

| Location | Content |
|---|---|
| `liquers-core/src/store.rs` module docs | The precondition, why it exists, and that it is well-formedness and *not* authorization (`CORE-SESSION-AND-KEY-ACL`) |
| `Store` / `AsyncStore` trait docs | Implementor's obligation: check every fallible key-taking method; `is_supported` is not sufficient, and why |
| Each guarded method | One line, or the trait-level statement where the method is a default |
| `Key::is_relative` / `as_absolute` / `try_into_absolute` | The rule, and the explicit `to_absolute` contrast |
| `Key::to_absolute` | Reverse cross-reference: this *resolves*, `as_absolute` *asserts* |
| `ErrorType::KeyNotAbsolute`, `Error::key_not_absolute` | When it is produced |

### Guide Plan

**No new guide.** Trait rustdoc plus `STORE05` cover writing a backend.

### Other Documents to Create

None.

### Existing Documents to Review or Update

| Path | Change | In `affects_docs`? |
|---|---|---|
| `specs/reference/api/API_DOCS_GAP_ANALYSIS.md` | §7 + tracker (**done**, `reviewed:` bumped, History row) | yes |
| `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` | `STORE05` expects `KeyNotAbsolute` for `.`/`..`; keep `KeyNotSupported` for empty; add "refuse on direct calls, not only routing"; History row + `reviewed:` | yes |
| `specs/reference/PROJECT_OVERVIEW.md` | §5 Storage states the precondition and links the gap-analysis entry; History row + `reviewed:` | yes |
| `specs/reference/WEB_API_SPECIFICATION.md` | §3.2 error table gains `KeyNotAbsolute` → 400 (`:164`); History row + `reviewed:` | yes |
| `specs/issues/STORE-FILESTORE-PATH-TRAVERSAL.md` | `design:` link now; `status: closed` + resolution note in Phase 5; settle the P0/P1 contradiction | yes |
| `specs/design/liquers-web-store/phase2-architecture.md` | §"Key guard" says the helper is a local copy "until that lands" — stale once this lands | **no** — archived design history, not current-state (`DOCS_STRUCTURE_GUIDE.md` §9); the live `key_guard.rs` docs carry the correction |
| `specs/README.md`, `specs/index.csv` | Capability map + regenerate | yes |

**Discarded candidates:** `STORE_CONFIG_FSD.md` (configuration only, says nothing about key shape);
`specs/reference/api/DOC_02_QUERY_LANGUAGE_REFERENCE.md` (the *language* is unchanged — `..` stays a
legal `ResourceName`; the rule is a store precondition, and putting it in the query reference would
misfile it); `ASSETS.md` / `DOC_03` (asset lifecycle is unaffected).

### Design and Capability Links

`specs/README.md` gains the design under its store capability; Phase 5 points the entry at the
rustdoc and the gap-analysis item rather than at this folder.

### Evidence to Collect During Implementation

- Whether `key.as_absolute()?` at the top of ~60 method bodies reads as intended or as noise — the
  honest input to reconsidering the rejected `AbsoluteKey` newtype.
- Any store method reached with a relative key during the test run that Phase 1's audit missed.
- The real diff size of making the path builders fallible, versus the estimate here.
- Whether `KeyNotAbsolute` vs `KeyNotSupported` proves distinguishable where it was meant to help.
- Whether a `recipes.yaml` with a relative `cwd` now fails somewhere confusing (the refusal arrives
  at the store, far from the field that caused it) — the input to deciding whether `Recipe` should
  validate `cwd` at parse time as a follow-up.

## Rejected Alternatives

| Alternative | Why rejected |
|---|---|
| `AbsoluteKey` newtype; stores take `&AbsoluteKey` | Makes forgetting *impossible* rather than visible — genuinely stronger. Rejected for cost: every method of both traits, both routers, all six stores, `liquers-py` and `liquers-web` signatures. Revisit if the evidence above says the manual check is being forgotten. |
| Overridable `fn check_key` default on the traits | An overridable security precondition is the wrong affordance; a backend could weaken it silently. |
| Guard only in `is_supported` (issue option 2) | Only the routers consult it; a directly held store skips it. |
| Refuse `..` at parse time (issue option 3) | Breaks `to_absolute` and CWD resolution, the legitimate use of `..`. |
| Normalize instead of refuse | A key is an address: silently equating `a/../b` with `b` makes two addresses alias one asset. |
| Reuse `KeyNotSupported` with a distinct message | Assertable only by string matching; conflates a malformed address with a routing miss; wrong HTTP status. |

## Review Outcomes

Two independent review passes were run against this document — **Reviewer A** (Phase 1 conformity)
and **Reviewer B** (codebase alignment, with `rust-best-practices`). This host does not launch
parallel review agents, so the passes were performed sequentially; findings and fixes are recorded
here unchanged, per the skill's host-compatibility rule.

**Reviewer A — Phase 1 conformity.** One finding, fixed.

| # | Finding | Severity | Resolution |
|---|---|---|---|
| A1 | Phase 1 open question 2 (`Query::absolute` is a third meaning of "absolute") was explicitly deferred to Phase 2, and Phase 2 did not mention it. | Blocking — an unanswered Phase 1 question | New §"`Query::absolute` — documented against, not renamed": leave the field, disambiguate the three doc sites, file a follow-up issue in Phase 5. |

Scope, purpose and all Phase 1 interactions otherwise carried over intact; no unscoped feature crept
in. Open questions 1, 3, 4, 5, 6 are each answered in the document.

**Reviewer B — codebase alignment.** Three findings, all fixed.

| # | Finding | Severity | Resolution |
|---|---|---|---|
| B1 | Path-builder call-site counts (18/15) counted the function *definitions* as call sites. | Advisory — Phase 4 estimate | Corrected to 15 / 13 / 15 (43 total, not 48). |
| B2 | **"`Key::join` is only ever called with a `listdir` name" is false**, and the claim that all in-tree dot-segment keys are pre-store missed recipes: `Recipe.cwd` is an unvalidated deserialized string (`recipes.rs:48,82,284`) that reaches `cwd.join(filename.name)` (`:298`) and `key.to_absolute(&cwd)` (`:198`). | Blocking — a false premise carried from Phase 1 | §"Phase 1 open question 5" rewritten with the recipe path; added to the Phase 3 test obligations and to the evidence list. |
| B3 | Verified against source and found correct: `is_supported` consulted only at `store.rs:1579/1588/1793`; `AsyncMemoryStore::is_supported` bare `true` at `:809`; `store.rs:1019` `.display()` inside `format!`; `opendal_store.rs:16-218` commented out; `query.rs:133` already imports `Error`; three `CwdCursor::is_relative` call sites; all five `ErrorType` match sites; `LANGUAGE-INTEGRATION_GUIDE.md:1767`; `WEB_API_SPECIFICATION.md:164`; 15 + 4 axum `parse_key` sites. | — | No change. |

No missed-reuse finding: `liquers-web/src/store/key_guard.rs` is the existing implementation and the
design collapses it rather than duplicating it. No `unwrap`/`expect`, no `Error::new`, no `_ =>`
arm, no backward dependency edge, no new bound.

## Compilation Validation

- [x] All signatures specified; no `unwrap`/`expect`; all fallible paths return `Result`
- [x] No new bounds, no new generics, object safety unaffected
- [x] No `_ =>` arms introduced; the new variant is enumerated at all five match sites
- [x] Typed error constructors only
- [x] Dependency flow unchanged

Checks for Phase 4: `cargo check -p liquers-core`, `cargo test -p liquers-lib --lib --tests`,
`cargo check -p liquers-py`, and `cargo test -p liquers-web --target wasm32-unknown-unknown
--features debug-handles` after `cargo clean`.
