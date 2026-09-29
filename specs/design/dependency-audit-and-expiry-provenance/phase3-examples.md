# Phase 3: Examples & Use-cases - dependency-audit-and-expiry-provenance

## High-Level Introduction

Phase 1 states one purpose: make the dependency-verification half of the expiry system *correct*
(an audit that finds a moved input really expires its dependents), *configurable* (the environment
says when audits run), and *explainable* (every transition into `Expired` records why), and then
extend it to the two inputs the graph could not see, a folder listing and content edited outside
Liquers. Phase 1 illustrates this with eight worked problems on two stored files, `data/a.csv` and
`data/report.txt`, whose recipe reads it. Phase 2 answers them with Parts A to G. This phase shows
the answers working, in the same vocabulary.

The examples are ordered from the representative path to the sharp edges:

1. **Example 1 (primary): a strict service after a restart.** The one story every reader of the
   design must follow. Monday computes `report.txt` from `a.csv@V1`, a batch job stores `a.csv@V2`
   overnight, and on Tuesday two operators, a strict service and a researcher, meet the same store.
   It exercises Parts A, B and C, and Phase 1 problems 1, 2 and 3.
2. **Example 2 (detail): a shared data folder.** Builds on Example 1's environment and adds the two
   inputs that are not a plain `-R/` value: a folder listing (Part D, problem 4) and a file edited by
   hand (Part G, problem 8), including the two policies for a recipe-backed file.
3. **Example 3 (pitfalls).** Ten ways to get the feature wrong, each with symptom, cause, correct
   use and the test that guards it. It also carries the correction to the drafts (unknown-expecting
   edges) that the test plan depends on.

Problems 5, 6 and 7 (the stale-dependency path, the immediate manager's deadline, the external asset
manager) are small and mechanical. They appear as the Part E command snippet in Example 3, and as
named integration tests and the from-scratch manager in the Test Plan, not as long narratives.

## Example Type

**User choice:** Conceptual code (the APIs do not exist yet).

The snippets follow the style of `liquers-core/tests/keyed_version_cascade.rs` and use Phase 2's
names and signatures exactly, but they are not compiled. Two commands, `summarize` and `index_files`,
are **hypothetical test commands**, registered inside each test; no `liquers-lib` namespace is
involved (Phase 2 §"Relevant Commands"). Queries were checked with `liquers-validate` (see Manual
Validation). Where a snippet leans on something Phase 2 leaves open, the Learning Log says so.

## Overview Table

| # | Type | Name | Purpose | Drafted By |
|---|------|------|---------|------------|
| 1 | Example | Strict service after a restart | Parts A, B, C: audit compares with recorded versions on first observation, `OnLoad` refuses a stale fast track, `ReportOnly` vs `Expire`, the reason persisted in the sidecar | Haiku drafter 1 (rewritten by Sonnet synthesizer) |
| 2 | Example | A shared data folder | Part D listing dependency; Part G hand edit of a `Source` and of a recipe-backed file under both policies; `verify_stored_versions` | Haiku drafter 2 (rewritten by Sonnet synthesizer) |
| 3 | Example | Pitfalls | Ten pitfalls with symptom, cause, correct use, guarding test; Part E snippet | Haiku drafter 3 (edited by Sonnet synthesizer) |
| U1 | Unit tests | `dependencies.rs` | `audit_version`, `stale_edges`, `report_no_version`, `listing_version`, `ExpiredDependents::for_trigger` | Haiku drafter 4 (corrected by Sonnet synthesizer) |
| U2 | Unit tests | `metadata.rs` | `ExpiryReason` serde and log entries, record compatibility, `Version` content hashes and `verify` | Haiku drafter 4 |
| U3 | Unit tests | `assets.rs` | `external_change_action` table, `AuditFinding`/`AuditReport`, reason at each route, plan `unknown` edges, `OnLoad` in fast track | Haiku drafter 4 (extended by Sonnet synthesizer) |
| U4 | Unit tests | `environment_builder.rs`, `environment_config.rs` | Policy defaults, serde, YAML omission, builder | Haiku drafter 4 |
| U5 | Unit tests | `context.rs` | `submit`, public `wait_for_dependency`, cycle, no drain | Haiku drafter 4 (corrected by Sonnet synthesizer) |
| I1 | Integration | `dependency_audit_integration.rs` | Parts A to E end to end, both managers | Haiku drafter 5 (corrected by Sonnet synthesizer) |
| I2 | Integration | `external_change_integration.rs` | Part G end to end | Haiku drafter 5 |
| I3 | Integration | `external_asset_manager.rs`, `common/manager_scenarios.rs`, `manager_parametric.rs` | Part F: a from-scratch manager runs the shared scenarios | Haiku drafter 5 (kind wiring corrected by Sonnet synthesizer) |
| I4 | Integration | `expiration_integration.rs` | Immediate manager's lazy deadline fires (problem 6) | Sonnet synthesizer |
| C | Corner cases | Five categories | Memory, concurrency, errors, serialization, integration, each with a covering test | Haiku drafter 5 (merged by Sonnet synthesizer) |
| M | Manual | Validation commands | Build matrix, full test loops, query validation | Sonnet synthesizer |
| T | Templates | `liquers-unittest` output | Three templates in repository conventions | Sonnet synthesizer |

