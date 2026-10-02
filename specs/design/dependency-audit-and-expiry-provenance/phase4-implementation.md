# Phase 4: Implementation Plan - Dependency Audit and Expiry Provenance

## Overview

**Feature:** Dependency audit correctness, audit policy, expiry provenance, outside-change detection
and external asset managers (Phase 1; Parts A–G of Phase 2).

**Architecture:** All in `liquers-core`, plus one compatibility line in `liquers-axum`.
- **Versions:** they say whether they are a content hash. Stored bytes are re-hashed on read.
- **Expiry reasons:** typed `Direct` / `Cascaded` reasons, written to every expired asset's log
  through `AssetManager::record_expiry`.
- **Audits:** compare current with recorded versions on demand, or on load.
- **Folder listings:** each listing has a version.
- **Commands:** `Context::submit` + `wait_for_dependency`.
- **External managers:** the `AssetManager` trait is unsealed.

**Estimated complexity:** High. The changes are many but mostly mechanical. The difficult parts are
the cascade signature change that touches every expiry route (Step 4) and the shared fast-track path
(Steps 7 and 9).

**Estimated time:** 5–7 working days for an experienced Rust developer familiar with
`assets.rs`. With the agent split below: about 14 agent steps, each closed by a green build.

**Prerequisites:**
- Phases 1–3 approved (2026-09-29, 2026-10-02); every owner decision is recorded in Phase 2's gate
  decisions, "Revision 2" and "Revision 2, clarifications".
- No blocking issue (Phase 2 §"Known-Issue Preflight").
- Branch `claude/elegant-feynman-8hci6u` up to date with `main` (merged 2026-10-02, commit
  `8e85e55`). Re-merge before Step 1 if `main` has moved, and re-check the `assets.rs` line numbers
  cited below. They are from that merge.
- No new crate dependencies: `blake3`, `scc`, `serde` and `tokio` are already used by `liquers-core`.

**Ground rules for every step** (from `CLAUDE.md`):
- No `unwrap`/`expect` outside tests.
- No `_ =>` on Liquers enums.
- Errors through typed constructors.
- `eprintln!`, never `println!`.
- Async by default.
- Each step ends with a build and test command that must pass before the next step starts, and with
  one commit. Commit messages name the step: `dependency-audit step N: …`.

**Build command used throughout** (the cloud disk limit rules out `--workspace`; see `CLAUDE.md`
§"Building and testing"):

```bash
CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib --tests
```

## Implementation Steps

### Step 0: Mark the issues as being worked on

**Files:** the eight issue records named in `DESIGN.md` `issues:`
(`AUDIT-CANNOT-EXPIRE-ON-A-FIRST-OBSERVED-VERSION`, `DEPENDENCY-AUDIT-POLICY-NOT-EXPRESSIBLE`,
`EXPIRY-RECORDS-NO-REASON`, `DIRECTORY-LISTING-DEPENDENCY-IS-NEVER-REGISTERED-OR-CHECKED`,
`STALE-DEPENDENCY-PATH-HAS-NO-END-TO-END-TEST`, `IMMEDIATE-MANAGER-LAZY-DEADLINE-EXPIRY-NEVER-FIRES`,
`ASSET-MANAGER-TRAIT-CANNOT-BE-IMPLEMENTED-OUTSIDE-CORE`,
`STORE-VERSION-BLIND-TO-CHANGES-MADE-OUTSIDE-LIQUERS`).

**Action:**
- Set `status: in_progress` on each, following `DOCS_STRUCTURE_GUIDE.md` §4.3.
- Regenerate the index with `python3 scripts/docs_index.py`.
- Update `DESIGN.md`: Phase 4 approved, and implementation started.

**Validation:**
```bash
python3 scripts/docs_index.py --check   # 0 errors
```

**Rollback:** `git revert` the commit.

**Agent Specification:**
- **Model:** haiku
- **Skills:** none
- **Knowledge:** `specs/DOCS_STRUCTURE_GUIDE.md` §4.3; this folder's `DESIGN.md`.
- **Rationale:** front-matter edits only.

---

### Step 1: Version kinds and verification (Part G1)

**File:** `liquers-core/src/metadata.rs` (`Version`, lines 17–90)

**Action:**
- Add `Version::HASH_FLAG`, `from_content`, `kind`, `verify`, and the enums `VersionKind`
  (`Unknown`, `ContentHash`, `Timestamp`) and `VersionCheck` (`Verified`,
  `Mismatch { actual, recorded }`). Use exactly the Phase 2 G1 signatures and semantics.
- Mask bit 127 to 0 in `from_time_now`, `from_specific_time` and `new_unique`.
- `verify` implements the legacy rule: an unflagged recorded value equal to `from_bytes(bytes)` is
  `Verified`.
- Leave `from_bytes` unchanged.
- Fix the `Version::unknown()` doc comment (`metadata.rs:26-28`). It says unknown is "compatible
  with any version". State instead where that holds (`matches`, the in-process fast-track check)
  and where it does not (cascades, audits, Part G).

**Code changes:**
```rust
impl Version {
    pub const HASH_FLAG: u128 = 1 << 127;
    pub fn from_content(bytes: &[u8]) -> Self;        // from_bytes(bytes).0 | HASH_FLAG
    pub fn kind(&self) -> VersionKind;
    pub fn verify(&self, bytes: &[u8]) -> VersionCheck;
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum VersionKind { Unknown, ContentHash, Timestamp }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionCheck { Verified, Mismatch { actual: Version, recorded: VersionKind } }
```

