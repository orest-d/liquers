---
id: ASSET_MANAGER_IMPLEMENTATION_GUIDE
title: Asset Manager Implementation Guide
kind: guide
audience: both
area: [core/assets]
reviewed: 2026-10-09
---
# Asset Manager Implementation Guide

How to implement `AssetManager<E>` outside `liquers-core`, and how to check that it behaves like the
built-in managers.

The contract lives in [`reference/ASSETS.md`](../reference/ASSETS.md) (status, ownership,
lifecycle) and [`reference/DEPENDENCIES_STATUS.md`](../reference/DEPENDENCIES_STATUS.md)
(dependency graph, audits, expiry reasons). This guide says *how* to build a manager that honours
it: the decisions to make first, what to hold, which methods are yours, which primitives to call
and what each one promises. It plays the role for an asset manager that
[`STORE_IMPLEMENTATION_GUIDE.md`](STORE_IMPLEMENTATION_GUIDE.md) plays for a store.

**The worked example is real code.** `liquers-core/tests/common/minimal_manager.rs` defines
`MinimalInlineAssetManager`, written from scratch against the public API only (an integration test
is its own crate, so it cannot see `pub(crate)` items). `liquers-core/tests/external_asset_manager.rs`
runs the shared manager scenarios against it. Every snippet below is taken from those two files.
If they compile and pass, the surface described here is enough.

The manager-implementation surface is **public and documented, not stable**. The primitives may be
refined; semver discipline for them starts when an external manager ships.

## 1. What implementing a manager actually means

| Question | If yes |
|---|---|
| A struct implementing `AssetManager<E>`, `DependencyManagerAccess<E>` and `KeyMutationAccess`? | Always. The two supertraits are how the default methods reach your graph and your lock (§3). |
| Must an environment be able to construct it? | Implement `AssetManagerKind` (§9). `GenericEnvironment<V, P, YourKind>` and `EnvironmentBuilder::<V, P, YourKind>` then work with no further code. |
| Do you also have global services of your own? | Implement `Environment` as well — see [`ENVIRONMENT_CONSTRUCTION_GUIDE.md`](ENVIRONMENT_CONSTRUCTION_GUIDE.md) §"Implementing your own `Environment`". Not needed just to swap the manager. |

What you do **not** write: dependency tracking, cascade expiry, expiry reasons, audits, listing
versions, outside-change detection, `remove`, `expire`, `to_override`, `get_asset_info`, the
listings. They are default methods of `AssetManager` that work through your graph and your maps.

### The required-method count is honest here

`AsyncStore`'s defaults are error stubs, so its required-method count misleads
(`STORE_IMPLEMENTATION_GUIDE.md` §1). `AssetManager` is the opposite: its 16 required methods are the
ones that need your maps and your execution model, and its defaults are real implementations. Keep
them. The exceptions are listed in §4.

## 2. The questions to answer first

- **Inline or queued?** `eval_mode()` returns a constant. An inline manager evaluates in the caller's
  task with `AssetRef::run_inline`, needs no Tokio runtime and is the only option on wasm32. A
  queued manager evaluates as a spawned job with `AssetRef::run`, which is native-only and spawns on
  Tokio (`CORE-TOKIO-REMOVAL`). The mode is not only yours: under `EvalMode::Inline` a keyed asset
  persists synchronously instead of in a background task.
- **Who runs an asset?** `run_inline` takes an internal claim, so concurrent callers are safe: one
  runs, the others wait for it. `run` takes no claim you can see (the run claims are not exposed),
  so a queued manager must itself guarantee that exactly one party calls `run` per asset — for
  example the one whose insert registered it.
- **What is registered, and where?** A key map and a query map, at most one asset per entry, never a
  volatile asset, never a `cached: false` key (§7).
