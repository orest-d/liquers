# Phase 3: Examples & Use-cases - dependency-audit-and-expiry-provenance

*Revised 2026-10-02 for Phase 2 "Revision 2" (owner corrections) and for the merge of `main`. The
reason shapes, the `Version` representation of a current version, `VersionCheck`, the directory
version and the `record_expiry` method all changed; every example and test below uses the new
forms. The Learning Log lists what changed.*

## High-Level Introduction

Phase 1 states one purpose: make the dependency-verification half of the expiry system *correct*
(an audit that finds a moved input really expires its dependents), *configurable* (the environment
says when audits run), and *explainable* (every transition into `Expired` records why, naming the
root cause and the path it took), and then extend it to the two inputs the graph could not see, a
folder listing and content edited outside Liquers. Phase 1 illustrates this with eight worked
problems on two stored files, `data/a.csv` and `data/report.txt`, whose recipe reads it. Phase 2
answers them with Parts A to G. This phase shows the answers working, in the same vocabulary.

The examples are ordered from the representative path to the sharp edges:

1. **Example 1 (primary): a strict service after a restart.** The one story every reader of the
   design must follow. Monday computes `report.txt` from `a.csv@V1`, a batch job stores `a.csv@V2`
   overnight, and on Tuesday two operators, a strict service and a researcher, meet the same store.
   It exercises Parts A, B and C, and Phase 1 problems 1, 2 and 3. A variant shows `via` on a
   two-step cascade (`a.csv` to `b.csv` to `report.txt`).
2. **Example 2 (detail): a shared data folder.** Builds on Example 1's environment and adds the two
   inputs that are not a plain `-R/` value: a folder listing (Part D, problem 4) and a file edited by
   hand (Part G, problem 8), including the two policies for a recipe-backed file and a file that
   carries no metadata at all.
3. **Example 3 (pitfalls).** Twelve ways to get the feature wrong, each with symptom, cause, correct
   use and the test that guards it. It also carries the correction to the drafts (unknown-expecting
   edges) that the test plan depends on.

Problems 5, 6 and 7 (the stale-dependency path, the immediate manager's deadline, the external asset
manager) are small and mechanical. They appear as the Part E command snippet in Example 3, and as
named integration tests and the from-scratch manager in the Test Plan, not as long narratives. The
provenance mechanism itself (`record_expiry`, the log line format, the `Removed` and `Updated`
causes) has its own integration file, listed as I5.

## Example Type

**User choice:** Conceptual code (the APIs do not exist yet).

The snippets follow the style of `liquers-core/tests/keyed_version_cascade.rs` and use Phase 2's
names and signatures exactly, but they are not compiled. Two commands, `summarize` and `index_files`,
are **hypothetical test commands**, registered inside each test (or its helper); no `liquers-lib`
namespace is involved (Phase 2 §"Relevant Commands"). Every `-R/` or `-R-dir/` query needs a store,
so each environment below is built over an `AsyncMemoryStore`. Queries were checked with
`liquers-validate` (see Manual Validation) and contain no spaces. Where a snippet leans on something
Phase 2 leaves open, the Learning Log says so.

## Overview Table

| # | Type | Name | Purpose | Drafted By |
|---|------|------|---------|------------|
| 1 | Example | Strict service after a restart | Parts A, B, C: audit compares with recorded versions on first observation, `OnLoad` refuses a stale fast track, `ReportOnly` vs `Expire`, the `Cascaded` reason persisted in the sidecar, `via` over a two-step cascade | Haiku drafter 1 (rewritten by Sonnet synthesizer; revised for Revision 2) |
| 2 | Example | A shared data folder | Part D listing dependency (`Updated` cause); Part G hand edit of a `Source`, of a recipe-backed file under both policies, and of a file with no metadata; `verify_stored_versions` | Haiku drafter 2 (rewritten by Sonnet synthesizer; revised for Revision 2) |
| 3 | Example | Pitfalls | Twelve pitfalls with symptom, cause, correct use, guarding test; Part E snippet | Haiku drafter 3 (edited by Sonnet synthesizer; revised for Revision 2) |
| U1 | Unit tests | `dependencies.rs` | `audit_version`, `stale_edges`, `report_no_version`, `listing_version`, `ExpiredDependents::for_root`, `ExpiredKey::via` from the walk | Haiku drafter 4 (corrected by Sonnet synthesizer; revised) |
| U2 | Unit tests | `metadata.rs` | `ExpiryReason` / `ExpiryCause` serde and log entries (wording and levels), record compatibility, `Version` content hashes, `kind`, `verify`, `VersionCheck::Mismatch` | Haiku drafter 4 (revised) |
| U3 | Unit tests | `assets.rs` | `external_change_action` table, `AuditFinding`/`AuditReport`, reason at each route, plan `unknown` edges, `OnLoad` in fast track | Haiku drafter 4 (extended by Sonnet synthesizer; revised) |
| U4 | Unit tests | `environment_builder.rs`, `environment_config.rs` | Policy defaults, serde, YAML omission, builder | Haiku drafter 4 |
| U5 | Unit tests | `context.rs` | `submit`, public `wait_for_dependency`, cycle, no drain | Haiku drafter 4 (corrected by Sonnet synthesizer) |
| I1 | Integration | `dependency_audit_integration.rs` | Parts A to E end to end, both managers | Haiku drafter 5 (corrected by Sonnet synthesizer; revised) |
| I2 | Integration | `external_change_integration.rs` | Part G end to end, including the version bump and the `UpdatedInStore` cascade | Haiku drafter 5 (revised) |
| I3 | Integration | `external_asset_manager.rs`, `common/manager_scenarios.rs`, `manager_parametric.rs` | Part F: a from-scratch manager runs the shared scenarios and overrides `record_expiry` | Haiku drafter 5 (kind wiring corrected by Sonnet synthesizer; revised) |
| I4 | Integration | `expiration_integration.rs` | Immediate manager's lazy deadline fires (problem 6) | Sonnet synthesizer |
| I5 | Integration | `expiry_provenance_integration.rs` | Part C end to end: every cause in its scope, `via` on a two-step cascade, `record_expiry` for live and stored-only assets, the log line format, `Removed` and `Updated` | Sonnet synthesizer (Revision 2) |
| C | Corner cases | Five categories | Memory, concurrency, errors, serialization, integration, each with a covering test | Haiku drafter 5 (merged by Sonnet synthesizer; revised) |
| M | Manual | Validation commands | Build matrix, full test loops, query validation | Sonnet synthesizer |
| T | Templates | `liquers-unittest` output | Three templates in repository conventions | Sonnet synthesizer (revised) |

### Coverage map

| Phase 1 problem | Part | Example | Guarding tests |
|---|---|---|---|
| 1. Audit after restart misses a changed input | A | Ex. 1 | `audit_version_first_observation_expires_mismatched_dependent` (U1), `audit_after_restart_expires_dependent` (I1) |
| 2. Nobody can say when to check | B | Ex. 1, Ex. 3 pitfalls 2 and 10 | `on_load_refuses_stale_fast_track`, `explicit_policy_serves_when_intermediate_deleted`, `report_only_audit_changes_nothing` (I1); `on_load_does_not_refuse_recorded_unknown_version` (U3) |
| 3. An expired asset cannot say why | C | Ex. 1, Ex. 3 pitfall 12 | `every_route_persists_its_reason`, `audit_never_expires_the_root` (I5); `expired_dependents_via_*` (U1); `ExpiryReason` / `ExpiryCause` tests (U2) |
| 3a. A cascade names the root and the path (`via`) | C | Ex. 1 variant | `via_names_the_direct_dependency_on_a_two_step_cascade` (I5), `expire_from_frontier_records_via_per_key` (U1) |
| 3b. One method writes every reason (`record_expiry`) | C, F | Ex. 3 pitfall 6 | `record_expiry_is_called_for_every_expired_asset`, `every_cause_writes_a_log_line` (I5), `record_expiry_is_overridable_by_a_manager` (I3), `scenario_every_expired_asset_has_reason_and_log_line` (I3) |
| 3c. The log line says what happened | C | Ex. 1 | `log_line_format_per_cause` (U2), `log_line_is_persisted_with_the_status` (I5) |
| 3d. Removal and new content are causes too | C | Ex. 2 | `removing_a_source_cascades_with_removed` (I5), `set_binary_of_a_dependency_cascades_with_updated` (I5) |
| 4. Folder listing never updates | D | Ex. 2 | `adding_a_file_expires_the_index`, `listing_gap_resolved_by_audit_after_restart` (I1) |
| 5. "Use the old input" rule untested | E | Ex. 3 pitfall 1 | `stale_dependency_end_to_end_queued` / `_immediate` (I1) |
| 6. Time limits never fire on the immediate manager | C | Ex. 3 pitfall 11 | `immediate_manager_deadline_fires` (I4) |
| 7. Nobody outside core can write a manager | F | Ex. 3 pitfall 6, Learning Log | `external_asset_manager.rs` (I3) |
| 8. Content changed by another program | G | Ex. 2 | `hand_edited_source_is_input_and_expires_dependents`, `recipe_backed_edit_follows_policy`, `file_with_no_metadata_and_no_recipe_becomes_source_with_hash_version`, `legacy_changed_value_is_a_mismatch` (I2) |

Every Part A to G appears in at least one row above.

## Example 1: Strict service after a restart

### Connection to the High-Level Design

Phase 1 problems 1 to 3, on the exact files of the Phase 1 glossary. Part A makes the audit compare
the *current* version of `a.csv` with the version `report.txt` recorded, even though the restarted
process has never seen `a.csv` (the map is empty). Part B lets the strict service refuse to serve
such a stale result at load time (`on_load`) while the researcher keeps today's behaviour
(`explicit`) and inspects with a report-only audit. Part C makes the resulting `Expired` explain
itself in the sidecar: the root cause (the audit, which found `a.csv` at another version), the root
key, and the dependency through which the cascade reached `report.txt`.

### Scenario

Monday, process 1 stores `data/a.csv` (`V1`, two rows) and computes `data/report.txt` = `summarize(a.csv)`, recording `a.csv@V1`. Overnight, a batch
job in *another* process stores a new `a.csv` (`V2`, three rows). Tuesday the store holds `a.csv@V2`
and `report.txt@[a.csv:V1]`, status `Ready`, and every process starts with an empty version map.

- The **strict service** runs with `dependency_audit: on_load`. It must never serve `report.txt`
  built on an old `a.csv`.
- The **researcher** runs with the default `explicit`. Loading is unchanged; she audits when she
  chooses, first with a report-only audit, then for real.

### Sequence of Steps

1. Monday: `set_binary(a.csv)` stores the bytes and records `Version::from_content(bytes)` (a flagged
   hash); evaluating `-R/data/report.txt` runs `summarize` and persists `report.txt` with
   `dependencies: [{key: -R/data/a.csv, version: V1}]`.
