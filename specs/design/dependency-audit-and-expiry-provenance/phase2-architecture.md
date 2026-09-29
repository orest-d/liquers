# Phase 2: Solution & Architecture - Dependency Audit and Expiry Provenance

## Overview

Seven parts in `liquers-core`. Parts A–E are built on the dependency graph that `keyed-expiry-cascade-fix`
built and the ordering precedent set by `stale-dependency-status-finalization`:

| Part | Closes | Shape |
|---|---|---|
| A. Audit compares current with recorded versions | `AUDIT-CANNOT-EXPIRE-ON-A-FIRST-OBSERVED-VERSION` | New `DependencyManager::audit_version`; `audit_gaps` uses it instead of `register_version` |
| B. Audit policy and report-only audits | `DEPENDENCY-AUDIT-POLICY-NOT-EXPRESSIBLE` | `DependencyAuditPolicy` in `AssetManagerOptions`; `AuditMode`; `AuditFinding` in `AuditReport` |
| C. Expiry provenance | `EXPIRY-RECORDS-NO-REASON` (+ `IMMEDIATE-MANAGER-LAZY-DEADLINE-EXPIRY-NEVER-FIRES`) | `ExpiryReason` in `MetadataRecord` / `AssetInfo`; a log entry naming both participants |
| D. Directory-listing dependencies | `DIRECTORY-LISTING-DEPENDENCY-IS-NEVER-REGISTERED-OR-CHECKED` | A version for a listing, registered at the step and refreshed on manager writes; audit and fast-track resolve it |
| E. Stale-dependency reachability | `STALE-DEPENDENCY-PATH-HAS-NO-END-TO-END-TEST` | `Context::submit` + public `Context::wait_for_dependency`, which double as the test seam |
| F. Asset managers outside core | `ASSET-MANAGER-TRAIT-CANNOT-BE-IMPLEMENTED-OUTSIDE-CORE` (added at the Phase 2 gate) | `DependencyManagerAccess` and an opaque `DependencyManager` made public; the lifecycle primitives a manager needs made public; an external manager in `tests/` runs the parametric manager suite |
| G. Content changed outside Liquers | `STORE-VERSION-BLIND-TO-CHANGES-MADE-OUTSIDE-LIQUERS` (added 2026-09-29) | Content-hash versions carry a flag bit; stored bytes are re-hashed on read and on demand; a mismatch is user input for `Source`/`Override` and a policy choice (`UserInput` → `Override`, or `Corrupted` → delete) for recipe-backed values |

No change to `Status`, to `AsyncStore`, to the query syntax, or to any command signature. No
`liquers-axum` change. The titled scope grows with Part F: the design is now also "asset managers
can be implemented outside core". Every other part is built so an external manager gets it for
free, through default trait methods.

## Terms

Plain definitions, with a worked example for each problem, are in Phase 1 §"Terms used in this
design" and §"The problems, on examples". Here the same terms are mapped to the code:

| Term | In the code |
|---|---|
| Version | `metadata::Version(u128)`, a blake3 hash of the serialized bytes (`Version::from_bytes`), stored in `MetadataRecord.version`. `Version::unknown()` = `Version(0)`. |
| Dependency key | `metadata::DependencyKey`: `-R/<key>` (a stored value), `-R-dir/<key>` (a folder's list of names), `ns-dep/command_metadata-…` / `ns-dep/command_impl-…` (a command). |
| Recorded version | Two copies of one fact. Persistent: `MetadataRecord.dependencies: Vec<DependencyRecord { key, version }>` of the dependent. In memory: the edge `keyed_dependents[dependency][dependent] = version` in `DependencyManager`. |
| Current version | `AssetManager::version(&Key) -> Result<Option<Version>, Error>` (`assets.rs:4346`): the live asset's metadata version if it has one, else the store's metadata version, else `None`. It never evaluates and never reads the value. Part D extends it to `-R-dir/` keys as `dependency_version`. |
| Version map | `DependencyManager.versions: scc::HashMap<DependencyKey, Version>`. It is empty in a new process. |
| Gap | A dependency that some dependent has a concrete recorded version for, but that is missing from the version map (`missing_versions_for`, `dependencies.rs:834`). Only gaps are audited. A key already in the map was checked when it got there. |
| Cascade | `DependencyManager::expire_stale_dependents` / `expire_from_frontier`, applied by `AssetManager::expire_dependencies_result`. |

### The Phase 1 examples, traced through the code

Each trace names the call that goes wrong today and the call that replaces it.

**Example 1 (Part A), audit after a restart.** A new process loads `report.txt`. Its metadata
records `-R/data/a.csv @ V1`, so `load_from_records` adds the edge `a.csv → report.txt @ V1`, and
the map has no `a.csv`. `trigger_dependency_audit` → `missing_versions_for(report.txt)` = `[a.csv]`
→ `version(a.csv)` = `Some(V2)` → `register_version(a.csv, V2)` finds the map entry *vacant*,
inserts `V2`, sets `version_changed = false`, and expires nothing (`dependencies.rs:158-180`).
**Replaced by** `audit_version(a.csv, V2)`: it inserts `V2`, then always runs
`expire_stale_dependents(a.csv, V2)`, which compares `V2` with the edge's `V1`, finds them different,
and expires `report.txt`.

**Example 2 (Part B), when to check.** The service sets `dependency_audit: on_load`. In
`try_fast_track` (`assets.rs:1170-1200`), a recorded dependency the map does not know is skipped
today (`if let Some(dm_version) = …`). Under `OnLoad` it is resolved through `dependency_version`,
and a mismatch or `None` refuses the fast track, so `report.txt` is recomputed. The researcher keeps
`explicit`, so loading is as today. `trigger_dependency_audit_with(q, ReportOnly)` fills `findings`
with `(a.csv, report.txt, expected V1, found Some(V2))` and changes nothing.

**Example 3 (Part C), why expired.** `a.csv` is stored again with `V3` in-process →
`register_version` → cascade → `expire_dependencies_result` → `report_ref.expire_without_cascade()`
→ `mark_expired_status` flips the status and writes nothing else (`assets.rs:3280`). **Replaced
by** the same path carrying `ExpiredDependents.trigger = -R/data/a.csv`. `mark_expired_status`
writes `expiry_reason = Cascade { trigger }` and a log entry under the same lock that flips the
status, so the persisted metadata carries both.

**Example 4 (Part D), folder listing.** `-R-dir/data/-/index_files` validates as
`GetAssetDirectory[data]` followed by the action (checked with `liquers-validate --command index_files`).
At runtime, `find_dependencies` adds the dependency `-R-dir/data` to the plan (`plan.rs:2647`; the
offline validator does not run that step, so it shows no dependencies). `register_plan_dependencies`
then drops it, because `get_version(-R-dir/data)` is `None` and nothing ever registers one. **Replaced
by** the step registering `listing_version(["a.csv", "b.csv"])` and recording it. Storing
`data/new.csv` calls `refresh_listing_version(data)`, the names become
`["a.csv", "b.csv", "new.csv"]`, the version moves, and `index.txt` is expired.

**Example 5 (Part E), stale input mid-evaluation.** A test command calls
`context.submit("-R/data/a.csv")`, then waits on a test-controlled gate. The test expires `a.csv`
and opens the gate. The command calls `context.wait_for_dependency(&a)`, which reaches the manager's
`Status::Expired` arm (`assets.rs:5446`), uses the retained value and calls
`note_expired_dependency`. The result finishes `Expired` with `StaleDependency { dependency: -R/data/a.csv }`.

**Example 6 (Part C), deadline on the immediate manager.** `get` checks
`status == Ready && assetref.is_expired()` (`assets.rs:6911`). `is_expired()` is
`status == Expired`, so the condition cannot hold. **Replaced by** `assetref.expiration_time().await.is_expired()`,
followed by `expire_without_cascade(Deadline { expiration_time })`.

**Example 7 (Part F), external manager.** `impl<E> AssetManager<E> for ClusterManager<E>` in
another crate fails, because the supertrait `DependencyManagerAccess` is `pub(crate)`. After Part F it
compiles, and `tests/external_asset_manager.rs` is that impl.

**Example 8 (Part G), edited by hand.** `data/a.csv` was stored with `from_content` → `V1`
(flag bit set). A user overwrites the file. On the next read, `try_fast_track` has the bytes and
calls `V1.verify(bytes)`, which returns `Mismatch { actual: V2 }`. `a.csv` is a `Source`, so
`external_change_action` returns `AcceptAsInput { actual: V2 }`. `apply_external_change` writes `V2`
into the metadata and calls `register_version(-R/data/a.csv, V2)`, which cascades to `report.txt`
(recorded `V1`) with `Cascade { trigger: -R/data/a.csv }`.

## Known-Issue Preflight

Searched: the issues linked from `DESIGN.md`; every open (`draft`, `accepted`, `in_progress`)
record in `specs/index.csv` with area `core/assets`; issues touching the integration points (fast
track, `save_to_store`, the expiration monitor, the inline run path, `listdir`, `EnvRef` ownership);
and store-listing issues, since Part D hashes `listdir`. Checked at HEAD on 2026-09-29.

| Issue | Status | Current priority | Relevance and solution impact | Must be addressed first? | Blocking? | Required action | Priority action |
|---|---|---|---|---|---|---|---|
| `ASSET-REGISTRATION-OWNERSHIP-CONTRACT` | draft | P2 | Part F: `AssetRef::bound_owner_key` decides ownership by looking the key up in the *manager's* map. An external manager whose `lookup_key_asset` is wrong breaks ownership, so persistence and cascades silently misbehave. | no | no | The new guide states today's registration invariants as a requirement (at most one registered asset per key; `lookup_key_asset` returns exactly that asset; never register a volatile asset), and the external test manager follows them. The open contract questions stay on the issue. | keep |
| `ENVIRONMENT-MANAGER-REFERENCE-CYCLE` | draft | P2 | Part F: an external manager holds `EnvRef<E>` strongly, as the built-in ones do, so it inherits the leak. | no | no | The guide notes the cycle and links the issue; nothing here makes it worse. | keep |
| `INLINE-DROP-REPAIR-STRANDS-EXISTING-WAITERS` | draft | P2 | Part F makes `run_inline` public, so external callers can reach this defect. | no | no | The `run_inline` contract doc states the limitation and links the issue. | keep |
| `DESCRIBING-AN-ASSET-CAN-TRIGGER-ITS-EVALUATION` | draft | P1 | Parts B and D must not evaluate while resolving a version. They go through `version()` (metadata only) and `AssetManager::listdir` (names only), never through `get_asset_info`. | no | no | Independent by construction. Phase 3 has a test that an audit never evaluates. | keep |
| `CORE-FILE-STORE-LISTDIR-DROPS-METADATA-ONLY-KEYS` | draft | P2 | Part D hashes `listdir`. A metadata-only key appears in an OpenDAL listing but not in a file-store listing, so the listing version differs by backend for the same logical contents. | no | no | Tolerated. The version is compared only within one store, so the difference never produces a false change. Fixing the issue later shifts versions once, which costs one recomputation. Noted in the `DEPENDENCIES_STATUS` update. | keep |
| `SAVE-TO-STORE-REPORTS-CANCELLED-WRITE-AS-PERSISTED` | draft | P2 | Part D refreshes a listing after `save_to_store`. A cancelled write reports success, so the listing is refreshed although nothing changed. | no | no | Harmless: `register_version` with an unchanged version cascades nothing. Only the cost of one `listdir`. | keep |
| `METADATA-ONLY-ENTRY-RELOADS-AS-CORRUPTED` | draft | P3 | Part B's `OnLoad` check sits in `try_fast_track`, beside the corrupted-data branch. | no | no | Independent. The version check runs after deserialization succeeds, so the two do not interact. | keep |
| `UNCACHED-STORED-COPY-EXPIRY-RACES-AN-INFLIGHT-EVALUATION` | draft | P3 | Same expiry path, different mechanism (write-back ordering). Excluded in Phase 1 Q5. | no | no | Part C makes the overwritten mark visible in the log, which helps a later fix. | keep |
| `IMMEDIATE-SET-STATE-STATUS-MATCH-HAS-DEFAULT-ARM` | draft | P3 | A one-line cleanup in `ImmediateAssetManager`, which Part C already edits. | no | no | Fold in if the same function is touched; otherwise leave it. | keep |
| `STORE-VERSION-BLIND-TO-CHANGES-MADE-OUTSIDE-LIQUERS` (filed 2026-09-29) | draft | P2 | Every check in Parts A, B and D compares *recorded* versions, which change only when Liquers writes. | — | no | **Brought into scope as Part G** (owner, 2026-09-29). | keep |
| `STORE-NO-READ-ONLY-ADAPTER` | draft | P2 | Part G writes metadata on read when it accepts a change, so reading can write to the store. Against a read-only backend that write fails. | no | no | Apply the action in memory, log the failed write, and do not fail the read. The in-memory version is correct for this process, and the next process re-detects the change. | keep |
| `ASSET-EXPIRATION-EVENTS-CANNOT-BE-OBSERVED-EXCEPT-PER-ASSET` | draft | P2 | Future consumer of Part C: an expiry event would carry the `ExpiryReason`. | no | no | None now. The reason type is serializable, so an event can reuse it. | keep |
| `COMBINED-EXPIRES` | accepted | P2 | Adjacent: the combined expiry of dependencies would produce `Deadline` reasons. | no | no | None now. | keep |
| `CORE-TOKIO-REMOVAL` | accepted | P3 | Part F makes `run` public, and `run` spawns tokio tasks. Publishing it makes that dependency part of the implementor surface. | no | no | The `run` contract doc says it is the queued (native) primitive, and that `run_inline` is the one to use on wasm32. | keep |

### Blocking and Priority Decision

No blocking issue. The two with the most influence on Part F, `ASSET-REGISTRATION-OWNERSHIP-CONTRACT`
and `INLINE-DROP-REPAIR-STRANDS-EXISTING-WAITERS`, are handled by documenting today's behaviour as
the implementor's contract, not by depending on their resolution. No priority change is recommended.

### How the Phase 1 open questions are settled

| # | Question | Decision | Why |
|---|---|---|---|
| 1 | What a listing version covers | **Membership only**: the sorted names `AssetManager::listdir` returns (store names ∪ recipe names), not recursive, no per-entry version | That is exactly the set a `-R-dir/` step exposes as its entries. A dependent that reads an entry's *content* already has a `-R/` edge to that entry, so folding entry versions in would duplicate edges and force a metadata read per entry. `listdir` is one call, whereas `listdir_asset_info` walks the subtree (`STORE-SEMANTICS-CHILDREN-RULE-CONTRADICTS-EVERY-STORE`). |
| 1b | When it is refreshed | When the listing is produced (the step), after every asset-manager write or removal under that directory **if something depends on it**, and when an audit resolves a `-R-dir/` gap | A write straight into the store without going through the manager is invisible except to an audit. That matches how `-R/` keys already behave. |
| 2 | Policy scope and depth | **Per environment**, in `AssetManagerOptions`. Direct dependencies only; transitivity comes from the cascade, and from `trigger_dependency_audit_all_registered` for a sweep | Per-key/per-recipe policy needs a place to declare it (recipe metadata) that is not otherwise needed now; it can be added later as an override without changing this shape. |
| 3 | Stale-dependency test | **Option 1**, through a public `Context::submit` + `Context::wait_for_dependency` (revised at the gate: no new handle type) | It is a real capability (a command that starts several dependencies and then waits for them), not a test-only hook. It is also the only way a command can hold a submitted dependency across its own work. |
| 4 | Log levels | `Deadline`, `Explicit`, `Cascade` → `info`; `Audit`, `StaleDependency` → `warning` | The first three are the contract working as designed. The last two mean a stored assumption turned out false, or the normal contract was departed from. |
| 5 | `UNCACHED-STORED-COPY-EXPIRY-RACES-AN-INFLIGHT-EVALUATION` | **Excluded.** It stays open | Its fix orders expiry against write-back through the key mutation lock, which is persistence ordering rather than verification. Part C makes the race *visible* (the overwritten mark has a reason in the log), and that makes it easier to test later. |

## Data Structures

### New Enums

#### `ExpiryReason` (`liquers-core/src/metadata.rs`)

```rust
/// Why an asset is `Expired`. Recorded, never consulted by read paths: every consumer treats
/// `Expired` the same regardless of reason (see `EXPIRY-RECORDS-NO-REASON` for why this is not a
/// `Status` variant).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExpiryReason {
    /// The asset's own expiration time elapsed.
    Deadline { expiration_time: ExpirationTime },
    /// Someone asked: `AssetRef::expire`, directly or through an API.
    Explicit,
    /// A dependency changed and the graph cascaded. `trigger` is the key whose change started
    /// the cascade: the root, not the immediate parent, because the root is what the operator can act on.
    Cascade { trigger: DependencyKey },
    /// An audit found that `dependency`'s current version is not the one recorded. `found` is
    /// `None` when the dependency has no current version at all.
    Audit { dependency: DependencyKey, found: Option<Version> },
    /// Evaluated using a dependency that expired mid-evaluation (the stale-value policy).
    StaleDependency { dependency: DependencyKey },
}

impl ExpiryReason {
    /// The log entry recording this transition for `subject` (the expired asset's key or query).
    /// Level per the table above.
    pub fn log_entry(&self, subject: &str) -> LogEntry;
}
```

`ExpirationTime` implements `Serialize`/`Deserialize` by hand (`expiration.rs:751-757`), so the
`Deadline` field needs nothing extra. `ExpiryReason` derives `PartialEq, Eq`, and `ExpirationTime`
already derives both (`expiration.rs:775`).

#### `DependencyAuditPolicy` (`liquers-core/src/environment_builder.rs`)

```rust
/// When recorded dependency versions are verified against current ones.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyAuditPolicy {
    /// Only when `trigger_dependency_audit*` is called. Today's behaviour, and the choice for
    /// exploratory work where intermediates are deleted by hand.
    #[default]
    Explicit,
    /// Also when a keyed asset is loaded from the store (`try_fast_track`): each recorded
    /// dependency the manager holds no version for is resolved, and a mismatch or a missing current
    /// version refuses the fast track, so the asset is recomputed. The strict service.
    OnLoad,
}
```

It gets a `pub fn is_explicit(&self) -> bool` for `skip_serializing_if`; no generic
`is_default` helper exists in the crate. The name is `Explicit` rather than `Never`, because the explicit entry points always work.

`OnLoad` refuses the fast track and does not *expire* the stored copy. The recomputation that
follows overwrites it, and expiring first would cost a store write for nothing.

#### `AuditMode` (`liquers-core/src/assets.rs`)

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AuditMode {
    /// Expire what is found stale (today's behaviour of `trigger_dependency_audit`).
    #[default]
    Expire,
    /// Change nothing: no version is registered, no edge removed, nothing expired. Only `findings`
    /// is filled.
    ReportOnly,
}
```

### New / Changed Structs

#### `AuditFinding` (new) and `AuditReport` (extended), `assets.rs`

```rust
/// One edge an audit found stale: `dependent` recorded `expected` for `dependency`, and the
/// current version is `found`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct AuditFinding {
    pub dependency: DependencyKey,
    pub dependent: DependencyKey,
    pub expected: Version,
    pub found: Option<Version>,
}