### Coverage map

| Phase 1 problem | Part | Example | Guarding tests |
|---|---|---|---|
| 1. Audit after restart misses a changed input | A | Ex. 1 | `audit_version_first_observation_expires_mismatched_dependent` (U1), `audit_after_restart_expires_dependent` (I1) |
| 2. Nobody can say when to check | B | Ex. 1, Ex. 3 pitfalls 2 and 10 | `on_load_refuses_stale_fast_track`, `explicit_policy_serves_when_intermediate_deleted`, `report_only_audit_changes_nothing` (I1) |
| 3. An expired asset cannot say why | C | Ex. 1 | `every_route_persists_its_reason` (I1), `ExpiryReason` tests (U2) |
| 4. Folder listing never updates | D | Ex. 2 | `adding_a_file_expires_the_index`, `listing_gap_resolved_by_audit_after_restart` (I1) |
| 5. "Use the old input" rule untested | E | Ex. 3 pitfall 1 | `stale_dependency_end_to_end_queued` / `_immediate` (I1) |
| 6. Time limits never fire on the immediate manager | C | (test only) | `immediate_manager_deadline_fires` (I4) |
| 7. Nobody outside core can write a manager | F | Ex. 3 pitfall 6, Learning Log | `external_asset_manager.rs` (I3) |
| 8. Content changed by another program | G | Ex. 2 | `hand_edited_source_is_input_and_expires_dependents`, `recipe_backed_edit_follows_policy` (I2) |

Every Part A to G appears in at least one row above.

## Example 1: Strict service after a restart

### Connection to the High-Level Design

Phase 1 problems 1 to 3, on the exact files of the Phase 1 glossary. Part A makes the audit compare
the *current* version of `a.csv` with the version `report.txt` recorded, even though the restarted
process has never seen `a.csv` (the map is empty). Part B lets the strict service refuse to serve
such a stale result at load time (`on_load`) while the researcher keeps today's behaviour
(`explicit`) and inspects with a report-only audit. Part C makes the resulting `Expired` explain
itself in the sidecar.

### Scenario

Monday, process 1 stores `data/a.csv` (`V1`, two rows) and computes `data/report.txt` = `summarize(a.csv)`, recording `a.csv@V1`. Overnight, a batch
job in *another* process stores a new `a.csv` (`V2`, three rows). Tuesday the store holds `a.csv@V2`
and `report.txt@[a.csv:V1]`, status `Ready`, and every process starts with an empty version map.

- The **strict service** runs with `dependency_audit: on_load`. It must never serve `report.txt`
  built on an old `a.csv`.
- The **researcher** runs with the default `explicit`. Loading is unchanged; she audits when she
  chooses, first with a report-only audit, then for real.

### Sequence of Steps

1. Monday: `set_binary(a.csv)` stores the bytes and records `Version::from_content(bytes)`; evaluating
   `-R/data/report.txt` runs `summarize` and persists `report.txt` with `dependencies: [{key:
   -R/data/a.csv, version: V1}]`.
2. Night: a fresh environment over a copy of the store calls `set_binary(a.csv)` again. It never loaded
   `report.txt`, so no edge exists and nothing cascades. The store now disagrees with itself, which
   is the situation to be detected.
3. Tuesday, strict: a new environment with `OnLoad` evaluates `-R/data/report.txt`. `try_fast_track`
   finds the recorded `a.csv@V1`, the map has no entry, so under `OnLoad` it resolves
   `dependency_version(-R/data/a.csv)` from metadata (`Some(V2)`), sees the mismatch and refuses the
   fast track. The report is recomputed from `V2` (`rows=3`). The stored copy is not expired first.
4. Tuesday, researcher: an `Explicit` environment serves the stored `rows=2` as `Ready`.
5. She calls `trigger_dependency_audit_with(q, ReportOnly)`: `missing_versions_for` finds the gap
   `a.csv`, `stale_edges(a.csv, Some(V2))` yields one `AuditFinding`. No version is registered, nothing
   is expired.
