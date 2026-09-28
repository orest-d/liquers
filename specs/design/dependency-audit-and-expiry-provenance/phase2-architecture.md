# Phase 2: Solution & Architecture - Dependency Audit and Expiry Provenance

## Overview

Four changes in `liquers-core`, all built on the dependency graph that `keyed-expiry-cascade-fix`
built and the ordering precedent set by `stale-dependency-status-finalization`:

| Part | Closes | Shape |
|---|---|---|
| A. Audit compares against expectations | `AUDIT-CANNOT-EXPIRE-ON-A-FIRST-OBSERVED-VERSION` | New `DependencyManager::audit_version`; `audit_gaps` uses it instead of `register_version` |
| B. Audit policy and report-only audits | `DEPENDENCY-AUDIT-POLICY-NOT-EXPRESSIBLE` | `DependencyAuditPolicy` in `AssetManagerOptions`; `AuditMode`; `AuditFinding` in `AuditReport` |
| C. Expiry provenance | `EXPIRY-RECORDS-NO-REASON` (+ `IMMEDIATE-MANAGER-LAZY-DEADLINE-EXPIRY-NEVER-FIRES`) | `ExpiryReason` in `MetadataRecord` / `AssetInfo`; a log entry naming both participants |
| D. Directory-listing dependencies | `DIRECTORY-LISTING-DEPENDENCY-IS-NEVER-REGISTERED-OR-CHECKED` | A version for a listing, registered at the step and refreshed on manager writes; audit and fast-track resolve it |
| E. Stale-dependency reachability | `STALE-DEPENDENCY-PATH-HAS-NO-END-TO-END-TEST` | A public two-phase dependency API on `Context` (schedule, then wait) that doubles as the test seam |

No change to `Status`, to `AsyncStore`, to the query syntax, or to any command signature. No
`liquers-axum` change.

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
    /// An audit found that `dependency`'s durable version is not the one recorded. `found` is
    /// `None` when the dependency has no durable version at all.
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
/// When recorded dependency versions are verified against durable ones.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyAuditPolicy {
    /// Only when `trigger_dependency_audit*` is called. Today's behaviour, and the choice for
    /// exploratory work where intermediates are deleted by hand.
    #[default]
    Explicit,
    /// Also when a keyed asset is loaded from the store (`try_fast_track`): each recorded
    /// dependency the manager holds no version for is resolved, and a mismatch or a missing durable
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
/// durable version is `found`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditFinding {
    pub dependency: DependencyKey,
    pub dependent: DependencyKey,
    pub expected: Version,
    pub found: Option<Version>,
}

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

/// Durable version of any store-addressable dependency key, without evaluating:
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
/// **every** dependent's recorded expectation, whether or not this manager held a version before.
/// Returns the expired set (trigger = key) and the direct findings.
pub(crate) async fn audit_version(&self, key: &DependencyKey, version: Version)
    -> (ExpiredDependents<E>, Vec<AuditFinding>);

/// Read-only: the direct edges of `key` that `version` (or `None` = no durable version)
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

- `AuditReport` gains a public field. Consider `#[non_exhaustive]` now, while it has a single
  external consumer (`keyed_version_cascade.rs`), so the next addition is not breaking. It is
  proposed, but not applied without a decision, because it forbids struct literals in tests too.
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
3. **`#[non_exhaustive]` on `AuditReport` / `AuditFinding`.** Explained to the owner and awaiting
   an answer. The recommendation is yes.
