# Phase 4: Implementation Plan - Dependency Audit and Expiry Provenance

## Overview

**Feature:** Dependency audit correctness, audit policy, expiry provenance, outside-change detection
and external asset managers (Phase 1; Parts A–G of Phase 2).

**Architecture:** All in `liquers-core`, plus one compatibility line in `liquers-axum` and one
optional getter in `liquers-py`.
- **Versions:** they say whether they are a content hash. Stored bytes are re-hashed on read.
- **Expiry reasons:** typed `Direct` / `Cascaded` reasons, written to every expired asset's log
  through `AssetManager::record_expiry`.
- **Audits:** compare current with recorded versions on demand, or on load.
- **Folder listings:** each listing has a version.
- **Commands:** `Context::submit` + `wait_for_dependency`.
- **External managers:** the `AssetManager` trait is unsealed.

**Estimated complexity:** High. The changes are many but mostly mechanical. The difficult parts are
the cascade signature change that touches every expiry route (Step 4), the shared fast-track path
(Steps 7 and 9) and the lock ordering of Part G's write on the read path (Step 9).

**Estimated time:** 8–11 working days for an experienced Rust developer familiar with `assets.rs`.
Phase 3 names about 140 tests, most of them integration tests over restarted environments; writing
them is at least half the work. With the agent split below: 14 agent steps, each closed by a green
build.

**Prerequisites:**
- Phases 1–3 approved (2026-09-28, 2026-09-29, 2026-10-02); every owner decision is recorded in
  Phase 2's gate decisions, "Revision 2" and "Revision 2, clarifications".
- No blocking issue (Phase 2 §"Known-Issue Preflight").
- Branch `claude/elegant-feynman-8hci6u` up to date with `main` (merged 2026-10-02, commit
  `8e85e55`). Re-merge before Step 1 if `main` has moved, and re-check the `assets.rs` line numbers
  cited below. They are from that merge (spot-checked again in the Phase 4 review, 2026-10-02).
- No new crate dependencies: `blake3`, `scc`, `serde` and `tokio` are already used by `liquers-core`.

**Ground rules for every step** (from `CLAUDE.md`):
- No `unwrap`/`expect` outside tests.
- No `_ =>` on Liquers enums, tests included (Phase 3 §"Repeatable Development Guidance").
- Errors through typed constructors.
- `eprintln!`, never `println!`.
- Async by default.
- Each step ends with a build and test command that must pass before the next step starts, and with
  one commit. Commit messages name the step: `dependency-audit step N: …`.
- **Test names are Phase 3's.** Every test named in Phase 3 is assigned to exactly one step below,
  the first one after which it can pass; the "Test assignment index" after the Testing Plan is the
  lookup. A test a step adds is written in that step, not earlier with `#[ignore]`. Tests marked
  *Phase 4 only* cover something Phase 3 does not name; the reason is given beside each.
- A test file is created by the first step that assigns a test to it.

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
`STORE-VERSION-BLIND-TO-CHANGES-MADE-OUTSIDE-LIQUERS`); this folder's `DESIGN.md`.

**Action:**
- Set `status: in_progress` on each issue, following `DOCS_STRUCTURE_GUIDE.md` §4.3.
- `DESIGN.md`: tick Phase 4, set `status: approved` (hand-written while `gh_pr` is empty,
  §5.5), and add a Notes line "implementation started". When the PR is opened (Step 12), write its
  number into `gh_pr` and **remove** `status:`, since from then on it is derived (§5.5).
- Regenerate the index with `python3 scripts/docs_index.py`.

**Validation:**
```bash
python3 scripts/docs_index.py --check   # 0 errors
```

**Rollback:** `git revert` the commit.

**Agent Specification:**
- **Model:** haiku
- **Skills:** none
- **Knowledge:** `specs/DOCS_STRUCTURE_GUIDE.md` §4.3 and §5.5; this folder's `DESIGN.md`.
- **Rationale:** front-matter edits only.

---

### Step 1: Version kinds, verification and flagged content hashes (Part G1)

**Files:** `liquers-core/src/metadata.rs` (`Version`, lines 17–90); the five content sites in
`liquers-core/src/assets.rs`; `liquers-core/tests/keyed_version_cascade.rs`.

**Action:**
- Add `Version::HASH_FLAG`, `from_content`, `kind`, `verify`, and the enums `VersionKind`
  (`Unknown`, `ContentHash`, `Timestamp`) and `VersionCheck` (`Verified`,
  `Mismatch { actual, recorded }`). Use exactly the Phase 2 G1 signatures and semantics.
- Mask bit 127 to 0 in `from_time_now`, `from_specific_time` and `new_unique`.
- `verify` implements the legacy rule: an unflagged recorded value equal to `from_bytes(bytes)` is
  `Verified`. Every other non-equal recorded value, including a timestamp and 0, is a `Mismatch`.
- Leave `from_bytes` unchanged (it also produces command metadata versions,
  `command_metadata.rs:1233`).
- **Switch the five content sites to `from_content` here, not in Step 9:** `assets.rs:2030`,
  `:6114`, `:6237`, `:7362`, `:7421`. *Moved from Step 9 by the Phase 4 review:* the tests of
  Steps 4, 6 and 7 compute expected versions as `Version::from_content(bytes)` (for example
  `set_binary_of_a_dependency_cascades_with_updated`, `strict_service_after_restart`), so they could
  not pass until the stored versions are flagged. Nothing verifies yet, so flagging early changes
  only the numbers.
- Update the one test that pins the old form: `keyed_version_cascade.rs:136`
  (`Some(Version::from_bytes(&stored))` → `from_content`), and the doc comment at
  `metadata.rs:998` ("computed at save time as `Version::from_bytes(content)`").
- Fix the `Version::unknown()` doc comment (`metadata.rs:26-28`). It says unknown is "compatible
  with any known version". State instead where that holds (`matches`, the in-process fast-track
  check) and where it does not (cascades, audits, the `on_load` check of a *current* 0, Part G).
- Leave `liquers-records/src/provider.rs:151` (`unwrap_or_else(|| Version::from_bytes(&bytes))`)
  unchanged: it is a manifest-cache fallback used only when the stored metadata has no version,
  and it is compared only with itself. Record that it was checked.

**Code changes:**
```rust
impl Version {
    pub const HASH_FLAG: u128 = 1 << 127;
    pub fn from_content(bytes: &[u8]) -> Self;        // Version(from_bytes(bytes).0 | HASH_FLAG)
    pub fn kind(&self) -> VersionKind;
    pub fn verify(&self, bytes: &[u8]) -> VersionCheck;
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum VersionKind { Unknown, ContentHash, Timestamp }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionCheck { Verified, Mismatch { actual: Version, recorded: VersionKind } }
```

**Tests (unit, `metadata.rs` `mod tests`; Phase 3 U2 and corner case 4):**
- `version_from_content_sets_hash_flag`
- `version_kind_of_each_constructor`
- `from_bytes_is_not_forced_to_a_flag`
- `time_and_unique_versions_mask_bit_127`
- `verify_matches_own_bytes`
- `verify_reports_mismatch_with_actual`
- `verify_mismatch_records_the_kind`
- `timestamp_never_verifies`
- `unknown_never_verifies`
- `legacy_unflagged_hash_verifies_when_unchanged`
- `legacy_changed_value_is_a_mismatch` (the unit twin; the integration twin of the same name is
  in Step 9). Both legacy tests pick bytes whose `from_bytes` has bit 127 clear with a small
  search helper (Phase 3 sketch).
- `flagged_version_round_trips_through_hex`
- `null_version_sidecar_still_loads`

**Validation:**
```bash
CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib --tests
CARGO_INCREMENTAL=0 cargo test -p liquers-lib --lib --tests     # every stored version is now flagged
```

**Rollback:** `git checkout` the three files. No caller of the new API exists yet.

**Agent Specification:**
- **Model:** haiku (escalate to sonnet if the `liquers-lib` suite fails on a version comparison)
- **Skills:** rust-best-practices, liquers-unittest
- **Knowledge:** Phase 2 Part G1 and "Revision 2" item 1; `metadata.rs:17-90`; the five
  `assets.rs` sites above.
- **Rationale:** pure functions with a fully specified contract, plus five one-line call-site edits.

---

### Step 2: Expiry reason types, their wording, and their place in metadata (Part C data)

**Files:** `liquers-core/src/metadata.rs`; `liquers-py/src/metadata.rs` (one getter).

**Action:**
- Add `ExpiryCause` (7 variants) and `ExpiryReason` (`Direct`, `Cascaded`), with the serde
  attributes from Phase 2: internally tagged, `"kind"` / `"scope"`, `snake_case`. Add
  `ExpiryReason::cause(&self) -> &ExpiryCause` (Step 4 cascades with the cause of a direct reason).
- Add `ExpiryReason::log_entry(&self, subject: &str) -> LogEntry`, with the wording **fixed here**
  (Phase 2 left it to Phase 4) in the table below and the per-cause levels of the Phase 2 scope
  table. Every `match` on `ExpiryCause` is explicit.
- Add the field `expiry_reason: Option<ExpiryReason>` with
  `#[serde(default, skip_serializing_if = "Option::is_none")]` to `MetadataRecord` (`:911`) and
  `AssetInfo` (`:678`), and initialise it in `AssetInfo::new()`. Project it in **both**
  `From<MetadataRecord> for AssetInfo` (`:890`) and `MetadataRecord::get_asset_info` (`:1160`).
- Add `Metadata::expiry_reason(&self) -> Option<ExpiryReason>`, which returns `Some` only for a
  record whose status is `Expired`.
- Add `Metadata::set_expiry_reason(&mut self, reason: ExpiryReason) -> Result<(), Error>`, which
  returns an error on legacy metadata, as `add_log_entry` does.
- **Setting any status other than `Expired` clears `expiry_reason`** (`MetadataRecord::set_status`
  and `Metadata::set_status`). Without this a recomputed record would carry its old reason in the
  sidecar: the accessor would hide it, but `non_expired_record_json_is_unchanged` would fail and
  the sidecar would mislead a human reader. A direct `mr.status = …` write elsewhere must be
  checked: `grep -n "\.status = Status::" liquers-core/src/assets.rs`.
- Add `DependencyKey::is_store_resolvable` (true for `-R/` and `-R-dir/`). *Moved here from Step 6:*
  it is a pure `metadata.rs` function, and Steps 6–8 use it.
- `liquers-py`: `AssetInfo` and `MetadataRecord` mirror fields with hand-written getters, so, as
  Phase 2 §"Other crates" requires, add `#[getter] expiry_reason(&self) -> Option<String>` returning
  the reason as JSON (there is no Python type for it). No setter.
- `liquers-web` needs no change: `AssetInfo` crosses through serde and has no hand-written
  TypeScript declaration (`src/typescript.rs` does not mention it). `check-stubs.sh` in Step 12
  confirms.