- **How do deadlines fire?** A monitor (`track_expiration` schedules, `remove_expired_from_maps`
  evicts), or lazily on access, as the minimal manager and `ImmediateAssetManager` do. Laziness is
  only how the expiry is discovered: once a lazy check finds the deadline passed, the dependents
  must follow, exactly as from a monitor. Expire the asset with `expire_without_cascade(Direct {
  Deadline })` and then, for a keyed asset, call `cascade_expire_dependents(key, Deadline)` — the
  minimal manager does exactly this.
- **Which `AssetManagerOptions` can you honour?** Store the policies and return them from the policy
  getters. Refuse a field you cannot honour with an error from `build` rather than ignoring it
  (§9).

## 3. What to hold

```rust
pub struct MinimalInlineAssetManager<E: Environment> {
    envref: EnvRef<E>,
    /// Created once and handed out; never called into directly.
    graph: DependencyManager<E>,
    /// Serialises keyed mutations (`KeyMutationAccess`).
    mutation_lock: tokio::sync::Mutex<()>,
    /// At most one asset per key.
    assets: Mutex<HashMap<Key, AssetRef<E>>>,
    queries: Mutex<HashMap<Query, AssetRef<E>>>,
    next_id: AtomicU64,
    started: AtomicBool,
    audit: DependencyAuditPolicy,
    verification: VersionVerification,
    external_change: ExternalChangePolicy,
    // … test-only fields …
}

impl<E: Environment> DependencyManagerAccess<E> for MinimalInlineAssetManager<E> {
    fn dependency_manager(&self) -> &DependencyManager<E> {
        &self.graph
    }
}

impl<E: Environment> KeyMutationAccess for MinimalInlineAssetManager<E> {
    fn key_mutation_lock(&self) -> &tokio::sync::Mutex<()> {
        &self.mutation_lock
    }
}
```

- **One `DependencyManager`** (`liquers_core::dependencies`), created with `DependencyManager::new()`
  or `Default`, kept for the manager's lifetime and returned by reference on every call. The type
  is public but opaque: its methods are crate-private, and the `AssetManager` defaults are the only
  callers. When you need to tell the graph something, use the provided methods (§5).
- **One key-mutation lock**, a `tokio::sync::Mutex<()>`, the same one on every call (§6).
- **An `EnvRef<E>`**, received in `AssetManagerKind::build`. It is a strong reference, and the
  environment holds the manager strongly too, so every environment leaks as the built-in ones do
  (`ENVIRONMENT-MANAGER-REFERENCE-CYCLE`).
- **A monotonic id counter** for `next_id_for_asset`, and a **started flag** for `start` /
  `is_started`.

The trait requires `MaybeSend + MaybeSync` and uses `async_trait`; copy the built-ins' attributes so
the impl also compiles on wasm32:

```rust
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<E: Environment> AssetManager<E> for MinimalInlineAssetManager<E> {
```

## 4. Required and provided methods

**Required — yours to write:**

| Method | What it must do |
|---|---|
| `get_asset(query)` | A pure key query delegates to `get`. Otherwise reuse a usable registered query asset, or create, register (unless volatile) and evaluate one. |
| `get(key)` | Reuse a usable registered asset, or create one, register it (§7), try `fast_track`, and evaluate if that refuses. Evict and retry on an unusable status. |
| `apply(recipe, to, payload)` | Build an unregistered, non-keyed asset with `AssetData::new_ext(.., to, None, ..)` and evaluate it before returning, on both modes. |
| `set_binary(key, binary, metadata)` | Under the key-mutation lock: replace the live asset, decide the status, set the version, write the store, `publish_version`. Then `refresh_listing_version(&key.parent())` (§6). |
| `set_state(key, state)` | Install a state. An external manager can only store one that has bytes (§11). |
| `eval_mode` | The constant from §2. |
| `lookup_key_asset`, `lookup_query_asset` | Read the maps. Never create, never evaluate. A pure key query in `lookup_query_asset` reads the key map. |
| `remove_key_asset` | Drop the key's entry. |
| `next_id_for_asset` | A fresh, monotonic id. |
| `get_envref` | The `EnvRef` from `build`. |
| `create_temporary_asset` | `AssetRef::new_temporary(self.envref.clone())`. |
| `start` | Call `self.refresh_command_versions()?`, then record that you started. Idempotent, synchronous. |
| `is_started` | The flag `start` set. |
| `track_expiration` | Schedule a deadline, or do nothing if you check lazily. |
| `remove_expired_from_maps(id, query, key)` | Drop the entry only if it is still the asset with that id, **atomically**: compare and remove in one map operation (`remove_if_async` on `scc`, or under one mutex guard). A lookup, compare, then separate remove lets a replacement inserted in between be the entry removed. |