6. She calls `trigger_dependency_audit(q)` (= `Expire`). `audit_version(a.csv, V2)` inserts `V2`, then
   `expire_stale_dependents` compares it with the edge's `V1` and expires `report.txt` with
   `ExpiryReason::Audit { dependency: -R/data/a.csv, found: Some(V2) }`, persisted with the status.

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
        .with_async_store(store)
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
    let report_key = parse_key("data/report.txt")?;
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
    let dry = mgr.trigger_dependency_audit_with(&q, AuditMode::ReportOnly).await?;
    assert!(dry.expired.is_empty());                             // ReportOnly never expires
    assert_eq!(dry.findings, vec![AuditFinding::new(
        a_dep.clone(), DependencyKey::new("-R/data/report.txt"),
        Version::from_content(A_V1), Some(v2))]);
    let store = lax.get_async_store();
    assert_eq!(store.get_metadata(&report_key).await?.status(), Status::Ready);

    let wet = mgr.trigger_dependency_audit(&q).await?;           // = with(AuditMode::Expire)
    assert_eq!(wet.expired, vec![DependencyKey::new("-R/data/report.txt")]);
    let meta = store.get_metadata(&report_key).await?;           // persisted, not just in memory
    assert_eq!(meta.status(), Status::Expired);
    assert_eq!(meta.expiry_reason(), Some(ExpiryReason::Audit { dependency: a_dep, found: Some(v2) }));
    Ok(())
}
```

`env_from(snapshot, policy)` replays the snapshot into a new `AsyncMemoryStore` and calls `env_over`.
The reading of the reason goes through the store's metadata on purpose: `get_asset_info(&key)` on a
key asset may evaluate it (`DESCRIBING-AN-ASSET-CAN-TRIGGER-ITS-EVALUATION`), which would hide the
`Expired` state this test wants to observe.

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
assumption was false; wording indicative):
```
data/report.txt expired: dependency data/a.csv is at a different version than recorded (audit)
```

## Example 2: A shared data folder

Same store and helpers as Example 1, plus a second recipe in `data/recipes.yaml`:
`-R-dir/data/-/index_files/index.txt`, where `index_files` (hypothetical test command) writes the
names it is handed, one per line.

### D. A result built from a listing (problem 4)

`index.txt` reads the *names* in `data/`, not their content, so its only edge is `-R-dir/data`.
During evaluation `register_plan_dependencies` adds that edge with `Version::unknown()` (it is no
longer skipped), then the `GetAssetDirectory` step registers `listing_version(names)` and
`add_context_dependency` upgrades the record. Writing through the manager refreshes the listing.

```rust
#[tokio::test]
async fn adding_a_file_expires_the_index() -> TestResult {
    let (store, env) = shared_folder_env().await?;   // a.csv stored; report.txt, index.txt computed
    let mgr = env.get_asset_manager();
    let listing = DependencyKey::new("-R-dir/data");
    // Manager-mediated write => refresh_listing_version(data) => new membership => cascade.
    mgr.set_binary(&parse_key("data/new.csv")?, b"n\n", MetadataRecord::new()).await?;

    let index = store.get_metadata(&parse_key("data/index.txt")?).await?;
    assert_eq!(index.expiry_reason(), Some(ExpiryReason::Cascade { trigger: listing }));
    // report.txt reads a.csv's content, not the listing: untouched.
    let report = store.get_metadata(&parse_key("data/report.txt")?).await?;
    assert_eq!(report.status(), Status::Ready);
    Ok(())
}
```

A write that bypasses the manager is invisible until an audit: after a restart the
`-R-dir/data` gap is resolved by `dependency_version`, which lists the directory, hashes the sorted
names and compares (`listing_gap_resolved_by_audit_after_restart`). Only membership counts:
overwriting `a.csv` does not move the listing.

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

    env.evaluate("-R/data/report.txt").await?.get().await?;  // served; edge a.csv -> report @V1 loaded
    let a = env.evaluate("-R/data/a.csv").await?.get().await?;  // read => V1.verify(bytes) = Mismatch
    assert_eq!(a.metadata.version(), Some(Version::from_content(A_V2)));   // adopted as input
    assert_eq!(a.status(), Status::Source);                                // a Source stays a Source

    let meta = store.get_metadata(&parse_key("data/report.txt")?).await?;
    assert_eq!(meta.expiry_reason(),
        Some(ExpiryReason::Cascade { trigger: DependencyKey::new("-R/data/a.csv") }));
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
same helper drives both outcomes:

| `external_change` | Status after the next read | Content served | Stored copy |
|---|---|---|---|
| `user_input` (default) | `Override` (log `warning`: "content of data/report.txt changed outside Liquers; accepted as user input") | the edit | kept, version adopted |
| `corrupted` | recomputed, `Ready` | `rows=2` again | deleted, then rewritten by the recompute |

`data/a.csv` is a `Source`, so under **either** policy it is accepted as input; `Override` is likewise
never deleted (Phase 2 decision table).

A sweep finds edits nobody has read yet. `ReportOnly` changes nothing and says what *would* happen:

```rust
let rep = mgr.verify_stored_versions(&parse_key("data")?, true, AuditMode::ReportOnly).await?;
assert_eq!(rep.changed, vec![(parse_key("data/a.csv")?,
    ExternalChangeAction::AcceptAsInput { actual: Version::from_content(A_V2) })]);