**Log wording** (fixed here; Phase 2 §"Message rule"). Keys are `DependencyKey`s as text
(`-R/data/a.csv`); `{subject}` is the expired asset's key (`data/report.txt`) or, without a key,
its query. **No version number or asset id appears in a message** (the version is in
`expiry_reason`); only a deadline names a time, and only in the direct form.

| Cause | Direct: `{subject} expired: …` | Cascaded: `{subject} expired: … triggered a cascade expiration` |
|---|---|---|
| `Deadline { t }` | `its expiration time {t} passed` | `expiration deadline on {root}` |
| `Explicit` | `expiration was requested explicitly` | `explicit expiration of {root}` |
| `Audit { .. }` | `an audit found it at a different version than recorded` ¹ | `an audit that found {root} at a different version than recorded` |
| `StaleDependency { dep }` | `it was evaluated with the expired value of {dep}` | `the evaluation of {root} with the expired value of {dep}` |
| `UpdatedInStore { .. }` | `its stored content was changed outside Liquers` ¹ | `a change to {root} made in the store outside Liquers` |
| `Updated { .. }` | `it received new content` ¹ | `new content of {root}` |
| `Removed` | `it was removed` ¹ | `the removal of {root}` |

¹ No route produces this as `Direct` (Phase 2 scope table); the wording exists because the type
allows it and the `match` must be exhaustive.

A cascaded message then appends ` via direct dependency {via}` **iff `via != root`**, for every
cause. So the three forms are: direct; cascaded with `via == root` (no `via` clause); cascaded with
`via != root` (with it). This settles an inconsistency between Phase 2's message rule and two of
its illustrative examples, which omitted "triggered a cascade expiration": **the rule wins**, and
`log_line_format_per_cause` asserts this table, not those examples. For example, Phase 2's
*"data/report.txt expired: an audit found -R/data/a.csv at a different version than recorded"*
becomes *"data/report.txt expired: an audit that found -R/data/a.csv at a different version than
recorded triggered a cascade expiration"*, and Phase 3's parenthesised `(via direct dependency …)`
becomes the plain suffix.

**Tests (unit, `metadata.rs` `mod tests`; Phase 3 U2 and corner case 4):**
- `expiry_reason_round_trips_json_and_yaml`
- `expiry_reason_is_internally_tagged_snake_case`
- `expiry_reason_json_shape`
- `log_line_format_per_cause`: asserts the wording table above as exact strings, covering **all
  three forms** (direct, using a fixed `ExpirationTime::At(..)` so no clock is needed; cascaded with
  `via == root`, which must not contain "via direct dependency"; cascaded with `via != root`, which
  must), plus the level per cause and the no-digit rule for the cascaded rows.
- `expiry_reason_log_entry_uses_the_query_when_the_asset_has_no_key`
- `record_without_expiry_reason_loads`
- `non_expired_record_json_is_unchanged`: byte for byte against a fixture serialized before this
  step.
- `expired_record_serializes_reason`
- `unknown_field_record_degrades_to_legacy`
- `expiry_reason_accessor_hides_reason_unless_expired`
- `asset_info_projects_expiry_reason` (both projections)
- `dependency_key_is_store_resolvable`
- *Phase 4 only:* `set_status_clears_expiry_reason`. Phase 3 does not cover the clearing rule
  added above.

**Validation:**
```bash
CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib metadata
CARGO_INCREMENTAL=0 cargo test -p liquers-lib --lib --tests     # liquers-lib consumes AssetInfo
CARGO_INCREMENTAL=0 cargo check -p liquers-py
```

**Rollback:** `git checkout liquers-core/src/metadata.rs liquers-py/src/metadata.rs`. No caller yet.

**Agent Specification:**
- **Model:** haiku
- **Skills:** rust-best-practices, liquers-unittest
- **Knowledge:** Phase 2 §"`ExpiryCause` and `ExpiryReason`" (including the scope table) and
  §"Serialization Strategy"; the wording table above; `metadata.rs` around `MetadataRecord` and
  `AssetInfo`; `expiration.rs:739-775` (`Display` and the hand-written serde of `ExpirationTime`).
- **Rationale:** data types, serde and string formatting, following an existing pattern (`stored`,
  `cached`).

---

### Step 3: Cascade bookkeeping in the dependency manager (Parts A, C graph side)

**Files:** `liquers-core/src/dependencies.rs`; `liquers-core/src/assets.rs` (`AuditFinding`, and
the mechanical adaptation of callers).

**Action:**
- Restructure `ExpiredDependents` (`:69`) to
  `{ root: Option<DependencyKey>, keys: Vec<ExpiredKey>, assets: Vec<(WeakAssetRef<E>, DependencyKey)> }`.
  Add `ExpiredKey { key, via }`, keep `new()` (no root) and add `for_root(key)`.
- In `expire_from_frontier` (`:738`), record the predecessor when a key is first queued. Frontier
  keys get `via = root`. Query assets carry the key in whose `dependent_assets` list they were
  found (Phase 2 "Revision 2, clarifications" item 2).
- Define `AuditFinding` (with `new`, `#[non_exhaustive]`) in `assets.rs` now, where Phase 2 puts
  it. `dependencies.rs` already imports from `crate::assets` (`WeakAssetRef`), and a module cycle
  inside one crate is fine, so no temporary definition or re-export is needed.
- Add `pub(crate) async fn audit_version(&self, key, version: Version) -> (ExpiredDependents<E>, Vec<AuditFinding>)`.
  It is `stale_edges` for the findings, then the version insert (occupied or vacant), then
  `expire_stale_dependents`. For a version of 0 it dispatches to `report_no_version`'s rules.
- Add `pub(crate) async fn stale_edges(&self, key, version: Version) -> Vec<AuditFinding>`, which is
  read-only.
- Add `pub(crate) fn listing_version(names: &[String]) -> Version`: `from_content` over the sorted,
  length-prefixed names.
- `register_version`, `expire`, `report_no_version` and `expire_stale_dependents` fill `root`.
  `register_version` itself is **not** changed in behaviour.
- Callers in `assets.rs` that read `expired.keys` as a `Vec<DependencyKey>` are adapted
  mechanically (`.key`). No cause is introduced yet, so this step changes no behaviour outside the
  graph.

**Tests (unit, `dependencies.rs` `mod tests`; Phase 3 U1, pitfall 9, corner cases 1–2):**
- `audit_version_first_observation_expires_mismatched_dependent`
- `audit_version_spares_dependent_with_equal_recorded_version`
- `audit_version_expires_unknown_expecting_edge`
- `audit_version_with_unknown_version_spares_unknown_expecting_edge`
- `report_no_version_spares_unknown_expecting_edge`
- `audit_version_with_unknown_version_still_expires_concrete_edges`
- `audit_version_returns_findings_for_direct_edges_only`
- `audit_version_sets_root` (Phase 3 Template 1)
- `expire_from_frontier_records_via_per_key`
- `expire_from_frontier_via_is_shortest_path_in_a_diamond`
- `expired_dependents_query_assets_carry_the_key_whose_dependents_they_were`
- `stale_edges_is_read_only`
- `stale_edges_unknown_reports_concrete_edges_only`
- `register_version_first_registration_still_expires_nothing`
- `expired_dependents_for_root_sets_root`
- `expired_dependents_new_has_no_root`
- `listing_version_is_content_hash_of_sorted_names`
- `listing_version_is_order_independent`
- `listing_version_length_prefix_prevents_collision`
- `listing_version_distinguishes_empty_list_from_empty_name`
- `listing_version_of_50k_names`
- `audit_concurrent_with_register`

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
  (`dependencies.rs:180-195`, `:612`); Phase 3 pitfall 9.
- **Rationale:** a graph algorithm change with a subtle sparing rule. It needs judgment, not only
  pattern-following.

---

### Step 3b: Shared manager scenarios module (Part F test scaffolding, no behaviour change)

*Added by the Phase 4 review.* Phase 3 puts five new manager-independent scenarios in
`liquers-core/tests/common/manager_scenarios.rs`, and they exercise code from Steps 4, 6, 8 and 10.
If the module first appeared in Step 11, those scenarios could not be added in the steps whose code
they test. So the move happens here, before Step 4, as a pure refactor.

**Files:** `liquers-core/tests/manager_parametric.rs`; new `liquers-core/tests/common/mod.rs` and
`liquers-core/tests/common/manager_scenarios.rs`.

**Action:**
- Move the generic `scenario_*` bodies and their helpers (`scenario_basic_eval`,
  `scenario_cache_and_mode`, `scenario_stored_value`, `scenario_keyed_eval`, …,
  `manager_parametric.rs:27-340` and the later `scenario_*` functions) to
  `tests/common/manager_scenarios.rs`, unchanged.
- `tests/common/mod.rs` declares `pub mod manager_scenarios;` and carries
  `#![allow(dead_code)]`: each integration test is its own crate and uses a subset of the module.
- `manager_parametric.rs` gains `mod common;` and keeps its `#[tokio::test]` wrappers, which now
  call `common::manager_scenarios::…`. The test names do not change.
- The existing `tests/fixtures/mod.rs` (`StoreSnapshot`, `CountingStore`) stays where it is; the
  scenarios that need it import it with `#[path = "../fixtures/mod.rs"] mod fixtures;` inside the
  common module, or the calling test file passes it in. Choose one and use it consistently.

**Tests:** none new. `scenario_basic_eval`, `scenario_cache_and_mode` and the other existing
scenarios (named in Phase 3 I3) keep passing on both built-in managers.

**Validation:**
```bash
CARGO_INCREMENTAL=0 cargo test -p liquers-core --test manager_parametric   # same count as before
```

**Rollback:** `git checkout liquers-core/tests/manager_parametric.rs` and delete `tests/common/`.

**Agent Specification:**
- **Model:** haiku
- **Skills:** liquers-unittest
- **Knowledge:** `liquers-core/tests/manager_parametric.rs`; `tests/fixtures/mod.rs`.
- **Rationale:** a file move with no change in behaviour.

---

### Step 4: Every route into `Expired` records its reason (Part C)

**Files:** `liquers-core/src/assets.rs` (the core of the design); new
`liquers-core/tests/expiry_provenance_integration.rs` and
`liquers-core/tests/dependency_audit_integration.rs`; `tests/common/manager_scenarios.rs`,
`tests/manager_parametric.rs`.

**Action:**
- Trait `AssetManager`:
  - Add `fn record_expiry(&self, metadata: &mut Metadata, subject: &str, reason: &ExpiryReason)`,
    with a default body: `set_expiry_reason` + `add_log_entry(reason.log_entry(subject))`, both
    results ignored on legacy metadata (as the existing `let _ = …add_log_entry(…)` calls do).
  - Change `expire_dependencies_result(&self, expired, cause: ExpiryCause)` (`:4865`): each asset
    gets `Cascaded { cause, root, via }`.
  - Change `cascade_expire_dependents(&self, key, cause)` (`:4854`).