**Provided — override these:**

| Method | Why |
|---|---|
| `dependency_audit_policy`, `version_verification`, `external_change_policy` | The defaults return constants and ignore `AssetManagerOptions`. Return what `build` received, or the options are silently lost. |
| `get_dependency_asset_with_payload` | The default ignores the payload. Create a fresh query asset, `set_payload_path`, then run it with the payload. |
| `remove_key_asset_if` | The default is lookup-compare-remove, correct but not atomic. Do it under your map lock. |

**Provided — override only deliberately:** `record_expiry` (§8), and `get_dependency_asset`,
`drain_dependencies`, `wait_for_dependency` if you add a local dependency queue as
`DefaultAssetManager` does. The default `wait_for_dependency` already applies the stale-dependency
policy.

**Provided — keep:** everything else. In particular `refresh_command_versions` (its default is the
only route to the command-version registration, which is crate-private), `recipe_opt` (it rejects
payload-requiring recipes for a key), `is_volatile`, `is_volatile_query`, `owned_key_asset`, and the
whole dependency, audit and expiry family.

## 5. The lifecycle primitives and their contracts

These are the public calls an implementation drives assets with. Each carries its contract in its
rustdoc; the summary:

| Primitive | Contract |
|---|---|
| `AssetData::new(id, recipe, key, envref).to_ref()` | Construct an asset. `key` is `Some` only for a keyed asset — the manager decides keyedness here, and only a keyed asset is ever stored. |
| `AssetData::new_ext(id, recipe, initial_state, key, envref)` | The same with an initial state. A non-empty state is not reproducible from the recipe, so pass `key = None` (`apply`). |
| `AssetRef::new_temporary(envref)` | A non-addressable asset for `create_temporary_asset`. |
| `AssetRef::fast_track()` | Try to load the asset from the store, and apply any outside change the load found, after the asset lock is dropped. `Ok(false)` means "evaluate it". Use this, not `AssetData::try_fast_track`. |
| `AssetRef::run_inline(payload)` | Evaluate in the current task. Concurrent callers wait for the one that won the claim. Await it to the end: a future dropped mid-run repairs the status but can strand waiters (`INLINE-DROP-REPAIR-STRANDS-EXISTING-WAITERS`). |
| `AssetRef::run(payload)` | Native only. Evaluate with the service loop spawned on Tokio. At most once per asset, from the party that owns the run. |
| `AssetRef::submitted()` | Mark the asset `Submitted` before handing it to `run`. A manager that runs immediately may skip it. |
| `AssetRef::set_payload_path(path)` | On a freshly built payload-evaluated asset, before running it, to carry cycle detection down the payload path. |
| `AssetRef::expire_without_cascade(reason)` | Expire this asset alone. `Ready` and `Override` become `Expired` and the reason is recorded through `record_expiry`; already `Expired` is a no-op; any other status is an error. Persists the `Expired` status for a stored keyed asset. Does not touch the graph or your maps — the cascade and the eviction are the caller's. |
| `AssetRef::cancel_for_replacement()` | Retire the asset a write or removal replaces. An in-flight one ends `Cancelled` at once (releasing its waiters with `JobFinished`), its run's late result is discarded, and it writes nothing more to the store; a finished one is left as it is. Call it before writing the key's new entry. |
| `AssetRef::cancel()` | A client's best-effort cancel: a request the run decides (a completed command still ends ready and is stored). Not for replacement — its late result could overwrite your write. |
| `AssetManager::publish_version(dep_key, version)` | Publish a written key's new version and cascade-expire dependents that recorded another (`Updated`). Graph only: no store write, no lock, safe under the key-mutation lock. Unchanged version, no cascade. |
| `AssetManager::refresh_listing_version(dir)` | Call it with the written key's parent after every write or removal. First tells the recipe provider (`directory_changed`, so a caching provider such as the manifest provider drops its folder listing), then re-hashes a directory listing that something depends on. |
| `AssetManager::is_volatile(key)`, `is_volatile_query(query)` | Volatility **without evaluating**, asked before registering (§7). |

