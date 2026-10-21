---
id: STORE-COMMAND-NAMESPACE
kind: design
title: Store commands — ns-store reads keys chosen at evaluation time, ns-store_write writes
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [lib/commands, core/store]
issues: [STORE-COMMAND-NAMESPACE-MISSING]
created: 2026-10-20
---
# Store commands — ns-store reads keys chosen at evaluation time, ns-store_write writes

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written, not approved, nothing
implemented.

## Phase 1: High-Level Design

### Purpose

Give queries and recipes the store operations that the resource syntax cannot express: reading,
listing or testing a key that is **computed** during evaluation, and writing a value to a key the
pipeline chooses. Every target (axum, web, py, MCP) gets them from `liquers-lib`, and they go
through the asset manager so caching, expiry and a future key ACL apply to them as to `-R/`.

### Problem Example

A recipe computes the name of today's input and wants to read it, then persist a cleaned copy:

```
-R/config/today.txt/-/to_text/-/ns-store/get/-/clean/-/ns-store_write/save-~X~-R-key/out/today.csv~E
```

Today: planning fails with `Action 'get' not registered` — no crate registers a command in a
`store` namespace (`specs/command_registry.yaml`), so the key held in `config/today.txt` cannot be
read, and nothing can write a result under a chosen key; a host has to do both outside the query.
Should be: `get` reads the key named by its input, through the asset manager, recording it as a
dependency; `save` stores the cleaned value at `out/today.csv` and returns that key.

**Already possible at HEAD** (resource instructions added since the issue was filed, 2026-10-10 to
2026-10-11): a key *written in the query* can be listed (`-R-dir/k`, `-R-sdir/k`), described
(`-R-meta/k`, `-R-stored_meta/k`) and read raw (`-R-stored/k`). This design does not duplicate them.

### Scope and Acceptance Criteria

- **AC-1** Read a computed key
  WHEN `ns-store/get` receives a key (a `Key` value, or text that parses as an absolute key)
  THEN it returns that key's value as `-R/<key>` would, and the asset records it as a dependency
- **AC-2** Test, describe and list a computed key
  WHEN `ns-store/exists`, `ns-store/metadata` or `ns-store/list` receives a key
  THEN it returns a boolean, the key's metadata record, or the folder's asset info, without
  evaluating any recipe (`COMMAND_REGISTRATION_GUIDE.md` §"must not evaluate it")
- **AC-3** Save a value under a chosen key
  WHEN `ns-store_write/save-<key>` runs on a state
  THEN the asset manager stores the state at `<key>` (as `set_state` does), a later `-R/<key>`
  returns it, and the command returns the key
- **AC-4** Remove a key
  WHEN `ns-store_write/remove` receives a key
  THEN the asset manager removes it, and `ns-store/exists` then returns `false`
- **AC-5** Writes are volatile and separable
  WHEN the registry is inspected
  THEN every `store_write` command is `volatile`, no `store` command writes, and the
  `liquers-agent` tool-set guard (no writing command in an allowed namespace) accepts `store`
- **AC-6** Refusals are the store's
  WHEN `save` targets a `read_only` mount, or any command gets a relative key or text that is not a
  key
  THEN it fails with the store's `NotSupported`, `KeyNotAbsolute` or a `ConversionError`, and
  nothing is written
- **AC-7** Saving the running key is refused
  WHEN `save` targets the key of the asset that is running it
  THEN it fails without writing (it would overwrite its own result)

Non-goals: authorization (`CORE-SESSION-AND-KEY-ACL`); conditional writes
(`store-conditional-write`); raw-byte writes (`set_binary`); `makedir`/`removedir`; streaming.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — adds commands (`specs/command_registry.yaml` changes,
  rule 4).
- **Leading issue:** **Open design - writes in queries:** whether `store_write` exists at all.
- **Explanation:** a `liquers-lib` module over existing `AssetManager` methods
  (`set_state`, `remove`, `contains`, `get_asset_info`, `listdir_asset_info`) and
  `Context::get_dependency_state`; both decisions the issue raised have a recommendation that
  Phases 3-4 assume.