**Tests (unit, `metadata.rs` `mod tests`):**
- `from_content_sets_hash_flag`
- `from_bytes_is_unchanged`
- `time_and_unique_versions_never_carry_the_flag`
- `unknown_is_never_a_hash`
- `verify_matches_own_content`
- `verify_reports_changed_content`
- `timestamp_or_zero_recorded_is_a_mismatch`
- `legacy_unflagged_hash_verifies_when_unchanged`
- `legacy_changed_value_is_a_mismatch`
- `kind_of_each_constructor`

(Phase 3 §"Unit Tests", `metadata.rs` group.)

**Validation:**
```bash
CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib metadata
```

**Rollback:** `git checkout liquers-core/src/metadata.rs`. The change is self-contained, because nothing calls the new
API yet.

**Agent Specification:**
- **Model:** haiku
- **Skills:** rust-best-practices, liquers-unittest
- **Knowledge:** Phase 2 Part G1; `metadata.rs:17-90`.
- **Rationale:** pure functions with a fully specified contract.

---

### Step 2: Expiry reason types and their place in metadata (Part C data)

**File:** `liquers-core/src/metadata.rs`

**Action:**
- Add `ExpiryCause` (7 variants) and `ExpiryReason` (`Direct`, `Cascaded`), with the serde attributes
  from Phase 2: internally tagged, `"kind"` / `"scope"`, `snake_case`.
- Add `ExpiryReason::log_entry(&self, subject: &str) -> LogEntry`. It implements the message rule
  (name `via` iff `via != root`) and the per-cause levels from the Phase 2 scope table.
- Add the field `expiry_reason: Option<ExpiryReason>` with
  `#[serde(default, skip_serializing_if = "Option::is_none")]` to `MetadataRecord` (around `:911`)
  and `AssetInfo` (`:678`). Project it in `MetadataRecord::get_asset_info` (`:1160`).
- Add `Metadata::expiry_reason(&self) -> Option<ExpiryReason>`, which returns `Some` only for a
  record whose status is `Expired`.
- Add `Metadata::set_expiry_reason(&mut self, reason: ExpiryReason) -> Result<(), Error>`, which
  returns an error on legacy metadata, as `add_log_entry` does.
- Do the same for `AssetInfo` where it is built field by field in `assets.rs`, if any site is not
  built through `get_asset_info`. Check with
  `grep -n "AssetInfo {" liquers-core/src`.

**Tests (unit):**
- `expiry_reason_round_trips_json_and_yaml_for_every_cause_and_scope`
- `metadata_record_without_expiry_reason_loads`
- `non_expired_record_serializes_unchanged`: compare byte for byte with a fixture serialized before
  this step.
- `expiry_reason_hidden_unless_expired`
- `log_line_format_per_cause`
- `log_line_names_via_only_when_it_differs_from_root`
- `log_line_names_keys_not_asset_ids`

**Validation:** as Step 1, plus `cargo test -p liquers-lib --lib --tests`, because `liquers-lib`
consumes `AssetInfo`.

**Rollback:** `git checkout liquers-core/src/metadata.rs`. No caller yet.

**Agent Specification:**
- **Model:** haiku
- **Skills:** rust-best-practices, liquers-unittest
- **Knowledge:** Phase 2 §"`ExpiryCause` and `ExpiryReason`" and §"Serialization Strategy";
  `metadata.rs` around `MetadataRecord` and `AssetInfo`; `expiration.rs:751-775` (the hand-written
  serde of `ExpirationTime`).
- **Rationale:** data types and serde, following an existing pattern (`stored`, `cached`).

---

### Step 3: Cascade bookkeeping in the dependency manager (Parts A, C graph side)

**File:** `liquers-core/src/dependencies.rs`

**Action:**
- Restructure `ExpiredDependents` (`:69`) to `{ root, keys: Vec<ExpiredKey>, assets: Vec<(WeakAssetRef<E>, DependencyKey)> }`.
  Add `ExpiredKey { key, via }`, `new()` and `for_root(key)`.
- In `expire_from_frontier` (`:738`), record the predecessor when a key is first queued. Frontier
  keys get `via = root`.
- Add `pub(crate) async fn audit_version(&self, key, version: Version) -> (ExpiredDependents<E>, Vec<AuditFinding>)`.
  For a version of 0 it dispatches to `report_no_version`'s rules.
- Add `pub(crate) async fn stale_edges(&self, key, version: Version) -> Vec<AuditFinding>`, which is
  read-only.
- Add `pub(crate) fn listing_version(names: &[String]) -> Version`: `from_content` over the sorted,
  length-prefixed names.
- `register_version`, `expire`, `report_no_version` and `expire_stale_dependents` fill `root`.
  `register_version` itself is **not** changed in behaviour.
- `AuditFinding` is defined in `assets.rs` (Step 6). To keep this step compiling, define it here
  first as `pub struct AuditFinding` and re-export it from `assets` in Step 6. Alternatively, do
  Steps 3 and 6 in one agent run.
- Callers in `assets.rs` that read `expired.keys` as a `Vec<DependencyKey>` are adapted
  mechanically (`.key`). No cause is introduced yet, so this step changes no behaviour outside the
  graph.