Not exposed, deliberately: the run claims, the job queue, `set_status`, `set_value`, `fail_asset`
and `notify_removed`. Raw status writes would bypass the status authority; the claims are taken
inside `run` / `run_inline`.

A `get` loop built from these, from the minimal manager:

```rust
async fn get(&self, key: &Key) -> Result<AssetRef<E>, Error> {
    loop {
        let asset = self.resource_asset(key).await?;
        let status = asset.status().await;
        if is_unusable(status) {
            let _mutation = self.mutation_lock.lock().await;
            self.remove_key_asset_if(key, asset.id()).await;
            continue;
        }
        if status.is_finished() {
            if self.expire_if_past_deadline(&asset).await? {
                let _mutation = self.mutation_lock.lock().await;
                self.remove_key_asset_if(key, asset.id()).await;
                continue;
            }
            return Ok(asset);
        }
        if asset.fast_track().await? {
            return Ok(asset);
        }
        asset.run_inline(None).await?;
        return Ok(asset);
    }
}
```

`is_unusable` is `Expired | Error | Cancelled | Volatile`. Lazy deadline expiry is
`expire_without_cascade` with `ExpiryReason::Direct { cause: ExpiryCause::Deadline { expiration_time } }`,
on a `Ready` asset whose `expiration_time().await.is_expired()`.

## 6. The key-mutation lock

Keyed mutations — `remove`, `set_binary`, `set_state`, `to_override`, `expire`, `set_description` —
are serialised by the one lock `key_mutation_lock()` returns. Three rules:

1. **One lock per manager**, the same on every call.
2. **Lock order: the key-mutation lock first, asset locks second.** Never take it while an asset's
   `data` lock is held. You cannot hold that lock directly, but `record_expiry` runs under it, so
   `record_expiry` must never take the key-mutation lock (§8).
3. **It is not re-entrant.** The default methods hold it while they call `lookup_key_asset`,
   `remove_key_asset`, `remove_key_asset_if`, `recipe_opt` and `refresh_listing_version`, so your
   implementations of these must not take it. `publish_version` does not take it either, so your
   own writes may call it while holding the lock. The trait documentation lists
   the methods a holder must not call: `get`, `owned_key_asset`, `to_override`, `set_binary`,
   `set_state`, `remove`, `remove_expired_from_maps`.

The shape of a write, from the minimal manager's `set_binary` (status decision elided):

```rust
let mutation = self.mutation_lock.lock().await;
if let Some(old) = self.lookup_key_asset(key) {
    old.cancel_for_replacement().await?;
    self.remove_key_asset(key).await;
}
// … decide `final_status`, set `metadata.version = Some(Version::from_content(binary))` …
if metadata.stored() {
    self.get_envref()
        .get_async_store()
        .set(key, stored_bytes, &Metadata::MetadataRecord(metadata.clone()))
        .await?;
}
if matches!(final_status, Status::Ready | Status::Source | Status::Override) {
    if let Some(version) = metadata.version {
        self.publish_version(&DependencyKey::from(key), version).await;
    }
}
drop(mutation);
self.refresh_listing_version(&key.parent()).await;
```