assert!(rep.verified.contains(&parse_key("data/report.txt")?));
assert!(rep.not_verifiable.contains(&parse_key("data/recipes.yaml")?)); // written with no version
```

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
   `Expired` with `StaleDependency { dependency: -R/data/base.txt }`, and both managers agree.

2. **`on_load` where intermediates are deleted on purpose.** *Symptom:* the researcher deletes the
   20 GB `data/big.parquet` (metadata kept), and `report.html` is recomputed, needing the file again.
   *Cause:* under `OnLoad` a dependency with no current version refuses the fast track; that is the
   point of the strict mode. *Correct:* keep `explicit` for exploratory work; use `on_load` for
   services that keep their intermediates. *Test:* `explicit_policy_serves_when_intermediate_deleted`
   and `on_load_refuses_when_dependency_has_no_version`.

3. **Expecting the audit to see a hand edit.** *Symptom:* `a.csv` was edited; a `ReportOnly` audit
   reports no findings. *Cause:* Part B compares *recorded* versions using metadata only. Part G is what
   makes the metadata truthful, and only when the changed file is read or swept. *Correct:* leave
   `verify_versions: on_read` on where files can be edited outside Liquers, and run
   `verify_stored_versions` for a sweep. *Test:* `audit_alone_does_not_see_unread_hand_edit`.

4. **Legacy hashes.** *Symptom:* a value stored before this change and edited afterwards is not
   noticed. *Cause:* an unflagged version that does not match the bytes might be a timestamp, so
   `verify` answers `NotVerifiable`, not `Mismatch`; unchanged legacy values still verify. *Recovery:*
   none by design (no migration pass); the value is protected once it is rewritten. *Test:*
   `unflagged_mismatch_is_not_verifiable` (unit), `legacy_unchanged_value_still_verifies`.

5. **`external_change: corrupted` deletes a deliberate edit.** *Symptom:* an operator's typo fix in
   `report.txt` disappears. *Cause:* a deliberate edit and damage are the same hash mismatch, and the
   policy is per environment. *Correct:* run `verify_stored_versions(.., ReportOnly)` and read
   `changed` before turning `corrupted` on; keep `user_input` otherwise. *Test:*
   `recipe_backed_edit_follows_policy`.

6. **External manager breaks the registration invariants.** *Symptom:* cascades and persistence hit
   the wrong asset. *Cause:* `bound_owner_key` decides ownership through the manager's own
   `lookup_key_asset`, so at most one registered asset per key, `lookup_key_asset` returning exactly
   that asset, and never registering a volatile asset are requirements
   (`ASSET-REGISTRATION-OWNERSHIP-CONTRACT`). A second easy mistake: not overriding
   `dependency_audit_policy()`, which silently downgrades `on_load` to `explicit` (the default body).
   *Test:* `external_manager_registers_one_asset_per_key`, `external_manager_honours_audit_policy`.

7. **Read-only store.** *Symptom:* the same hand edit is reported again after every restart.
   *Cause:* accepting a change writes metadata; against a read-only backend the write fails and is
   logged, the read succeeds and the in-memory version is right for this process only. *Correct:*
   nothing to do; use a writable store if the repeated detection matters. *Test:*
   `read_only_store_accepts_in_memory_and_does_not_fail_the_read`.

8. **Listing versions are membership only.** *Symptom:* `index.txt` is not expired when an entry's
   *content* changes. *Cause:* by decision, a dependent that reads content already has a `-R/` edge.
   Also, a file store omits metadata-only keys that OpenDAL lists, so the same logical folder can
   have a different listing version per backend; versions are only compared within one store, so this
   costs at most one recomputation when switching (`CORE-FILE-STORE-LISTDIR-DROPS-METADATA-ONLY-KEYS`).
   *Test:* `content_change_does_not_move_listing_version`.

9. **Unknown-expecting edges: spared by one path, expired by the other.** *Symptom (for test
   authors):* a test asserts that an edge recorded with `Version::unknown()` is left alone by an audit
   and the graph loses a dependent. *Cause:* `audit_version` inserts the version and then runs
   `expire_stale_dependents`, which spares a dependent **only on positive evidence** (a concrete equal
   version); an unknown edge is expired, because attribution edges from non-keyed expressions are all
   recorded as unknown (`dependencies.rs:180-195`, `:612`). Only `report_no_version` spares unknown
   edges. *Tests:* `audit_version_expires_unknown_expecting_edge`,
   `report_no_version_spares_unknown_expecting_edge`.

10. **Reading `expired` in a `ReportOnly` report.** *Symptom:* "nothing would recompute". *Cause:*
    `expired` is always empty in `ReportOnly`; the answer is in `findings`. *Test:*
    `report_only_audit_changes_nothing` asserts both.

## Corner Cases

### 1. Memory
- Verification hashes bytes the manager already read, in place: `verify_reads_the_store_once`
  (I2, with a byte-read counter added to `fixtures::CountingStore`) checks no second read of the value.
- One changed dependency with 1000 stale dependents: `audit_report_lists_all_findings` (I1) returns
  1000 findings and expires 1000 assets without holding more than the report.
- Very large folder: `listing_version_of_50k_names` (U1) is linear and allocation-bounded.
- A 100-link chain expires from its root: `cascade_over_100_link_chain` (I1).

### 2. Concurrency
- `audit_version` racing `register_version` on one key: the `scc` entry lock orders them, each cascade
  is right for the version it saw. `audit_concurrent_with_register` (U1).
- Ten concurrent manager writes into `data/`: the last listing wins, equal versions cascade nothing.
  `concurrent_writes_settle_on_true_membership` (I1).
- Two audits of overlapping keys: the second finds `Expired` and adds no second log entry.
  `concurrent_audits_do_not_double_expire` (I1).
- Two readers of one hand-edited key: `key_mutation_lock` gives one cascade.
  `concurrent_reads_apply_external_change_once` (I2).
- The stale-dependency race is made deterministic by the gate (Example 3, pitfall 1).

### 3. Errors
- `version()` store error in an audit propagates as `Err`, never as "no version":
  `audit_store_error_propagates` (I1). Under `OnLoad` the same error refuses the fast track:
  `on_load_store_error_refuses_fast_track` (U3).
- `listdir` fails while refreshing after a write: `eprintln!`, the write stands.
  `listdir_error_after_write_is_logged_not_fatal` (I1).
- A key whose bytes are missing is skipped by `verify_stored_versions`, not reported changed:
  `missing_bytes_are_skipped` (I2).
- `submit` cycle: `Error::dependency_cycle` (`submit_cycle_is_an_error`, U5).
- Read-only store in Part G: pitfall 7.

### 4. Serialization
- `ExpiryReason` round-trips (all five) through JSON and YAML, internally tagged: U2.
- A record without `expiry_reason` loads as `None`; a non-expired record serializes byte-identically:
  `record_without_expiry_reason_loads`, `non_expired_record_json_is_unchanged` (U2).
- An older binary cannot be run in a test, so its behaviour is pinned by proxy: a record with an
  unknown field falls into `Metadata::LegacyMetadata` and still reports the status:
  `unknown_field_record_degrades_to_legacy` (U2).
- A flagged version (bit 127 set) survives the 32-digit hex sidecar form:
  `flagged_version_round_trips_through_hex` (U2). Config with and without `dependency_audit`: U4.

### 5. Integration
- Immediate manager deadline: `immediate_manager_deadline_fires` (I4).
- An audit never evaluates: `audit_never_evaluates` (I1), with a counting command.
- A result stored inside the folder it lists does not expire itself (its name is already a recipe
  name, so membership is unchanged): `index_inside_listed_folder_does_not_self_expire` (I1).
- Build matrix and wasm32: `bash scripts/check-build-matrix.sh` (`run_inline`, `submit`,
  `wait_for_dependency` must compile on wasm32). `liquers-web` `.d.ts` stubs: `check-stubs.sh`.
- Two environments over one store see each other's persisted reasons: `two_envs_share_persisted_reason` (I1).

## Documentation and Learning Log

### Guide Candidate Workflows and Examples

**1. Implementing a manager outside core** (`ASSET_MANAGER_IMPLEMENTATION_GUIDE.md`, snippet from
`tests/external_asset_manager.rs`). Shape only; it holds one graph, hands it out, and drives assets
through the public lifecycle primitives:

```rust
pub struct MinimalInlineAssetManager<E: Environment> {
    envref: EnvRef<E>,
    graph: DependencyManager<E>,                 // created once, never called into directly
    assets: scc::HashMap<Key, AssetRef<E>>,      // at most one asset per key
    policy: DependencyAuditPolicy,
}
impl<E: Environment> DependencyManagerAccess<E> for MinimalInlineAssetManager<E> {
    fn dependency_manager(&self) -> &DependencyManager<E> { &self.graph }
}
#[async_trait]
impl<E: Environment> AssetManager<E> for MinimalInlineAssetManager<E> {
    fn dependency_audit_policy(&self) -> DependencyAuditPolicy { self.policy } // honour the option
    async fn start(&self) -> Result<(), Error> { self.refresh_command_versions().await; Ok(()) }
    // get/get_asset/...: create with AssetData::new(..).to_ref(), evaluate with asset.run_inline(None);
    // lazy deadline: if asset.expiration_time().await.is_expired() {
    //     asset.expire_without_cascade(ExpiryReason::Deadline { expiration_time }).await?; }
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

### Usage and Meaning

- An audit compares two facts: what a result *recorded* about its inputs and what the inputs are
  *now*. Before this design, the second fact was unavailable to a process that had not seen the input.
- `OnLoad` is the strict mode (`the strict service`), `Explicit` the exploratory mode. Neither reads
  values; both read metadata or a folder listing. That is why "delete intermediates, keep results"
  still works under `Explicit`.
- `expiry_reason` is a record for people and tools; nothing branches on it. It is meaningful only
  while the status is `Expired`.
- Part G is what makes Part A honest for files edited outside Liquers.

### Repeatable Development Guidance

- Prove restart behaviour with `StoreSnapshot` and a **new** environment; a same-process test cannot
  show a missing version map.
- Observe expiry through the store's metadata, not `get_asset_info(&Key)` and not `get()` (a fresh
  `evaluate` may recompute, and `get` on `Expired` errors); await `get()` on the producer first, as
  `keyed_version_cascade.rs` documents.