impl AuditFinding {
    pub fn new(dependency: DependencyKey, dependent: DependencyKey, expected: Version,
               found: Option<Version>) -> Self;
}

#[non_exhaustive] // was already Debug, Clone, Default, PartialEq, Eq
pub struct AuditReport {
    pub checked: Vec<DependencyKey>,
    pub expired: Vec<DependencyKey>,   // unchanged; empty in ReportOnly
    pub findings: Vec<AuditFinding>,   // new: direct edges only, filled in both modes
}
```

Adding a public field is technically breaking for struct-literal construction. The only
constructions are `AuditReport::default()` in core and equality asserts in
`liquers-core/tests/keyed_version_cascade.rs`, and those get updated.

#### `ExpiredDependents<E>` (`dependencies.rs:69`), one new field

```rust
pub struct ExpiredDependents<E: Environment> {
    pub keys: Vec<DependencyKey>,
    pub assets: Vec<WeakAssetRef<E>>,
    /// The dependency whose change produced this set. `None` only for `new()` (empty).
    pub trigger: Option<DependencyKey>,
}
```

This is the one change that lets all 15 `expire_dependencies_result` call sites record a `Cascade`
reason without being edited: the producer (`register_version`, `expire`, `report_no_version`,
`audit_version`) already knows the key. It is constructed only at `dependencies.rs:79` and `:790`,
and nothing outside `liquers-core/src` constructs it.

#### `MetadataRecord` and `AssetInfo` (`metadata.rs`), one new field each

```rust
/// Why this asset is `Expired`. Meaningful only while `status == Expired`; read it through
/// `Metadata::expiry_reason()`, which enforces that.
#[serde(default, skip_serializing_if = "Option::is_none")]
pub expiry_reason: Option<ExpiryReason>,
```

It is projected in both `From<MetadataRecord> for AssetInfo` (`metadata.rs:890`) and
`MetadataRecord::get_asset_info` (`:1160`).

**Forward compatibility is weaker than the issue assumed.** `MetadataRecord` is
`#[serde(deny_unknown_fields)]`. An *older* binary reading a sidecar that carries `expiry_reason`
therefore does not ignore it: the record falls into the legacy branch
(`Metadata::LegacyMetadata`), which keeps the fields verbatim and still reports the status. That is
degraded, not unreadable, and `skip_serializing_if` confines it to expired records. It is the same
trade-off `stored` and `cached` made. This is recorded rather than worked around.

#### `AssetData` (`assets.rs:596`)

`stale_dependency: bool` becomes `stale_dependency: Option<DependencyKey>` (the first stale
dependency observed; later ones only add log entries). The two readers at `:2098` and `:2869` test
`is_some()`, and `finalize_status_with_version` builds `ExpiryReason::StaleDependency` from it.

#### `AssetManagerOptions` (`environment_builder.rs:48`)

```rust
#[serde(default, skip_serializing_if = "DependencyAuditPolicy::is_explicit")]
pub dependency_audit: DependencyAuditPolicy,
```

It is read into both managers at `build` and stored as a plain field (`DependencyAuditPolicy` is
`Copy`), because the options struct is not kept after construction today.

### ExtValue Extensions

None.

## Trait Implementations

No new trait. Additive methods only, all with default implementations where a default is
meaningful, so `liquers-py` / `liquers-web` implementors are unaffected.

### `AssetManager<E>` (`assets.rs`)

```rust
/// The configured audit policy. Default: `Explicit`.
fn dependency_audit_policy(&self) -> DependencyAuditPolicy { DependencyAuditPolicy::Explicit }

/// `trigger_dependency_audit` with a mode. The existing method becomes
/// `self.trigger_dependency_audit_with(query, AuditMode::Expire)`.
async fn trigger_dependency_audit_with(&self, query: &Query, mode: AuditMode)
    -> Result<AuditReport, Error>;
async fn trigger_dependency_audit_all_registered_with(&self, mode: AuditMode)
    -> Result<AuditReport, Error>;

/// Current version of any store-addressable dependency key, without evaluating:
/// `-R/` → the existing `version(&Key)`; `-R-dir/` → `listing_version` of `listdir(key)`;
/// anything else → `Ok(None)` with "not store-resolvable" meaning, distinguished by the caller
/// through `DependencyKey::is_store_resolvable()` (below) rather than by the `None`.
async fn dependency_version(&self, dep_key: &DependencyKey) -> Result<Option<Version>, Error>;

/// Recompute and register the listing version of `dir` **iff** the dependency manager already
/// holds one (i.e. something depends on the listing); cascades if it moved. Called after every
/// manager-mediated store write or removal, with the written key's parent.
async fn refresh_listing_version(&self, dir: &Key);

/// `expire_dependencies_result` with an explicit reason, for the audit. The existing method
/// becomes `..._with(expired, reason_from_trigger)`, where the reason is `Cascade { trigger }`.
async fn expire_dependencies_result_with(&self, expired: ExpiredDependents<E>, reason: ExpiryReason);
```

`register_plan_dependencies` (`:4452`) changes behaviour but not signature. A plan dependency with
no registered version is **added with `Version::unknown()`** instead of being skipped. An
unknown-expecting edge is expired by any `register_version` change and spared by
`report_no_version` (`dependencies.rs:196-240`), which is the conservative answer for a dependency
whose version will arrive later. This is what makes the `-R-dir/` edge exist during evaluation,
before the step registers the listing version. Nothing is logged for the skip any more, because
nothing is skipped.

### `DependencyManager<E>` (`dependencies.rs`)

```rust
/// Audit counterpart of `register_version`: record `version` for `key`, then compare it against
/// **every** dependent's recorded version, whether or not this manager held a version before.
/// Returns the expired set (trigger = key) and the direct findings.
pub(crate) async fn audit_version(&self, key: &DependencyKey, version: Version)
    -> (ExpiredDependents<E>, Vec<AuditFinding>);

/// Read-only: the direct edges of `key` that `version` (or `None` = no current version)
/// contradicts, under the same sparing rules as `audit_version` / `report_no_version`.
/// Used by `AuditMode::ReportOnly`; mutates nothing.
pub(crate) async fn stale_edges(&self, key: &DependencyKey, version: Option<Version>)
    -> Vec<AuditFinding>;
```