The version is the content hash (`Version::from_content`), so outside-change detection can verify it
later. `publish_version` runs after the store write, so a dependent recomputed after the cascade
reads the new content.

## 7. Registration invariants

`AssetRef::bound_owner_key` decides whether an asset owns its key by looking the key up in **your**
map. Ownership drives persistence, dependency recording and the expiry cascade, so a wrong map
silently breaks all three. Until `ASSET-REGISTRATION-OWNERSHIP-CONTRACT` settles a fuller contract,
these are the requirements:

- **At most one registered asset per key.** Concurrent first requests must converge on one asset.
  Check-then-insert is not enough; insert under the map lock and return whatever won.
- **`lookup_key_asset` returns exactly that asset**, and nothing else.
- **Never register a volatile asset.** Ask `is_volatile(key)` for a key and `is_volatile_query(query)`
  for a query, before registering. A volatile request gets a fresh asset every time.
- **Never register a `cached: false` key.** It too gets a fresh asset per request, but carry the
  recipe's `stored` / `cached` flags into the asset's recipe, so it still persists correctly and
  stays the key's graph node ([`ASSETS.md`](../reference/ASSETS.md) §"`stored` and `cached`").
- **Evict by id.** Remove an entry only if it is still the asset you looked at, so a slow caller does
  not evict a replacement.

```rust
async fn resource_asset(&self, key: &Key) -> Result<AssetRef<E>, Error> {
    let recipe = self.recipe_opt(key).await?;
    let (stored, cached) = match &recipe {
        Some(recipe) => (recipe.stored, recipe.cached),
        None => (None, None),
    };
    let fresh = || {
        AssetData::new(
            self.new_id(),
            Self::resource_recipe(key, stored, cached),
            Some(key.clone()),
            self.envref.clone(),
        )
        .to_ref()
    };
    if self.is_volatile(key).await? || !cached.unwrap_or(true) {
        return Ok(fresh());
    }
    if let Some(existing) = self.lookup_key_asset(key) {
        return Ok(existing);
    }
    let candidate = fresh();
    let mut map = self.assets.lock().unwrap_or_else(|e| e.into_inner());
    Ok(map.entry(key.clone()).or_insert(candidate).clone())
}
```

`external_manager_registers_one_asset_per_key` checks all of this: eight concurrent `get`s of one
key yield one id and one command run, and a volatile key is evaluated but never registered.

## 8. Overriding `record_expiry`

Every route into `Expired` — deadline, explicit, audit, stale dependency, outside change, update,
removal — calls `record_expiry(metadata, subject, reason)` once for each expired asset, live or
stored-only. The default sets the reason on the metadata and appends `reason.log_entry(subject)`.
Override it to change wording or levels, add fields, or forward the event.

It runs **under the asset's `data` write lock**, so it is synchronous and must not block, await,
touch any asset or take any lock that could wait on one — the key-mutation lock included. `subject`
is the asset's key, or its query when it has none.

Keep the default's two writes unless you mean to replace them: the shared scenarios check that every
expired asset carries a reason and a log line.

```rust
fn record_expiry(&self, metadata: &mut Metadata, subject: &str, reason: &ExpiryReason) {
    // … record the call …
    match metadata {
        Metadata::MetadataRecord(_) => {
            let _ = metadata.set_expiry_reason(reason.clone());
            let _ = metadata.add_log_entry(reason.log_entry(subject));
            if self.audit_trail.load(Ordering::SeqCst) {
                let _ = metadata.add_log_entry(LogEntry::info(format!(
                    "{AUDIT_TRAIL_PREFIX}{subject}"
                )));
            }
        }
        Metadata::LegacyMetadata(_) => {}
    }
}
```

`record_expiry_is_overridable_by_a_manager` checks that the override's line reaches the stored
metadata of a cascaded dependent and that the default line is still written once.

## 9. Providing an `AssetManagerKind`