- A test for a pure decision (`external_change_action`, `listing_version`) is a plain `#[test]`
  with an explicit `Status` match; do not write `_ =>` on Liquers enums, in tests either.
- Put any manager-contract test in `tests/common/manager_scenarios.rs` so an external manager runs it.
- Diagnostics from library code go to `eprintln!`, never `println!`.

### Corrections and Unexpected Learning

- **Unknown-expecting edges (correction to the unit-test draft).** The draft claimed `audit_version`
  spares dependents whose edge records `Version::unknown()`. Phase 2 is the opposite: `audit_version`
  inserts the version then calls `expire_stale_dependents`, which spares only on positive evidence,
  so an unknown edge is expired (attribution edges are all unknown, `dependencies.rs:180-195`,
  `:612`). Only `report_no_version` spares them. The plan now has both tests, and a note in the guide.
- **Doc comment to fix in Phase 4.** `Version::unknown()` (`metadata.rs:28`) says dependency checks treat
  it "as compatible with any known version". That is true of `version_consistent` and of the
  missing-version path, but not of the cascade or the audit. Phase 5 should reword it.
- **`report_no_version` return shape.** The draft assumed `(ExpiredDependents, findings)`. Phase 2
  only says it gets findings through `stale_edges(key, None)` and sets `trigger`; the tests assert those
  two things, so they hold whichever shape Phase 4 picks.