2. Night: a fresh environment over a copy of the store calls `set_binary(a.csv)` again. It never loaded
   `report.txt`, so no edge exists and nothing cascades. The store now disagrees with itself, which
   is the situation to be detected.
3. Tuesday, strict: a new environment with `OnLoad` evaluates `-R/data/report.txt`. `try_fast_track`
   finds the recorded `a.csv@V1`, the map has no entry, so under `OnLoad` it resolves
   `dependency_version(-R/data/a.csv)` from metadata (`V2`; a missing version would be
   `Version::unknown()`, not `None`), sees the mismatch and refuses the fast track. The report is
   recomputed from `V2` (`rows=3`). The stored copy is not expired first, so it gets no reason.
4. Tuesday, researcher: an `Explicit` environment serves the stored `rows=2` as `Ready`.
5. She calls `trigger_dependency_audit_with(q, ReportOnly)`: `missing_versions_for` finds the gap
   `a.csv`, `stale_edges(a.csv, V2)` yields one `AuditFinding`. No version is registered, nothing
   is expired.
6. She calls `trigger_dependency_audit(q)` (= `Expire`). `audit_version(a.csv, V2)` inserts `V2`, then
   `expire_stale_dependents` compares it with the edge's `V1`. The audit never expires the root:
   `a.csv` is the audited *dependency*, so it is not expired and holds no reason. `report.txt`
   is a dependent, so it is expired with
   `Cascaded { cause: Audit { found: V2 }, root: -R/data/a.csv, via: -R/data/a.csv }` (a direct
   dependent of the root has `via == root`). `AssetManager::record_expiry` writes the reason and the
   log entry under the lock that flips the status, so both persist with it.

### Core Example Code

Setup helpers first (they are shared with Example 2, and `summarize` is a hypothetical test command):

```rust
const A_V1: &[u8] = b"x,1\ny,2\n";
const A_V2: &[u8] = b"x,1\ny,2\nz,3\n";
const RECIPES: &str = "recipes:\n  - query: \"-R/data/a.csv/-/summarize/report.txt\"\n    \
                       title: Report\n    description: rows of a.csv\n"; // data/recipes.yaml

fn summarize(state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from(format!("rows={}", state.try_into_string()?.lines().count())))
}

fn env_over(store: Arc<dyn AsyncStore>, policy: DependencyAuditPolicy)
    -> Result<EnvRef<TestEnv>, Error>
{
    type CommandEnvironment = TestEnv;
    let mut b = EnvironmentBuilder::<Value>::new()
        .with_async_store(store)                       // every -R/ query needs a store
        .with_recipe_provider_choice(RecipeProviderChoice::Default)
        .with_asset_manager_options(AssetManagerOptions::default().with_dependency_audit(policy));
    let cr = &mut b.command_registry;
    register_command!(cr, fn summarize(state) -> result version: 1)?;
    b.build()
}

/// Monday in process 1; the batch job in process 2. Returns what is on disk on Tuesday.
async fn store_after_batch_job() -> Result<StoreSnapshot, Box<dyn std::error::Error>> {
    let (a, report) = (parse_key("data/a.csv")?, parse_key("data/report.txt")?);
    let recipes = parse_key("data/recipes.yaml")?;
    let monday: Arc<dyn AsyncStore> = Arc::new(AsyncMemoryStore::new(&Key::new()));
    monday.set(&recipes, RECIPES.as_bytes(), &Metadata::new()).await?;
    let env = env_over(monday.clone(), DependencyAuditPolicy::Explicit)?;
    env.get_asset_manager().set_binary(&a, A_V1, MetadataRecord::new()).await?;
    env.evaluate("-R/data/report.txt").await?.get().await?;   // records a.csv@V1
    let keys = [a.clone(), report, recipes];
    let night = Arc::new(AsyncMemoryStore::new(&Key::new()));  // fresh process, fresh map
    StoreSnapshot::capture(&monday, &keys).await?.replay_into(&night).await?;
    let job = env_over(night.clone(), DependencyAuditPolicy::Explicit)?;
    job.get_asset_manager().set_binary(&a, A_V2, MetadataRecord::new()).await?;
    Ok(StoreSnapshot::capture(&(night as Arc<dyn AsyncStore>), &keys).await?)
}
```

The test itself:

```rust
#[tokio::test]
async fn strict_service_after_restart() -> TestResult {
    let disk = store_after_batch_job().await?;
    let (v2, a_dep) = (Version::from_content(A_V2), DependencyKey::new("-R/data/a.csv"));
    let (a_key, report_key) = (parse_key("data/a.csv")?, parse_key("data/report.txt")?);
    let q = parse_query("-R/data/report.txt")?;

    // Tuesday, strict service: the stale result is refused, not served.
    let strict = env_from(&disk, DependencyAuditPolicy::OnLoad).await?;
    let text = strict.evaluate("-R/data/report.txt").await?.get().await?.try_into_string()?;
    assert_eq!(text, "rows=3");                                  // recomputed from a.csv@V2

    // Tuesday, researcher: loading is as today, so the stale copy is served.
    let lax = env_from(&disk, DependencyAuditPolicy::Explicit).await?; // env_over + replay
    let served = lax.evaluate("-R/data/report.txt").await?.get().await?;
    assert_eq!(served.try_into_string()?, "rows=2");

    let mgr = lax.get_asset_manager();
    assert_eq!(mgr.dependency_version(&a_dep).await?, v2);       // a Version, never an Option
    let dry = mgr.trigger_dependency_audit_with(&q, AuditMode::ReportOnly).await?;
    assert!(dry.expired.is_empty());                             // ReportOnly never expires
    assert_eq!(dry.findings, vec![AuditFinding::new(
        a_dep.clone(), DependencyKey::new("-R/data/report.txt"),
        Version::from_content(A_V1), v2)]);                      // found: Version
    let store = lax.get_async_store();
    assert_eq!(store.get_metadata(&report_key).await?.status(), Status::Ready);

    let wet = mgr.trigger_dependency_audit(&q).await?;           // = with(AuditMode::Expire)
    assert_eq!(wet.expired, vec![DependencyKey::new("-R/data/report.txt")]);
    let meta = store.get_metadata(&report_key).await?;           // persisted, not just in memory
    assert_eq!(meta.status(), Status::Expired);
    assert_eq!(meta.expiry_reason(), Some(ExpiryReason::Cascaded {
        cause: ExpiryCause::Audit { found: v2 },
        root: a_dep.clone(),
        via: a_dep.clone(),                                      // a direct dependent: via == root
    }));
    // The audit never expires the root: a.csv is untouched and holds no reason.
    let a_meta = store.get_metadata(&a_key).await?;
    assert_eq!(a_meta.status(), Status::Source);
    assert_eq!(a_meta.expiry_reason(), None);
    Ok(())
}
```

`env_from(snapshot, policy)` replays the snapshot into a new `AsyncMemoryStore` and calls `env_over`.
The reading of the reason goes through the store's metadata on purpose, so the test observes the
persisted record. (`DESCRIBING-AN-ASSET-CAN-TRIGGER-ITS-EVALUATION` is closed on `main`, and
`get_asset_info` no longer evaluates; but the store's metadata is still the one thing the test is
about, since the point is that the reason reached the sidecar.)

### Variant: through an intermediate (`via` on a two-step cascade)

Take the same store with one more link: `data/b.csv` is computed from `a.csv`
(`-R/data/a.csv/-/summarize/b.csv`) and `data/report.txt` is computed from `b.csv`
(`-R/data/b.csv/-/summarize/report.txt`). The graph is `a.csv -> b.csv -> report.txt`. After the
same restart and the same audit of `a.csv`, both dependents expire, but they do not expire for the
same reason:

| Asset | Reason |
|---|---|
| `a.csv` | none (it is the audited dependency; the audit never expires the root) |
| `b.csv` | `Cascaded { cause: Audit { found: V2 }, root: -R/data/a.csv, via: -R/data/a.csv }` (direct dependent: `via == root`) |
| `report.txt` | `Cascaded { cause: Audit { found: V2 }, root: -R/data/a.csv, via: -R/data/b.csv }` (`via` is `report.txt`'s own dependency through which the cascade arrived) |

```rust
#[tokio::test]
async fn via_names_the_direct_dependency_on_a_two_step_cascade() -> TestResult {
    let disk = chain_store_after_batch_job().await?;        // a.csv@V2 on disk, b.csv, report.txt @V1 chain
    let lax = env_from(&disk, DependencyAuditPolicy::Explicit).await?;
    lax.evaluate("-R/data/b.csv").await?.get().await?;       // served from store: loads b's edge
    lax.evaluate("-R/data/report.txt").await?.get().await?;  // served from store: loads report's edge
    let (a, b) = (DependencyKey::new("-R/data/a.csv"), DependencyKey::new("-R/data/b.csv"));
    let v2 = Version::from_content(A_V2);

    lax.get_asset_manager().trigger_dependency_audit(&parse_query("-R/data/b.csv")?).await?;

    let store = lax.get_async_store();
    let report = store.get_metadata(&parse_key("data/report.txt")?).await?;
    assert_eq!(report.expiry_reason(), Some(ExpiryReason::Cascaded {
        cause: ExpiryCause::Audit { found: v2 }, root: a.clone(), via: b }));   // via != root
    let b_meta = store.get_metadata(&parse_key("data/b.csv")?).await?;
    assert_eq!(b_meta.expiry_reason(), Some(ExpiryReason::Cascaded {
        cause: ExpiryCause::Audit { found: v2 }, root: a.clone(), via: a }));   // via == root
    Ok(())
}
```

`via` comes from the breadth-first walk (`expire_from_frontier`): frontier keys get `via = root`, and
each later key gets the key it was first queued from, so it is the shortest path in a diamond
(`expire_from_frontier_records_via_per_key`, U1). The audit query is `b.csv` as well as `report.txt`
in this variant because `b.csv` is itself a gap: it recorded `a.csv@V1`.

### Guide and Executable Example

The canonical executable form is the test above,
`liquers-core/tests/dependency_audit_integration.rs::strict_service_after_restart`, and
`reference/DEPENDENCIES_STATUS.md` will cite it. A file under `examples/` is not proposed: every
piece needs a registered command and a store snapshot, which is test scaffolding. The guide that
Phase 2 creates, `ASSET_MANAGER_IMPLEMENTATION_GUIDE.md`, takes its snippets from
`tests/external_asset_manager.rs` instead (see the Learning Log).

**Expected output:**
```
running 1 test
test strict_service_after_restart ... ok
```
and, in the researcher's log after the real audit (`warning`, because an audit means a stored
assumption was false; wording indicative, fixed in Phase 4, from Phase 2 §"ExpiryCause and
ExpiryReason"):
```
data/report.txt expired: an audit that found -R/data/a.csv at a different version than recorded triggered a cascade expiration
```
(Wording as fixed in Phase 4 Step 2.)
For the two-step variant, `report.txt` names the path it came through:
```
data/report.txt expired: an audit that found -R/data/a.csv at a different version than recorded triggered a cascade expiration via direct dependency -R/data/b.csv
```
Both lines follow the wording table fixed in Phase 4 Step 2; `log_line_format_per_cause` asserts
them exactly.

## Example 2: A shared data folder

Same store and helpers as Example 1, plus a second recipe in `data/recipes.yaml`:
`-R-dir/data/-/index_files/index.txt`, where `index_files` (hypothetical test command, registered in
`env_over` beside `summarize`) writes the names it is handed, one per line.

### D. A result built from a listing (problem 4)

`index.txt` reads the *names* in `data/`, not their content, so its only edge is `-R-dir/data`.
During evaluation `register_plan_dependencies` adds that edge with `Version::unknown()` (it is no
longer skipped), then the `GetAssetDirectory` step registers `listing_version(names)` and
`add_context_dependency` upgrades the record. The listing version is the content hash of the ordered
listing (flagged, since it is a hash). Writing through the manager refreshes the listing. The listing
is a *root* whose content changed through Liquers, so the cause is `Updated`, and `index.txt` is a
direct dependent of the listing.

```rust
#[tokio::test]
async fn adding_a_file_expires_the_index() -> TestResult {
    let (store, env) = shared_folder_env().await?;   // a.csv stored; report.txt, index.txt computed
    let mgr = env.get_asset_manager();
    let listing = DependencyKey::new("-R-dir/data");
    let before = mgr.dependency_version(&listing).await?;
    // Manager-mediated write => refresh_listing_version(data) => new membership => cascade.
    mgr.set_binary(&parse_key("data/new.csv")?, b"n\n", MetadataRecord::new()).await?;
    let after = mgr.dependency_version(&listing).await?;
    assert_ne!(before, after);

    let index = store.get_metadata(&parse_key("data/index.txt")?).await?;
    assert_eq!(index.expiry_reason(), Some(ExpiryReason::Cascaded {
        cause: ExpiryCause::Updated { version: after },
        root: listing.clone(),
        via: listing,                                 // a direct dependent of the listing
    }));
    // report.txt reads a.csv's content, not the listing: untouched.
    let report = store.get_metadata(&parse_key("data/report.txt")?).await?;
    assert_eq!(report.status(), Status::Ready);
    Ok(())
}
```

A write that bypasses the manager is invisible until an audit: after a restart the
`-R-dir/data` gap is resolved by `dependency_version`, which lists the directory, hashes the ordered
names with `from_content` and compares (`listing_gap_resolved_by_audit_after_restart`). Only
membership counts: overwriting `a.csv` does not move the listing. In that audit `index.txt` gets
`Cascaded { cause: Audit { found: <listing version> }, root: -R-dir/data, via: -R-dir/data }`, and the
listing key itself, which is not an asset, gets no reason.

### G. A file edited by hand (problem 8)

The edit is simulated by rewriting the **bytes only**, keeping the old sidecar, which is exactly what
a text editor does. Detection happens when the changed file is *read*, by a process that did not
already hold it in memory, so the test restarts first and loads `report.txt` before `a.csv`.

```rust
#[tokio::test]
async fn hand_edited_source_is_input_and_expires_dependents() -> TestResult {
    let store = store_with_report_computed().await?;        // a.csv@V1 (Source), report.txt Ready
    edit_outside_liquers(&store, "data/a.csv", A_V2).await?; // old metadata kept: still says V1
    let env = env_over(store.clone(), DependencyAuditPolicy::Explicit)?;      // restart
    let (a_key, a_dep) = (parse_key("data/a.csv")?, DependencyKey::new("-R/data/a.csv"));
    let v2 = Version::from_content(A_V2);

    env.evaluate("-R/data/report.txt").await?.get().await?;  // served; edge a.csv -> report @V1 loaded
    let a = env.evaluate("-R/data/a.csv").await?.get().await?;  // read => V1.verify(bytes) = Mismatch
    assert_eq!(a.status(), Status::Source);                  // a Source stays a Source
    assert_eq!(env.get_asset_manager().version(&a_key).await?, v2);   // version bumped to actual
    let a_meta = store.get_metadata(&a_key).await?;
    assert_eq!(a_meta.expiry_reason(), None);                // the root is not expired: it is input

    let meta = store.get_metadata(&parse_key("data/report.txt")?).await?;
    assert_eq!(meta.expiry_reason(), Some(ExpiryReason::Cascaded {
        cause: ExpiryCause::UpdatedInStore { actual: v2 },
        root: a_dep.clone(),
        via: a_dep,
    }));
    Ok(())
}

/// Bytes only: read the current sidecar and write it back next to the new bytes.
async fn edit_outside_liquers(store: &Arc<dyn AsyncStore>, key: &str, bytes: &[u8]) -> Result<(), Error> {
    let key = parse_key(key)?;
    let meta = store.get_metadata(&key).await?;
    store.set(&key, bytes, &meta).await
}
```

For a **recipe-backed** file (`report.txt` edited by hand) the environment policy decides, and the
same helper drives both outcomes. The helper's store also holds `data/summary.txt`, computed from
`report.txt` (`-R/data/report.txt/-/summarize/summary.txt`), so there is a dependent to expire:

| `external_change` | Status after the next read | Version | Content served | Stored copy | `summary.txt` |
|---|---|---|---|---|---|
| `user_input` (default) | `Override` (log `warning`: "content of data/report.txt changed outside Liquers; accepted as user input") | bumped to `from_content(edit)` | the edit | kept | `Cascaded { UpdatedInStore { actual }, root: -R/data/report.txt, via: -R/data/report.txt }` |
| `corrupted` | recomputed, `Ready` | recomputed | `rows=2` again | deleted, then rewritten by the recompute | the same reason, through `cascade_expire_dependents(key, UpdatedInStore { actual })` |

`data/a.csv` is a `Source`, so under **either** policy it is accepted as input (status stays
`Source`, version bumped to the new hash, dependents `Cascaded { UpdatedInStore }`); `Override` is
likewise never deleted (Phase 2 decision table).

A file **with no metadata at all** (dropped into `data/` by hand) has recorded version `0`, which can
never equal the recomputed hash, so it is a mismatch like any other:

| Dropped file | Recipe? | Result |
|---|---|---|
| `data/notes.csv`, no sidecar | no | stays `Source`; its version is the hash `from_content(bytes)`, **held in memory only, and no sidecar is written** (owner decision, 2026-10-02): nothing about the value changes, and the next process computes the same hash. No log line, because there is no sidecar to hold one |
| `data/report.txt` bytes, no sidecar | yes | `user_input`: `Override`, and a sidecar is written (the status really changes); `corrupted`: deleted and recomputed |

A sweep finds edits nobody has read yet. `ReportOnly` changes nothing and says what *would* happen:

```rust
let rep = mgr.verify_stored_versions(&parse_key("data")?, true, AuditMode::ReportOnly).await?;
assert_eq!(rep.changed, vec![(parse_key("data/a.csv")?,
    ExternalChangeAction::AcceptAsInput { actual: Version::from_content(A_V2) })]);
assert!(rep.verified.contains(&parse_key("data/report.txt")?));
// data/ghost.csv: bytes deleted, metadata kept (the exploratory workflow): nothing to hash.
assert!(rep.skipped.contains(&parse_key("data/ghost.csv")?));
```

In this sweep `data/recipes.yaml`, seeded by a raw `store.set` with `Metadata::new()`, also has no
recorded version, so it is a mismatch too and appears in `changed` as `AcceptAsInput`. The helper
`store_with_report_computed` therefore seeds it through `set_binary` (so it is `verified`); a separate
test, `verify_stored_versions_reports_an_unversioned_file`, pins the unseeded case.

Environment configuration for the three settings (defaults shown; each is omitted from a written
document when it equals the default):

```yaml
assets:
  dependency_audit: explicit   # or on_load
  verify_versions: on_read     # or off
  external_change: user_input  # or corrupted
```

## Example 3 (Optional): Pitfalls and Edge Cases

Each entry: symptom, cause, correct use or recovery, protective test.

1. **Waiting on a submitted dependency with `asset.get()`** (Part E). *Symptom:* a command that
   started a dependency and did other work gets an error when the dependency expired meanwhile, and
   its own result is never marked for recomputation. *Cause:* `AssetRef::get` knows nothing about a
   waiting parent and errors on `Expired`; only the context can apply the stale-value policy.
   *Correct:*
   ```rust
   async fn compare(_s: State<Value>, ctx: Context<CommandEnvironment>) -> Result<Value, Error> {
       let left = ctx.submit(&parse_query("-R/data/a.csv")?).await?;   // start both first
       let right = ctx.submit(&parse_query("-R/data/b.csv")?).await?;
       // ... other work ...
       let l = ctx.wait_for_dependency(&left).await?;   // not left.get(): records the version and
       let r = ctx.wait_for_dependency(&right).await?;  // uses a stale value, ending Expired
       Ok(Value::from(format!("{}|{}", l.try_into_string()?, r.try_into_string()?)))
   }
   ```
   *Test:* `stale_dependency_end_to_end_queued` and `_immediate`. A gate command (two file-level
   `static` `tokio::sync::Notify`) submits a **computed** key, signals, and waits; the test expires the
   dependency in between. It must be computed: expiring a `Source` is refused
   (`expiration_integration.rs::test_asset_ref_expire_from_source_errors`). The parent ends
   `Expired` with `Direct { cause: StaleDependency { dependency: -R/data/base.txt } }` (the parent is
   the asset that used the stale value, so the scope is `Direct`), and both managers agree. A
   dependent of the parent gets `Cascaded { cause: StaleDependency { dependency: -R/data/base.txt },
   root: <the parent>, via: <the parent> }` when `cascade_expire_dependents` runs at finalization.

2. **`on_load` where intermediates are deleted on purpose.** *Symptom:* the researcher deletes the
   20 GB `data/big.parquet` (metadata kept), and `report.html` is recomputed, needing the file again.
   *Cause:* under `OnLoad` a dependency with no current version (`dependency_version` returns
   `Version::unknown()`) refuses the fast track; that is the point of the strict mode. Note the
   opposite case does not refuse: a *recorded* unknown version is compatible with anything
   (`Version::matches`), so enabling `on_load` does not recompute every legacy result.
   *Correct:* keep `explicit` for exploratory work; use `on_load` for services that keep their
   intermediates. *Test:* `explicit_policy_serves_when_intermediate_deleted`,
   `on_load_refuses_when_dependency_has_no_version`, `on_load_does_not_refuse_recorded_unknown_version`.

3. **Expecting the audit to see a hand edit.** *Symptom:* `a.csv` was edited; a `ReportOnly` audit
   reports no findings. *Cause:* Part B compares *recorded* versions using metadata only. Part G is what
   makes the metadata truthful, and only when the changed file is read or swept. *Correct:* leave
   `verify_versions: on_read` on where files can be edited outside Liquers, and run
   `verify_stored_versions` for a sweep. *Test:* `audit_alone_does_not_see_unread_hand_edit`.

4. **Legacy hashes are verified, not converted.** *Symptom 1:* after the upgrade, a sidecar written
   before this change still carries an unflagged hash, and nothing rewrites it. *Symptom 2:* the first
   time Liquers re-stores such an `a.csv` (same bytes), the version changes to the flagged hash and
   every dependent is expired once. *Cause:* an unflagged recorded version equal to `from_bytes(bytes)`
   is `Verified`, so upgrading does not turn every stored result into `Override`; but it is a different
   number from `from_content(bytes)`, so the first Liquers write moves it. A legacy value that *was*
   edited matches neither hash and is a `Mismatch`, so it is detected like any other (an earlier draft
   of this pitfall said legacy edits are missed; Revision 2 removed that, because there is no
   "cannot tell" outcome any more). *Recovery:* none needed; accept the one-time cascade, or rewrite
   the inputs in a quiet period. *Test:* `legacy_unchanged_value_still_verifies` (U2 and I2),
   `legacy_changed_value_is_a_mismatch` (U2 and I2), `restoring_a_legacy_value_costs_one_cascade` (I2).

5. **`external_change: corrupted` deletes a deliberate edit.** *Symptom:* an operator's typo fix in
   `report.txt` disappears. *Cause:* a deliberate edit and damage are the same hash mismatch, and the
   policy is per environment. *Correct:* run `verify_stored_versions(.., ReportOnly)` and read
   `changed` before turning `corrupted` on; keep `user_input` otherwise. *Test:*
   `recipe_backed_edit_follows_policy`.

6. **External manager breaks the registration invariants, or forgets an override.** *Symptom:*
   cascades and persistence hit the wrong asset. *Cause:* `bound_owner_key` decides ownership through
   the manager's own `lookup_key_asset`, so at most one registered asset per key, `lookup_key_asset`
   returning exactly that asset, and never registering a volatile asset are requirements
   (`ASSET-REGISTRATION-OWNERSHIP-CONTRACT`). Two easier mistakes: not overriding
   `dependency_audit_policy()`, which silently downgrades `on_load` to `explicit` (the default body);
   and overriding `record_expiry` without setting `expiry_reason` on the metadata, which loses the
   reason although the status still flips (the default body does both, and an override replaces
   both). *Test:* `external_manager_registers_one_asset_per_key`,
   `external_manager_honours_audit_policy`, `record_expiry_is_overridable_by_a_manager`.

7. **Read-only store.** *Symptom:* the same hand edit is reported again after every restart.
   *Cause:* accepting a change writes metadata; against a read-only backend the write fails and is
   logged, the read succeeds and the in-memory version is right for this process only. *Correct:*
   nothing to do; use a writable store if the repeated detection matters. *Test:*
   `read_only_store_accepts_in_memory_and_does_not_fail_the_read`.

8. **Listing versions are membership only, and a listing counts metadata-only entries.** *Symptom 1:*
   `index.txt` is not expired when an entry's *content* changes. *Cause:* by decision, a dependent that
   reads content already has a `-R/` edge. *Symptom 2:* deleting the bytes of an intermediate (the
   exploratory workflow, metadata kept) does not expire `index.txt`. *Cause:* every store lists
   metadata-only keys (conformance rule `sidecar04`), so the name is still a member of the listing and
   the listing version does not move; this agrees with "delete intermediates, keep results".
   *Test:* `content_change_does_not_move_listing_version`,
   `deleting_bytes_keeps_the_listing_version`.

9. **Unknown-expecting edges: spared by one path, expired by the other.** *Symptom (for test
   authors):* a test asserts that an edge recorded with `Version::unknown()` is left alone by an audit
   and the graph loses a dependent. *Cause:* `audit_version` with a concrete version inserts it and then
   runs `expire_stale_dependents`, which spares a dependent **only on positive evidence** (a concrete
   equal version); an unknown edge is expired, because attribution edges from non-keyed expressions are
   all recorded as unknown (`dependencies.rs:180-195`, `:612`). Only the "dependency has no version"
   branch spares unknown edges, and with versions as `Version` that branch is `audit_version(key,
   Version::unknown())`, whose private implementation is `report_no_version`. *Tests:*
   `audit_version_expires_unknown_expecting_edge`,
   `audit_version_with_unknown_version_spares_unknown_expecting_edge`,
   `report_no_version_spares_unknown_expecting_edge`.

10. **Reading `expired` in a `ReportOnly` report.** *Symptom:* "nothing would recompute". *Cause:*
    `expired` is always empty in `ReportOnly`; the answer is in `findings`. *Test:*
    `report_only_audit_changes_nothing` asserts both.

11. **Relying on `expires:` on the immediate manager** (problem 6). *Symptom:* in `liquers-web`
    (immediate manager), a result declared `expires: "in 1 sec"` is still served, `Ready`, minutes
    later. *Cause (before this design):* the lazy check compared the status with itself, so it never
    fired. *Correct behaviour (after):* every lookup compares the clock with the deadline, and the
    second request recomputes:

    ```rust
    let env = immediate_env_with(|cr| register_command!(cr, fn stamp() -> result expires: "in 1 sec"));
    let first = env.evaluate("stamp").await?.get().await?;
    wait_past_deadline().await;                  // test clock / short sleep
    let second = env.evaluate("stamp").await?.get().await?;
    assert_ne!(first.try_into_string()?, second.try_into_string()?);  // recomputed
    ```

    The expired copy carries `Direct { cause: Deadline { expiration_time } }`: the asset whose time
    elapsed *is* the root. *Note:* lazy expiry on the immediate manager expires the asset without
    cascading (`expire_without_cascade`), so its dependents get no `Cascaded { Deadline, .. }` here.
    That matches today's code and differs from the queued monitor, and it is recorded on the issue
    rather than changed here. *Test:* `immediate_manager_deadline_fires` (I4).

12. **Looking for the reason on the wrong asset.** *Symptom:* after `a.csv` was audited, edited by
    hand, replaced or removed, `a.csv` has no `expiry_reason`, and the investigator concludes that
    "nothing recorded why". *Cause:* the reason lives on the asset that *became* `Expired`, and the
    root of an `Audit`, `UpdatedInStore`, `Updated` or `Removed` cause is not expired (it holds the
    new value, or is gone). Those causes occur only as `Cascaded` on the dependents, with `root`
    naming the key the cause happened to. Only `Deadline`, `Explicit` and `StaleDependency` occur as
    `Direct`. *Correct:* read the reason of the *dependent*, and follow `root` to the key and `via` to
    the step it came through. *Test:* `audit_never_expires_the_root`, `every_route_persists_its_reason`
    (I5), which checks the scope of each cause.

## Corner Cases

### 1. Memory
- Verification hashes bytes the manager already read, in place: `verify_reads_the_store_once`
  (I2, with a byte-read counter added to `fixtures::CountingStore`) checks no second read of the value.
- One changed dependency with 1000 stale dependents: `audit_report_lists_all_findings` (I1) returns
  1000 findings and expires 1000 assets without holding more than the report; each carries its own
  `via` (here always the root), so the `ExpiredKey` list is one entry per dependent.
- Very large folder: `listing_version_of_50k_names` (U1) is linear and allocation-bounded.
- A 100-link chain expires from its root: `cascade_over_100_link_chain` (I1); the last link's `via`
  is the 99th, and `root` is the head, in every reason (checked at both ends).

### 2. Concurrency
- `audit_version` racing `register_version` on one key: the `scc` entry lock orders them, each cascade
  is right for the version it saw. `audit_concurrent_with_register` (U1).
- Ten concurrent manager writes into `data/`: the last listing wins, equal versions cascade nothing.
  `concurrent_writes_settle_on_true_membership` (I1).
- Two audits of overlapping keys: the second finds `Expired` and adds no second log entry and no
  second `record_expiry` call. `concurrent_audits_do_not_double_expire` (I1).
- Two readers of one hand-edited key: `key_mutation_lock` gives one cascade.
  `concurrent_reads_apply_external_change_once` (I2).
- The stale-dependency race is made deterministic by the gate (Example 3, pitfall 1).
- Reason and status are written together under the `data` write lock, so no reader sees `Expired`
  without its reason: `reader_never_sees_expired_without_reason` (I5, polls `get_metadata` while a
  cascade runs).

### 3. Errors
- `version()` store error in an audit propagates as `Err`, never as `Version::unknown()`:
  `audit_store_error_propagates` (I1). Under `OnLoad` the same error refuses the fast track:
  `on_load_refuses_fast_track_on_store_error` (U3).
- `listdir` fails while refreshing after a write: `eprintln!`, the write stands.
  `listdir_error_after_write_is_logged_not_fatal` (I1).
- A key whose bytes are missing is skipped by `verify_stored_versions` (`skipped`), not reported
  changed: `missing_bytes_are_skipped` (I2). An *empty* data object is bytes and is checked:
  `empty_data_object_is_checked_not_skipped` (I2).
- `submit` cycle: `Error::dependency_cycle` (`submit_cycle_is_an_error`, U5).
- Read-only store in Part G: pitfall 7.

### 4. Serialization
- `ExpiryReason` round-trips (every `ExpiryCause` in both scopes) through JSON and YAML, internally
  tagged (`scope`, `kind`): U2. The wire form of Phase 2 is pinned literally:
  `{"scope":"cascaded","cause":{"kind":"deadline","expiration_time":"…"},"root":"-R/data/a.csv","via":"-R/data/b.csv"}`
  (`expiry_reason_json_shape`, U2).
- A record without `expiry_reason` loads as `None`; a non-expired record serializes byte-identically:
  `record_without_expiry_reason_loads`, `non_expired_record_json_is_unchanged` (U2).
- An older binary cannot be run in a test, so its behaviour is pinned by proxy: a record with an
  unknown field falls into `Metadata::LegacyMetadata` and still reports the status:
  `unknown_field_record_degrades_to_legacy` (U2).
- A flagged version (bit 127 set) survives the 32-digit hex sidecar form:
  `flagged_version_round_trips_through_hex` (U2). The stored `MetadataRecord.version` stays an
  `Option`, so an old sidecar with `version: null` still loads: `null_version_sidecar_still_loads` (U2).
  Config with and without `dependency_audit`: U4.

### 5. Integration
- Immediate manager deadline: `immediate_manager_deadline_fires` (I4).
- An audit never evaluates: `audit_never_evaluates` (I1), with a counting command.
- A result stored inside the folder it lists does not expire itself (its name is already a recipe
  name, so membership is unchanged): `index_inside_listed_folder_does_not_self_expire` (I1).
- Build matrix and wasm32: `bash scripts/check-build-matrix.sh` (`run_inline`, `submit`,
  `wait_for_dependency` must compile on wasm32). `liquers-web` `.d.ts` stubs: `check-stubs.sh`.
- Two environments over one store see each other's persisted reasons: `two_envs_share_persisted_reason` (I1).
- `remove()` of a `Source` while a dependent is live (the `main` cascade): `removing_a_source_cascades_with_removed` (I5).

## Documentation and Learning Log

### Guide Candidate Workflows and Examples

**1. Implementing a manager outside core** (`ASSET_MANAGER_IMPLEMENTATION_GUIDE.md`, snippet from
`tests/external_asset_manager.rs`). Shape only; it holds one graph, hands it out, and drives assets
through the public lifecycle primitives:

```rust
pub struct MinimalInlineAssetManager<E: Environment> {
    envref: EnvRef<E>,
    graph: DependencyManager<E>,                 // created once, never called into directly
    mutation_lock: tokio::sync::Mutex<()>,       // serializes keyed mutations (KeyMutationAccess)
    assets: scc::HashMap<Key, AssetRef<E>>,      // at most one asset per key
    policy: DependencyAuditPolicy,
    // test-only: every record_expiry call, to prove the method is the single writer
    expiries: std::sync::Mutex<Vec<(String, ExpiryReason)>>,
}
impl<E: Environment> DependencyManagerAccess<E> for MinimalInlineAssetManager<E> {
    fn dependency_manager(&self) -> &DependencyManager<E> { &self.graph }
}
impl<E: Environment> KeyMutationAccess for MinimalInlineAssetManager<E> {   // second supertrait
    fn key_mutation_lock(&self) -> &tokio::sync::Mutex<()> { &self.mutation_lock }
}
#[async_trait]
impl<E: Environment> AssetManager<E> for MinimalInlineAssetManager<E> {
    fn dependency_audit_policy(&self) -> DependencyAuditPolicy { self.policy } // honour the option
    fn start(&self) -> Result<(), Error> { self.refresh_command_versions()?; Ok(()) } // both sync
    // Override the single writer of expiry provenance: remember the call, then do what the
    // default does (set the reason, append the log entry). Called under the asset's data lock.
    fn record_expiry(&self, metadata: &mut Metadata, subject: &str, reason: &ExpiryReason) {
        match self.expiries.lock() {
            Ok(mut calls) => calls.push((subject.to_string(), reason.clone())),
            Err(poisoned) => poisoned.into_inner().push((subject.to_string(), reason.clone())),
        }
        metadata.set_expiry_reason(reason.clone());
        metadata.add_log_entry(reason.log_entry(subject));
    }
    // get/get_asset/...: create with AssetData::new(..).to_ref(), evaluate with asset.run_inline(None);
    // lazy deadline: if asset.expiration_time().await.is_expired() {
    //     asset.expire_without_cascade(ExpiryReason::Direct {
    //         cause: ExpiryCause::Deadline { expiration_time } }).await?; }
}
pub struct MinimalKind;
impl AssetManagerKind for MinimalKind {
    type Manager<E: Environment> = MinimalInlineAssetManager<E>;
    fn build<E: Environment>(envref: EnvRef<E>, o: &AssetManagerOptions) -> Result<Arc<Self::Manager<E>>, Error> {
        Ok(Arc::new(MinimalInlineAssetManager::new(envref, o.dependency_audit)))
    }
}
// let envref = EnvironmentBuilder::<Value, (), MinimalKind>::new().with_async_store(store).build()?;
```

**2. Starting several dependencies in a command** (`COMMAND_REGISTRATION_GUIDE.md`): the `compare`
snippet in Example 3, pitfall 1, replacing the existing `context.evaluate(..)` then `asset.get()`
example, with the rule "wait through the context, never through the handle".

**3. Configuring audits and verification** (`ENVIRONMENT_CONFIG.md`): the YAML block of Example 2,
plus `AssetManagerOptions::default().with_dependency_audit(DependencyAuditPolicy::OnLoad)` in code.
Linked executable references: `strict_service_after_restart`,
`hand_edited_source_is_input_and_expires_dependents`, `tests/external_asset_manager.rs`.

**4. Reading why an asset expired** (`ASSETS.md`, "The one meaning of `Expired`"): the table of
Example 1's variant, which shows `root` and `via` together, and the scope table of Phase 2 (which cause
occurs as `Direct` and which only as `Cascaded`).