The kind is a compile-time selector: a type with a generic associated `Manager<E>` and a `build`
that constructs it for an environment that already exists.

```rust
pub type MinimalEnv = GenericEnvironment<Value, (), MinimalKind>;

pub struct MinimalKind;

impl AssetManagerKind for MinimalKind {
    type Manager<E: Environment> = MinimalInlineAssetManager<E>;

    fn build<E: Environment>(
        envref: EnvRef<E>,
        options: &AssetManagerOptions,
    ) -> Result<Arc<Self::Manager<E>>, Error> {
        if options.job_capacity.is_some() {
            return Err(Error::general_error(
                "job_capacity is set, but the minimal inline asset manager has no job queue"
                    .to_string(),
            ));
        }
        Ok(Arc::new(MinimalInlineAssetManager::new(envref, options)))
    }
}
```

- `build` is called from `Environment::init_with_envref`, before anything can observe the
  `EnvRef`; the environment then calls `start`. It is synchronous and must not touch a store.
- Read every option you support (`options.dependency_audit`, `options.verify_versions`,
  `options.external_change`) and return an error for one you cannot honour.
- `default_recipe_provider` defaults to `Trivial`, the core behaviour. Override it only to carry
  another crate's default.

Both construction paths then work unchanged:

```rust
let envref = MinimalEnv::new().to_ref();

let builder = EnvironmentBuilder::<Value, (), MinimalKind>::new()
    .with_asset_manager_options(AssetManagerOptions::default().with_dependency_audit(policy))
    .with_async_store(Arc::new(store))
    .with_recipe_provider(Arc::new(DefaultRecipeProvider::new()));
```