- **`AssetManagerKind` is a type parameter, not an enum.** The draft used `AssetManagerKind::Minimal` and
  `with_asset_manager_kind(..)`. The real form is a marker type (`MinimalKind`) as the builder's third
  type parameter (`EnvironmentBuilder::<V, P, K>`), as in the sketch above.
- **`set_binary` takes `MetadataRecord`, and an ordinary `store.set(..)` records no version.** The
  draft simulated Liquers writing `V2` with `store.set(.., &Metadata::new())`, which would record
  none. Example 1 uses `set_binary`; Example 2 uses `store.set` only to model an *outside* edit.
- **A `Source` cannot be expired**, so the stale-dependency test needs a computed dependency.
- **`get_asset_info` may evaluate**; the drafts read reasons through it. Replaced by store metadata.
- **A hand edit is found only when read.** Loading `report.txt` under `explicit` never touches
  `a.csv`, so the edit is found by reading `a.csv`, by `verify_stored_versions`, or not at all.
- Draft errors removed: `VersionCheck` has no serde; `Metadata::expiry_reason()` returns an owned
  `Option<ExpiryReason>`; a `changed` entry holds an `ExternalChangeAction`, not a policy; a
  fabricated `store.capabilities().supports_write` call was dropped; recipes are `query/filename`,
  not `.../summarize/data/report.txt`.
- **To verify in Phase 5:** the real wording of each reason's log line for `ASSETS.md`; whether a
  transitive dependent expired by an audit is best recorded as `Audit` (what the single-reason
  `expire_dependencies_result_with` gives) or `Cascade` (an owner decision; the tests record what the single-reason signature gives).

## Test Plan

### Unit Tests

Unit tests live beside the code in `#[cfg(test)] mod tests`. The dependency manager's methods become
`pub(crate)` under Part F, which is why the `dependencies.rs` tests must be in-crate. Types below:
`type TestEnv = SimpleEnvironment<Value>;` with `a = DependencyKey::new("-R/a")`, `b = DependencyKey::new("-R/b")`.