**Tests (unit, `dependencies.rs` `mod tests`):**
- `audit_version_first_observation_expires_mismatched_dependent`
- `audit_version_spares_equal_concrete_version`
- `audit_version_expires_unknown_expecting_edge`
- `audit_version_zero_spares_unknown_expecting_edge`
- `stale_edges_mutates_nothing`
- `expire_from_frontier_records_via_per_key`
- `via_takes_the_shortest_path_in_a_diamond`
- `listing_version_is_order_independent`
- `listing_version_resists_concatenation_collisions`
- `listing_version_of_empty_listing_is_stable_and_flagged`
- `register_version_vacant_still_expires_nothing`, which pins the evaluation path unchanged.

**Validation:**
```bash
CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib dependencies
CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib --tests      # nothing else regressed
```

**Rollback:** `git checkout liquers-core/src/dependencies.rs liquers-core/src/assets.rs`.

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices, liquers-unittest
- **Knowledge:** `dependencies.rs` (whole file); Phase 2 §"`ExpiredDependents<E>`",
  §"`DependencyManager<E>`", Part A trace (Example 1); the unknown-edge rule
  (`dependencies.rs:180-195`, `:612`).
- **Rationale:** a graph algorithm change with a subtle sparing rule. It needs judgment, not only
  pattern-following.

---

### Step 4: Every route into `Expired` records its reason (Part C)

**File:** `liquers-core/src/assets.rs` (the core of the design)

**Action:**
- Trait `AssetManager`:
  - Add `fn record_expiry(&self, metadata: &mut Metadata, subject: &str, reason: &ExpiryReason)`,
    with a default body: `set_expiry_reason` + `add_log_entry(reason.log_entry(subject))`.
  - Change `expire_dependencies_result(&self, expired, cause: ExpiryCause)` (`:4865`): each asset
    gets `Cascaded { cause, root, via }`.
  - Change `cascade_expire_dependents(&self, key, cause)` (`:4854`).
- `AssetRef`:
  - `mark_expired_status(&self, reason: ExpiryReason)` (`:3316`) calls `record_expiry` **under the
    `data` write lock, before `persist_info` is cloned**.
  - `expire()` (`:3299`) becomes `expire_with_reason(Direct { Explicit })`.
  - `expire_without_cascade(reason)` (`:3388`).
- `expire_stored_copy` (`:3880`) takes the manager and the reason, and calls `record_expiry` on the
  metadata it read before `set_metadata`.
- `AssetManager::expire(key)` (`:4290`), stored-only branch (`:4318-4324`): call `record_expiry` with
  `Direct { Explicit }` before `set_metadata`.
- `AssetData::stale_dependency: bool` (`:600`) becomes `Option<DependencyKey>`.
  `note_expired_dependency` (`:1621`) records the dependency's key or query and logs by key, not by
  `id()`. The finalize stale branch (`:2106`) produces `Direct { StaleDependency { dependency } }`,
  and its `cascade_expire_dependents` call passes `StaleDependency`.
- Queued expiration monitor (`:5214`): `expire_with_reason(Direct { Deadline { expiration_time } })`.
- Pass a cause at **every** call site. The full list (verified at HEAD) is in Phase 2 §"Call sites
  that pick a reason": `:1218`, `:1223`, `:1734`, `:2927`, `:2934`, `:3309`, `:4261`, `:4326`,
  `:4748`, `:4828`, `:4856`, `:4896`, `:6151`, `:6296`, `:6300`, `:7383`, `:7456`, `:7459`.
  The compiler lists any that are missed, since the signature changes.

**Tests:**
- Unit, in `assets.rs` `mod tests`:
  - `mark_expired_status_persists_reason_with_status`
  - `stale_dependency_records_dependency_key_not_id`
  - `explicit_expire_of_stored_copy_records_reason`
- New integration file `liquers-core/tests/expiry_provenance_integration.rs` (Phase 3 I5):
  - `record_expiry_is_called_for_every_expired_asset`
  - `every_cause_writes_a_log_line` (table-driven, all seven causes; the `Audit` and
    `UpdatedInStore` rows are added in Steps 6 and 9, and until then they are `#[ignore]` with a
    comment naming the step)
  - `log_line_is_persisted_with_the_status`
  - `removing_a_source_cascades_with_removed`
  - `set_binary_of_a_dependency_cascades_with_updated`
  - `via_names_the_direct_dependency_on_a_two_step_cascade`
  - `reader_never_sees_expired_without_reason`

**Validation:**
```bash
CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib --tests
CARGO_INCREMENTAL=0 cargo test -p liquers-core --test expiration_integration   # no regression
CARGO_INCREMENTAL=0 cargo test -p liquers-core --test keyed_version_cascade
```

**Rollback:** `git checkout liquers-core/src/assets.rs` plus the new test file. Steps 1–3 stay.

**Agent Specification:**
- **Model:** opus
- **Skills:** rust-best-practices, liquers-unittest
- **Knowledge:**
  - Phase 2 Part C in full: the scope table, the message rule, the call-site table and
    §"Concurrency Considerations".
  - `assets.rs` around every cited line.
  - The lock-ordering history in `specs/design/stale-dependency-status-finalization/`
    (`ASSET-STALE-DEPENDENCY-PERSISTED-AS-READY`).