### Usage and Meaning

- An audit compares two facts: what a result *recorded* about its inputs and what the inputs are
  *now*. Before this design, the second fact was unavailable to a process that had not seen the input.
- `OnLoad` is the strict mode (`the strict service`), `Explicit` the exploratory mode. Neither reads
  values; both read metadata or a folder listing. That is why "delete intermediates, keep results"
  still works under `Explicit`.
- `expiry_reason` is a record for people and tools; nothing branches on it. It is meaningful only
  while the status is `Expired`. It answers three questions: *what happened* (`cause`), *to which key*
  (`root`, absent when the asset is itself the root) and *by which route* (`via`).
- The root of an audit, an outside edit, a store, or a removal is never expired by that event; its
  dependents are, as `Cascaded`.
- Part G is what makes Part A honest for files edited outside Liquers.

### Repeatable Development Guidance

- Prove restart behaviour with `StoreSnapshot` and a **new** environment; a same-process test cannot
  show a missing version map.
- Observe expiry through the store's metadata, not `get()` (a fresh `evaluate` may recompute, and
  `get` on `Expired` errors); await `get()` on the producer first, as `keyed_version_cascade.rs`
  documents.
- A test for a pure decision (`external_change_action`, `listing_version`, `Version::verify`) is a
  plain `#[test]` with an explicit match; do not write `_ =>` on Liquers enums, in tests either. Where
  a test needs "is this variant", use `let ... else` with a message rather than `matches!`.