**`liquers-core/src/dependencies.rs`**
- `audit_version_first_observation_expires_mismatched_dependent`: the Part A regression (sketch below).
- `audit_version_spares_dependent_with_equal_recorded_version`: positive evidence spares.
- `audit_version_expires_unknown_expecting_edge`: the correction (sketch below).
- `report_no_version_spares_unknown_expecting_edge`: the other path (sketch below).
- `audit_version_returns_findings_for_direct_edges_only`: `findings` has the direct edge, `keys` the transitive set.
- `audit_version_sets_trigger`: `expired.trigger == Some(a)`, also when nothing expired.
- `stale_edges_is_read_only`: two calls agree; `get_version(a)` is still `None` (sketch below).
- `stale_edges_none_reports_concrete_edges_only`: `found: None`; unknown-expecting edges omitted.
- `register_version_first_registration_still_expires_nothing`: the evaluation path is unchanged.
- `expired_dependents_for_trigger_sets_trigger`; `expired_dependents_new_has_no_trigger`.
- `listing_version_is_order_independent`; `listing_version_length_prefix_prevents_collision`
  (`["ab","c"]` vs `["a","bc"]`); `listing_version_distinguishes_empty_list_from_empty_name`;
  `listing_version_of_50k_names` (corner case, Memory).
- `audit_concurrent_with_register` (corner case, Concurrency).

**`liquers-core/src/metadata.rs`**
- `expiry_reason_round_trips_json_and_yaml` (all five); `expiry_reason_is_internally_tagged_snake_case`.
- `expiry_reason_log_entry_level_and_subject`: `Info` for `Deadline`/`Explicit`/`Cascade`, `Warning` for
  `Audit`/`StaleDependency`; the message contains the keys and no numeric id.
- `record_without_expiry_reason_loads`; `non_expired_record_json_is_unchanged`;
  `expired_record_serializes_reason`; `unknown_field_record_degrades_to_legacy`.
- `expiry_reason_accessor_hides_reason_unless_expired` (owned `Option`).
- `asset_info_projects_expiry_reason`: both `From<MetadataRecord>` and `get_asset_info`.
- `version_from_content_sets_hash_flag`; `version_from_bytes_leaves_flag_clear`;
  `time_and_unique_versions_mask_bit_127`.
- `verify_matches_own_bytes`; `verify_reports_mismatch_with_actual`; `legacy_unflagged_hash_verifies_when_unchanged`;
  `unflagged_mismatch_is_not_verifiable`; `unknown_is_not_verifiable`.
- `flagged_version_round_trips_through_hex`; `dependency_key_is_store_resolvable`.

**`liquers-core/src/assets.rs`**
- `external_change_action_decision_table` (sketch below); `external_change_action_statuses_not_checked` if the
  function is called for them (see hand-off question).
- `audit_finding_new_and_report_default_assignment`: an external manager can build a report.
- `mark_expired_status_persists_reason_with_status`: metadata read back carries both.
- `each_route_sets_its_reason`: deadline, explicit, cascade, audit, stale dependency.
- `register_plan_dependencies_adds_unknown_edge_for_unregistered_key`: pins the new behaviour.
- `dependency_blocks_fast_track_is_false_for_listing_key`.
- `on_load_refuses_fast_track_on_mismatch` / `_on_missing_version` / `_on_store_error`;
  `explicit_ignores_unknown_map_entries`.
- `immediate_manager_lazy_deadline_expires_with_deadline_reason`.

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
    assert_eq!(expired.keys, vec![b.clone()]);             // register_version would expire nothing
    assert_eq!(expired.trigger, Some(a.clone()));
    assert_eq!(findings, vec![AuditFinding::new(a.clone(), b.clone(), Version::new(1), Some(Version::new(2)))]);
    Ok(())
}

#[tokio::test]
async fn audit_version_expires_unknown_expecting_edge() -> TestResult {
    let dm = DependencyManager::<TestEnv>::new();
    dm.add_dependency(&b, &a, Version::unknown()).await?;  // e.g. an attribution edge
    let (expired, _) = dm.audit_version(&a, Version::new(99)).await;
    assert_eq!(expired.keys, vec![b.clone()], "no positive evidence: not spared");
    Ok(())
}

#[tokio::test]
async fn report_no_version_spares_unknown_expecting_edge() -> TestResult {
    let dm = DependencyManager::<TestEnv>::new();
    dm.add_dependency(&b, &a, Version::unknown()).await?;
    let expired = dm.report_no_version(&a).await;          // shape per Phase 4; only keys/trigger used
    assert!(expired.keys.is_empty());
    assert!(dm.stale_edges(&a, None).await.is_empty());
    Ok(())
}

#[tokio::test]
async fn stale_edges_is_read_only() -> TestResult {
    let dm = DependencyManager::<TestEnv>::new();
    dm.add_dependency(&b, &a, Version::new(1)).await?;
    let first = dm.stale_edges(&a, Some(Version::new(2))).await;
    assert_eq!(first, dm.stale_edges(&a, Some(Version::new(2))).await);
    assert_eq!(dm.get_version(&a).await, None);            // nothing was registered
    Ok(())
}