- **Rationale:** 18 call sites across both managers, a lock-ordering invariant, and the status
  authority. A mistake here reintroduces the persisted-as-ready class of bug.

---

### Step 5: The immediate manager's deadline check (Part C, `IMMEDIATE-MANAGER-LAZY-DEADLINE-EXPIRY-NEVER-FIRES`)

**File:** `liquers-core/src/assets.rs` (`:7218`, `:7306`)

**Action:**
- Replace `status == Status::Ready && assetref.is_expired().await` with
  `status == Status::Ready && assetref.expiration_time().await.is_expired()`.
- Call `expire_without_cascade(Direct { Deadline { expiration_time } })`.
- Keep the non-cascading behaviour, as decided in Phase 2, and add a comment that points to the
  issue for the cascade question.

**Tests:** in `liquers-core/tests/expiration_integration.rs`, `immediate_manager_deadline_fires`
(Phase 3 pitfall 11 / I4), which uses `expires: "in 1 sec"` on an `ImmediateEnvironment` and a short
wait. Phase 3 suggests a test clock; the crate has none, so a sleep of about 1.2 s is used.

**Validation:**
```bash
CARGO_INCREMENTAL=0 cargo test -p liquers-core --test expiration_integration immediate_manager_deadline_fires
```

**Rollback:** `git checkout` the two hunks.

**Agent Specification:**
- **Model:** haiku
- **Skills:** liquers-unittest
- **Knowledge:** the issue file; `assets.rs:7200-7320`; `expiration.rs:813-830`.
- **Rationale:** a one-line fix plus one test.

---

### Step 6: Audits: current version as `Version`, findings, report-only (Parts A, B)

**Files:** `liquers-core/src/assets.rs`, `liquers-core/src/metadata.rs`,
`liquers-axum/src/assets/common.rs`, `liquers-core/tests/keyed_version_cascade.rs`

**Action:**
- `AssetManager::version` (`:4777`) changes to `-> Result<Version, Error>`, returning `Version::unknown()` where
  it returned `None`.
- Add `dependency_version(&self, &DependencyKey) -> Result<Version, Error>`. `-R-dir/` keys return
  their listing version (Step 8 adds the real listing; until then they return 0).
- Add `DependencyKey::is_store_resolvable`.
- Add `AuditMode`, `AuditFinding` (with `new`, `#[non_exhaustive]`), and `AuditReport.findings`
  with `#[non_exhaustive]`.
- Add `trigger_dependency_audit_with` and `trigger_dependency_audit_all_registered_with`. The
  existing two methods delegate with `AuditMode::Expire`.
- Rewrite `audit_gaps` (`:4726`) over `dependency_version` + `audit_version` (or `stale_edges` in
  `ReportOnly`). Expire with cause `Audit { found }`.
- `liquers-axum/src/assets/common.rs:267`: drop `.unwrap_or_else(Version::unknown)`. The behaviour
  is identical and this is a compile fix only.
- `keyed_version_cascade.rs:341` compares `AuditReport::default()`. Keep that comparison,
  because `default()` stays available.

**Tests:**
- Unit:
  - `version_of_absent_key_is_unknown` (renamed from `version_of_an_absent_key_is_none`)
  - `store_error_is_not_unknown`
  - `is_store_resolvable_per_key_form`
- In `liquers-core/tests/dependency_audit_integration.rs` (new, Phase 3 I1):
  - `audit_after_restart_expires_dependent`. This is the withdrawn test R2 from the issue.
  - `report_only_audit_changes_nothing`
  - `audit_never_expires_the_root`
  - `audit_never_evaluates`
  - `unloaded_dependent_is_refused_on_later_load`
- Un-ignore the `Audit` row of `every_cause_writes_a_log_line`.

**Validation:**
```bash
CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib --tests
CARGO_INCREMENTAL=0 cargo check -p liquers-axum
```

**Rollback:** `git checkout` the four files.

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices, liquers-unittest
- **Knowledge:**
  - Phase 2 Part A, Part B and §"Trait Implementations".
  - `assets.rs:4700-4830`.
  - The issue `AUDIT-CANNOT-EXPIRE-ON-A-FIRST-OBSERVED-VERSION` (its reproduction).
  - `keyed_version_cascade.rs` for the cross-process pattern.
- **Rationale:** API changes visible to axum, and test scenarios across two environments.

---

### Step 7: Policies and their accessors; the `on_load` check (Part B, G3 options)

**Files:** `liquers-core/src/environment_builder.rs`, `liquers-core/src/assets.rs`

**Action:**
- Add `DependencyAuditPolicy { Explicit, OnLoad }`, `VersionVerification { Off, OnRead }` and
  `ExternalChangePolicy { UserInput, Corrupted }`, each with its `is_<default>` helper for
  `skip_serializing_if`.
- Add fields to `AssetManagerOptions` (`:48`), plus `with_*` builder methods.
- Store the three values in `DefaultAssetManager` and `ImmediateAssetManager` at `build`
  (`environment_builder.rs:111-150`).
- Add the trait accessors `dependency_audit_policy()`, `version_verification()` and
  `external_change_policy()`, with defaults.