- Put any manager-contract test in `tests/common/manager_scenarios.rs` so an external manager runs it.
- To prove `record_expiry` is the single writer, count its calls in a manager that overrides it
  (I3, I5); do not infer it from the metadata alone.
- Diagnostics from library code go to `eprintln!`, never `println!`.

### Corrections and Unexpected Learning

- **Unknown-expecting edges (correction to the unit-test draft).** The draft claimed `audit_version`
  spares dependents whose edge records `Version::unknown()`. Phase 2 is the opposite for a concrete
  version: `audit_version` inserts the version then calls `expire_stale_dependents`, which spares only
  on positive evidence, so an unknown edge is expired (attribution edges are all unknown,
  `dependencies.rs:180-195`, `:612`). Only the no-version branch spares them, which Revision 2 writes
  as `audit_version(key, Version::unknown())` (private implementation `report_no_version`). The plan
  has all three tests, and a note in the guide.
- **Doc comment to fix in Phase 4.** `Version::unknown()` (`metadata.rs:28`) says dependency checks treat
  it "as compatible with any known version". That is true of `version_consistent` and of the
  missing-version path, but not of the cascade or the audit. Phase 5 should reword it.
- **`AssetManagerKind` is a type parameter, not an enum.** The draft used `AssetManagerKind::Minimal` and
  `with_asset_manager_kind(..)`. The real form is a marker type (`MinimalKind`) as the builder's third
  type parameter (`EnvironmentBuilder::<V, P, K>`), as in the sketch above.
- **`set_binary` takes `MetadataRecord`, and an ordinary `store.set(..)` records no version.** The
  draft simulated Liquers writing `V2` with `store.set(.., &Metadata::new())`, which would record
  none. Example 1 uses `set_binary`; Example 2 uses `store.set` only to model an *outside* edit.
- **A `Source` cannot be expired**, so the stale-dependency test needs a computed dependency.
- **A hand edit is found only when read.** Loading `report.txt` under `explicit` never touches
  `a.csv`, so the edit is found by reading `a.csv`, by `verify_stored_versions`, or not at all.
- Draft errors removed: `VersionCheck` has no serde; `Metadata::expiry_reason()` returns an owned
  `Option<ExpiryReason>`; a `changed` entry holds an `ExternalChangeAction`, not a policy; a
  fabricated `store.capabilities().supports_write` call was dropped; recipes are `query/filename`,
  not `.../summarize/data/report.txt`.

**Revision 2 (2026-10-02), applied to this document.** Phase 2 was revised by the owner and `main`
was merged. What changed here:

- **Reason shapes.** Every `ExpiryReason::Cascade { trigger }`, `Audit { dependency, found: Some(..) }`
  and top-level `Deadline` / `Explicit` / `StaleDependency` was replaced by `Direct { cause }` or
  `Cascaded { cause, root, via }` over the seven `ExpiryCause`s. Direct versus cascaded follows the
  Phase 2 scope table: an audit never expires its root, so its dependents are `Cascaded` with `root`
  the audited dependency and `via` the dependent's own direct dependency (equal to `root` for a direct
  dependent). The question left open in the previous draft ("`Audit` or `Cascade` for a transitive
  dependent?") is settled: every dependent gets `Cascaded { cause: Audit { found }, root, via }`.
- **Versions.** `version()` and `dependency_version()` return `Version` (0 means none);
  `AuditFinding.found`, `ExpiryCause::Audit.found` and `stale_edges(key, Version)` use `Version`.
  `MetadataRecord.version` stays `Option`, so snippets that read the *stored* field still show
  `Option`, and `AssetManager::version` is used for the current version.
- **`VersionCheck`.** It is now `Verified` or `Mismatch { actual, recorded: VersionKind }`;
  `NotVerifiable` is gone, and so are the tests named for it (`unflagged_mismatch_is_not_verifiable`,
  `unknown_is_not_verifiable`). Any non-matching recorded version, timestamp or 0, is a mismatch.
  `is_content_hash()` is replaced by `kind()`. `VersionVerificationReport` is
  `{ verified, skipped, changed }`; `skipped` means "no bytes", and the sweep example no longer relies
  on `not_verifiable`.
- **Legacy hashes.** The old pitfall "legacy hashes are missed" was wrong after Revision 2: a changed
  legacy value matches neither hash and is detected. Pitfall 4 now covers what really remains, that a
  legacy value is verified but not converted and costs one cascade on its first Liquers write.
- **Mismatch on load.** Default `UserInput`: recipe-backed becomes `Override`, a `Source` stays
  `Source`, the version is bumped to `actual`, and dependents get `Cascaded { UpdatedInStore { actual } }`.
  A file without metadata (recorded 0) under no recipe stays `Source` and its version is set to the
  hash; a test that expected "not a change" for it was renamed
  (`file_with_no_metadata_and_no_recipe_becomes_source_with_hash_version`).
- **Directory version** is the `from_content` hash of the ordered, length-prefixed listing. The
  previous caveat that a file store drops metadata-only keys, so listing versions differ per backend,
  is gone (`CORE-FILE-STORE-LISTDIR-DROPS-METADATA-ONLY-KEYS` is closed). Pitfall 8 now covers the
  real consequence of the fix: a metadata-only entry is still a member.
- **Cascade API.** `expire_dependencies_result(expired, cause)` and
  `cascade_expire_dependents(key, cause)` take the cause; `expire_dependencies_result_with` no longer
  exists. `ExpiredDependents` has `root: Option<DependencyKey>`, `keys: Vec<ExpiredKey { key, via }>`
  and `assets: Vec<(WeakAssetRef, DependencyKey)>`; `for_root` replaces `for_trigger`, and tests that
  read `.trigger` read `.root`.
- **`record_expiry`.** One overridable method writes the reason and the log entry for every expired
  asset, live and stored-only. New tests: I5 `record_expiry_is_called_for_every_expired_asset`, I3
  `record_expiry_is_overridable_by_a_manager`, shared scenario
  `scenario_every_expired_asset_has_reason_and_log_line`.
- **New causes.** `Removed` (a `remove()` of a `Source`, on `main` at `assets.rs:4195`, which
  cascades) and `Updated` (new content through Liquers, for example `set_binary` of a dependency, a
  changed listing) each have an integration test (I5), and a row in the route table below.
- **Citations.** This document cites no `assets.rs` line except the `remove()` one above (re-checked
  at HEAD); the `dependencies.rs` citations (`:180-195`, `:612`) were re-checked and still point at
  `expire_stale_dependents` and the attribution edges. `get_asset_info` no longer evaluates
  (`DESCRIBING-AN-ASSET-CAN-TRIGGER-ITS-EVALUATION` is closed), so the earlier warning against it was
  softened to "read the store's metadata because that is what is being tested".
- **Left to Phase 4 / Phase 5.** The real wording of each reason's log line for `ASSETS.md`, including
  the wording for a cascaded reason with `via != root` for causes other than a deadline (Phase 2
  gives one example for `via != root`, a deadline, and one for `via == root`, an audit; the format
  test pins only the parts both agree on). Whether `kind()` can tell a legacy unflagged hash from a
  timestamp (it cannot: both have bit 127 clear, so `recorded` may read `Timestamp` for a changed
  legacy value, which affects only the wording of one log line).

## Test Plan

### Unit Tests

Unit tests live beside the code in `#[cfg(test)] mod tests`. The dependency manager's methods become
`pub(crate)` under Part F, which is why the `dependencies.rs` tests must be in-crate. Types below:
`type TestEnv = SimpleEnvironment<Value>;` with `a = DependencyKey::new("-R/a")`, `b = DependencyKey::new("-R/b")`,
`c = DependencyKey::new("-R/c")`. `Version::new(n)` has bit 127 clear, so the numbers in the sketches
are timestamp-kind versions; they are arbitrary concrete values for the graph, which compares and
never recomputes.

**`liquers-core/src/dependencies.rs`**
- `audit_version_first_observation_expires_mismatched_dependent`: the Part A regression (sketch below).
- `audit_version_spares_dependent_with_equal_recorded_version`: positive evidence spares.
- `audit_version_expires_unknown_expecting_edge`: the correction (sketch below).
- `audit_version_with_unknown_version_spares_unknown_expecting_edge`: `audit_version(a, Version::unknown())`
  is the no-version branch.
- `report_no_version_spares_unknown_expecting_edge`: the private implementation of that branch (sketch below).
- `audit_version_with_unknown_version_still_expires_concrete_edges`: a dependency with no current
  version contradicts a concrete recorded version; the finding's `found` is `Version::unknown()`.
- `audit_version_returns_findings_for_direct_edges_only`: `findings` has the direct edge, `keys` the transitive set.
- `audit_version_sets_root`: `expired.root == Some(a)`, also when nothing expired.
- `expire_from_frontier_records_via_per_key`: chain `a <- b <- c`: `keys == [ExpiredKey{b, via: a}, ExpiredKey{c, via: b}]` (sketch below).
- `expire_from_frontier_via_is_shortest_path_in_a_diamond`: `a <- b <- d` and `a <- d`: `d` has `via == a`.
- `expired_dependents_query_assets_carry_the_key_whose_dependents_they_were`: the `assets` pairs.
- `stale_edges_is_read_only`: two calls agree; `get_version(a)` is still `None` (sketch below).
- `stale_edges_unknown_reports_concrete_edges_only`: `stale_edges(a, Version::unknown())`; unknown-expecting edges omitted.
- `register_version_first_registration_still_expires_nothing`: the evaluation path is unchanged.
- `expired_dependents_for_root_sets_root`; `expired_dependents_new_has_no_root`.
- `listing_version_is_content_hash_of_sorted_names` (flag bit set; `kind()` is `ContentHash`);
  `listing_version_is_order_independent`; `listing_version_length_prefix_prevents_collision`
  (`["ab","c"]` vs `["a","bc"]`); `listing_version_distinguishes_empty_list_from_empty_name`;
  `listing_version_of_50k_names` (corner case, Memory).
- `audit_concurrent_with_register` (corner case, Concurrency).

**`liquers-core/src/metadata.rs`**
- `expiry_reason_round_trips_json_and_yaml` (all seven causes, each in the scopes where it can occur
  plus one impossible-in-practice combination to show the type does not forbid it);
  `expiry_reason_is_internally_tagged_snake_case`; `expiry_reason_json_shape` (the literal of Phase 2).
- `log_line_format_per_cause` (sketch below): level per cause, `Info` for `Deadline`, `Explicit`,
  `Updated`, `Removed`; `Warning` for `Audit`, `StaleDependency`, `UpdatedInStore`. The message
  contains the subject, the root for a cascade, `via` when it differs from the root, and no numeric
  asset id. The four illustrative messages of Phase 2 are asserted as exact strings in a table
  marked "wording fixed in Phase 4; update here then".
- `expiry_reason_log_entry_uses_the_query_when_the_asset_has_no_key`: the `subject` argument.
- `record_without_expiry_reason_loads`; `non_expired_record_json_is_unchanged`;
  `expired_record_serializes_reason`; `unknown_field_record_degrades_to_legacy`;
  `null_version_sidecar_still_loads`.
- `expiry_reason_accessor_hides_reason_unless_expired` (owned `Option`).
- `asset_info_projects_expiry_reason`: both `From<MetadataRecord>` and `get_asset_info`.
- `version_from_content_sets_hash_flag`; `version_kind_of_each_constructor` (`from_content` is
  `ContentHash`, `from_time_now` / `from_specific_time` / `new_unique` are `Timestamp`, `unknown()` is
  `Unknown`); `from_bytes_is_not_forced_to_a_flag` (the value equals the plain blake3 prefix, so command
  versions do not change on upgrade); `time_and_unique_versions_mask_bit_127`.
- `verify_matches_own_bytes`; `verify_reports_mismatch_with_actual` (`actual == from_content(bytes)`);
  `verify_mismatch_records_the_kind` (a hash, a timestamp and 0 give `ContentHash`, `Timestamp`,
  `Unknown`); `timestamp_never_verifies`; `unknown_never_verifies`;
  `legacy_unflagged_hash_verifies_when_unchanged`; `legacy_changed_value_is_a_mismatch`
  (sketch below). Both legacy tests pick bytes whose `from_bytes` has bit 127 clear with a small
  search helper, because roughly half of all legacy hashes have it set (see the Learning Log).
- `flagged_version_round_trips_through_hex`; `dependency_key_is_store_resolvable`.

**`liquers-core/src/assets.rs`**
- `external_change_action_decision_table` (sketch below); `external_change_action_statuses_not_checked`.
- `audit_finding_new_and_report_default_assignment`: an external manager can build a report.
- `mark_expired_status_persists_reason_with_status`: metadata read back carries both.
- `mark_expired_status_calls_record_expiry_once_under_the_lock`: a test manager counts calls; a second
  `expire()` on an `Expired` asset adds none.
- `expire_stored_copy_calls_record_expiry`: the stored-only route.
- `expire_dependencies_result_assigns_cascaded_reason_per_key`: two dependents of `a` with different
  `via`; each persisted reason has `root == a` and its own `via`.
- `cascade_expire_dependents_takes_the_cause`: `(key, Removed)` gives `Cascaded { Removed, root: key, .. }`.
- `each_route_sets_its_reason`: the route table below, deadline, explicit, audit, stale dependency,
  updated, removed, updated in store.
- `register_plan_dependencies_adds_unknown_edge_for_unregistered_key`: pins the new behaviour.
- `dependency_blocks_fast_track_is_false_for_listing_key`.
- `on_load_refuses_fast_track_on_mismatch` / `_on_missing_version` / `_on_store_error`;
  `on_load_does_not_refuse_recorded_unknown_version`; `explicit_ignores_unknown_map_entries`.
- `immediate_manager_lazy_deadline_expires_with_deadline_reason`: `Direct { Deadline }`.

The route table the last tests check (from Phase 2 §"Which causes occur in which scope"):

| Route | The asset itself | Its dependents |
|---|---|---|
| Queued monitor or immediate lazy check | `Direct { Deadline { expiration_time } }` | `Cascaded { Deadline, root: the asset, via }` (queued only) |
| `AssetRef::expire` | `Direct { Explicit }` | `Cascaded { Explicit, root, via }` |
| Stale dependency at finalization | `Direct { StaleDependency { dependency } }` | `Cascaded { StaleDependency { dependency }, root: the asset, via }` |
| Audit | the audited key is not expired | `Cascaded { Audit { found }, root: the audited key, via }` |
| `set_state`, `set_binary`, re-evaluation, command version, listing refresh | the key holds the new value | `Cascaded { Updated { version }, root, via }` |
| `remove` of a `Source` / `Override` | the key is gone | `Cascaded { Removed, root, via }` |
| Mismatch on load (Part G) | `Source` stays, or becomes `Override`, or is deleted | `Cascaded { UpdatedInStore { actual }, root, via }` |

**`liquers-core/src/environment_builder.rs`, `environment_config.rs`**
- `audit_policy_defaults_to_explicit_and_round_trips`; `options_omit_defaults_in_yaml`;
  `options_write_non_defaults`; `config_with_and_without_assets_section_parses`;
  `with_dependency_audit_sets_policy`; the same for `VersionVerification` and `ExternalChangePolicy`.

**`liquers-core/src/context.rs`**
- `submit_records_dependency_under_the_key_wait_uses`; `submit_cycle_is_an_error`;
  `submit_does_not_run_on_inline_manager_until_waited` (a counter, not a status name);
  `wait_for_dependency_on_unsubmitted_asset_records_no_version_upgrade`;
  `get_dependency_state_matches_submit_then_wait`.

Sketches of the most important:

```rust
#[tokio::test]
async fn audit_version_first_observation_expires_mismatched_dependent() -> TestResult {
    let dm = DependencyManager::<TestEnv>::new();          // map is empty: a restarted process
    dm.add_dependency(&b, &a, Version::new(1)).await?;     // b recorded a@V1
    let (expired, findings) = dm.audit_version(&a, Version::new(2)).await;
    assert_eq!(expired.keys, vec![ExpiredKey { key: b.clone(), via: a.clone() }]);  // register_version would expire nothing
    assert_eq!(expired.root, Some(a.clone()));
    assert_eq!(findings, vec![AuditFinding::new(a.clone(), b.clone(), Version::new(1), Version::new(2))]);
    Ok(())
}

#[tokio::test]
async fn expire_from_frontier_records_via_per_key() -> TestResult {
    let dm = DependencyManager::<TestEnv>::new();
    dm.add_dependency(&b, &a, Version::new(1)).await?;     // a <- b
    dm.add_dependency(&c, &b, Version::new(1)).await?;     // b <- c
    let (expired, _) = dm.audit_version(&a, Version::new(2)).await;
    assert_eq!(expired.keys, vec![
        ExpiredKey { key: b.clone(), via: a.clone() },     // direct dependent: via == root
        ExpiredKey { key: c.clone(), via: b.clone() },     // two steps: via is c's own dependency
    ]);
    assert_eq!(expired.root, Some(a.clone()));
    Ok(())
}

#[tokio::test]
async fn audit_version_expires_unknown_expecting_edge() -> TestResult {
    let dm = DependencyManager::<TestEnv>::new();
    dm.add_dependency(&b, &a, Version::unknown()).await?;  // e.g. an attribution edge
    let (expired, _) = dm.audit_version(&a, Version::new(99)).await;
    assert_eq!(expired.keys, vec![ExpiredKey { key: b.clone(), via: a.clone() }],
        "no positive evidence: not spared");
    Ok(())
}

#[tokio::test]
async fn audit_version_with_unknown_version_spares_unknown_expecting_edge() -> TestResult {
    let dm = DependencyManager::<TestEnv>::new();
    dm.add_dependency(&b, &a, Version::unknown()).await?;
    let (expired, findings) = dm.audit_version(&a, Version::unknown()).await;  // a has no version
    assert!(expired.keys.is_empty(), "no version does not contradict an edge that expected none");
    assert!(findings.is_empty());
    assert_eq!(expired.root, Some(a.clone()));
    Ok(())
}

#[tokio::test]
async fn report_no_version_spares_unknown_expecting_edge() -> TestResult {
    let dm = DependencyManager::<TestEnv>::new();
    dm.add_dependency(&b, &a, Version::unknown()).await?;
    let expired = dm.report_no_version(&a).await;          // private impl of the branch above
    assert!(expired.keys.is_empty());
    assert!(dm.stale_edges(&a, Version::unknown()).await.is_empty());
    Ok(())
}

#[tokio::test]
async fn stale_edges_is_read_only() -> TestResult {
    let dm = DependencyManager::<TestEnv>::new();
    dm.add_dependency(&b, &a, Version::new(1)).await?;
    let first = dm.stale_edges(&a, Version::new(2)).await;
    assert_eq!(first, dm.stale_edges(&a, Version::new(2)).await);
    assert_eq!(dm.get_version(&a).await, None);            // nothing was registered
    Ok(())
}

#[test]
fn log_line_format_per_cause() {
    let subject = "data/report.txt";
    let (root, via) = (DependencyKey::new("-R/data/a.csv"), DependencyKey::new("-R/data/b.csv"));
    let v = Version::from_content(b"x");
    let causes = [
        (ExpiryCause::Explicit, LogEntryKind::Info),
        (ExpiryCause::Audit { found: v }, LogEntryKind::Warning),
        (ExpiryCause::StaleDependency { dependency: root.clone() }, LogEntryKind::Warning),
        (ExpiryCause::UpdatedInStore { actual: v }, LogEntryKind::Warning),
        (ExpiryCause::Updated { version: v }, LogEntryKind::Info),
        (ExpiryCause::Removed, LogEntryKind::Info),
    ];                                                      // Deadline needs a clock: separate test
    for (cause, kind) in causes {
        let cascaded = ExpiryReason::Cascaded { cause, root: root.clone(), via: via.clone() };
        let entry = cascaded.log_entry(subject);
        assert_eq!(entry.kind, kind);
        for part in [subject, "-R/data/a.csv", "-R/data/b.csv"] {
            assert!(entry.message.contains(part), "{part} missing from {:?}", entry.message);
        }
        assert!(!entry.message.chars().any(|c| c.is_ascii_digit()),
            "no numeric asset id in {:?}", entry.message);   // the keys above contain no digits
    }
}

#[test]
fn legacy_changed_value_is_a_mismatch() {
    let original = bytes_with_unflagged_hash();             // from_bytes(..) has bit 127 clear
    let legacy = Version::from_bytes(&original);
    assert_eq!(legacy.verify(&original), VersionCheck::Verified);
    let edited = [original.as_slice(), b"!"].concat();
    assert_eq!(legacy.verify(&edited), VersionCheck::Mismatch {
        actual: Version::from_content(&edited),
        recorded: legacy.kind(),                            // Timestamp: kind() cannot tell a legacy hash
    });
}

#[test]
fn external_change_action_decision_table() {
    let (v, ui, co) = (Version::from_content(b"x"), ExternalChangePolicy::UserInput, ExternalChangePolicy::Corrupted);
    let input = Some(ExternalChangeAction::AcceptAsInput { actual: v });
    for policy in [ui, co] {
        assert_eq!(external_change_action(Status::Source, false, policy, v), input);
        assert_eq!(external_change_action(Status::Source, true, policy, v), input);     // recipe added since
        assert_eq!(external_change_action(Status::Override, true, policy, v), input);
        assert_eq!(external_change_action(Status::Ready, false, policy, v), input);     // recipe removed since
        for unchecked in [Status::None, Status::Directory, Status::Error, Status::Volatile] {
            assert_eq!(external_change_action(unchecked, true, policy, v), None);
        }
    }
    for status in [Status::Ready, Status::Expired] {       // recipe-backed
        assert_eq!(external_change_action(status, true, ui, v), Some(ExternalChangeAction::ConvertToOverride { actual: v }));
        assert_eq!(external_change_action(status, true, co, v), Some(ExternalChangeAction::Delete));
    }
}
```

### Integration Tests

All in `liquers-core/tests/`, over `AsyncMemoryStore`, restarting with `fixtures::StoreSnapshot`.

**`dependency_audit_integration.rs`** (Parts A to E)
- A: `audit_after_restart_expires_dependent` (problem 1: the withdrawn test from
  `AUDIT-CANNOT-EXPIRE-ON-A-FIRST-OBSERVED-VERSION`); `audit_never_evaluates` (counter stays 0);
  `strict_service_after_restart` (Example 1).
- B: `on_load_refuses_stale_fast_track`; `on_load_refuses_when_dependency_has_no_version`;
  `explicit_policy_serves_when_intermediate_deleted`; `report_only_audit_changes_nothing`;
  `audit_policy_from_config_yaml` (an `EnvironmentConfig` with `assets: {dependency_audit: on_load}`).
- C (what stays here; the provenance tests proper are in I5): `audit_expires_transitive_dependents`
  (every dependent of the audited key gets `Cascaded { Audit { found }, root, via }`, with `via`
  the dependent's own dependency).
- D: `adding_a_file_expires_the_index`; `listing_gap_resolved_by_audit_after_restart`;
  `plan_dependency_without_version_gets_unknown_edge_then_upgrade`;
  `content_change_does_not_move_listing_version`; `deleting_bytes_keeps_the_listing_version`;
  `index_inside_listed_folder_does_not_self_expire`.
- E: `stale_dependency_end_to_end_queued`, `stale_dependency_end_to_end_immediate` (gate command).
- Corner cases: the I1 names in the section above.

**`external_change_integration.rs`** (Part G)
- `hand_edited_source_is_input_and_expires_dependents` (Example 2: status `Source`, version bumped to
  `actual`, dependents `Cascaded { UpdatedInStore { actual }, root, via }`);
  `recipe_backed_edit_follows_policy` (both policies, one helper; under `user_input` the version is
  bumped and `summary.txt` is expired with `UpdatedInStore`, under `corrupted` the stored copy is
  deleted and `summary.txt` gets the same reason); `override_is_never_deleted`;
  `file_with_no_metadata_and_no_recipe_becomes_source_with_hash_version` (recorded 0, so a mismatch;
  status stays `Source`; the version map holds the hash; **no sidecar is written**, which is
  asserted by `store.get_metadata` still recording no version: the manager writes nothing. A store
  can synthesize a sidecar on a bare file's first read by itself, which `AsyncFileStore` does today
  (`STORE-NO-READ-ONLY-ADAPTER`), so the test does not assert that no sidecar exists);
  `file_with_no_metadata_under_recipe_follows_policy`;
  `timestamp_versioned_value_with_bytes_is_adopted` (recorded kind `Timestamp`);
  `verify_stored_versions_report_only_changes_nothing`; `verify_stored_versions_applies_policy`;
  `verify_stored_versions_reports_an_unversioned_file` (the seeded `recipes.yaml` case);
  `legacy_unchanged_value_still_verifies`; `legacy_changed_value_is_a_mismatch`;
  `restoring_a_legacy_value_costs_one_cascade`; `audit_alone_does_not_see_unread_hand_edit`;
  `read_only_store_accepts_in_memory_and_does_not_fail_the_read`; `verify_reads_the_store_once`;
  `missing_bytes_are_skipped`; `empty_data_object_is_checked_not_skipped`;
  `concurrent_reads_apply_external_change_once`.

**`external_asset_manager.rs`, `common/manager_scenarios.rs`, `manager_parametric.rs`** (Part F)
- `manager_scenarios.rs` receives the generic bodies now in `manager_parametric.rs`
  (`scenario_basic_eval`, `scenario_cache_and_mode`, ...) plus new, manager-independent ones:
  `scenario_audit_after_restart`, `scenario_expiry_reason_cascade` (checks the `Cascaded` shape with
  `root` and `via`), `scenario_every_expired_asset_has_reason_and_log_line` (observes only the store's
  metadata, so every manager can run it), `scenario_listing_dependency`, `scenario_stale_dependency`.
- `manager_parametric.rs` runs all scenarios over `DefaultAssetManager` and `ImmediateAssetManager`.
- `external_asset_manager.rs` defines `MinimalInlineAssetManager<E>` from scratch (not a wrapper of
  `ImmediateAssetManager`) and `MinimalKind`, and runs the same scenarios:
  `external_manager_passes_shared_scenarios`, `external_manager_registers_one_asset_per_key`,
  `external_manager_honours_audit_policy`, `record_expiry_is_overridable_by_a_manager` (sketch below).
  Any primitive found missing while writing it is evidence for Phase 5.

**`expiration_integration.rs`** (changed): `immediate_manager_deadline_fires` (problem 6): register a
command with `expires: "in 1 sec"`, request, wait, request again, assert a recompute and a
`Direct { Deadline { .. } }` reason. Existing tests stay unchanged; `keyed_version_cascade.rs` updates
its `AuditReport` equality asserts for `findings`.

**`expiry_provenance_integration.rs`** (Part C; new, Revision 2)
- `every_route_persists_its_reason`: one environment, one scenario per row of the route table, each
  read from the store's metadata with a `let ... else` on the expected variant (queued deadline with a
  short `expires: "in 1 sec"`, explicit, stale dependency, audit, `set_binary` update, removal,
  hand edit), checking the scope of each: `Deadline`, `Explicit` and `StaleDependency` are `Direct` on
  the asset and `Cascaded` on its dependents; the other four occur only as `Cascaded`.
- `audit_never_expires_the_root`: after an audit that expires dependents, the audited key's status and
  `expiry_reason` are unchanged.
- `via_names_the_direct_dependency_on_a_two_step_cascade`: Example 1's variant, exactly as sketched
  there. `a.csv -> b.csv -> report.txt` after a restart and an `Audit` cascade: `b.csv` has
  `via == root == a.csv`, and `report.txt` has `root == a.csv` and `via == b.csv`. The same `via`
  shape for an in-process `Updated` cascade is covered by `set_binary_of_a_dependency_cascades_with_updated`.
- `every_cause_writes_a_log_line` (table-driven, added after the Phase 3 review): one row per
  `ExpiryCause` (`Deadline`, `Explicit`, `Audit`, `StaleDependency`, `UpdatedInStore`, `Updated`,
  `Removed`). Each row drives its route (see the route table below) on an `a.csv → report.txt`
  fixture, and asserts that the expired asset's stored log gained exactly one entry naming the cause
  with the level from Phase 2's table, and that its `expiry_reason` has the expected scope.
- `record_expiry_is_called_for_every_expired_asset` (sketch below): a manager that overrides
  `record_expiry` and counts calls; `a.csv` changes with three dependents, one live and held, one live
  and finished, one **stored-only** (live asset unmapped with `remove_key_asset`, graph edge and stored
  copy kept). Exactly three calls, one per expired asset, none for `a.csv`; the stored-only copy's
  sidecar carries the reason and the log line.
- `log_line_is_persisted_with_the_status`: the last log entry of the sidecar read from the store has
  the level and subject of `ExpiryReason::log_entry`, and appears in the same metadata write as
  `status: Expired`. (Format: `log_line_format_per_cause`, U2.)
- `log_line_names_keys_not_asset_ids`: no `Asset <number>` text anywhere in the log of an asset
  expired by a cascade (the defect of Phase 1 problem 3).
- `removing_a_source_cascades_with_removed`: store `a.csv` as a `Source` with no recipe, compute
  `report.txt` from it, `remove(a.csv)` (`assets.rs:4195`); `report.txt` is
  `Cascaded { Removed, root: -R/data/a.csv, via: -R/data/a.csv }`, the log level is `Info`, and
  `a.csv` is gone from the store.
- `set_binary_of_a_dependency_cascades_with_updated`: `set_binary(a.csv, A_V3)` with `report.txt` live;
  `report.txt` is `Cascaded { Updated { version: from_content(A_V3) }, root: -R/data/a.csv, via:
  -R/data/a.csv }`; storing the same bytes again expires nothing.
- `reader_never_sees_expired_without_reason` (corner case, Concurrency).
- `two_step_cascade_log_names_root_and_via` is the integration twin of `log_line_format_per_cause`.

Sketch of the single-writer proof:

```rust
#[tokio::test]
async fn record_expiry_is_called_for_every_expired_asset() -> TestResult {
    let env = recording_env().await?;                  // MinimalKind-based manager from I3, over a store
    let mgr = env.get_asset_manager();
    seed_a_with_three_dependents(&env).await?;         // live+held, live+finished, stored-only
    mgr.remove_key_asset(&parse_key("data/stored_only.txt")?).await;   // unmap live; edge + stored copy stay
    let before = recorded_calls(&env);                 // the manager's Vec<(String, ExpiryReason)>

    mgr.set_binary(&parse_key("data/a.csv")?, A_V2, MetadataRecord::new()).await?;

    let calls = recorded_calls(&env)[before.len()..].to_vec();
    let mut subjects: Vec<_> = calls.iter().map(|(s, _)| s.as_str()).collect();
    subjects.sort();
    assert_eq!(subjects, ["data/finished.txt", "data/held.txt", "data/stored_only.txt"]);
    let v2 = Version::from_content(A_V2);
    let a = DependencyKey::new("-R/data/a.csv");
    for (_, reason) in &calls {
        assert_eq!(reason, &ExpiryReason::Cascaded {
            cause: ExpiryCause::Updated { version: v2 }, root: a.clone(), via: a.clone() });
    }
    // The stored-only copy was reached too: its sidecar carries the reason the method wrote.
    let store = env.get_async_store();
    let stored = store.get_metadata(&parse_key("data/stored_only.txt")?).await?;
    assert_eq!(stored.status(), Status::Expired);
    assert_eq!(stored.expiry_reason(), Some(calls[0].1.clone()));
    Ok(())
}
```

`record_expiry_is_overridable_by_a_manager` (I3) is the same manager used to change behaviour: its
override appends a distinguishing log entry (for example `LogEntry::info("audit-trail: …")`) and
the test asserts the sidecar of an expired asset ends with that entry while the status and
`expiry_reason` are still the ones the default would set. That shows both halves of the contract: the
method is the one place that decides the wording, and every route calls it.

### Manual Validation

```bash
cargo test -p liquers-core --lib --tests      # every unit and integration test above
cargo test -p liquers-lib --lib --tests       # AssetInfo consumers unaffected
bash scripts/check-build-matrix.sh            # cfg matrix and wasm32; run after Part F visibility changes
cargo run -p liquers-core --features cli --bin liquers-validate -- \
  --command index_files --command summarize -- '-R-dir/data/-/index_files' '-R/data/a.csv/-/summarize'
```

Expected: the test commands end `test result: ok`; the matrix script prints its final computed total
with no failing row; `liquers-validate` exits 0 with `status: Ok` for both queries and their
`encoded` fields equal to the input (they are spelt `-R-dir/data/-/index_files` and
`-R/data/a.csv/-/summarize`). Because the offline validator does not run `find_dependencies`, the
plan shows no `-R-dir/data` dependency; that appears only at runtime (Phase 2, Example 4). Recipe
queries with a trailing filename (`.../summarize/report.txt`, `.../index_files/index.txt`) also
validate, as do the chain recipes of Example 1's variant (`-R/data/a.csv/-/summarize/b.csv`,
`-R/data/b.csv/-/summarize/report.txt`) and the Example 2 recipe
`-R/data/report.txt/-/summarize/summary.txt`. `liquers-web`: `./liquers-web/scripts/check-stubs.sh`
after `cargo clean`, for the new `AssetInfo` field.