`audit_version` is `stale_edges(key, Some(version))` for the findings, then the version insert
(occupied or vacant alike), then `expire_stale_dependents`. That last step already spares only on
positive evidence, which is why it is safe on a first observation (the
`AUDIT-CANNOT-EXPIRE-ON-A-FIRST-OBSERVED-VERSION` analysis). `register_version` is **not** changed,
because on the evaluation path a first registration really is not a change.

`report_no_version` gets the same findings via `stale_edges(key, None)` and sets `trigger`.

### `DependencyKey` (`metadata.rs`)

```rust
/// True for `-R/` and `-R-dir/` keys, whose version a store can answer. Replaces the implicit
/// "`key()` returned None, skip" in `audit_gaps`.
pub fn is_store_resolvable(&self) -> bool;
```

### `Context<E>` (`context.rs`), for Part E (revised after the Phase 2 gate)

The owner asked for the "start" half to be called **submit** and for **no new handle type**:
`AssetRef` already is the handle. The revision follows both requests:

```rust
/// Start evaluating `query` as a dependency of the current asset and return at once with its
/// asset. The dependency is recorded (and cycle-checked) exactly as `get_dependency_state` does.
/// The command can then do other work and later wait with `wait_for_dependency`.
pub async fn submit(&self, query: &Query) -> Result<AssetRef<E>, Error>;

/// Wait for a dependency previously returned by `submit` (or by `evaluate`), applying the
/// dependency policy: while waiting the parent is shown as `Status::Dependencies`, and a
/// dependency that expired in the meantime is used as-is and the parent is marked for
/// recomputation (`ExpiryReason::StaleDependency`). The dependency's version is recorded.
pub async fn wait_for_dependency(&self, dependency: &AssetRef<E>) -> Result<State<E::Value>, Error>;
```

`wait_for_dependency` already exists as `pub(crate)` with this exact signature (`context.rs:711`).
The change makes it public and has it record the version.

**Why a context method rather than `AssetRef::get` / `poll_state`.** They answer different
questions. `AssetRef::get` asks for the value of *an* asset. It knows nothing about who is waiting,
and on an expired asset it returns an error (`assets.rs:3401-3412`). Waiting *as a dependency*
also needs the parent: to show the parent as `Dependencies` while it waits, to use a stale value
rather than fail, and to record which version was consumed. Only the context knows the parent,
which is why the wait lives on `Context` and `AssetRef` stays unchanged.

**How the key is found without a handle.** The version record has to be updated under the
same `DependencyKey` that `submit` wrote, because `Context::add_dependency` matches records by key
(`context.rs:996`). A key re-derived from the asset could differ, since the record uses the
*resolved query* and the asset may be keyed. So `submit` also remembers
`asset id → DependencyKey` in a small map shared by the context's clones (beside
`pending_dependencies`, `context.rs:424`), and `wait_for_dependency` looks it up. An asset the
context did not submit is waited on without a version upgrade, which is what the `pub(crate)`
method does today.

**Relation to the existing `Context::evaluate`** (`context.rs:746`, public, no callers in
`liquers-lib`). It already does most of `submit`: it schedules, records, drains the local queue and
returns the `AssetRef`. It becomes `submit` followed by the drain, so both remember the key and
`evaluate` keeps its behaviour. `submit` does not drain. On the immediate (inline) manager that
means the dependency runs when it is first waited for, so a command can submit several
dependencies before any of them runs.

`get_dependency_state` becomes `let a = self.submit(q).await?; self.wait_for_dependency(&a).await`,
which behaves the same as today. An `AssetRef` that is submitted and never waited for simply
completes. That is what a pre-pass-scheduled dependency does today, and no parent is left in
`Status::Dependencies`.

### Part F: implementing `AssetManager` outside `liquers-core`

**Today.** `AssetManager<E>` is sealed by accident of visibility. It requires
`DependencyManagerAccess<E>`, which is `pub(crate)` (`assets.rs:3871`) and returns the
`pub(crate)` `DependencyManager<E>` (`dependencies.rs:114`). The trait is compiled under
`#[allow(private_bounds)]` (`assets.rs:3894`), so the compiler does not complain. The owner
decided at the gate that `DependencyManagerAccess` can be public.

**Principle: expose what an implementor must *hold* and *call*, not the graph.** An
implementor never reads the graph. Every dependency-graph operation lives in default trait methods
inside core (`cascade_expire_dependents`, `expire_dependencies_result`, `audit_gaps`,
`register_plan_dependencies`, and Part A–D additions). What an implementor must do is own one
graph, hand it out, and drive assets through their lifecycle.

#### F1. The graph: public but opaque

```rust
// assets.rs: `#[allow(private_bounds)]` removed
pub trait DependencyManagerAccess<E: Environment> {
    /// The dependency graph this manager owns. Implementors create one with
    /// `DependencyManager::new()`, keep it for their lifetime, and return it here; they do not
    /// call into it: the `AssetManager` default methods do.
    fn dependency_manager(&self) -> &DependencyManager<E>;
}

// dependencies.rs
pub struct DependencyManager<E: Environment> { /* fields stay private */ }
impl<E: Environment> DependencyManager<E> {
    pub fn new() -> Self;                 // exists, already `pub`
}
impl<E: Environment> Default for DependencyManager<E> { .. } // new
```

Every method that is `pub` on the type today (`register_version`, `add_dependency`, `track_asset`,
`expire`, `load_from_records`, `remove`, …; see `dependencies.rs:143-887`) is **narrowed to
`pub(crate)`** in the same change. The type becomes public, but its interface does not.
Otherwise making the struct public would silently publish the whole graph API, which
`keyed-expiry-cascade-fix` deliberately kept private. `ExpiredDependents` stays public (it is
already), because `expire_dependencies_result*` takes it.

`keyed-expiry-cascade-fix` planned a second sealed supertrait, `VersionResolver`. It does not exist
at HEAD (no `trait VersionResolver` in `liquers-core/src`), so `DependencyManagerAccess` is the only
seal. Narrowing the graph methods breaks nothing: the type is `pub(crate)` today, so no code outside
`liquers-core/src` can call them now.

#### F2. The lifecycle primitives an implementor calls

Derived from what `ImmediateAssetManager` (the simpler built-in) calls to implement its
*required* methods (`assets.rs:6616-7175`). The same set covers a queued manager with `run`.

| Primitive | Today | Becomes | Why an implementor needs it |
|---|---|---|---|
| `AssetData::new(id, recipe, key, envref)`, `.to_ref()` | `pub` | unchanged | construct an asset |
| `AssetRef::run_inline(payload)` | `pub(crate)` (`:2657`) | `pub` | evaluate in the current task (inline manager) |
| `AssetRef::run(payload)` | `pub(crate)` (`:2597`) | `pub` | evaluate as a spawned job (queued manager) |
| `AssetRef::submitted()` | `pub(crate)` (`:2315`) | `pub` | mark an asset queued before `run` |
| `AssetRef::set_payload_path(path)` | `pub(crate)` (`:1916`) | `pub` | `get_dependency_asset_with_payload` |
| `AssetRef::expire_without_cascade(reason)` | `pub(crate)` (`:3349`) | `pub` | lazy/deadline expiry in the manager's own lookup (Part C adds `reason`) |
| `load_command_versions_sync` | `pub(crate)` free fn (`:3800`) | stays `pub(crate)` | not needed: see F3 |

Each one made public gets a doc comment stating its contract: when it may be called, what
status it expects and leaves, and what it must not be combined with. That is the documentation
`STORE_IMPLEMENTATION_GUIDE` provides for stores.

Explicitly **not** exposed: the run claims (`RunClaim`, `InlineRunClaim`, `try_claim_for_run*`),
the job queue, `set_status`, `set_value`, `fail_asset`. `run` and `run_inline` already take the
claim internally, and raw status writes would let an implementor bypass the status authority that
`stale-dependency-status-finalization` centralized.

#### F3. Fewer required methods

`refresh_command_versions` is identical in both built-ins (`assets.rs:6068`, `:7124`). It moves to
a default trait body over `self.dependency_manager()` and `self.get_envref()`. That removes the
only reason an implementor would need `load_command_versions_sync`. `start` stays required,
because it owns the manager's own "started" flag, but its documentation says to call
`self.refresh_command_versions()`.

#### F4. Proof: an external manager in `liquers-core/tests/`

An integration test is its own crate, so it sees only the public API. That makes it the right
proof that F1–F3 suffice:

- `liquers-core/tests/external_asset_manager.rs` defines `MinimalInlineAssetManager<E>` **from
  scratch** (not a wrapper around `ImmediateAssetManager`, which would prove nothing), plus an
  `AssetManagerKind` for it, and builds an environment with it.
- The scenario bodies of `tests/manager_parametric.rs` (already generic over `E: Environment`) move
  to `tests/common/manager_scenarios.rs`. The parametric suite and the external suite both run
  them, and the Part A–E tests that are not manager-specific join that shared set.

This is the asset-manager counterpart of the store conformance suite that
`ASSET-MANAGER-TRAIT-CANNOT-BE-IMPLEMENTED-OUTSIDE-CORE` asks for. It is deliberately smaller: the
shared scenarios, not a rule-numbered suite. A rule-numbered suite with a capability model is the
follow-up to file if external managers appear in practice.

#### F5. What stays out

- No change to how an environment *selects* a manager: `AssetManagerKind` is already public
  (`environment_builder.rs:71`) and `build` already receives `AssetManagerOptions`.
- No stability promise beyond "public and documented". The primitives carry a doc note that
  they are the manager-implementation surface and may be refined. Semver discipline for them is the
  owner's call when an external manager ships.

### Part G: detecting content changed outside Liquers

*Added 2026-09-29 at the owner's request, together with the solution direction. Closes
`STORE-VERSION-BLIND-TO-CHANGES-MADE-OUTSIDE-LIQUERS`. Phase 1 example 8.*

**Precondition verified at HEAD.** The version recorded for a stored value is the hash of *exactly*
the bytes handed to `store.set`: evaluation reuses the same buffer (`PreparedVersion.binary`,
`assets.rs:2020-2024`), `set_binary` hashes the binary it stores (`:5746`), and `set_state` hashes
`state.as_bytes()`, which is the same call whose result it stores (`:5860`, `:5887`). Verification
re-hashes the *stored* bytes rather than re-serializing, so whether a serializer is deterministic
does not matter.

#### G1. Versions that say whether they are a hash

```rust
// metadata.rs
impl Version {
    /// Bit 127. Set on every content hash, and on nothing else.
    pub const HASH_FLAG: u128 = 1 << 127;