- In `try_fast_track` (`:1178-1200`), under `OnLoad`, resolve each recorded dependency that the map
  does not know through `dependency_version`.
  - A mismatch refuses the fast track.
  - A current version of 0 against a concrete recorded version refuses it too.
  - A recorded `unknown` is compatible (`Version::matches`, as at `:1186`).
  - A store error refuses as well.

**Tests:**
- Unit: `asset_manager_options_yaml_with_and_without_new_fields`.
- Integration, in `dependency_audit_integration.rs`:
  - `on_load_refuses_stale_fast_track`
  - `on_load_refuses_when_dependency_vanished`
  - `on_load_treats_recorded_unknown_as_compatible`
  - `explicit_policy_serves_when_intermediate_deleted`
- `liquers-core/tests/environment_builder.rs`: an `EnvironmentConfig` YAML with
  `assets: {dependency_audit: on_load, verify_versions: off, external_change: corrupted}`
  parses and round-trips.

**Validation:** the build command, and `cargo test -p liquers-core --test environment_builder`.

**Rollback:** `git checkout` the two files and the test additions.

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices, liquers-unittest
- **Knowledge:**
  - Phase 2 Part B and G3.
  - Phase 2 "Corrections made during Phase 3", items 1 and 3.
  - `environment_builder.rs:40-160`; `environment_config.rs`; `assets.rs:1112-1240`.
- **Rationale:** touches the fast-track path that every load shares.

---

### Step 8: Folder-listing dependencies (Part D)

**Files:** `liquers-core/src/assets.rs`, `liquers-core/src/interpreter.rs`

**Action:**
- `register_plan_dependencies` (`:4883`): add an edge with `Version::unknown()` when there is no
  version, instead of skipping it.
- `Step::GetAssetDirectory` (`interpreter.rs:780`): after `listdir_asset_info`, compute
  `listing_version(asset_manager.listdir(&key))`. Then:
  - `register_version` it and apply the result with cause `Updated`;
  - `context.add_dependency(DependencyRecord::new(dir_key, v))`;
  - `dm.add_dependency(owner, dir_key, v)` when the context has an owner key.
- Add `refresh_listing_version(&self, dir: &Key)` as a default trait method. It acts only if
  `get_version(-R-dir/dir)` is `Some`, and then cascades with `Updated`.
- Call `refresh_listing_version` with the parent key after `save_to_store`, `set_binary`,
  `set_state` and `remove`, in both managers.
- `dependency_version` for `-R-dir/` returns the listing version.
- `dependency_blocks_fast_track` (`:1080`): return an explicit early `false` for a `-R-dir/` key,
  with the comment from Phase 2.

**Tests (integration, `dependency_audit_integration.rs`):**
- `adding_a_file_expires_the_index`
- `listing_gap_resolved_by_audit_after_restart`
- `deleting_bytes_keeps_the_listing_version`
- `unchanged_listing_after_write_expires_nothing`

**Validation:** the build command. Also run
`cargo run -p liquers-core --features cli --bin liquers-validate -- --command index_files -- '-R-dir/data/-/index_files'`.

**Rollback:** `git checkout` the two files.

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices, liquers-unittest, liquers-validate
- **Knowledge:** Phase 2 Part D and §"Directory listing: where it plugs in"; `interpreter.rs:760-800`;
  `plan.rs:2640-2670`; `context.rs:685-720` (the record-upgrade pattern).
- **Rationale:** the interpreter, the asset manager and the dependency manager all take part.

---

### Step 9: Content changed outside Liquers (Part G)

**Files:** `liquers-core/src/assets.rs`

**Action:**
- Switch the five content sites to `from_content`: `:2030`, `:6114`, `:6237`, `:7362`, `:7421`.
- Add `ExternalChangeAction` and `external_change_action(status, has_recipe, policy, actual) -> Option<…>`,
  implementing the full decision table, including the no-metadata rows.
- Add `apply_external_change`, a default trait method. It:
  - bumps the version;
  - writes status and version to the sidecar, **except** for a `Source` with no prior metadata
    (owner decision: kept in memory only);
  - calls `register_version` and cascades with `UpdatedInStore { actual }`;
  - logs on the asset;
  - takes `key_mutation_lock`;
  - when a store write fails (a read-only store), keeps the result in memory and logs to stderr.
- Under `OnRead`, check in `try_fast_track` after deserialization succeeds, and in the store branches
  of `get_any_status` (`:4907`) and `get_binary_any_status` (`:4947`).
- Add `verify_stored_versions(key, deep, mode) -> Result<VersionVerificationReport, Error>` with
  `verified`, `skipped` and `changed`.

**Tests (integration, `liquers-core/tests/external_change_integration.rs`, new, Phase 3 I2):**
- `hand_edited_source_is_input_and_expires_dependents`
- `recipe_backed_edit_follows_policy`
- `override_is_never_deleted`
- `file_with_no_metadata_and_no_recipe_becomes_source_with_hash_version`, which asserts that no
  sidecar is written
- `file_with_no_metadata_under_recipe_follows_policy`
- `legacy_unchanged_value_is_not_converted`
- `legacy_changed_value_is_a_mismatch`
- `read_only_store_keeps_result_in_memory`
- `verify_stored_versions_report_only_changes_nothing`
- `missing_bytes_are_skipped`
- `verification_off_changes_nothing`

Unit: `external_change_action_decision_table`, `source_is_always_input_under_both_policies`.

Un-ignore the `UpdatedInStore` row of `every_cause_writes_a_log_line`.