`external_manager_honours_audit_policy` checks that the option reaches the manager and changes what
a restart does. The stored-records walk behind `on_load` and the audits
(`stored_dependency_state`, `trigger_dependency_audit_store`) are provided methods built on
`dependency_manager()`, the store and `expire_dependencies_result`, so an external manager inherits
them; see [`DEPENDENCIES_STATUS.md` §Consistency policies](../reference/DEPENDENCIES_STATUS.md#consistency-policies).

## 10. Running the shared scenarios

`liquers-core/tests/common/manager_scenarios.rs` holds the manager contract as generic scenario
functions, `scenario_*<E: Environment<Value = Value>>(envref: EnvRef<E>) -> Result<(), Error>`,
with their fixture stores (`recipe_store`, `provenance_store`, …) and command registrations
(`register_greet`, `register_provenance_commands`, …). The same bodies run against
`DefaultAssetManager` and `ImmediateAssetManager` (`tests/manager_parametric.rs`) and against the
minimal manager (`tests/external_asset_manager.rs`). They cover basic, cached, keyed and volatile
evaluation, delegation, fast tracking, persistence rules, expiry reasons, restart audits, listing
dependencies and stale dependencies.

Run every scenario, each in a fresh environment, and name the failing one:

```rust
async fn scenario(
    name: &str,
    run: impl std::future::Future<Output = Result<(), Error>>,
) -> TestResult {
    within(run)
        .await
        .map_err(|e| format!("scenario '{name}' failed on the external manager: {e}").into())
}

let mut env = MinimalEnv::new();
register_greet(&mut env.command_registry);
scenario("basic_eval", scenario_basic_eval(env.to_ref())).await?;
```

`external_manager_passes_shared_scenarios` is the complete list to copy, with its helpers `within`
(a 20 s timeout, so a hang fails instead of blocking the suite) and `env_over` (a fresh environment
over a fixture store).

- **In this repository:** add a test file under `liquers-core/tests/` with `mod common;` and import
  from `common::manager_scenarios`.
- **In another crate:** the scenarios are test code, not a published API. In the same workspace,
  include the file with `#[path = "…/liquers-core/tests/common/manager_scenarios.rs"] mod
  manager_scenarios;`; elsewhere, copy it. It needs `tokio`, `futures` and `async-trait` as
  dev-dependencies, and a Tokio runtime: some scenarios spawn tasks.
- **Then add your own manager-specific tests**, as `external_asset_manager.rs` does for registration,
  the policies and `record_expiry`.

```bash
cargo test -p liquers-core --test external_asset_manager
cargo test -p liquers-core --test manager_parametric
```

A new manager-contract test belongs in `manager_scenarios.rs`, so every manager runs it
([`UNITTEST_GUIDE.md`](UNITTEST_GUIDE.md) §"Testing Assets"). This is deliberately smaller than the
store conformance suite: shared scenarios, no rule numbers and no capability model.

## 11. Known limits

| Limit | Issue |
|---|---|
| A write cannot send the replaced asset its `Removed` notification (`notify_removed` is crate-private); a waiter is released by `cancel_for_replacement`'s `JobFinished` only. `set_state` can only store a state that has bytes, and the next `get` fast-tracks it; a non-serializable value is refused. | `EXTERNAL-MANAGER-CANNOT-NOTIFY-REPLACED-ASSET` |
| An inline run dropped mid-flight can strand callers already waiting on it. | `INLINE-DROP-REPAIR-STRANDS-EXISTING-WAITERS` |
| Ownership is read from your map; what registration guarantees beyond §7 is open. | `ASSET-REGISTRATION-OWNERSHIP-CONTRACT` |
| The manager and the environment hold each other strongly. | `ENVIRONMENT-MANAGER-REFERENCE-CYCLE` |
| `run` spawns on Tokio, so a queued manager is native-only. | `CORE-TOKIO-REMOVAL` |
| A manager on another machine needs manager-owned metadata to travel between peers. | `NO-REMOTE-STORE-OR-ASSET-MANAGER` |

## Related

- Reference: [Assets](../reference/ASSETS.md), [Dependencies status](../reference/DEPENDENCIES_STATUS.md)
- Guide: [Building and Configuring an Environment](./ENVIRONMENT_CONSTRUCTION_GUIDE.md)
- Guide: [Store Implementation](./STORE_IMPLEMENTATION_GUIDE.md), the pattern this guide follows
- Design: [`design/dependency-audit-and-expiry-provenance/`](../design/dependency-audit-and-expiry-provenance/) Part F
- Executable evidence: `liquers-core/tests/common/minimal_manager.rs`,
  `liquers-core/tests/external_asset_manager.rs`, `liquers-core/tests/common/manager_scenarios.rs`

## History

| Date | Change | Source |
|---|---|---|
| 2026-10-09 | §Primitives: `cancel_for_replacement` for writes and removals (the `cancel()` row says why not); the write example and the known-limits row follow. | phase-5 (`design/asset-cancellation-outcome/`) |
| 2026-10-08 | `DefaultRecipeProvider` is constructed with `::new()` (it holds a recipe cache). The audit-policy section notes that the stored-records walk and the store audit are inherited provided methods. | phase-5 (`design/dependency-chain-analysis-cost/`) |
| 2026-10-07 | `remove_expired_from_maps`: the id comparison and the removal must be one atomic map operation. Deadlines: a lazy check that finds the deadline passed must cascade (`expire_without_cascade` then `cascade_expire_dependents`); the known-limit row is removed. | phase-5 (`design/queued-manager-conditional-eviction/`, `design/immediate-lazy-expiry-cascade/`) |
| 2026-10-06 | `refresh_listing_version` also notifies the recipe provider; every write path must call it. | phase-5 |
| 2026-10-02 | Created: decisions before writing code, what to hold, required and provided methods, the lifecycle primitives and their contracts, the key-mutation lock, registration invariants, overriding `record_expiry`, providing an `AssetManagerKind`, running the shared scenarios, known limits. Snippets from `tests/common/minimal_manager.rs` and `tests/external_asset_manager.rs`. | phase-5 (`design/dependency-audit-and-expiry-provenance/`) |