    /// Content hash of stored bytes: blake3, first 128 bits, with `HASH_FLAG` set (127 bits
    /// of hash). Used wherever an *asset's content* is versioned.
    pub fn from_content(bytes: &[u8]) -> Self;

    /// True when this version was produced by `from_content`, and so can be re-checked.
    pub fn is_content_hash(&self) -> bool;

    /// Compare against `bytes`.
    pub fn verify(&self, bytes: &[u8]) -> VersionCheck;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionCheck {
    /// The bytes are what this version fingerprinted.
    Verified,
    /// A content hash whose bytes have changed; `actual` is `from_content(bytes)`.
    Mismatch { actual: Version },
    /// Not a content hash (unknown, time-based, unique, or a legacy value that does not match),
    /// so nothing can be concluded.
    NotVerifiable,
}
```

- **`from_bytes` stays as it is** (unflagged). It is also used for command metadata versions
  (`command_metadata.rs:1233`), and flagging those would change the version of about half of all
  commands on upgrade, which would recompute about half of every stored result once. Only the
  five *content* sites switch to `from_content` (`assets.rs:2022`, `:5746`, `:5860`, `:6985`,
  `:7036`). `:1961` is a `new_unique` fallback, not a hash, and stays.
- `from_time_now`, `from_specific_time` and `new_unique` **mask bit 127 off** explicitly. Today
  they leave it clear only by magnitude, and `new_unique` would reach it in 2262.
- **Legacy hashes** (stored before this change, unflagged): `verify` also accepts an unflagged value
  equal to the *unflagged* hash of the bytes, and reports `Verified`, because a 128-bit hash cannot
  equal a timestamp by chance. An unflagged value that does *not* match is `NotVerifiable`, because
  it might be a timestamp. So unchanged legacy data is recognised, and a changed legacy value is
  missed until the value is rewritten. There is no migration pass and no forced recomputation.

#### G2. The decision, as one pure function

```rust
// assets.rs
/// What to do when a stored value's bytes no longer match its recorded version.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExternalChangePolicy {
    /// Keep the new content as the user's: a recipe-backed value becomes `Override`.
    #[default]
    UserInput,
    /// Treat the content as damaged: delete the stored copy, so the recipe recomputes it.
    Corrupted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExternalChangeAction {
    /// `Source` / `Override`: keep the status, adopt `actual` as the version.
    AcceptAsInput { actual: Version },
    /// Recipe-backed, `UserInput`: set status `Override`, adopt `actual` as the version.
    ConvertToOverride { actual: Version },
    /// Recipe-backed, `Corrupted`: remove data and metadata from the store.
    Delete,
}

fn external_change_action(
    status: Status,
    has_recipe: bool,
    policy: ExternalChangePolicy,
    actual: Version,
) -> ExternalChangeAction;
```

| Stored status | Recipe? | `UserInput` (default) | `Corrupted` |
|---|---|---|---|
| `Source` | no | accept as input | accept as input (the owner's rule: a `Source` is always input) |
| `Override` | either | accept as input | accept as input (an override is the user's by definition) |
| `Ready`, `Expired` | yes | convert to `Override` | delete |
| *no metadata at all* (file dropped in) | no | not a change: already a `Source`, nothing recorded to compare | same |
| *no metadata at all* | yes | convert to `Override` (owner, gate answer 3) | delete |

The match over `Status` is explicit. Every other status (`None`, `Directory`, `Error`, `Volatile`, …)
is **not checked**, because there is no reusable stored value to protect.

Applying an action, in `AssetManager::apply_external_change(key, metadata, action)` (a default method):

1. For `AcceptAsInput` and `ConvertToOverride`: write the new version (and status) into the stored
   metadata, and log *"content of data/a.csv changed outside Liquers; accepted as user input"*
   (`warning`). Then call `register_version(key, actual)`, which cascades to dependents with reason
   `Cascade { trigger: key }` (Part C). They recorded the old version, so they are now stale.
2. For `Delete`: remove the key's data and metadata from the store, log to stderr (the metadata
   that would hold the log is gone), and `cascade_expire_dependents(key)`.
3. Take `key_mutation_lock` around the store writes, as the other keyed mutations do.

#### G3. When it runs

```rust
// environment_builder.rs, AssetManagerOptions
#[serde(default, skip_serializing_if = "VersionVerification::is_on_read")]
pub verify_versions: VersionVerification,   // off | on_read (default on_read)
#[serde(default, skip_serializing_if = "ExternalChangePolicy::is_user_input")]
pub external_change: ExternalChangePolicy,  // user_input (default) | corrupted
```

- **On read** (default `on_read`): wherever the asset manager has already read the stored
  bytes, which is `try_fast_track` (`assets.rs:1116`) and the store branches of `get_any_status`
  / `get_binary_any_status` (`:4476`, `:4516`). The extra cost is one blake3 pass over bytes that
  were read anyway. In `try_fast_track`, `ConvertToOverride` and `AcceptAsInput` continue loading
  with the new version and status, and `Delete` returns `false`, so the recipe recomputes.
- **On demand:**

```rust
/// Read every stored value under `key` (recursively when `deep`), verify its version and apply
/// the configured policy (or only report, with `AuditMode::ReportOnly` from Part B).
async fn verify_stored_versions(&self, key: &Key, deep: bool, mode: AuditMode)
    -> Result<VersionVerificationReport, Error>;

#[non_exhaustive]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VersionVerificationReport {
    pub verified: Vec<Key>,
    pub not_verifiable: Vec<Key>,
    pub changed: Vec<(Key, ExternalChangeAction)>,   // in ReportOnly: what *would* be done
}
```

- **Not a mismatch:** missing bytes. Deleted data with metadata kept is the exploratory workflow
  (`DEPENDENCY-AUDIT-POLICY-NOT-EXPRESSIBLE`), so a key whose data is gone is skipped. Also not a
  mismatch: a metadata-only entry (no bytes by design), whose version is `new_unique` and so
  `NotVerifiable` (`METADATA-ONLY-ENTRY-RELOADS-AS-CORRUPTED`).
- **The Part B audit is unchanged.** It compares *recorded* versions using metadata only. Part G is
  what makes that metadata truthful: after verification, a changed `a.csv` carries its real
  version, so example 1 then works for outside edits too.

#### G4. Relation to the other parts

- **C:** no new `ExpiryReason`. Dependents are expired by the ordinary cascade, and the changed
  asset itself is not expired: it becomes input (`Source` / `Override`) or is deleted. The log
  line on the asset records what happened.
- **F:** `apply_external_change` and `verify_stored_versions` are default trait methods, so
  external managers get them. The read-path check lives in `AssetData::try_fast_track`, which is
  shared by all managers.

## Generic Parameters & Bounds

Everything is generic over `E: Environment`, as the surrounding code is. No new bounds.
The new `asset id → DependencyKey` map is `Arc<tokio::sync::Mutex<HashMap<u64, DependencyKey>>>`, the same shape as `pending_dependencies`.

## Sync vs Async Decisions

| Function | Async? | Rationale |
|---|---|---|
| `audit_version`, `stale_edges` | async | `scc` async entry APIs, same as `register_version` |
| `dependency_version`, `refresh_listing_version` | async | store I/O (`listdir`, `get_metadata`) |
| `ExpiryReason::log_entry`, `listing_version` | sync | pure |
| `dependency_audit_policy` | sync | reads a `Copy` field |
| `Context::submit`, `Context::wait_for_dependency` | async | wrap existing async calls |

**Lock discipline (blocking constraint from `EXPIRY-RECORDS-NO-REASON`).** `mark_expired_status`
writes `expiry_reason` and the log entry **under the same `data` write lock** that flips the status,
before `persist_info` is cloned (`assets.rs:3280-3325`). The persisted metadata therefore carries the
reason. `expire_stored_copy` sets both on the metadata it reads before `set_metadata`.

## Function Signatures (changed internals)

```rust
// assets.rs: AssetRef
pub async fn expire(&self) -> Result<(), Error>;                    // = expire_with_reason(Explicit)
pub(crate) async fn expire_with_reason(&self, reason: ExpiryReason) -> Result<(), Error>;
async fn mark_expired_status(&self, reason: ExpiryReason) -> Result<bool, Error>;
pub(crate) async fn expire_without_cascade(&self, reason: ExpiryReason) -> Result<(), Error>;
pub(crate) async fn note_expired_dependency(&self, dependency: &AssetRef<E>) -> Result<(), Error>;
    // unchanged signature; message names DependencyKey of `dependency` (key, else query) and
    // this asset's `asset_reference()`, never `id()`