**Validation:** the build command, and the whole `liquers-lib` test suite
(`cargo test -p liquers-lib --lib --tests`), because every value `liquers-lib` stores now gets a
flagged version.

**Rollback:** `git checkout liquers-core/src/assets.rs` plus the new test file. Steps 1–8 are
unaffected.

**Agent Specification:**
- **Model:** opus
- **Skills:** rust-best-practices, liquers-unittest
- **Knowledge:**
  - Phase 2 Part G in full, including the owner decisions in "Gate decisions" 5, "Revision 2" and
    the clarification on no-metadata files.
  - `assets.rs:1112-1240`, `:4900-4980` and `:6100-6300`.
  - The issue `STORE-VERSION-BLIND-TO-CHANGES-MADE-OUTSIDE-LIQUERS`.
- **Rationale:** it changes what every stored value's version is, and it adds a write on the read
  path. Both are easy to get subtly wrong.

---

### Step 10: Commands can start a dependency and wait for it later (Part E)

**File:** `liquers-core/src/context.rs`

**Action:**
- Add `pub async fn submit(&self, query: &Query) -> Result<AssetRef<E>, Error>`. It calls
  `schedule_dependency_asset_with_key` and remembers `asset.id() → DependencyKey` in a new shared
  `Arc<tokio::sync::Mutex<HashMap<u64, DependencyKey>>>`, beside `pending_dependencies` (`:424`).
  Clone it into every `Context` construction site (`:817`, `:879`, `:1057`, `:1573`).
- Make `wait_for_dependency(&self, &AssetRef<E>)` (`:711`) `pub`. It looks the key up and calls
  `wait_for_dependency_recording`.
- `evaluate` (`:746`) becomes `submit` + drain.
- `get_dependency_state` (`:734`) becomes `submit` + `wait_for_dependency`, with the same behaviour.

**Tests:**
- Unit:
  - `submit_records_the_dependency`
  - `submit_does_not_drain`
  - `wait_for_unknown_asset_records_nothing`
- Integration, `dependency_audit_integration.rs`:
  - `stale_dependency_end_to_end_queued`
  - `stale_dependency_end_to_end_immediate`

  Both use a gate command that submits, blocks on a test-controlled `tokio::sync::Notify`, and then
  waits. The test expires the dependency in between and asserts the parent ends with
  `Direct { StaleDependency }`.

**Validation:** the build command, and `cargo test -p liquers-lib --lib --tests`, because commands
use `Context`.