#[test]
fn external_change_action_decision_table() {
    let (v, ui, co) = (Version::from_content(b"x"), ExternalChangePolicy::UserInput, ExternalChangePolicy::Corrupted);
    for policy in [ui, co] {
        assert_eq!(external_change_action(Status::Source, false, policy, v), ExternalChangeAction::AcceptAsInput { actual: v });
        assert_eq!(external_change_action(Status::Override, true, policy, v), ExternalChangeAction::AcceptAsInput { actual: v });
    }
    for status in [Status::Ready, Status::Expired] {       // recipe-backed
        assert_eq!(external_change_action(status, true, ui, v), ExternalChangeAction::ConvertToOverride { actual: v });
        assert_eq!(external_change_action(status, true, co, v), ExternalChangeAction::Delete);
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
- C: `every_route_persists_its_reason` (queued deadline with a short `expires: "in 1 sec"`,
  cascade, explicit, audit, stale dependency, each read from the store's metadata, log entries naming
  keys); `audit_expires_transitive_dependents` (records the reason a transitive dependent gets).
- D: `adding_a_file_expires_the_index`; `listing_gap_resolved_by_audit_after_restart`;
  `plan_dependency_without_version_gets_unknown_edge_then_upgrade`;
  `content_change_does_not_move_listing_version`; `index_inside_listed_folder_does_not_self_expire`.
- E: `stale_dependency_end_to_end_queued`, `stale_dependency_end_to_end_immediate` (gate command).
- Corner cases: the I1 names in the section above.

**`external_change_integration.rs`** (Part G)
- `hand_edited_source_is_input_and_expires_dependents` (Example 2);
  `recipe_backed_edit_follows_policy` (both policies, one helper); `override_is_never_deleted`;
  `file_with_no_metadata_and_no_recipe_is_not_a_change`; `file_with_no_metadata_under_recipe_follows_policy`;
  `verify_stored_versions_report_only_changes_nothing`; `verify_stored_versions_applies_policy`;
  `legacy_unchanged_value_still_verifies`; `audit_alone_does_not_see_unread_hand_edit`;
  `read_only_store_accepts_in_memory_and_does_not_fail_the_read`; `verify_reads_the_store_once`;
  `missing_bytes_are_skipped`; `concurrent_reads_apply_external_change_once`.

**`external_asset_manager.rs`, `common/manager_scenarios.rs`, `manager_parametric.rs`** (Part F)
- `manager_scenarios.rs` receives the generic bodies now in `manager_parametric.rs`
  (`scenario_basic_eval`, `scenario_cache_and_mode`, ...) plus new, manager-independent ones:
  `scenario_audit_after_restart`, `scenario_expiry_reason_cascade`, `scenario_listing_dependency`,
  `scenario_stale_dependency`.
- `manager_parametric.rs` runs all scenarios over `DefaultAssetManager` and `ImmediateAssetManager`.
- `external_asset_manager.rs` defines `MinimalInlineAssetManager<E>` from scratch (not a wrapper of
  `ImmediateAssetManager`) and `MinimalKind`, and runs the same scenarios:
  `external_manager_passes_shared_scenarios`, `external_manager_registers_one_asset_per_key`,
  `external_manager_honours_audit_policy`. Any primitive found missing while writing it is evidence
  for Phase 5.

**`expiration_integration.rs`** (changed): `immediate_manager_deadline_fires` (problem 6): register a
command with `expires: "in 1 sec"`, request, wait, request again, assert a recompute and a
`Deadline` reason. Existing tests stay unchanged; `keyed_version_cascade.rs` updates its
`AuditReport` equality asserts for `findings`.

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
validate. `liquers-web`: `./liquers-web/scripts/check-stubs.sh` after `cargo clean`, for the new
`AssetInfo` field.

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
    async fn audit_version_sets_trigger() -> Result<(), Box<dyn std::error::Error>> {
        let dm = DependencyManager::<TestEnv>::new();
        let (a, b) = (DependencyKey::new("-R/a"), DependencyKey::new("-R/b"));
        dm.add_dependency(&b, &a, Version::new(7)).await?;

        let (expired, findings) = dm.audit_version(&a, Version::new(7)).await;

        assert!(expired.keys.is_empty(), "equal recorded version is positive evidence");
        assert!(findings.is_empty());
        assert_eq!(expired.trigger, Some(a));
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
        .with_async_store(store)
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
            ExternalChangeAction::AcceptAsInput { actual } => assert_eq!(actual, v),
            ExternalChangeAction::ConvertToOverride { .. } => unreachable!("Source is never converted"),
            ExternalChangeAction::Delete => unreachable!("Source is never deleted"),
        }
    }
}
```