// assets.rs: free fn
async fn expire_stored_copy(store: Arc<dyn AsyncStore>, key: &Key, reason: &ExpiryReason);

// dependencies.rs: free fn
/// Version of a directory's membership: blake3 over the sorted names, each length-prefixed so
/// no concatenation of names can collide with another.
pub(crate) fn listing_version(names: &[String]) -> Version;

// metadata.rs: Metadata
/// `Some` only for a `MetadataRecord` whose status is `Expired`.
pub fn expiry_reason(&self) -> Option<ExpiryReason>;
```

Call sites that pick a reason:

| Site | Reason |
|---|---|
| Queued expiration monitor, `assets.rs:4779` (`expire()` today) | `expire_with_reason(Deadline { expiration_time })` |
| Immediate manager lazy check, `:6823` / `:6911` | `Deadline`, **after fixing the dead condition** (see below) |
| `AssetRef::expire` | `Explicit` |
| `expire_dependencies_result` | `Cascade { trigger }` from `ExpiredDependents::trigger` |
| `audit_gaps` | `Audit { dependency, found }` via `expire_dependencies_result_with` |
| `finalize_status_with_version` stale branch, `:2098` | `StaleDependency { dependency }` from `AssetData::stale_dependency` |

**Found while enumerating the routes: the immediate manager's deadline route is dead code.**
`status == Status::Ready && assetref.is_expired().await` compares status with status, not the
deadline, so it is true only in a race. It is filed as
`IMMEDIATE-MANAGER-LAZY-DEADLINE-EXPIRY-NEVER-FIRES`. Including it costs a one-line condition fix
(`expiration_time().await.is_expired()`) and a test. Without it the `Deadline` reason cannot be
tested on the immediate manager. **Proposed in scope; see Questions.** Whether lazy expiry should
cascade (the queued monitor's `expire()` does, and this path's `expire_without_cascade` does not)
is left as it is and noted on the issue.

### Directory listing: where it plugs in

1. **Plan time.** `plan.rs:2663` already emits the `-R-dir/` `PlanDependency`.
   `finalize_plan_expanded` records it as `Version::unknown()`, and `register_plan_dependencies` now
   adds the edge with `unknown`.
2. **Step time.** `interpreter.rs:780` (`Step::GetAssetDirectory`), after `listdir_asset_info`:
   compute `listing_version` from `asset_manager.listdir(&key)`, call
   `dm.register_version(&DependencyKey::from_dir_key(&key), v)` and apply the result. Then call
   `context.add_dependency(DependencyRecord::new(dir_dep_key, v))`, which upgrades the unknown
   record, since `add_dependency` prefers a concrete version (`context.rs:699`). It also calls
   `dm.add_dependency(owner, dir_dep_key, v)` when the context has an owner key. This is the pattern
   `wait_for_dependency_recording` already uses for `-R/` keys.
3. **Write time.** After `save_to_store`, `set_binary`, `set_state` and `remove` on a key `k`:
   `refresh_listing_version(&k.parent())`. It costs one `get_version` map read when nobody depends
   on the listing, and one `listdir` otherwise.
4. **Audit and load.** `audit_gaps` resolves `-R-dir/` gaps through `dependency_version`.
   `try_fast_track` under `OnLoad` does the same. `dependency_blocks_fast_track`'s
   `Key::try_from` rejection of a `-R-dir/` key becomes an explicit early `false` with a comment
   ("a listing has no status; its staleness is a version question"). The two
   "not store-addressable" branches the issue names therefore become a deliberate answer.

## Integration Points

### Crate: liquers-core

| File | Change |
|---|---|
| `metadata.rs` | `ExpiryReason`; `expiry_reason` on `MetadataRecord` + `AssetInfo` + projections; `Metadata::expiry_reason`; `DependencyKey::is_store_resolvable` |
| `dependencies.rs` | `ExpiredDependents::trigger`; `audit_version`; `stale_edges`; `listing_version`; `report_no_version` findings |
| `assets.rs` | `AuditMode`, `AuditFinding`, `AuditReport::findings`; reasons at every route; `audit_gaps` rewrite; `OnLoad` in `try_fast_track`; `register_plan_dependencies` unknown edges; `refresh_listing_version` calls; immediate-manager condition; `stale_dependency: Option<DependencyKey>` |
| `environment_builder.rs` | `DependencyAuditPolicy`; `AssetManagerOptions::dependency_audit` (+ `with_dependency_audit`) |
| `interpreter.rs` | `GetAssetDirectory` registers and records the listing version |
| `assets.rs` (Part F) | `DependencyManagerAccess` public, `#[allow(private_bounds)]` removed; `run`, `run_inline`, `submitted`, `set_payload_path`, `expire_without_cascade` public with contracts; `refresh_command_versions` default body |
| `dependencies.rs` (Part F) | `DependencyManager` public and opaque (methods narrowed to `pub(crate)`), `Default` |
| `tests/external_asset_manager.rs`, `tests/common/manager_scenarios.rs` (Part F) | from-scratch external manager; scenarios shared with `manager_parametric.rs` |
| `metadata.rs`, `assets.rs`, `environment_builder.rs` (Part G) | `Version::HASH_FLAG`, `from_content`, `is_content_hash`, `verify`, `VersionCheck`; time/unique constructors mask bit 127; five content sites use `from_content`; `ExternalChangePolicy`, `ExternalChangeAction`, `external_change_action`, `apply_external_change`, `verify_stored_versions`, `VersionVerificationReport`; check in `try_fast_track` and the `*_any_status` store branches; `AssetManagerOptions::{verify_versions, external_change}` |
| `context.rs` | `submit`; `wait_for_dependency` made public and version-recording; submitted-key map; `evaluate` and `get_dependency_state` rewritten on top |

### Other crates

- **liquers-py**: `grep` shows no `ExpiredDependents`/`AuditReport` use. If it mirrors `AssetInfo`
  fields by hand, it gets an optional `expiry_reason`. Phase 4 checks this.