- `AssetRef`:
  - `mark_expired_status(&self, reason: ExpiryReason)` (`:3316`) calls `record_expiry` **under the
    `data` write lock, before `persist_info` is cloned**, and only when it transitions (a second
    `expire()` on an `Expired` asset records nothing).
  - Add `expire_with_reason(reason)`; `expire()` (`:3299`) becomes
    `expire_with_reason(Direct { Explicit })`. Its cascade at `:3309` passes `reason.cause()`.
  - `expire_without_cascade(reason)` (`:3388`).
- **Lock rule for `record_expiry`:** get the manager and the subject **from the guard**
  (`lock.get_envref().get_asset_manager()`, `lock.key` / the query), never through the `AssetRef`
  async accessors (`get_envref` at `:1904`, `key()`), which take the same `data` lock and would
  deadlock. `mark_expired_status` already reads `owner_key` before locking; keep that shape.
- `expire_stored_copy` (`:3880`) takes the manager and the reason, and calls `record_expiry` on the
  metadata it read, before `set_metadata`.
- `AssetManager::expire(key)` (`:4290`), stored-only branch (`:4318-4324`): call `record_expiry`
  with `Direct { Explicit }` **after** `metadata.set_status(Status::Expired)` (which, from Step 2,
  clears a reason on any other status) and before `set_metadata`.
- `AssetData::stale_dependency: bool` (`:600`) becomes `Option<DependencyKey>` (first stale
  dependency wins). `note_expired_dependency` (`:1621`) records the dependency's key, or its query
  when it has none, and its log line (`:1628`) names keys and `asset_reference()`, not `id()`.
- **The finalize stale branch (`:2106`) does not go through `mark_expired_status`:** it calls
  `lock.set_status(Status::Expired)` directly. Call `record_expiry(Direct { StaleDependency {
  dependency } })` there, under that same lock, after `set_status`. Its dependents are expired not
  by `cascade_expire_dependents` (Phase 2's scope table names that function, but there is no such
  call in this branch) but by `track_keyed_asset` at `:2927`, which registers the new content
  version; that call passes `StaleDependency { dependency }`, so the dependents get
  `Cascaded { StaleDependency, root: this asset, via }` as Phase 2 clarification 1 requires.
- Queued expiration monitor (`:5214`): `expire_with_reason(Direct { Deadline { expiration_time } })`.
- Pass a cause at **every** call site (15 `expire_dependencies_result` + 3 `cascade_expire_dependents`
  calls, verified at HEAD). The compiler lists any that are missed, since the signature changes:

| Line | Site | Cause |
|---|---|---|
| `:1218`, `:1223` | `try_fast_track`: `register_version`, `load_from_records` | `Updated { version: the loaded version, or 0 }` |
| `:1734` | `record_dependency_on_asset` (`add_dependency`, always empty) | `Updated { version: 0 }`, nominal |
| `:2927` | finalize, stale branch: `track_keyed_asset` | `StaleDependency { dependency }` |
| `:2934` | finalize, normal branch: `track_asset` | `Updated { version }` |
| `:3309` | `AssetRef::expire_with_reason` cascade | `reason.cause()` (`Explicit`, or `Deadline` from the monitor) |
| `:4261` | `AssetManager::remove` | `Removed` |
| `:4326` | `AssetManager::expire(key)`, stored-only | `Explicit` |
| `:4748` | `audit_gaps` | unchanged here; `Audit { found }` in Step 6 (pass `Updated { version }` until then) |
| `:4828` | `refresh_command_versions_and_expire` | `Updated { version: dm.get_version(key) or 0 }` |
| `:4856` | inside `cascade_expire_dependents` | its `cause` argument |
| `:4896` | `register_plan_dependencies` (always empty) | `Updated { version: 0 }`, nominal |
| `:6151`, `:7383` | `set_binary` (queued, immediate) | `Updated { version }` |
| `:6296`, `:6300`, `:7456`, `:7459` | `set_state` (queued, immediate) | `Updated { version }` |

**Tests:**
- Unit, in `assets.rs` `mod tests` (Phase 3 U3):
  - `mark_expired_status_persists_reason_with_status`
  - `mark_expired_status_calls_record_expiry_once_under_the_lock`
  - `expire_stored_copy_calls_record_expiry`
  - `expire_dependencies_result_assigns_cascaded_reason_per_key`
  - `cascade_expire_dependents_takes_the_cause`
  - *Phase 4 only:* `stale_dependency_records_dependency_key_not_id`. Phase 3 has no test of
    `note_expired_dependency`'s new key-based message and `Option<DependencyKey>` field; it uses the
    existing `stale_dep_fixture`.
  - *Phase 4 only:* `explicit_expire_of_stored_copy_records_reason`. The stored-only branch of
    `AssetManager::expire(key)` (`:4318`) rewrites the status itself, not through
    `expire_stored_copy`, so `expire_stored_copy_calls_record_expiry` does not cover it.
- New `liquers-core/tests/expiry_provenance_integration.rs` (Phase 3 I5):
  - `via_names_the_direct_dependency_on_a_two_step_cascade` (the `Updated` form of I5, by
    `set_binary(a.csv)` in process)
  - `log_line_is_persisted_with_the_status`
  - `log_line_names_keys_not_asset_ids`
  - `two_step_cascade_log_names_root_and_via` (integration twin of the `via != root` form)
  - `removing_a_source_cascades_with_removed`
  - `set_binary_of_a_dependency_cascades_with_updated`
  - `reader_never_sees_expired_without_reason`
- New `liquers-core/tests/dependency_audit_integration.rs` (Phase 3 I1, corner case 5):
  - `two_envs_share_persisted_reason`
- Shared scenarios, in `tests/common/manager_scenarios.rs`, run by `manager_parametric.rs` on both
  built-in managers (Phase 3 I3):
  - `scenario_expiry_reason_cascade`
  - `scenario_every_expired_asset_has_reason_and_log_line`

**Validation:**
```bash
CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib --tests
# includes expiration_integration and keyed_version_cascade: no regression expected
```

**Rollback:** `git checkout liquers-core/src/assets.rs` and the two test files touched; delete the
two new test files. Steps 1–3b stay.