**Rollback:** `git checkout liquers-core/src/context.rs` plus the test additions.

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices, liquers-unittest
- **Knowledge:**
  - Phase 2 §"`Context<E>` … for Part E".
  - `context.rs:400-760` and `:990-1010`.
  - `assets.rs:5842-5900` (the manager's `wait_for_dependency` and its expired arm).
  - The issue `STALE-DEPENDENCY-PATH-HAS-NO-END-TO-END-TEST`.
- **Rationale:** a public command-facing API, and a concurrency test that must be deterministic.

---

### Step 11: Unseal `AssetManager` (Part F)

**Files:** `liquers-core/src/assets.rs`, `liquers-core/src/dependencies.rs`

**Action:**
- Make `DependencyManagerAccess` (`:4004`) `pub`, and remove `#[allow(private_bounds)]` (`:4038`).
- Make `DependencyManager` (`dependencies.rs:114`) `pub` and **opaque**:
  - narrow every method that is `pub` today to `pub(crate)`, except `new`;
  - add `Default`.
- Make `AssetRef::run` (`:2608`), `run_inline` (`:2668`), `submitted` (`:2324`), `set_payload_path`
  (`:1924`) and `expire_without_cascade` (`:3388`) `pub`, each with a contract doc comment:
  - `run`: the native/queued primitive.
  - `run_inline`: the one to use on wasm32; link `INLINE-DROP-REPAIR-STRANDS-EXISTING-WAITERS`.
- Give `refresh_command_versions` a default trait body, and delete the two identical impls
  (`:6452`, `:7526`).

**Tests:**
- `liquers-core/tests/external_asset_manager.rs` (new, Phase 3 I3) implements
  `MinimalInlineAssetManager` from scratch, plus its `AssetManagerKind`, using only public API.
  - It overrides `record_expiry` to count calls.
  - The test `record_expiry_is_overridable_by_a_manager` uses that count.
- Move the generic scenario bodies of `manager_parametric.rs` to `tests/common/manager_scenarios.rs`
  (`mod common;` in both files). Add `scenario_every_expired_asset_has_reason_and_log_line`, and run
  the shared set from both suites.

**Validation:**
```bash
CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib --tests
CARGO_INCREMENTAL=0 cargo check -p liquers-axum -p liquers-lib
cargo check -p liquers-web --target wasm32-unknown-unknown       # needs the wasm32 target
```

**Rollback:** `git checkout` the two source files, and delete the two new test files and the
common module.

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices, liquers-unittest
- **Knowledge:**
  - Phase 2 Part F and the preflight rows for `ASSET-REGISTRATION-OWNERSHIP-CONTRACT`,
    `ENVIRONMENT-MANAGER-REFERENCE-CYCLE` and `INLINE-DROP-REPAIR-STRANDS-EXISTING-WAITERS`.
  - `ImmediateAssetManager` (`assets.rs:6990-7575`) as the reference for which primitives are
    needed.
  - `manager_parametric.rs`.
- **Rationale:** a public API decision, plus the proof that it is enough. If a primitive is missing,
  record it as Phase 5 evidence and make it public with a contract. Do not reach into `pub(crate)`.

---

### Step 12: Full validation, feature matrix and wasm

**Files:** none, unless a configuration breaks.

**Action:** run the whole matrix, fix only what the change broke, and record anything else as an
issue.

**Validation:**
```bash
CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib --tests
CARGO_INCREMENTAL=0 cargo test -p liquers-lib --lib --tests
CARGO_INCREMENTAL=0 cargo test -p liquers-lib --test registry_export   # no command signature changed
CARGO_INCREMENTAL=0 cargo check -p liquers-axum -p liquers-py
bash scripts/check-build-matrix.sh
cargo clean && cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles
./liquers-web/scripts/check-stubs.sh                                  # AssetInfo gained a field
python3 scripts/docs_index.py --check
```

**Rollback:** not applicable. It changes nothing unless something is broken.

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices
- **Knowledge:** `CLAUDE.md` §"Building and testing" (the disk limit: run the wasm loop after `cargo
  clean`); `liquers-web/README.md`.
- **Rationale:** triage across configurations.

## Testing Plan

### Unit Tests

**When to run:** at the end of Steps 1, 2, 3, 4, 6, 7, 9, 10 and 11, each with its own new tests
(listed in the step) and the whole `liquers-core` library suite, to catch regressions.

**Files:** the `mod tests` of `metadata.rs`, `dependencies.rs`, `assets.rs` and `context.rs`, and
`environment_builder.rs`.

**Command:**
```bash
CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib
```

**Expected:** all new tests pass, and every existing test passes unchanged except the two renamed in
Step 6 (`version_of_an_absent_key_is_none` → `version_of_absent_key_is_unknown`) and any test that
asserted a now-replaced message string (`note_expired_dependency`'s id-based text), which is
updated to the key-based text.

### Integration Tests

**When to run:** from Step 4 onward, each step adds to its file.

| File | Created in | Phase 3 id |
|---|---|---|
| `liquers-core/tests/expiry_provenance_integration.rs` | Step 4 | I5 |
| `liquers-core/tests/dependency_audit_integration.rs` | Step 6 (extended in 7, 8, 10) | I1 |
| `liquers-core/tests/external_change_integration.rs` | Step 9 | I2 |
| `liquers-core/tests/external_asset_manager.rs` + `tests/common/manager_scenarios.rs` | Step 11 | I3 |
| `liquers-core/tests/expiration_integration.rs` (extended) | Step 5 | I4 |
| `liquers-core/tests/manager_parametric.rs` (refactored to the common module) | Step 11 | — |

**Command:**
```bash
CARGO_INCREMENTAL=0 cargo test -p liquers-core --tests
```

**Expected:** every Phase 3 integration test passes on both built-in managers where Phase 3 says so,
and on the external manager for the shared scenarios.

### Manual Validation

**When to run:** after Step 12.

```bash
# 1. The example queries still mean what Phase 3 says
cargo run -p liquers-core --features cli --bin liquers-validate -- \
  --command index_files --command summarize -- '-R-dir/data/-/index_files' '-R/data/a.csv/-/summarize'
# Expected: both OK; GetAssetDirectory[data] then Action{index_files}

# 2. A reason in a stored sidecar, by eye: run expiry_provenance_integration with --nocapture and
#    a file store in a temp dir, then inspect data/report.txt.__metadata__
# Expected: "expiry_reason": {"scope":"cascaded","cause":{"kind":"updated",…},"root":…,"via":…}
#    and a matching log line; no runtime ids in either
```

**Success criteria:** every command passes, and the sidecar matches Phase 3 Example 1's expected
output.

## Agent Assignment Summary

| Step | Model | Skills | Rationale |
|---|---|---|---|
| 0 | haiku | — | front-matter edits |
| 1 | haiku | rust-best-practices, liquers-unittest | pure functions, fully specified |
| 2 | haiku | rust-best-practices, liquers-unittest | data types and serde, existing pattern |
| 3 | sonnet | rust-best-practices, liquers-unittest | graph algorithm and sparing rules |
| 4 | **opus** | rust-best-practices, liquers-unittest | 18 call sites, lock ordering, status authority |
| 5 | haiku | liquers-unittest | one-line fix plus test |
| 6 | sonnet | rust-best-practices, liquers-unittest | API change visible to axum; cross-process tests |
| 7 | sonnet | rust-best-practices, liquers-unittest | shared fast-track path |
| 8 | sonnet | rust-best-practices, liquers-unittest, liquers-validate | interpreter, asset manager and graph together |
| 9 | **opus** | rust-best-practices, liquers-unittest | changes every stored version; adds a write on read |
| 10 | sonnet | rust-best-practices, liquers-unittest | public command API; deterministic concurrency test |
| 11 | sonnet | rust-best-practices, liquers-unittest | public API decision and its proof |
| 12 | sonnet | rust-best-practices | cross-configuration triage |

**Order and parallelism.** The steps are sequential. Each one changes `assets.rs`, or depends on a
type from the previous step, so running them in parallel would mean merging conflicts in a
10,000-line file. Step 5 is the exception: it touches only `:7218` / `:7306` and can run beside
Step 3. Step 3 and Step 6 share `AuditFinding`, and Step 3 says how to keep them independent.

## Rollback Plan

### Per-Step Rollback

Each step is one commit and leaves a green build. To undo a step, `git revert <commit>`. The later
steps depend on earlier types (Step 4 on Steps 2–3; Steps 6–9 on Step 4), so revert in reverse
order. The exception is Step 5, which is independent.

**If a step fails:**
1. Run the step's rollback.
2. Read the failure. If it shows a design assumption is wrong, stop and return to Phase 2, as the
   workflow requires. Do not patch around it.
3. Re-attempt the step.

### Full Feature Rollback

The work is on `claude/elegant-feynman-8hci6u`, which has not been merged. Abandoning it means not
merging, so `main` is untouched.

**Files created:**
- `liquers-core/tests/expiry_provenance_integration.rs`
- `liquers-core/tests/dependency_audit_integration.rs`
- `liquers-core/tests/external_change_integration.rs`
- `liquers-core/tests/external_asset_manager.rs`
- `liquers-core/tests/common/manager_scenarios.rs`

**Files modified:**
- In `liquers-core/src/`: `metadata.rs`, `dependencies.rs`, `assets.rs`, `context.rs`,
  `interpreter.rs`, `environment_builder.rs`.
- In `liquers-core/tests/`: `keyed_version_cascade.rs`, `manager_parametric.rs`,
  `expiration_integration.rs`, `environment_builder.rs`.
- `liquers-axum/src/assets/common.rs`, one line.

**Cargo.toml changes:** none.

### Partial Completion

If work pauses, the last green step is the resume point. `DESIGN.md` §Notes records "Steps 0–N
done". Steps 1, 2, 3 and 5 have value on their own and could ship alone. From Step 4 onward the
steps build on each other.

**Data compatibility on rollback.** Sidecars written by a build with this change may carry
`expiry_reason`, and versions with bit 127 set. An older build reads such a sidecar through the
legacy branch (`deny_unknown_fields`, Phase 2 §"Forward compatibility"), so it degrades but stays
readable. Flagged versions are opaque numbers to an older build, so its dependency checks still
work. Rolling back therefore needs no data migration. At worst, some results are recomputed once.

## Documentation Updates

### New Reference and Guide Documents

| Path | Kind | Written in | Captured during implementation |
|---|---|---|---|
| `specs/guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md` | guide | Phase 5 | Step 11: which primitives the external manager needed, any primitive found missing, and snippets from `tests/external_asset_manager.rs` |

### Existing Documents and `affects_docs`

The authoritative set is in `DESIGN.md` `affects_docs`. The Phase 2 §"Documentation Architecture"
table says what changes in each document. Each document gets `reviewed:` bumped and a
`## History` row in Phase 5 (§9.2). These are the claims to check against the implemented and tested
behaviour:

- **`DEPENDENCIES_STATUS`:**
  - audits on first observation;
  - `audit_version(key, 0)` spares unknown edges;
  - `on_load` and a recorded unknown;
  - the listing version.
- **`ASSETS`:**
  - the scope and cause tables;
  - `record_expiry` as the single writer;
  - the Part G decision table, including the no-metadata `Source` kept in memory only.
- **`COMMAND_REGISTRATION_GUIDE:123`:** its `context.evaluate` + `asset.get()` example is rewritten
  to `wait_for_dependency`.
- **`ENVIRONMENT_CONFIG`:** the three YAML keys.

### Design, Capability, and Cross-Links

- **`specs/README.md`, the capability line for this design:** it moves from `designing` to
  `documented` in Phase 5, pointing at `reference/DEPENDENCIES_STATUS.md`.
- **New capability line:** "Asset managers outside core", pointing at the new guide.
- **`STORE_IMPLEMENTATION_GUIDE.md`:** a "see also" link to the new guide.

### Phase 5 Evidence Capture

Phase 5 must be able to quote these. Collect them while implementing:

- **Log lines:** the real log line for each of the seven causes, from
  `every_cause_writes_a_log_line --nocapture`.
- **External manager:** any primitive that the from-scratch external manager needed and Phase 2 did
  not list (Step 11).
- **Test outcomes:** whether any existing test changed outcome because content versions now carry
  the flag (Step 9), and why.
- **Lock ordering:** anything surprising about it in Step 4.
- **Timing:** the measured cost of on-read verification on the largest test value, if it is
  noticeable.

## Phase 5 Entry Criteria

Phase 5 starts only when all of these hold:

- [ ] Steps 0–12 are committed, each with a green build.
- [ ] Every Phase 3 test listed above exists and passes, with no remaining `#[ignore]` rows in
      `every_cause_writes_a_log_line`.
- [ ] Step 12's full matrix passes, including the wasm32 checks and `check-stubs.sh`.
- [ ] The PR is open, and every review comment is answered or addressed.
- [ ] The Phase 5 evidence above has been collected.
- [ ] Every newly discovered defect is filed under `specs/issues/` (§4.8).

## Execution Options

After approval:
- **Execute now:** run Steps 0–12 in order with the agents above, report after each step that
  needed a decision, and open a PR at the end.
- **Create a task list** for later execution.
- **Revise the plan** (stay in Phase 4).
- **Exit:** implement manually; Phase 5 remains outstanding.