- **liquers-web**: `AssetInfo` crosses to JavaScript through serde, so the field appears there
  automatically and is optional. The `.d.ts` stub check (`check-stubs.sh`) may need the field.
  Phase 4 checks this.
- **liquers-axum**: out of scope. It serializes `AssetInfo`, so it gains the field with no code change.

### Dependencies

None added. `blake3` is already used by `Version::from_bytes`.

## Documentation Architecture

### Reference Plan

Extend, no new reference (Phase 1 rationale: one contract, one document).

| Path | Audience | Area | Change |
|---|---|---|---|
| `specs/reference/DEPENDENCIES_STATUS.md` | internal | core/assets | §"Current contract": audits compare current versions with recorded ones even on first observation; `DependencyAuditPolicy` (`explicit` / `on_load`); `AuditMode::ReportOnly` and `AuditFinding`; `-R-dir/` dependencies (membership version, when refreshed, backend caveat); plan dependencies with no version get an `unknown` edge. §"Function glossary": `audit_version`, `stale_edges`, `dependency_version`, `refresh_listing_version`. Replace the sentence citing `DEPENDENCY-AUDIT-POLICY-NOT-EXPRESSIBLE` as open. |
| `specs/reference/ASSETS.md` | internal | core/assets | §"The one meaning of `Expired`": `ExpiryReason`, its five variants and which route sets each, the "meaningful only while `Expired`" rule, the log levels, and why it is not a status. §"AssetManager": the trait is implementable outside core; point to the new guide. New section "Content changed outside Liquers" (Part G): content-hash versions and the flag bit, what is verifiable (including legacy values), the decision table, when the check runs, and the read-only-store behaviour. |

### Guide Plan

| Path | Action | Audience | Area | Task and content | Links |
|---|---|---|---|---|---|
| `specs/guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md` | **create** | internal and external implementors | core/assets | How to implement `AssetManager` outside core: what to hold (a `DependencyManager`, an `EnvRef`), which methods are required, which defaults to keep, the five lifecycle primitives and their contracts, the registration invariants (from `ASSET-REGISTRATION-OWNERSHIP-CONTRACT`), how to provide an `AssetManagerKind`, and how to run the shared manager scenarios. Snippets come from `tests/external_asset_manager.rs`. | `ASSETS.md`, `DEPENDENCIES_STATUS.md`, `ENVIRONMENT_CONSTRUCTION_GUIDE.md`, `STORE_IMPLEMENTATION_GUIDE.md` (as the pattern) |
| `specs/guides/COMMAND_REGISTRATION_GUIDE.md` | extend | command authors | core/commands | New section: start several dependencies with `context.submit`, wait with `context.wait_for_dependency`, and why not `asset.get()`. The existing example at `COMMAND_REGISTRATION_GUIDE.md:123` waits through `asset.get()` after `context.evaluate`. That is exactly the pattern that fails on an expired dependency, so it is rewritten to `context.wait_for_dependency(&asset)`. | `DOC_04` |

### Other Documents to Create

None. The per-design summary is `phase5-documentation.md` in this folder.

### New Reference or Guide Documents

| Path | Kind | Audience | Area | Purpose |
|---|---|---|---|---|
| `specs/guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md` | guide | both | core/assets | Implement and verify an asset manager outside `liquers-core` |

### Existing Documents to Review or Update

Every row gets `reviewed:` bumped and a `## History` row (§9.2).

| Document | In `affects_docs` | Change |
|---|---|---|
| `DEPENDENCIES_STATUS` | yes | see Reference Plan |
| `ASSETS` | yes | see Reference Plan |
| `ASSET_LIFECYCLE` | yes | where it lists the routes into `Expired` (monitor, lazy, explicit, cascade), name the reason each sets; correct the immediate manager's lazy check |
| `DOC_03_ASSETS_EXECUTION_LIFECYCLE` | yes | §"Expiration, recovery, and cancellation": the same route/reason table. Its P1 API finding ("public trait exposes a private dependency-manager type") is resolved by Part F, so update that row. |
| `DOC_04_ENVIRONMENT_CONTEXT_EVALUATION` | yes | §"Dependency and apply methods": add `submit` and the now-public `wait_for_dependency` to the table (payload and CWD rules as for `get_dependency_state`); `evaluate` = `submit` + drain |
| `ENVIRONMENT_CONFIG` | yes | §"Format": `assets.dependency_audit: explicit \| on_load`, `assets.verify_versions: off \| on_read`, `assets.external_change: user_input \| corrupted`, with defaults and meaning |
| `COMMAND_REGISTRATION_GUIDE` | yes | see Guide Plan |
| `ENVIRONMENT_CONSTRUCTION_GUIDE` | yes | where manager kinds are chosen: one line and a link to the new guide for a custom kind; `with_dependency_audit` on `AssetManagerOptions` |
| `STORE_IMPLEMENTATION_GUIDE` | yes | one "see also" line to the new guide |
| `UNITTEST_GUIDE` | yes | a short note on `tests/common/manager_scenarios.rs`: write manager-contract tests there so every manager, including external ones, runs them |
| `PROJECT_OVERVIEW` | **no** (discarded) | area match only. It states no expiry or audit detail that changes, and the key types list is unaffected |
| `DOC_01_ARCHITECTURE_REFERENCE`, `DOC_08_RECIPES_PLANS` | **no** (discarded) | area match only. The plan still emits the same `-R-dir/` dependency, and only its handling changes |
| `ASSET_SET_OPERATION` | **no** (discarded) | `set_state` / `set_binary` semantics are unchanged; the listing refresh after them is internal |
| `PAYLOAD_GUIDE` | **no** (discarded) | `submit` inherits the payload exactly as `get_dependency_state` does. It is covered by the DOC_04 row, so the payload guide has nothing new to say |

### Design and Capability Links

- **During design:** the `specs/README.md` line "Dependency audit correctness, audit policy and
  expiry provenance — designing" is renamed to include external asset managers.
- **At Phase 5:** that line moves to `documented`, pointing at `reference/DEPENDENCIES_STATUS.md` with the
  design in parentheses. A new capability line, "Asset managers outside core — documented", points at
  the new guide. The existing "Versions for computed keyed assets, and dependency audit" line gains
  a pointer to the policy section.
- `STORE_IMPLEMENTATION_GUIDE.md` gets a one-line "see also" to the new guide. §9.2 has no
  link-only exemption, so it is in `affects_docs` and gets a History row and a `reviewed:` bump like
  the rest.

### Evidence to Collect During Implementation

- Which primitive, if any, turned out to be missing when writing the from-scratch external manager.
  This is the most likely place for Part F to be wrong, and the guide must say it.
- Whether `OnLoad` changes any existing test outcome (it should not: the default is `Explicit`).
- Real log lines for each `ExpiryReason`, for use in the `ASSETS.md` examples.
- Whether any backend's `listdir` ordering or duplicates affected the listing version. The function
  sorts, but record it if a store returns duplicates.

## Relevant Commands

### New Commands

None. The feature lives entirely below the command layer.

### Relevant Existing Namespaces

None are affected. Tests register small ad-hoc commands (a gate command, a counting command) inline,
the way `liquers-core/tests/expiration_integration.rs` does, and use `-R-dir/` through the existing
directory step. No `liquers-lib` namespace is involved.

## Web Endpoints

None.

## Error Handling

No new error types or constructors. Existing typed constructors cover everything:

| Scenario | Handling |
|---|---|
| `listdir` fails while refreshing a listing version after a write | `eprintln!` and continue. The write already succeeded, and the next audit resolves the listing |
| `dependency_version` store error in an audit | propagated as `Err` (unchanged `version()` contract: an error is not "no version") |
| Same, under `OnLoad` in `try_fast_track` | refuse the fast track (recompute). A store that cannot answer is not evidence of freshness |
| `submit` cycle | `Error::dependency_cycle`, as `get_dependency_state` today |
| `wait_for_dependency` on an expired-and-evicted dependency | unchanged `wait_for_dependency` error |

## Serialization Strategy

- `ExpiryReason`: internally tagged (`"kind": "cascade", "trigger": "-R/a.txt"`), `snake_case`.
  Readable in a sidecar, and new variants can be added.
- `expiry_reason`: `default` + `skip_serializing_if = "Option::is_none"` on both structs. Old
  records load. New non-expired records serialize byte-identically to today. See the
  `deny_unknown_fields` note above for old binaries reading new expired records.
- `DependencyAuditPolicy`: `snake_case` in `EnvironmentConfig` YAML
  (`assets: { dependency_audit: on_load }`). The field is omitted when it has the default value.