**Agent Specification:**
- **Model:** opus
- **Skills:** rust-best-practices, liquers-unittest
- **Knowledge:**
  - Phase 2 Part C in full: the scope table, the message rule, the call-site table, "Revision 2,
    clarifications" items 1–2 and §"Concurrency Considerations"; the cause table above.
  - `assets.rs` around every cited line, especially `:2057-2160` and `:2880-2940`
    (`finalize_status_with_version` and the stale branch's registration).
  - The lock-ordering history in `specs/design/stale-dependency-status-finalization/`
    (`ASSET-STALE-DEPENDENCY-PERSISTED-AS-READY`).
- **Rationale:** 18 call sites across both managers, a lock-ordering invariant, and the status
  authority. A mistake here reintroduces the persisted-as-ready class of bug.

---

### Step 5: The immediate manager's deadline check (Part C, `IMMEDIATE-MANAGER-LAZY-DEADLINE-EXPIRY-NEVER-FIRES`)

**File:** `liquers-core/src/assets.rs` (`:7218`, `:7306`); `liquers-core/tests/expiration_integration.rs`.

**Depends on Step 4** (`expire_without_cascade` takes a reason from Step 4). *The earlier draft let
it run beside Step 3; it cannot.*

**Action:**
- Replace `status == Status::Ready && assetref.is_expired().await` with
  `status == Status::Ready && assetref.expiration_time().await.is_expired()` at both sites.
- Call `expire_without_cascade(Direct { Deadline { expiration_time } })`.
- Keep the non-cascading behaviour, as decided in Phase 2, and add a comment that points to the
  issue for the cascade question.
- Fold in `IMMEDIATE-SET-STATE-STATUS-MATCH-HAS-DEFAULT-ARM` only if the same function is touched
  (Phase 2 preflight); otherwise leave it.

**Tests:**
- Unit, `assets.rs` (Phase 3 U3): `immediate_manager_lazy_deadline_expires_with_deadline_reason`.
- `liquers-core/tests/expiration_integration.rs` (Phase 3 I4, pitfall 11):
  `immediate_manager_deadline_fires`, with `expires: "in 1 sec"` on an immediate environment. The
  crate has no test clock, so the test sleeps about 1.2 s.

**Validation:**
```bash
CARGO_INCREMENTAL=0 cargo test -p liquers-core --test expiration_integration immediate_manager_deadline_fires
CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib immediate_manager_lazy_deadline
```

**Rollback:** `git checkout` the two hunks and the test additions.

**Agent Specification:**
- **Model:** haiku
- **Skills:** liquers-unittest
- **Knowledge:** the issue file; `assets.rs:7200-7320`; `expiration.rs:813-830`.
- **Rationale:** a one-line fix at two sites plus two tests.

---

### Step 6: Audits: current version as `Version`, findings, report-only (Parts A, B)

**Files:** `liquers-core/src/assets.rs`; `liquers-axum/src/assets/common.rs`;
`liquers-core/tests/keyed_version_cascade.rs`, `asset_manager_remove_expire_describe.rs`,
`dependency_audit_integration.rs`, `expiry_provenance_integration.rs`, `common/manager_scenarios.rs`,
`manager_parametric.rs`.

**Action:**
- `AssetManager::version` (`:4777`) changes to `-> Result<Version, Error>`, returning
  `Version::unknown()` where it returned `None`. A store error stays `Err`.
- Add `dependency_version(&self, &DependencyKey) -> Result<Version, Error>`: `-R/` → `version`;
  `-R-dir/` → 0 until Step 8 adds the listing; any other key → 0.
- Add `AuditMode` and `AuditReport.findings`; put `#[non_exhaustive]` on `AuditReport` (it keeps
  `Debug, Clone, Default, PartialEq, Eq`).
- Add `trigger_dependency_audit_with` and `trigger_dependency_audit_all_registered_with`. The
  existing two methods delegate with `AuditMode::Expire`.
- Rewrite `audit_gaps` (`:4726`) over `is_store_resolvable` + `dependency_version` +
  `audit_version` (or `stale_edges`, registering nothing, in `ReportOnly`). Expire with cause
  `Audit { found }` (the `:4748` row of Step 4's table).
- Update every caller of `version()` (verified at HEAD):
  - `assets.rs:4739` (`audit_gaps`, rewritten anyway);
  - the in-file tests at `assets.rs:10632-10701`; rename `version_of_an_absent_key_is_none` →
    `version_of_absent_key_is_unknown`;
  - `tests/asset_manager_remove_expire_describe.rs:516`, `:522` (`is_some()` → `!is_unknown()`,
    `None` → `Version::unknown()`);
  - `tests/keyed_version_cascade.rs:303` (compare with `a_metadata.version().unwrap_or_default()`);
  - `liquers-axum/src/assets/common.rs:267-269`: drop `.unwrap_or_else(Version::unknown)`. The
    behaviour is identical; it is a compile fix only. `AuditResult::from` reads fields, which
    `#[non_exhaustive]` allows.
- `keyed_version_cascade.rs:341` compares `AuditReport::default()`. Keep that comparison, because
  `default()` stays available and `findings` is empty there.

**Tests:**
- Unit, `assets.rs` (Phase 3 U3): `audit_finding_new_and_report_default_assignment`.
- *Phase 4 only*, `assets.rs`: `store_error_is_not_unknown` (the changed `version()` contract
  directly; Phase 3 covers it only through an audit), and the renamed existing
  `version_of_absent_key_is_unknown`.
- `dependency_audit_integration.rs` (Phase 3 I1, corner cases 1–3, pitfall 10):
  - `audit_after_restart_expires_dependent` (the withdrawn test R2 of the issue)
  - `audit_never_evaluates` (Phase 3 Template 2)
  - `report_only_audit_changes_nothing`
  - `audit_expires_transitive_dependents`
  - `audit_report_lists_all_findings`
  - `cascade_over_100_link_chain` (driven by an audit of the chain's head)
  - `concurrent_audits_do_not_double_expire`
  - `audit_store_error_propagates`
  - *Phase 4 only:* `unloaded_dependent_is_refused_on_later_load`. Phase 2 "Revision 2,
    clarifications" item 4 (an audit leaves the current version in the map, and a dependent loaded
    later is refused by the existing check at `:1186`) has no Phase 3 test.
- `expiry_provenance_integration.rs` (Phase 3 I5, pitfall 12): `audit_never_expires_the_root`.
- Shared scenario: `scenario_audit_after_restart`.

**Validation:**
```bash
CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib --tests
CARGO_INCREMENTAL=0 cargo check -p liquers-axum
```

**Rollback:** `git checkout` the files listed.

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices, liquers-unittest
- **Knowledge:**
  - Phase 2 Part A, Part B and §"Trait Implementations".
  - `assets.rs:4700-4830`.
  - The issue `AUDIT-CANNOT-EXPIRE-ON-A-FIRST-OBSERVED-VERSION` (its reproduction).
  - `keyed_version_cascade.rs` and `tests/fixtures/mod.rs` (`StoreSnapshot`) for the cross-process
    pattern.
- **Rationale:** API changes visible to axum, and test scenarios across two environments.

---

### Step 7: Policies and their accessors; the `on_load` check (Part B, G3 options)

**Files:** `liquers-core/src/environment_builder.rs`, `liquers-core/src/environment_config.rs`
(tests), `liquers-core/src/assets.rs`; `tests/dependency_audit_integration.rs`.

**Action:**
- Add `DependencyAuditPolicy { Explicit, OnLoad }` and `VersionVerification { Off, OnRead }` in
  `environment_builder.rs`, and `ExternalChangePolicy { UserInput, Corrupted }` in `assets.rs`
  (where Phase 2 G2 puts it), each with its `is_<default>` helper for `skip_serializing_if`
  (`is_explicit`, `is_on_read`, `is_user_input`).
- Add the three fields to `AssetManagerOptions` (`:48`), with `with_dependency_audit`,
  `with_verify_versions` and `with_external_change`. The struct keeps `Eq`, so the enums derive it.
- Store the three values in `DefaultAssetManager` and `ImmediateAssetManager` at `build`
  (`environment_builder.rs:111-150`).
- Add the trait accessors `dependency_audit_policy()`, `version_verification()` and
  `external_change_policy()` with the Phase 2 defaults, and **override all three in both built-in
  managers** to return the stored values.
- In `try_fast_track` (`:1178-1200`), under `OnLoad`, resolve each recorded dependency that the map
  does not know and that `is_store_resolvable()` through `dependency_version`:
  - a recorded `unknown` is compatible (do not refuse);
  - otherwise the current version must **equal** the recorded one. Do **not** use
    `Version::matches` for this comparison: it treats a *current* 0 as compatible, and a missing
    current version must refuse;
  - a store error refuses as well.

**Tests:**
- Unit, `assets.rs` (Phase 3 U3, corner case 3):
  - `on_load_refuses_fast_track_on_mismatch`
  - `on_load_refuses_fast_track_on_missing_version`
  - `on_load_refuses_fast_track_on_store_error`
  - `on_load_does_not_refuse_recorded_unknown_version`
  - `explicit_ignores_unknown_map_entries`
- Unit, `environment_builder.rs` / `environment_config.rs` (Phase 3 U4):
  - `audit_policy_defaults_to_explicit_and_round_trips`
  - `options_omit_defaults_in_yaml`
  - `options_write_non_defaults`
  - `config_with_and_without_assets_section_parses` (`environment_config.rs`)
  - `with_dependency_audit_sets_policy`
  - `with_verify_versions_sets_policy`, `with_external_change_sets_policy`: Phase 3 asks for "the
    same for `VersionVerification` and `ExternalChangePolicy`" without naming them; these are the
    names.
- `dependency_audit_integration.rs` (Phase 3 I1, Example 1, pitfall 2):
  - `strict_service_after_restart`
  - `on_load_refuses_stale_fast_track`
  - `on_load_refuses_when_dependency_has_no_version`
  - `explicit_policy_serves_when_intermediate_deleted`
  - `audit_policy_from_config_yaml`

**Validation:** the build command (it includes `--test environment_builder`).

**Rollback:** `git checkout` the source files and the test additions.

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices, liquers-unittest
- **Knowledge:**
  - Phase 2 Part B and G3.
  - Phase 2 "Corrections made during Phase 3", items 1 and 3.
  - `environment_builder.rs:40-160`; `environment_config.rs`; `assets.rs:1112-1240`;
    `metadata.rs` `Version::matches`.
- **Rationale:** touches the fast-track path that every load shares.

---

### Step 8: Folder-listing dependencies (Part D)

**Files:** `liquers-core/src/assets.rs`, `liquers-core/src/interpreter.rs`;
`tests/dependency_audit_integration.rs`, `tests/common/manager_scenarios.rs`,
`tests/manager_parametric.rs`.

**Action:**
- `register_plan_dependencies` (`:4883`): add an edge with `Version::unknown()` when there is no
  version, instead of skipping it.
- `Step::GetAssetDirectory` (`interpreter.rs:780`): after `listdir_asset_info`, compute
  `listing_version(asset_manager.listdir(&key))`. Then:
  - `register_version` it and apply the result with cause `Updated { version }`;
  - `context.add_dependency(DependencyRecord::new(dir_key, v))`, which upgrades the unknown record;
  - `dm.add_dependency(owner, dir_key, v)` when the context has an owner key.
- Add `refresh_listing_version(&self, dir: &Key)` as a default trait method. It acts only if
  `get_version(-R-dir/dir)` is `Some`, and then cascades with `Updated { version }`. A `listdir`
  error is reported with `eprintln!` and the write stands.
- Call `refresh_listing_version(&key.parent())` after `save_to_store`, `set_binary`, `set_state`
  and `remove`, in both managers. In `remove` it runs while `key_mutation_lock` is held; that is
  safe because it does not take the lock.
- `dependency_version` for `-R-dir/` returns the listing version.
- `dependency_blocks_fast_track` (`:1080`): return an explicit early `false` for a `-R-dir/` key,
  with the comment from Phase 2.

**Tests:**
- Unit, `assets.rs` (Phase 3 U3):
  - `register_plan_dependencies_adds_unknown_edge_for_unregistered_key`
  - `dependency_blocks_fast_track_is_false_for_listing_key`
- `dependency_audit_integration.rs` (Phase 3 I1, Example 2, pitfall 8, corner cases 2, 3, 5):
  - `adding_a_file_expires_the_index`
  - `listing_gap_resolved_by_audit_after_restart`
  - `plan_dependency_without_version_gets_unknown_edge_then_upgrade`
  - `content_change_does_not_move_listing_version`
  - `deleting_bytes_keeps_the_listing_version`
  - `index_inside_listed_folder_does_not_self_expire`
  - `concurrent_writes_settle_on_true_membership`
  - `listdir_error_after_write_is_logged_not_fatal`
- Shared scenario: `scenario_listing_dependency`.

**Validation:** the build command. Also run
`cargo run -p liquers-core --features cli --bin liquers-validate -- --command index_files -- '-R-dir/data/-/index_files'`.

**Rollback:** `git checkout` the two source files and the test additions.

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices, liquers-unittest, liquers-validate
- **Knowledge:** Phase 2 Part D and §"Directory listing: where it plugs in"; `interpreter.rs:760-800`;
  `plan.rs:2640-2670`; `context.rs:685-720` (the record-upgrade pattern).
- **Rationale:** the interpreter, the asset manager and the dependency manager all take part.

---

### Step 9: Content changed outside Liquers (Part G)

**Files:** `liquers-core/src/assets.rs`; new `liquers-core/tests/external_change_integration.rs`;
`liquers-core/tests/fixtures/mod.rs` (a byte-read counter on `CountingStore`, Phase 3 corner
case 1).

**Action:**
- (The content sites already use `from_content` since Step 1.)
- Add `ExternalChangeAction` and `external_change_action(status, has_recipe, policy, actual) -> Option<…>`,
  implementing the full decision table with an explicit `Status` match.
- **"No metadata", operationally.** A store cannot be relied on to say that a file had no sidecar:
  `AsyncFileStore::get` and `get_metadata` *synthesize and write* a sidecar for a bare file on
  first read (status `Source`, no version; `store.rs:1118-1132`, `:1199-1209`, tracked by
  `STORE-NO-READ-ONLY-ADAPTER`), and a memory store always holds some metadata. So the "no metadata"
  rows of the decision table are recognised by **a recorded version of 0 on a stored status of
  `Source` or `None`**. With no recipe the caller passes `Status::Source`; with a recipe it passes
  `Status::Ready` (Phase 2 clarification 3). A Liquers-written value always carries a version, so
  this does not catch one.
- Add `apply_external_change(key, metadata, action)`, a default trait method. It:
  - takes `key_mutation_lock`, then **re-reads the stored metadata and does nothing if its version
    already equals `actual`** (that is what makes `concurrent_reads_apply_external_change_once`
    pass);
  - bumps the version to `actual` and writes status and version to the sidecar, **except** for an
    `AcceptAsInput` whose recorded version was 0 (the owner decision, 2026-10-02: a file with no
    metadata and no recipe is adopted in memory only; nothing is written, so a whole-store sweep
    does not litter the store with sidecars). Under a recipe (`ConvertToOverride`) the sidecar is
    written, because the status changes;
  - registers `actual` in the version map (`register_version`) and cascades with
    `UpdatedInStore { actual }`; each dependent gets its reason through `record_expiry`;
  - logs on the asset (`warning`): the "content changed" wording for a recorded hash, the "no
    content hash was recorded" wording for a timestamp or 0 (Phase 2 G2);
  - for `Delete`: removes data and metadata, logs to stderr, and calls
    `cascade_expire_dependents(key, UpdatedInStore { actual })`;
  - when a store write fails (a read-only store), keeps the result in memory and logs to stderr; the
    read does not fail.
- **Lock ordering (blocking).** `try_fast_track` runs with the asset's `data` write lock held by
  all four callers (`assets.rs:5709`, `:5764`, `:6007`, `:7323`), while `remove`, `expire`,
  `set_description` and `removedir` take `key_mutation_lock` and then reach asset `data` locks.
  Calling `apply_external_change` (which takes `key_mutation_lock`) from inside `try_fast_track`
  would therefore invert the order and can deadlock against a concurrent `expire(key)`. So:
  `try_fast_track` verifies, sets the in-memory status and version on `self` under its lock, and
  **returns the pending action** (for example in a new `AssetData` field read by the caller); the
  caller applies it with `apply_external_change` **after dropping the `data` lock**. A `Delete`
  makes `try_fast_track` return `false` (recompute) and is applied the same way. Never acquire
  `key_mutation_lock` while holding an asset's `data` lock.
- Under `OnRead`, verify in `try_fast_track` after deserialization succeeds (that path sees only
  `Ready`, `Source` and `Override`; the stored-status gate at `:1128` rejects the rest first), and
  in the store branches of `get_any_status` (`:4907`) and `get_binary_any_status` (`:4947`), which
  hold no asset lock. Under `Off` nothing is hashed.
- Add `verify_stored_versions(key, deep, mode) -> Result<VersionVerificationReport, Error>` with
  `verified`, `skipped` (metadata but no data object) and `changed`; `ReportOnly` writes nothing and
  registers nothing.

**Tests:**
- Unit, `assets.rs` (Phase 3 U3 and Template 3):
  - `external_change_action_decision_table`
  - `external_change_action_statuses_not_checked`
  - `source_is_always_input_under_both_policies`
  - `each_route_sets_its_reason`: every row of the Phase 3 route table. It is in-crate, so it
    drives the stale-dependency route through `note_expired_dependency` and the finalize branch
    directly; the `UpdatedInStore` row is why it lands here.
- New `external_change_integration.rs` (Phase 3 I2, Example 2, pitfalls 3–5 and 7, corner cases
  1–3):
  - `hand_edited_source_is_input_and_expires_dependents`
  - `recipe_backed_edit_follows_policy`
  - `override_is_never_deleted`
  - `file_with_no_metadata_and_no_recipe_becomes_source_with_hash_version`: asserts that the
    asset's version is the hash and that the stored metadata **still records no version** (the
    manager wrote nothing). It cannot assert "no sidecar exists": the memory store always holds
    metadata, and the file store writes its own on first read.
  - `file_with_no_metadata_under_recipe_follows_policy`
  - `timestamp_versioned_value_with_bytes_is_adopted`
  - `verify_stored_versions_report_only_changes_nothing`
  - `verify_stored_versions_applies_policy`
  - `verify_stored_versions_reports_an_unversioned_file`
  - `legacy_unchanged_value_still_verifies`
  - `legacy_changed_value_is_a_mismatch` (the integration twin of the Step 1 unit test)
  - `restoring_a_legacy_value_costs_one_cascade`
  - `audit_alone_does_not_see_unread_hand_edit`
  - `read_only_store_accepts_in_memory_and_does_not_fail_the_read`
  - `verify_reads_the_store_once`
  - `missing_bytes_are_skipped`
  - `empty_data_object_is_checked_not_skipped`
  - `concurrent_reads_apply_external_change_once`
  - *Phase 4 only:* `verification_off_changes_nothing`. Phase 3 has no test for
    `verify_versions: off`.

**Validation:** the build command, and `cargo test -p liquers-lib --lib --tests` (the read path of
every stored value now verifies).

**Rollback:** `git checkout liquers-core/src/assets.rs tests/fixtures/mod.rs` and delete the new
test file. Steps 1–8 are unaffected.

**Agent Specification:**
- **Model:** opus
- **Skills:** rust-best-practices, liquers-unittest
- **Knowledge:**
  - Phase 2 Part G in full, including the owner decisions in "Gate decisions" 5, "Revision 2" and
    "Revision 2, clarifications" item 3.
  - `assets.rs:1112-1240`, `:4900-4980`, `:5700-5770`, `:6000-6010`, `:7320-7330`, and
    `:4190-4440` (the `key_mutation_lock` holders).
  - `store.rs:1118-1210` (the file store's metadata synthesis) and the issues
    `STORE-VERSION-BLIND-TO-CHANGES-MADE-OUTSIDE-LIQUERS` and `STORE-NO-READ-ONLY-ADAPTER`.
- **Rationale:** it adds a write on the read path under a lock-ordering constraint, and it decides
  what a stored value's status becomes. Both are easy to get subtly wrong.

---

### Step 10: Commands can start a dependency and wait for it later (Part E)

**Files:** `liquers-core/src/context.rs`; `tests/dependency_audit_integration.rs`,
`tests/expiry_provenance_integration.rs`, `tests/common/manager_scenarios.rs`,
`tests/manager_parametric.rs`.

**Action:**
- Add `#[must_use] pub async fn submit(&self, query: &Query) -> Result<AssetRef<E>, Error>`. It calls
  `schedule_dependency_asset_with_key` (`:543`) and remembers `asset.id() → DependencyKey` in a new
  shared `Arc<tokio::sync::Mutex<HashMap<u64, DependencyKey>>>`, beside `pending_dependencies`
  (`:424`). Clone it at every `Context` construction site (`:817`, `:879`, `:1057`) and create it
  at `:1573`.
- Make `wait_for_dependency(&self, &AssetRef<E>)` (`:711`) `pub`. It looks the key up and calls
  `wait_for_dependency_recording(asset, key)`; an asset it did not submit is waited on with `None`,
  as today.
- `evaluate` (`:746`) becomes `submit` + `evaluate_local_queue`. Rewrite its doc comment, which
  today tells callers to wait through `AssetRef::get`: point to `wait_for_dependency` instead.
- `get_dependency_state` (`:734`) becomes `submit` + `wait_for_dependency`, with the same behaviour.

**Tests:**
- Unit, `context.rs` (Phase 3 U5, corner case 3):
  - `submit_records_dependency_under_the_key_wait_uses`
  - `submit_cycle_is_an_error`
  - `submit_does_not_run_on_inline_manager_until_waited`
  - `wait_for_dependency_on_unsubmitted_asset_records_no_version_upgrade`
  - `get_dependency_state_matches_submit_then_wait`
- `dependency_audit_integration.rs` (Phase 3 I1, pitfall 1):
  - `stale_dependency_end_to_end_queued`
  - `stale_dependency_end_to_end_immediate`

  Both use a gate command that submits a **computed** key, signals a test-controlled
  `tokio::sync::Notify`, blocks on a second one, and then waits. The test expires the dependency in
  between and asserts the parent ends `Direct { StaleDependency { dependency } }`.
- `expiry_provenance_integration.rs` (Phase 3 I5, pitfall 12). These need every route, and the
  stale-dependency route is reachable from outside the crate only through `submit`, so they land
  here:
  - `every_route_persists_its_reason`
  - `every_cause_writes_a_log_line`
- Shared scenario: `scenario_stale_dependency`.

**Validation:** the build command; `cargo test -p liquers-lib --lib --tests`, because commands
use `Context`; and `cargo check -p liquers-core --target wasm32-unknown-unknown` (`submit` and
`wait_for_dependency` are now public and must compile there).

**Rollback:** `git checkout liquers-core/src/context.rs` and the test additions.

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

**Files:** `liquers-core/src/assets.rs`, `liquers-core/src/dependencies.rs`; new
`liquers-core/tests/common/minimal_manager.rs` and `liquers-core/tests/external_asset_manager.rs`;
`tests/expiry_provenance_integration.rs`.

**Action:**
- Make `DependencyManagerAccess` (`:4004`) `pub`, and remove `#[allow(private_bounds)]` (`:4038`).
- **Also make `KeyMutationAccess` (`:4015`) `pub`.** *Found by the Phase 4 review:* it is a second
  `pub(crate)` supertrait of `AssetManager` (added on `main`), so unsealing only
  `DependencyManagerAccess` would leave the trait sealed and this step's proof would not compile.
  It exposes only `fn key_mutation_lock(&self) -> &tokio::sync::Mutex<()>`; give it a contract doc
  (one lock per manager, serialising keyed mutations; never taken while an asset's `data` lock is
  held). Phase 2 F1 was corrected to say so.
- Make `DependencyManager` (`dependencies.rs:114`) `pub` and **opaque**:
  - narrow every method that is `pub` today to `pub(crate)`, except `new`;
  - add `Default`.
- Make `AssetRef::run` (`:2608`), `run_inline` (`:2668`), `submitted` (`:2324`), `set_payload_path`
  (`:1924`) and `expire_without_cascade` (`:3388`) `pub`, each with a contract doc comment:
  - `run`: the native/queued primitive (`CORE-TOKIO-REMOVAL`).
  - `run_inline`: the one to use on wasm32; link `INLINE-DROP-REPAIR-STRANDS-EXISTING-WAITERS`.
- Give `refresh_command_versions` a default trait body over `self.dependency_manager()` and
  `self.get_envref()`, and delete the two identical impls (`:6452`, `:7526`). It and `start` are
  synchronous (`fn … -> Result<…, Error>`); `start` stays required, and its doc says to call
  `self.refresh_command_versions()?`.
- The external manager uses the same `#[cfg_attr(…, async_trait)]` / `async_trait(?Send)` pair as
  the trait (`assets.rs:4036-4037`).

**Tests:**
- `MinimalInlineAssetManager` and `MinimalKind` live in `tests/common/minimal_manager.rs`, written
  from scratch against the public API only (Phase 3 §"Guide Candidate Workflows" sketch, with the
  `KeyMutationAccess` impl added). *Deviation from Phase 2 F4, which says `external_asset_manager.rs`
  defines it:* `record_expiry_is_called_for_every_expired_asset` (I5) needs the same manager, and an
  integration test file cannot import another; the proof is unchanged, because the common module is
  also outside the crate.
- New `external_asset_manager.rs` (Phase 3 I3, pitfall 6):
  - `external_manager_passes_shared_scenarios` (all shared scenarios, including the five added in
    Steps 4, 6, 8 and 10)
  - `external_manager_registers_one_asset_per_key`
  - `external_manager_honours_audit_policy`
  - `record_expiry_is_overridable_by_a_manager`
- `expiry_provenance_integration.rs` (Phase 3 I5): `record_expiry_is_called_for_every_expired_asset`.
  *Moved here from Step 4:* its sketch uses the recording manager (`recording_env`, "MinimalKind-based
  manager from I3"), which cannot exist before this step.

**Validation:**
```bash
CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib --tests
CARGO_INCREMENTAL=0 cargo check -p liquers-axum -p liquers-lib
cargo check -p liquers-core --target wasm32-unknown-unknown       # needs the wasm32 target
```

**Rollback:** `git checkout` the two source files and `expiry_provenance_integration.rs`; delete the
two new test files.

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices, liquers-unittest
- **Knowledge:**
  - Phase 2 Part F (with the F1 correction) and the preflight rows for
    `ASSET-REGISTRATION-OWNERSHIP-CONTRACT`, `ENVIRONMENT-MANAGER-REFERENCE-CYCLE` and
    `INLINE-DROP-REPAIR-STRANDS-EXISTING-WAITERS`.
  - `ImmediateAssetManager` (`assets.rs:6990-7600`) as the reference for which primitives are
    needed, including its `KeyMutationAccess` impl (`:7577`).
  - `tests/common/manager_scenarios.rs`.
- **Rationale:** a public API decision, plus the proof that it is enough. If a primitive is missing,
  record it as Phase 5 evidence and make it public with a contract. Do not reach into `pub(crate)`.

---

### Step 12: Full validation, feature matrix and wasm

**Files:** none, unless a configuration breaks; `DESIGN.md` `gh_pr` when the PR is opened.

**Action:** run the whole matrix, fix only what the change broke, and file anything else as an
issue. Then open the PR and record its number in `DESIGN.md` (Step 0 note).

**Validation:**
```bash
CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib --tests
CARGO_INCREMENTAL=0 cargo test -p liquers-lib --lib --tests
CARGO_INCREMENTAL=0 cargo test -p liquers-lib --test registry_export   # no command signature changed
CARGO_INCREMENTAL=0 cargo test -p liquers-records --all-features --lib --tests   # Version consumer
CARGO_INCREMENTAL=0 cargo test -p liquers-axum                          # version and audit endpoints
CARGO_INCREMENTAL=0 cargo check -p liquers-py
bash scripts/check-build-matrix.sh
cargo clean && cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles
./liquers-web/scripts/check-stubs.sh --build                            # builds the quick-start first
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

**When to run:** at the end of Steps 1–11, each with its own new tests (listed in the step) and the
whole `liquers-core` library suite, to catch regressions.

**Files:** the `mod tests` of `metadata.rs`, `dependencies.rs`, `assets.rs`, `context.rs`,
`environment_builder.rs` and `environment_config.rs`.

**Command:**
```bash
CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib
```

**Expected:** all new tests pass, and every existing test passes unchanged except:
`version_of_an_absent_key_is_none` → `version_of_absent_key_is_unknown` and the other `version()`
callers (Step 6); `keyed_version_cascade.rs:136` (`from_content`, Step 1); any test that asserted
`note_expired_dependency`'s id-based message, updated to the key-based text (Step 4).

### Integration Tests

**When to run:** from Step 3b onward; each step adds to its files.

| File | Created in | Extended in | Phase 3 id |
|---|---|---|---|
| `liquers-core/tests/common/mod.rs`, `common/manager_scenarios.rs` | Step 3b | 4, 6, 8, 10 | I3 |
| `liquers-core/tests/manager_parametric.rs` (now calls the common module) | — (refactored in 3b) | 4, 6, 8, 10 | I3 |
| `liquers-core/tests/expiry_provenance_integration.rs` | Step 4 | 6, 10, 11 | I5 |
| `liquers-core/tests/dependency_audit_integration.rs` | Step 4 | 6, 7, 8, 10 | I1 |
| `liquers-core/tests/expiration_integration.rs` (extended) | — | 5 | I4 |
| `liquers-core/tests/external_change_integration.rs` | Step 9 | — | I2 |
| `liquers-core/tests/common/minimal_manager.rs`, `external_asset_manager.rs` | Step 11 | — | I3 |

**Command:**
```bash
CARGO_INCREMENTAL=0 cargo test -p liquers-core --tests
```

**Expected:** every Phase 3 integration test passes on both built-in managers where Phase 3 says so,
and on the external manager for the shared scenarios.

### Test assignment index

Every test named in Phase 3, the step that adds it and its file (paths relative to `liquers-core/`).
Names that Phase 3's corrections log lists as removed (`unflagged_mismatch_is_not_verifiable`,
`unknown_is_not_verifiable`, the `not_verifiable` sweep case) are deliberately absent. Phase 3's
corner case 3 called one test `on_load_store_error_refuses_fast_track`; that was the U3 test
`on_load_refuses_fast_track_on_store_error` under a second name, and Phase 3 now uses the one name.
`legacy_changed_value_is_a_mismatch` is two tests by Phase 3's design (U2 and I2), one per file.

| Phase 3 test | Group | Step | File |
|---|---|---|---|
| `version_from_content_sets_hash_flag` | U2 | 1 | `src/metadata.rs` |
| `version_kind_of_each_constructor` | U2 | 1 | `src/metadata.rs` |
| `from_bytes_is_not_forced_to_a_flag` | U2 | 1 | `src/metadata.rs` |
| `time_and_unique_versions_mask_bit_127` | U2 | 1 | `src/metadata.rs` |
| `verify_matches_own_bytes` | U2 | 1 | `src/metadata.rs` |
| `verify_reports_mismatch_with_actual` | U2 | 1 | `src/metadata.rs` |
| `verify_mismatch_records_the_kind` | U2 | 1 | `src/metadata.rs` |
| `timestamp_never_verifies` | U2 | 1 | `src/metadata.rs` |
| `unknown_never_verifies` | U2 | 1 | `src/metadata.rs` |
| `legacy_unflagged_hash_verifies_when_unchanged` | U2, pitfall 4 | 1 | `src/metadata.rs` |
| `legacy_changed_value_is_a_mismatch` (unit) | U2, pitfall 4 | 1 | `src/metadata.rs` |
| `flagged_version_round_trips_through_hex` | U2, C4 | 1 | `src/metadata.rs` |
| `null_version_sidecar_still_loads` | U2, C4 | 1 | `src/metadata.rs` |
| `expiry_reason_round_trips_json_and_yaml` | U2, C4 | 2 | `src/metadata.rs` |
| `expiry_reason_is_internally_tagged_snake_case` | U2 | 2 | `src/metadata.rs` |
| `expiry_reason_json_shape` | U2, C4 | 2 | `src/metadata.rs` |
| `log_line_format_per_cause` | U2 | 2 | `src/metadata.rs` |
| `expiry_reason_log_entry_uses_the_query_when_the_asset_has_no_key` | U2 | 2 | `src/metadata.rs` |
| `record_without_expiry_reason_loads` | U2, C4 | 2 | `src/metadata.rs` |
| `non_expired_record_json_is_unchanged` | U2, C4 | 2 | `src/metadata.rs` |
| `expired_record_serializes_reason` | U2 | 2 | `src/metadata.rs` |
| `unknown_field_record_degrades_to_legacy` | U2, C4 | 2 | `src/metadata.rs` |
| `expiry_reason_accessor_hides_reason_unless_expired` | U2 | 2 | `src/metadata.rs` |
| `asset_info_projects_expiry_reason` | U2 | 2 | `src/metadata.rs` |
| `dependency_key_is_store_resolvable` | U2 | 2 | `src/metadata.rs` |
| `audit_version_first_observation_expires_mismatched_dependent` | U1 | 3 | `src/dependencies.rs` |
| `audit_version_spares_dependent_with_equal_recorded_version` | U1 | 3 | `src/dependencies.rs` |
| `audit_version_expires_unknown_expecting_edge` | U1, pitfall 9 | 3 | `src/dependencies.rs` |
| `audit_version_with_unknown_version_spares_unknown_expecting_edge` | U1, pitfall 9 | 3 | `src/dependencies.rs` |
| `report_no_version_spares_unknown_expecting_edge` | U1, pitfall 9 | 3 | `src/dependencies.rs` |
| `audit_version_with_unknown_version_still_expires_concrete_edges` | U1 | 3 | `src/dependencies.rs` |
| `audit_version_returns_findings_for_direct_edges_only` | U1 | 3 | `src/dependencies.rs` |
| `audit_version_sets_root` | U1, Template 1 | 3 | `src/dependencies.rs` |
| `expire_from_frontier_records_via_per_key` | U1 | 3 | `src/dependencies.rs` |
| `expire_from_frontier_via_is_shortest_path_in_a_diamond` | U1 | 3 | `src/dependencies.rs` |
| `expired_dependents_query_assets_carry_the_key_whose_dependents_they_were` | U1 | 3 | `src/dependencies.rs` |
| `stale_edges_is_read_only` | U1 | 3 | `src/dependencies.rs` |
| `stale_edges_unknown_reports_concrete_edges_only` | U1 | 3 | `src/dependencies.rs` |
| `register_version_first_registration_still_expires_nothing` | U1 | 3 | `src/dependencies.rs` |
| `expired_dependents_for_root_sets_root` | U1 | 3 | `src/dependencies.rs` |
| `expired_dependents_new_has_no_root` | U1 | 3 | `src/dependencies.rs` |
| `listing_version_is_content_hash_of_sorted_names` | U1 | 3 | `src/dependencies.rs` |
| `listing_version_is_order_independent` | U1 | 3 | `src/dependencies.rs` |
| `listing_version_length_prefix_prevents_collision` | U1 | 3 | `src/dependencies.rs` |
| `listing_version_distinguishes_empty_list_from_empty_name` | U1 | 3 | `src/dependencies.rs` |
| `listing_version_of_50k_names` | U1, C1 | 3 | `src/dependencies.rs` |
| `audit_concurrent_with_register` | U1, C2 | 3 | `src/dependencies.rs` |
| `scenario_basic_eval`, `scenario_cache_and_mode`, … (existing, moved) | I3 | 3b | `tests/common/manager_scenarios.rs` |
| `mark_expired_status_persists_reason_with_status` | U3 | 4 | `src/assets.rs` |
| `mark_expired_status_calls_record_expiry_once_under_the_lock` | U3 | 4 | `src/assets.rs` |
| `expire_stored_copy_calls_record_expiry` | U3 | 4 | `src/assets.rs` |
| `expire_dependencies_result_assigns_cascaded_reason_per_key` | U3 | 4 | `src/assets.rs` |
| `cascade_expire_dependents_takes_the_cause` | U3 | 4 | `src/assets.rs` |
| `via_names_the_direct_dependency_on_a_two_step_cascade` | I5, Ex. 1 variant | 4 | `tests/expiry_provenance_integration.rs` |
| `log_line_is_persisted_with_the_status` | I5 | 4 | `tests/expiry_provenance_integration.rs` |
| `log_line_names_keys_not_asset_ids` | I5 | 4 | `tests/expiry_provenance_integration.rs` |
| `two_step_cascade_log_names_root_and_via` | I5 | 4 | `tests/expiry_provenance_integration.rs` |
| `removing_a_source_cascades_with_removed` | I5, C5 | 4 | `tests/expiry_provenance_integration.rs` |
| `set_binary_of_a_dependency_cascades_with_updated` | I5 | 4 | `tests/expiry_provenance_integration.rs` |
| `reader_never_sees_expired_without_reason` | I5, C2 | 4 | `tests/expiry_provenance_integration.rs` |
| `two_envs_share_persisted_reason` | I1, C5 | 4 | `tests/dependency_audit_integration.rs` |
| `scenario_expiry_reason_cascade` | I3 | 4 | `tests/common/manager_scenarios.rs` |
| `scenario_every_expired_asset_has_reason_and_log_line` | I3 | 4 | `tests/common/manager_scenarios.rs` |
| `immediate_manager_lazy_deadline_expires_with_deadline_reason` | U3 | 5 | `src/assets.rs` |
| `immediate_manager_deadline_fires` | I4, pitfall 11, C5 | 5 | `tests/expiration_integration.rs` |
| `audit_finding_new_and_report_default_assignment` | U3 | 6 | `src/assets.rs` |
| `audit_after_restart_expires_dependent` | I1 | 6 | `tests/dependency_audit_integration.rs` |
| `audit_never_evaluates` | I1, Template 2, C5 | 6 | `tests/dependency_audit_integration.rs` |
| `report_only_audit_changes_nothing` | I1, pitfall 10 | 6 | `tests/dependency_audit_integration.rs` |
| `audit_expires_transitive_dependents` | I1 | 6 | `tests/dependency_audit_integration.rs` |
| `audit_report_lists_all_findings` | I1, C1 | 6 | `tests/dependency_audit_integration.rs` |
| `cascade_over_100_link_chain` | I1, C1 | 6 | `tests/dependency_audit_integration.rs` |
| `concurrent_audits_do_not_double_expire` | I1, C2 | 6 | `tests/dependency_audit_integration.rs` |
| `audit_store_error_propagates` | I1, C3 | 6 | `tests/dependency_audit_integration.rs` |
| `audit_never_expires_the_root` | I5, pitfall 12 | 6 | `tests/expiry_provenance_integration.rs` |
| `scenario_audit_after_restart` | I3 | 6 | `tests/common/manager_scenarios.rs` |
| `on_load_refuses_fast_track_on_mismatch` | U3 | 7 | `src/assets.rs` |
| `on_load_refuses_fast_track_on_missing_version` | U3 | 7 | `src/assets.rs` |
| `on_load_refuses_fast_track_on_store_error` | U3, C3 | 7 | `src/assets.rs` |
| `on_load_does_not_refuse_recorded_unknown_version` | U3, pitfall 2 | 7 | `src/assets.rs` |
| `explicit_ignores_unknown_map_entries` | U3 | 7 | `src/assets.rs` |
| `audit_policy_defaults_to_explicit_and_round_trips` | U4 | 7 | `src/environment_builder.rs` |
| `options_omit_defaults_in_yaml` | U4 | 7 | `src/environment_builder.rs` |
| `options_write_non_defaults` | U4 | 7 | `src/environment_builder.rs` |
| `config_with_and_without_assets_section_parses` | U4, C4 | 7 | `src/environment_config.rs` |
| `with_dependency_audit_sets_policy` (+ the two unnamed siblings) | U4 | 7 | `src/environment_builder.rs` |
| `strict_service_after_restart` | Ex. 1, I1 | 7 | `tests/dependency_audit_integration.rs` |
| `on_load_refuses_stale_fast_track` | I1 | 7 | `tests/dependency_audit_integration.rs` |
| `on_load_refuses_when_dependency_has_no_version` | I1, pitfall 2 | 7 | `tests/dependency_audit_integration.rs` |
| `explicit_policy_serves_when_intermediate_deleted` | I1, pitfall 2 | 7 | `tests/dependency_audit_integration.rs` |
| `audit_policy_from_config_yaml` | I1 | 7 | `tests/dependency_audit_integration.rs` |
| `register_plan_dependencies_adds_unknown_edge_for_unregistered_key` | U3 | 8 | `src/assets.rs` |
| `dependency_blocks_fast_track_is_false_for_listing_key` | U3 | 8 | `src/assets.rs` |
| `adding_a_file_expires_the_index` | I1, Ex. 2 | 8 | `tests/dependency_audit_integration.rs` |
| `listing_gap_resolved_by_audit_after_restart` | I1 | 8 | `tests/dependency_audit_integration.rs` |
| `plan_dependency_without_version_gets_unknown_edge_then_upgrade` | I1 | 8 | `tests/dependency_audit_integration.rs` |
| `content_change_does_not_move_listing_version` | I1, pitfall 8 | 8 | `tests/dependency_audit_integration.rs` |
| `deleting_bytes_keeps_the_listing_version` | I1, pitfall 8 | 8 | `tests/dependency_audit_integration.rs` |
| `index_inside_listed_folder_does_not_self_expire` | I1, C5 | 8 | `tests/dependency_audit_integration.rs` |
| `concurrent_writes_settle_on_true_membership` | I1, C2 | 8 | `tests/dependency_audit_integration.rs` |
| `listdir_error_after_write_is_logged_not_fatal` | I1, C3 | 8 | `tests/dependency_audit_integration.rs` |
| `scenario_listing_dependency` | I3 | 8 | `tests/common/manager_scenarios.rs` |
| `external_change_action_decision_table` | U3 | 9 | `src/assets.rs` |
| `external_change_action_statuses_not_checked` | U3 | 9 | `src/assets.rs` |
| `source_is_always_input_under_both_policies` | Template 3 | 9 | `src/assets.rs` |
| `each_route_sets_its_reason` | U3 | 9 | `src/assets.rs` |
| `hand_edited_source_is_input_and_expires_dependents` | I2, Ex. 2 | 9 | `tests/external_change_integration.rs` |
| `recipe_backed_edit_follows_policy` | I2, pitfall 5 | 9 | `tests/external_change_integration.rs` |
| `override_is_never_deleted` | I2 | 9 | `tests/external_change_integration.rs` |
| `file_with_no_metadata_and_no_recipe_becomes_source_with_hash_version` | I2 | 9 | `tests/external_change_integration.rs` |
| `file_with_no_metadata_under_recipe_follows_policy` | I2 | 9 | `tests/external_change_integration.rs` |
| `timestamp_versioned_value_with_bytes_is_adopted` | I2 | 9 | `tests/external_change_integration.rs` |
| `verify_stored_versions_report_only_changes_nothing` | I2 | 9 | `tests/external_change_integration.rs` |
| `verify_stored_versions_applies_policy` | I2 | 9 | `tests/external_change_integration.rs` |
| `verify_stored_versions_reports_an_unversioned_file` | I2 | 9 | `tests/external_change_integration.rs` |
| `legacy_unchanged_value_still_verifies` | I2, pitfall 4 | 9 | `tests/external_change_integration.rs` |
| `legacy_changed_value_is_a_mismatch` (integration) | I2, pitfall 4 | 9 | `tests/external_change_integration.rs` |
| `restoring_a_legacy_value_costs_one_cascade` | I2, pitfall 4 | 9 | `tests/external_change_integration.rs` |
| `audit_alone_does_not_see_unread_hand_edit` | I2, pitfall 3 | 9 | `tests/external_change_integration.rs` |
| `read_only_store_accepts_in_memory_and_does_not_fail_the_read` | I2, pitfall 7 | 9 | `tests/external_change_integration.rs` |
| `verify_reads_the_store_once` | I2, C1 | 9 | `tests/external_change_integration.rs` |
| `missing_bytes_are_skipped` | I2, C3 | 9 | `tests/external_change_integration.rs` |
| `empty_data_object_is_checked_not_skipped` | I2, C3 | 9 | `tests/external_change_integration.rs` |
| `concurrent_reads_apply_external_change_once` | I2, C2 | 9 | `tests/external_change_integration.rs` |
| `submit_records_dependency_under_the_key_wait_uses` | U5 | 10 | `src/context.rs` |
| `submit_cycle_is_an_error` | U5, C3 | 10 | `src/context.rs` |
| `submit_does_not_run_on_inline_manager_until_waited` | U5 | 10 | `src/context.rs` |
| `wait_for_dependency_on_unsubmitted_asset_records_no_version_upgrade` | U5 | 10 | `src/context.rs` |
| `get_dependency_state_matches_submit_then_wait` | U5 | 10 | `src/context.rs` |
| `stale_dependency_end_to_end_queued` | I1, pitfall 1 | 10 | `tests/dependency_audit_integration.rs` |
| `stale_dependency_end_to_end_immediate` | I1, pitfall 1 | 10 | `tests/dependency_audit_integration.rs` |
| `every_route_persists_its_reason` | I5, pitfall 12 | 10 | `tests/expiry_provenance_integration.rs` |
| `every_cause_writes_a_log_line` | I5 | 10 | `tests/expiry_provenance_integration.rs` |
| `scenario_stale_dependency` | I3 | 10 | `tests/common/manager_scenarios.rs` |
| `external_manager_passes_shared_scenarios` | I3 | 11 | `tests/external_asset_manager.rs` |
| `external_manager_registers_one_asset_per_key` | I3, pitfall 6 | 11 | `tests/external_asset_manager.rs` |
| `external_manager_honours_audit_policy` | I3, pitfall 6 | 11 | `tests/external_asset_manager.rs` |
| `record_expiry_is_overridable_by_a_manager` | I3, pitfall 6 | 11 | `tests/external_asset_manager.rs` |
| `record_expiry_is_called_for_every_expired_asset` | I5 | 11 | `tests/expiry_provenance_integration.rs` |

**Phase 4-only tests** (each covers something Phase 3 does not name): `set_status_clears_expiry_reason`
(Step 2), `stale_dependency_records_dependency_key_not_id` and
`explicit_expire_of_stored_copy_records_reason` (Step 4), `store_error_is_not_unknown`,
`version_of_absent_key_is_unknown` (a rename) and `unloaded_dependent_is_refused_on_later_load`
(Step 6), `with_verify_versions_sets_policy` and `with_external_change_sets_policy` (Step 7, naming
Phase 3's unnamed siblings), `verification_off_changes_nothing` (Step 9).

**Where the log-message format is tested.** The three forms of the wording table (Step 2) are
asserted as exact strings by `log_line_format_per_cause` (Step 2): direct; cascaded with
`via == root`; cascaded with `via != root`. In the integration suite, `via == root` is
`log_line_is_persisted_with_the_status` and `every_cause_writes_a_log_line`, `via != root` is
`two_step_cascade_log_names_root_and_via`, the subject-by-query rule is
`expiry_reason_log_entry_uses_the_query_when_the_asset_has_no_key`, and the no-asset-id rule is
`log_line_names_keys_not_asset_ids`.

### Manual Validation

**When to run:** after Step 12.

```bash
# 1. The example queries still mean what Phase 3 says
cargo run -p liquers-core --features cli --bin liquers-validate -- \
  --command index_files --command summarize -- '-R-dir/data/-/index_files' '-R/data/a.csv/-/summarize'
# Expected: both OK; GetAssetDirectory[data] then Action{index_files}

# 2. A reason in a stored sidecar, by eye: log_line_is_persisted_with_the_status prints the
#    persisted metadata as JSON with eprintln! on its way out
CARGO_INCREMENTAL=0 cargo test -p liquers-core --test expiry_provenance_integration \
  log_line_is_persisted_with_the_status -- --nocapture
# Expected: "expiry_reason": {"scope":"cascaded","cause":{"kind":"updated",…},"root":…,"via":…}
#    and a matching log line in the wording of Step 2; no runtime ids in either
```

**Success criteria:** every command passes, and the sidecar matches the shape of Phase 2
§"Serialization Strategy" and the wording table of Step 2.

## Agent Assignment Summary

| Step | Model | Skills | Rationale |
|---|---|---|---|
| 0 | haiku | — | front-matter edits |
| 1 | haiku | rust-best-practices, liquers-unittest | pure functions, fully specified; five one-line sites |
| 2 | haiku | rust-best-practices, liquers-unittest | data types, serde and wording, existing pattern |
| 3 | sonnet | rust-best-practices, liquers-unittest | graph algorithm and sparing rules |
| 3b | haiku | liquers-unittest | test-file move, no behaviour change |
| 4 | **opus** | rust-best-practices, liquers-unittest | 18 call sites, lock ordering, status authority |
| 5 | haiku | liquers-unittest | one-line fix plus tests |
| 6 | sonnet | rust-best-practices, liquers-unittest | API change visible to axum; cross-process tests |
| 7 | sonnet | rust-best-practices, liquers-unittest | shared fast-track path |
| 8 | sonnet | rust-best-practices, liquers-unittest, liquers-validate | interpreter, asset manager and graph together |
| 9 | **opus** | rust-best-practices, liquers-unittest | a write on the read path under a lock-ordering constraint |
| 10 | sonnet | rust-best-practices, liquers-unittest | public command API; deterministic concurrency test |
| 11 | sonnet | rust-best-practices, liquers-unittest | public API decision and its proof |
| 12 | sonnet | rust-best-practices | cross-configuration triage |

**Order and parallelism.** The steps are sequential. Each one changes `assets.rs`, or depends on a
type from the previous step, so running them in parallel would mean merging conflicts in a
10,000-line file. Step 3b touches only `tests/` and can run beside Step 3. Step 10 touches
`context.rs` and tests only, and needs Step 4 (the stale reason), so it could run beside Steps 6–9
if conflicts in the shared test files are acceptable; the default is sequential.

## Rollback Plan

### Per-Step Rollback

Each step is one commit and leaves a green build. To undo a committed step, `git revert <commit>`;
the per-step `git checkout` commands above are for abandoning a step before its commit. The later
steps depend on earlier types (Step 4 on Steps 2–3b; Steps 5–11 on Step 4; Step 9 on Step 7;
Step 11's shared-scenario run on Steps 4–10), so revert in reverse order. Steps 1, 2 and 3b are
independent of each other.

**If a step fails:**
1. Run the step's rollback.
2. Read the failure. If it shows a design assumption is wrong, stop and return to Phase 2, as the
   workflow requires. Do not patch around it.
3. Re-attempt the step.

### Full Feature Rollback

The work is on `claude/elegant-feynman-8hci6u`, which has not been merged. Abandoning it means not
merging, so `main` is untouched.

**Files created:**
- `liquers-core/tests/common/mod.rs`
- `liquers-core/tests/common/manager_scenarios.rs`
- `liquers-core/tests/common/minimal_manager.rs`
- `liquers-core/tests/expiry_provenance_integration.rs`
- `liquers-core/tests/dependency_audit_integration.rs`
- `liquers-core/tests/external_change_integration.rs`
- `liquers-core/tests/external_asset_manager.rs`

**Files modified:**
- In `liquers-core/src/`: `metadata.rs`, `dependencies.rs`, `assets.rs`, `context.rs`,
  `interpreter.rs`, `environment_builder.rs`, `environment_config.rs` (tests).
- In `liquers-core/tests/`: `keyed_version_cascade.rs`, `asset_manager_remove_expire_describe.rs`,
  `manager_parametric.rs`, `expiration_integration.rs`, `fixtures/mod.rs`.
- `liquers-axum/src/assets/common.rs`, one line.
- `liquers-py/src/metadata.rs`, one getter.
- Docs: the eight issue records, this folder's `DESIGN.md`, `specs/index.csv` (regenerated).

**Cargo.toml changes:** none.

### Partial Completion

If work pauses, the last green step is the resume point. `DESIGN.md` §Notes records "Steps 0–N
done". Steps 1, 2, 3 and 3b have value on their own and could ship alone (Step 1 changes stored
version numbers, which costs each dependent one cascade on its first re-store, as pitfall 4
describes for legacy values). From Step 4 onward the steps build on each other.

**Data compatibility on rollback.** Sidecars written by a build with this change may carry
`expiry_reason`, and versions with bit 127 set. An older build reads such a sidecar through the
legacy branch (`deny_unknown_fields`, Phase 2 §"Forward compatibility"), so it degrades but stays
readable. Flagged versions are opaque numbers to an older build, so its dependency checks still
work. Rolling back therefore needs no data migration. At worst, some results are recomputed once.
Values adopted as `Override` by Part G stay `Override` after a rollback; that is the user's content
and is not reverted.

## Documentation Updates

### New Reference and Guide Documents

| Path | Kind | Written in | Captured during implementation |
|---|---|---|---|
| `specs/guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md` | guide | Phase 5 | Step 11: which primitives the external manager needed (including `KeyMutationAccess`), any primitive found missing, and snippets from `tests/common/minimal_manager.rs` |

### Existing Documents and `affects_docs`

The authoritative set is in `DESIGN.md` `affects_docs`. The Phase 2 §"Documentation Architecture"
table says what changes in each document. Each document gets `reviewed:` bumped and a
`## History` row in Phase 5 (§9.2). These are the claims to check against the implemented and tested
behaviour:

- **`DEPENDENCIES_STATUS`:**
  - audits on first observation;
  - `audit_version(key, 0)` spares unknown edges;
  - `on_load` and a recorded unknown, and a current 0 that refuses;
  - the listing version.
- **`ASSETS`:**
  - the scope and cause tables, and the wording table of Step 2;
  - `record_expiry` as the single writer;
  - the Part G decision table, including the no-metadata `Source` kept in memory only, how "no
    metadata" is recognised, and the file store's own sidecar synthesis
    (`STORE-NO-READ-ONLY-ADAPTER`).
- **`COMMAND_REGISTRATION_GUIDE:123`:** its `context.evaluate` + `asset.get()` example is rewritten
  to `wait_for_dependency`.
- **`ENVIRONMENT_CONFIG`:** the three YAML keys.
- **`UNITTEST_GUIDE`:** `tests/common/manager_scenarios.rs` and `tests/common/minimal_manager.rs`.

### Design, Capability, and Cross-Links

- **`specs/README.md`, the capability line for this design:** it moves from `designing` to
  `documented` in Phase 5, pointing at `reference/DEPENDENCIES_STATUS.md`.
- **New capability line:** "Asset managers outside core", pointing at the new guide.
- **`STORE_IMPLEMENTATION_GUIDE.md`:** a "see also" link to the new guide.

### Phase 5 Evidence Capture

Phase 5 must be able to quote these. Collect them while implementing:

- **Log lines:** the real log line for each of the seven causes, from
  `every_cause_writes_a_log_line -- --nocapture`.
- **External manager:** any primitive that the from-scratch external manager needed and Phase 2 did
  not list (Step 11; `KeyMutationAccess` is already known).
- **Test outcomes:** whether any existing test changed outcome because content versions now carry
  the flag (Step 1), and why.
- **Lock ordering:** anything surprising about it in Steps 4 and 9.
- **Timing:** the measured cost of on-read verification on the largest test value, if it is
  noticeable.

## Phase 5 Entry Criteria

Phase 5 starts only when all of these hold:

- [ ] Steps 0–12 (with 3b) are committed, each with a green build.
- [ ] Every test in the "Test assignment index" exists under its Phase 3 name and passes, and none
      is `#[ignore]`d.
- [ ] Step 12's full matrix passes, including the wasm32 checks and `check-stubs.sh`.
- [ ] The PR is open, its number is in `DESIGN.md` `gh_pr`, and every review comment is answered or
      addressed.
- [ ] The Phase 5 evidence above has been collected.
- [ ] Every newly discovered defect is filed under `specs/issues/` (§4.8).

## Execution Options

After approval:
- **Execute now:** run Steps 0–12 in order with the agents above, report after each step that
  needed a decision, and open a PR at the end.
- **Create a task list** for later execution.
- **Revise the plan** (stay in Phase 4).
- **Exit:** implement manually; Phase 5 remains outstanding.

## Phase 4 review (2026-10-02)

Four parallel reviewers, then a final reviewer. Three found nothing; reviewer 3 found that the step
test lists used names invented here instead of Phase 3's, leaving about 90 Phase 3 tests
unassigned. The final review rewrote every step's tests with Phase 3's names, added the index, and
checked the plan against the code. Changes it made, besides the test lists:

- The content-site switch to `from_content` moved from Step 9 to Step 1, because Steps 4–7 test
  against flagged versions.
- Step 3b added, so shared scenarios can be added in the steps whose code they test.
- Step 5 depends on Step 4 (it passes a reason), not on nothing.
- `record_expiry_is_called_for_every_expired_asset` moved from Step 4 to Step 11 (it needs the
  external manager), and `every_route_persists_its_reason` / `every_cause_writes_a_log_line` to
  Step 10 (they need the stale-dependency route); the `#[ignore]`-row mechanism is gone.
- `KeyMutationAccess` is a second seal on `AssetManager` (Step 11; Phase 2 F1 corrected).
- The finalize stale branch writes `Expired` without `mark_expired_status`, and its dependents are
  expired at `:2927`, not by `cascade_expire_dependents` (Step 4).
- `record_expiry` must take its manager and subject from the lock guard (Step 4), and
  `apply_external_change` must never run under an asset's `data` lock (Step 9).
- The `on_load` check must not use `Version::matches` for a current 0 (Step 7).
- The log wording is fixed (Step 2), resolving Phase 2's rule-versus-example inconsistency.
- "No metadata" is defined operationally, because the file store writes a sidecar on a bare file's
  first read (Step 9).
- `expiry_reason` is cleared when the status leaves `Expired` (Step 2); `AssetInfo` is projected in
  both places (Step 2); the `liquers-py` getter Phase 2 asked for is planned (Step 2).
- `is_store_resolvable` moved to Step 2; `AuditFinding` is defined in `assets.rs` from Step 3.
- Every `version()` caller is listed (Step 6); `liquers-records` and `liquers-axum` tests are in
  Step 12; `check-stubs.sh` runs with `--build`.
- The estimate rose from 5–7 to 8–11 days, for about 140 tests.

## Approval (2026-10-02)

Approved by the owner, together with the three answers recorded in `DESIGN.md`. The three answers
match the assumptions this plan already makes, so no step changes.