- **Open questions:**
  1. **Open design - writes in queries** (issue question 1): offer `ns-store_write` as volatile
     commands (recommended: `ns-mem/write` and `ns-mem/edit` already write from queries the same
     way, and a recipe that persists an intermediate is the use case), or reads only.
  2. **Open design - authorization** (issue question 2): ship with the posture of `-R/` — any key
     the environment's store reaches — and let `CORE-SESSION-AND-KEY-ACL` add checks in the asset
     manager, which these commands go through (recommended), or wait for that issue.
  3. **Proposed resolution - two namespaces:** reads in `store`, writes in `store_write`, so a tool
     allow-list can name `store` safely (`COMMAND_REGISTRATION_GUIDE.md` §"Commands served as MCP
     tools").
  4. **Proposed resolution - the key travels as state for reads and as an argument for `save`**:
     reads take the key as input (`-R-key/<k>/-/ns-store/…` or a computed value); `save` takes the
     value as input and the key as `key: Key` (a literal, `~I`-escaped, or a link).

### Design Dependencies

- `store-conditional-write` — **overlaps**, weak: a later `save_if` would use its `set_if`. No order.
- `store-streaming-read-write` — **overlaps**, weak: large `get`/`save` values would benefit; no
  shared interface.
- `CORE-SESSION-AND-KEY-ACL` (issue, no design) — **overlaps**: authorization for the reads and
  writes; Q2 keeps it independent.
- `agent-memory-mvp` — precedent: writing commands registered `volatile` in `ns-mem`.

### Consolidated Findings

- Three of the issue's four gaps (listing, metadata, raw read of a key in the query text) closed at
  HEAD through `-R-dir/`, `-R-sdir/`, `-R-meta/`, `-R-stored/`; the issue is re-scoped to computed
  keys and writes.
- Going through `AssetManager`, not `AsyncStore`, keeps the asset cache coherent: `save` expires
  dependents of the overwritten key exactly as an upload does.
- `get` is not volatile: the dependency on the computed key is recorded, so the result expires with
  it. The others are volatile: they describe store state no dependency captures.
- No core change; the registry gains six commands.

## Phase 2: Architecture

### Solution

New module `liquers-lib/src/store_commands.rs`, generic over the lib environment like
`search`, with `register_store_commands!` added to `register_all_commands!` (always on; no
feature). A private `key_of(state)` takes `try_into_key()`, else the text parsed with `parse_key`,
then `as_absolute()`.

```rust
register_command!(cr, async fn get(state, context) -> result
    namespace: "store" label: "Get key" doc: "The value of the key given as input, as -R/<key>")?;
register_command!(cr, async fn exists(state, context) -> result
    namespace: "store" volatile: true doc: "Whether the key given as input is stored or has a recipe")?;
register_command!(cr, async fn metadata(state, context) -> result
    namespace: "store" volatile: true doc: "The metadata record of the key given as input")?;
register_command!(cr, async fn list(state, context) -> result
    namespace: "store" volatile: true doc: "Asset info of the folder given as input; evaluates nothing")?;
register_command!(cr, async fn save(state, key: Key (label: "Key", description: "Where to store the input"), context) -> result
    namespace: "store_write" volatile: true doc: "Stores the input at key; returns the key")?;
register_command!(cr, async fn remove(state, context) -> result
    namespace: "store_write" volatile: true doc: "Removes the key given as input; returns it")?;
```

Bodies: `get` → `context.get_dependency_state(&Query::from(key))` and its value; `exists` →
`asset_manager.contains`; `metadata` → `asset_manager.get_asset_info` for a folder, else the
store's `get_metadata` record through the asset manager's metadata read (the same path
`-R-meta/` takes, `interpreter.rs` `GetAssetMetadata`); `list` → `asset_manager.listdir_asset_info`
→ `Value::from_asset_info`; `save` → refuse when `key` equals the running asset's key (AC-7), then
`asset_manager.set_state(&key, state)` → `Value::from_key`; `remove` → `asset_manager.remove`.

Rejected: commands on `AsyncStore` directly (bypass the asset cache and its expiry); one `store`
namespace with writes (unsafe in an allow-list); key as a `String` argument for reads (a key is a
`Key`; `-R-key/` is the documented way to pass one, guide §"Passing the working directory");
`put-<key>-<text>` with inline data (`ns-mem/write` covers documents).

Known-issue preflight: `CORE-SESSION-AND-KEY-ACL` (P2) not blocking (Q2);
`JS-STORE-RESOURCE-NOT-FOUND-WITHOUT-A-RECIPE` (P2) affects `get` exactly as `-R/` — no
workaround here. Namespaces: new `store`, `store_write`; existing `mem` untouched.

### Changes

- `liquers-lib/src/store_commands.rs` (new), `lib.rs` `pub mod store_commands;`,
  `commands.rs` `register_all_commands!` line.
- `specs/command_registry.yaml` regenerated with a CHANGELOG line.
- Documents: `COMMAND_REGISTRATION_GUIDE.md` §4 file layout (the module), a short `ns-store`
  section in `specs/reference/PROJECT_OVERVIEW.md` §1 beside the resource instructions ("a key in
  the query text: `-R-…/`; a computed key: `ns-store`"); History rows, `reviewed:`.

Errors: `Error::conversion_error` for a non-key input; store and asset-manager errors pass through;
AC-7 is `Error::general_error` naming the key. Async (asset-manager calls); no blocking I/O.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| `save` inside a recipe creates a write per evaluation | volatile, documented; an overwrite of equal bytes keeps the version | high |
| `exists` true for a recipe-only key surprises | doc string states it; `metadata` shows the status | medium |
| A computed key escapes what a policy inspecting the query sees | Q2; the ACL issue owns it | medium |
| New commands change counts in registry tests | regenerate; `registry_export` enforces | high |

Recovery: remove the module and the macro line; regenerate the registry.

## Phase 3: Examples and Tests

### Examples

Validated with `liquers-validate --command store/… --command store_write/…`:

- `-R-key/data/x.csv/-/ns-store/exists`, `-R-key/data/-/ns-store/list`,
  `-R-key/data/x.csv/-/ns-store/metadata`
- `-R/config/target.txt/-/to_text/-/ns-store/get` (computed key)
- `-R/data/x.csv/-/ns-store_write/save-~X~-R-key/out/x.csv~E` and `…/save-out~Ix.csv`
- `-R-key/out/x.csv/-/ns-store_write/remove`

### Tests

`liquers-lib/tests/store_commands.rs` (memory store, `register_all_commands!`):

- `get_reads_a_computed_key_and_records_it` — `config/target.txt` holds `data/a.txt`; dependency
  listed in the asset's metadata: AC-1
- `exists_metadata_list_do_not_evaluate_recipes` — a recipe-only key: listed with status `Recipe`,
  its command never runs (counter): AC-2
- `save_then_read_back` — `save` then `-R/out/x.txt`; returned value is the key: AC-3
- `remove_then_exists_is_false` — AC-4
- `store_namespace_has_no_writing_command` — no `store` command writes (a read-only mount
  accepts all four), every `store_write` command is volatile: AC-5
- `save_to_read_only_mount_is_refused` / `relative_or_non_key_input_is_refused` — AC-6
- `save_refuses_its_own_key` — a recipe `out/self.txt` whose query saves to `out/self.txt`: AC-7

`liquers-lib --test registry_export` passes after regeneration (AC-5).
Run: `cargo test -p liquers-lib --lib --tests`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-lib/src/store_commands.rs` — `key_of`, the four `store` commands, `register_store_commands!`; `lib.rs`, `commands.rs` registration — `cargo test -p liquers-lib --test store_commands get_ exists_`. Rollback: remove the module.
- [ ] 2. Same module — `save`, `remove`, AC-7 guard; their tests — `cargo test -p liquers-lib --test store_commands`. Depends on 1. Rollback: drop the `store_write` block (reads stay).
- [ ] 3. Regenerate `specs/command_registry.yaml` with a CHANGELOG line — `cargo test -p liquers-lib --test registry_export`.
- [ ] 4. `liquers-agent` tool-set guard test still passes (no allowed namespace gains a writer) — `cargo test -p liquers-agent --test mcp_tools` (needs Rust 1.96).
- [ ] 5. Documents: `PROJECT_OVERVIEW.md` §1 note, `COMMAND_REGISTRATION_GUIDE.md` §4; History, `reviewed:`; update and close `STORE-COMMAND-NAMESPACE-MISSING` — `python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-lib --lib --tests`; `bash scripts/check-build-matrix.sh` (the module is
ungated: check the minimal and wasm rows); every query above through `liquers-validate` without
`--command` after step 3.