### Round-trip Compatibility

Phase 3 carries tests for: a record without the field loads as `None`; each variant
round-trips through JSON and YAML; a non-expired record's JSON is unchanged byte-for-byte; and
an `EnvironmentConfig` with and without `dependency_audit` parses.

## Concurrency Considerations

- **Reason and status are written together** under the asset's `data` write lock, so no reader
  observes `Expired` without its reason, and the persisted copy has both.
- **`audit_version` vs a concurrent `register_version`**: the same `scc` entry lock orders them.
  The later one wins the stored version, and each one's cascade is correct for the version it saw.
  This is the snapshot tolerance `audit_gaps` already documents.
- **`refresh_listing_version` after a write** runs outside `key_mutation_lock`. Two concurrent
  writes into the same directory may each list and register. `register_version` is idempotent for
  equal versions, and the last listing wins, which is the true current membership.
- **The `AssetRef` returned by `submit`** keeps the dependency's value alive, so the
  stale-value arm has a value to use. That is the property the end-to-end test depends on.
- The `Option<DependencyKey>` in `AssetData` is written under the existing `data` lock
  (`note_expired_dependency` already takes it).

## Compilation Validation

```bash
cargo check -p liquers-core
cargo test -p liquers-core --lib --tests
cargo test -p liquers-lib --lib --tests          # AssetInfo consumers
bash scripts/check-build-matrix.sh               # wasm32 row
```

## rust-best-practices review (applied)

**Blocking: none remaining.** Checked and handled in the text above:

- Every new `match` on `ExpiryReason`, `DependencyAuditPolicy` and `AuditMode` is explicit (no `_ =>`).
  The two existing `Status` matches touched keep their explicit arms.
- No `unwrap`/`expect`. `listing_version` uses the existing `Version::from_bytes`, which already
  avoids `unwrap` (`unwrap_or`).
- `eprintln!` only; no `println!`.
- Additive trait methods have defaults, or are implemented once as shared trait defaults (as
  `audit_gaps` is today). Neither built-in manager needs a bespoke body except
  `dependency_audit_policy`.
- No backward crate dependency; all in `liquers-core`.

**Advisory:**

- `AuditReport` gains a public field. `#[non_exhaustive]` is applied to `AuditReport` and
  `AuditFinding` so the next addition is not breaking (see gate decision 3).
- `submit` is `#[must_use]`, so a dropped handle is a lint rather than silence.
- The `trigger` field on `ExpiredDependents` only makes sense for a non-empty set. Keep
  `ExpiredDependents::new()` for the empty case and add `ExpiredDependents::for_trigger(key)`,
  so there is no way to build a non-empty set without a trigger.

## Phase 2 review

The two parallel reviewer agents could not run: both failed at start with an API spend-limit error.
The two review passes were done inline against HEAD instead.

**Reviewer A, Phase 1 conformity.** Every Phase 1 interaction is addressed: Query (no syntax;
`-R-dir/` made live), Store (no trait change; metadata/`listdir` only), Command (no new commands;
the test seam), Asset (parts A–E), Value (none), Web/UI (additive `AssetInfo` field). All five
Phase 1 open questions are settled with rationale in §"How the Phase 1 open questions are settled".
The scope grows by one item, `IMMEDIATE-MANAGER-LAZY-DEADLINE-EXPIRY-NEVER-FIRES`, and that is put
to the owner as a question rather than assumed. Every "Expected behaviour" clause of the five
issues maps to a part:
`EXPIRY-RECORDS-NO-REASON`: all four routes, typed field, log entry, written under the lock
(C). `DIRECTORY-LISTING-…`: version, registration, both `Key::try_from` sites, no silent skip (D).
`AUDIT-CANNOT-…`: an audit-specific entry point, `register_version` untouched (A).
`DEPENDENCY-AUDIT-POLICY-…`: recording separated from verification, a policy, report-only, depth (B).
`STALE-DEPENDENCY-PATH-…`: option 1 with a seam, pending confirmation (E). No contradictions found.
One addition beyond the issues: `Audit` is a fifth reason besides the four in
`EXPIRY-RECORDS-NO-REASON`. It is justified because the audit is a route into `Expired` that the
issue did not enumerate.

**Reviewer B, codebase alignment.** Checked every citation and every existing signature the
document leans on. Corrections applied:
- `Context::evaluate` already exists and is public. It is now discussed as a reuse candidate in
  Part E, and the new handle is kept because `evaluate` loses the dependency key and bypasses the
  stale-value policy.
- `ExpirationTime` has hand-written serde (`expiration.rs:751`). An earlier draft said it had none.
- `AssetManagerOptions` has no generic `is_default` helper, so the field uses
  `DependencyAuditPolicy::is_explicit`.

Confirmed as stated: `register_version`'s `Vacant` arm makes no comparison (`dependencies.rs:158`).
`add_dependency` on the dependency manager tolerates an unregistered dependency, with tests
(`dependencies.rs:1076`). `register_plan_dependencies` skips on `get_version == None`
(`assets.rs:4452`). `MetadataRecord` is `deny_unknown_fields` (`metadata.rs:910`). The immediate
manager's lazy check compares status with status (`assets.rs:6823`, `:6911` against `:3365`).
`ExpiredDependents` is constructed only at `dependencies.rs:79` and `:790`. No crate outside core
builds `AuditReport` or `AssetInfo` by struct literal (`liquers-py`'s `AssetInfo` is an enum
variant wrapping the core type). No existing test asserts that `register_plan_dependencies` skips an
unregistered dependency, so adding `unknown` edges breaks no test; Phase 3 adds one that pins the
new behaviour.

Left for Phase 4 to verify at implementation time: whether `liquers-web`'s `.d.ts` stubs list
`AssetInfo` fields (`check-stubs.sh`).

## Gate decisions (2026-09-28)

1. **Part E API.** The owner accepts the capability, asks for the name `submit`, and asks for no
   new handle type because `AssetRef` is already the handle. Applied as `Context::submit` plus a
   now-public `Context::wait_for_dependency`, with the key kept in the context (see Part E).
2. **`IMMEDIATE-MANAGER-LAZY-DEADLINE-EXPIRY-NEVER-FIRES`.** In scope, because it fits the design:
   it is one of the routes into `Expired` that Part C gives a reason to.
3. **`#[non_exhaustive]` on `AuditReport` / `AuditFinding`: applied, with public constructors.** The
   owner noted that a custom asset manager outside core may be needed. That manager would have to
   *build* reports, not only read them. `#[non_exhaustive]` still allows it: only listing every field
   in a struct literal is forbidden, while `Default` plus assigning or pushing to the public fields
   works from any crate. To make it convenient:
   - `AuditReport::default()` (exists); fields stay `pub` and mutable;
   - `AuditFinding::new(dependency, dependent, expected, found)`;
   - `ExpiredDependents::for_trigger(key)` (already proposed in the rust-best-practices advisory).

   Checking this showed that a custom manager outside core is **impossible today** for an unrelated
   reason. `AssetManager` requires the crate-private supertrait `DependencyManagerAccess`
   (`assets.rs:3871`, `:3895`), so it is sealed. That was filed as
   `ASSET-MANAGER-TRAIT-CANNOT-BE-IMPLEMENTED-OUTSIDE-CORE`.
4. **`ASSET-MANAGER-TRAIT-CANNOT-BE-IMPLEMENTED-OUTSIDE-CORE` is in scope**, and
   `DependencyManagerAccess` may be public (owner, 2026-09-28). This is Part F. Parts A–E are built
   so an external manager needs nothing extra:
   - every new trait method has a default body;
   - every new type an implementor would construct has a public constructor
     (`ExpiryReason` is a plain public enum; `AuditReport`, `AuditFinding` and `ExpiredDependents`
     are covered above);
   - the new policy arrives through the already-public `AssetManagerOptions` passed to
     `AssetManagerKind::build`.
5. **`STORE-VERSION-BLIND-TO-CHANGES-MADE-OUTSIDE-LIQUERS` is in scope as Part G** (owner,
   2026-09-29). The owner set the solution direction: a hash flag bit, verification on read, a
   `Source` always taken as input, and a choice for recipe-backed values. Default `UserInput`
   (convert to `Override`), with `Corrupted` (delete) opt-in, set per environment. A file with no
   metadata under a recipe follows the same choice.
   Two refinements in the design, not in the owner's statement: only *content* hashes carry the flag
   (`from_content`), so command metadata versions do not change on upgrade; and unflagged legacy
   hashes still verify when unchanged.