## Auto-Invoke: liquers-unittest Skill Output

Templates in the conventions of `.claude/skills/liquers-unittest` (`-> Result<(), Box<dyn
std::error::Error>>` for tests using `?`, `type CommandEnvironment` before `register_command!`, memory
store, `parse_key`/`parse_query`, no `unwrap` in helpers, no `_ =>`, `eprintln!` not `println!`).

**Template 1: in-file unit test of the dependency graph** (`liquers-core/src/dependencies.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::{DependencyKey, Version};

    type TestEnv = crate::context::SimpleEnvironment<crate::value::Value>;

    #[tokio::test]
    async fn audit_version_sets_root() -> Result<(), Box<dyn std::error::Error>> {
        let dm = DependencyManager::<TestEnv>::new();
        let (a, b) = (DependencyKey::new("-R/a"), DependencyKey::new("-R/b"));
        dm.add_dependency(&b, &a, Version::new(7)).await?;

        let (expired, findings) = dm.audit_version(&a, Version::new(7)).await;

        assert!(expired.keys.is_empty(), "equal recorded version is positive evidence");
        assert!(findings.is_empty());
        assert_eq!(expired.root, Some(a));
        Ok(())
    }
}
```

**Template 2: integration test with a counting command** (`tests/dependency_audit_integration.rs`)

```rust
#[tokio::test]
async fn audit_never_evaluates() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = TestEnv;
    let runs = Arc::new(AtomicUsize::new(0));
    let store: Arc<dyn AsyncStore> = Arc::new(AsyncMemoryStore::new(&Key::new()));
    seed_report_recipe(&store).await?;                       // data/recipes.yaml, see Example 1
    let counter = runs.clone();
    let mut b = EnvironmentBuilder::<Value>::new()
        .with_async_store(store)                             // -R/ queries need a store
        .with_recipe_provider_choice(RecipeProviderChoice::Default);
    b.command_registry.register_command(CommandKey::new_name("summarize"),
        move |_, _, _| Ok(Value::I32(counter.fetch_add(1, Ordering::SeqCst) as i32)))?;
    let env = b.build()?;
    // ... store a.csv, evaluate report.txt once (runs == 1), restart over a snapshot ...

    let before = runs.load(Ordering::SeqCst);
    env.get_asset_manager().trigger_dependency_audit(&parse_query("-R/data/report.txt")?).await?;

    assert_eq!(runs.load(Ordering::SeqCst), before, "an audit reads metadata, never evaluates");
    Ok(())
}
```

**Template 3: pure decision, explicit matching** (`liquers-core/src/assets.rs`)

```rust
#[test]
fn source_is_always_input_under_both_policies() {
    let v = Version::from_content(b"edited");
    for policy in [ExternalChangePolicy::UserInput, ExternalChangePolicy::Corrupted] {
        let action = external_change_action(Status::Source, false, policy, v);
        match action {
            Some(ExternalChangeAction::AcceptAsInput { actual }) => assert_eq!(actual, v),
            Some(ExternalChangeAction::ConvertToOverride { .. }) => panic!("Source is never converted"),
            Some(ExternalChangeAction::Delete) => panic!("Source is never deleted"),
            None => panic!("Source is always checked"),
        }
    }
}
```
