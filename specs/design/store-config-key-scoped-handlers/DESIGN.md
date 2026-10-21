---
id: STORE-CONFIG-KEY-SCOPED-HANDLERS
kind: design
title: A store_config chain can scope handlers to a key or folder with -R-key/
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/store, store/config, core/plan]
issues: [STORE-CONFIG-SPEC-CANNOT-SCOPE-HANDLERS-TO-A-KEY]
created: 2026-10-20
---
# A store_config chain can scope handlers to a key or folder with -R-key/

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written, not approved, nothing
implemented.

## Phase 1: High-Level Design

### Purpose

Let one handler chain apply some steps to every key of a mount and others only under a folder or to
one key, so a mount that mixes content (Markdown notes beside data files) needs neither a mount per
kind nor handlers that guess. The syntax is the one `design/store-adapters/` reserved: a resource
segment with the `key` instruction.

### Problem Example

```yaml
- type: filesystem
  prefix: kb
  config: { path: ./kb }
  metadata:
    on_validate: -R-key/kb/notes/-/require-title
    on_read: media_type-fill/file_facts/-R-key/kb/notes/-/front_matter
```

Today: the build fails — `StoreConfigFunctions::compile_plan` (`liquers-core/src/store/realm.rs`)
refuses the `UseKeyValue` step: "Store configuration 'on_validate: -R-key/kb/notes/-store_config/
require-title': contains a resource segment, which is not supported in store configuration yet".
Should be: the store builds; `set` of `kb/notes/a.md` without a title is refused, `set` of
`kb/data/x.csv` without one succeeds, and only files under `kb/notes` get `front_matter`.
(The issue's own example, `front_matter-title`, does not plan: `front_matter`'s first parameter is
`mode`; `front_matter-overwrite-title` is the spelling.)

### Scope and Acceptance Criteria

- **AC-1** A guarded step runs only under its key
  WHEN a chain `A/-R-key/<k>/-/B` runs for a key equal to `<k>` or below it
  THEN `A` and then `B` run; for any other key only `A` runs
- **AC-2** Several guards apply in order
  WHEN a chain has guards `-R-key/<k1>/-/B/-R-key/<k2>/-/C` and a key is under both `<k1>` and `<k2>`
  THEN `B` then `C` run, in spec order; a key under one guard gets only that guard's steps
- **AC-3** Bytes are read only for a matching key
  WHEN a guarded step needs the bytes and the key is outside its guard
  THEN the chain's `needs_bytes` is false for that key (no read is caused by the guarded step)
- **AC-4** Verification follows the guard
  WHEN `on_read` has `-R-key/<k>/-/content_hash`
  THEN `version_verification` is `OnRead` for keys under `<k>` and unchanged (the unguarded
  policy, or `None`) for other keys
- **AC-5** Mistakes fail the build
  WHEN a spec has a guard followed by no handler (`-R-key/kb/notes/front_matter`, missing `/-/`),
  a relative guard key (`-R-key/./x`), a guard in `storage` or `writes`, or a resource segment
  other than `-R-key/` (`-R/kb/notes/-/front_matter`)
  THEN the build fails with a `configuration_error` naming the field, and for the last case saying
  to write `-R-key/`
- **AC-6** Unscoped chains are unchanged
  WHEN a spec has no resource segment
  THEN it compiles and runs exactly as at HEAD (existing `realm.rs` and `pipeline.rs` tests pass)

Non-goals: patterns or globs (`*.md`); excluding a key (`not`); first-match semantics; guards in
`storage` or `writes`; a guard that selects by media type.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — changes the accepted configuration language and adds `pub`
  fields to `PipelineParts` (rule 4).
- **Leading issue:** **Open design - match rule:** prefix (recommended) or exact key.
- **Explanation:** a contained change to the compiler in `liquers-core/src/store/realm.rs` and the
  pipeline's verification answer; the planner already produces the needed steps (validated).
  Phases 3-4 assume the recommendations.
- **Open questions:**
  1. **Open design - match rule:** a guard matches its key and every key below it,
     segment-wise (`Key::has_key_prefix`, recommended: one guard covers a folder, which is the use
     case; an exact key is the special case of a leaf), or only the exact key (what `-R-key/` names
     elsewhere; a folder would need a pattern).
  2. **Open design - combination:** every matching guard's steps run, in spec order, after the
     unguarded steps before them (recommended: it is how the spec reads and keeps "later wins"),
     or only the first matching guard (needs an else-branch nobody asked for).
  3. **Proposed resolution - accepted segment:** only `-R-key/` (`UseKeyValue`) is a guard;
     `-R/…` (`GetAsset`) and the other instructions stay refused with a hint (recommended: the
     guard is a key, not data; keeps `-R/` free for a later meaning).
  4. **Proposed resolution - patterns:** none now. `front_matter` and `markdown_tiers` already
     ignore non-Markdown keys; a pattern can be added later as another resource instruction
     without changing guards.

### Design Dependencies

- `store-handler-field-access` — **overlaps**, weak: both change `store_config` compilation, in
  different functions (`compile_plan` here, the handlers' field access there). No order.
- `central-storage-model` — **overlaps**, weak: under central storage a guard names router keys,
  which this design handles without change.
- `store-adapters` (complete, E4) — origin of the reserved syntax and of `FactsIn::key`.

### Consolidated Findings

- The planner already emits `UseKeyValue(<k>)` followed by the guarded `Action`s, also after
  unguarded actions and repeatedly (`liquers-validate`, four forms), so no planner change.
- A guard is a wrapper around each `CompiledStep` (both `run` and `needs_bytes`); the chain shape,
  `CompiledFn` and every handler stay unchanged.
- `version_verification` is answered per key already (`AsyncStore::version_verification(&Key)`),
  so AC-4 only needs the pipeline to hold per-prefix rules.
- `fill_realm` leaves a resource segment as it is, so a headerless `-R-key/kb/notes/-/front_matter`
  is filled to `-R-key/kb/notes/-store_config/front_matter`; a spec forgetting `/-/` parses as one
  long key and is caught by AC-5.
- `REALMS.md`, `STORE_CONFIG_GUIDE.md` §3 and §8, and `STORE_SEMANTICS.md` §14 state the refusal;
  all change.

## Phase 2: Architecture

### Solution

`StoreConfigFunctions::compile_plan` gains a `guard: &mut Option<Key>` threaded through the walk
(nested `Plan` steps share it):

- `Step::UseKeyValue(key)`: refuse a relative key (`key.as_absolute()` error → `configuration_error`);
  if the previous guard collected no action, refuse ("a resource segment … is followed by no
  handler"); set `*guard = Some(key)`.
- `Step::Action`: build the step as today, then, when a guard is set, wrap it with
  `guarded(key, step)`.
- `Step::GetAsset`, `GetResource`, `GetResourceMetadata`, `GetResourceDirectory`: refused, the
  message naming `-R-key/` as the way to scope handlers. Other refusals unchanged.
- After the walk, a guard with no action is refused (AC-5).

```rust
/// Runs `step` only for keys at or below `prefix`.
fn guarded(prefix: Key, step: CompiledStep) -> CompiledStep   // private, realm.rs
```

`guarded` returns a `CompiledStep` whose `needs_bytes` is `facts.key.has_key_prefix(&prefix) &&
inner(facts)` and whose `run` returns `Ok(())` for a key outside the prefix. Both closures `move`
the prefix (`Key` is `Send + Sync`); the wasm32 aliases are unchanged.

`compile_component` keeps refusing every non-`Action` step, with the message "storage and writes
apply to the whole mount".

Version verification: `version_verification(spec, registry)` keeps its signature and returns the
unguarded policy. A new

```rust
pub fn version_verification_rules(&self, spec: &str, registry: &CommandMetadataRegistry)
    -> Result<Vec<(Key, VersionVerification)>, Error>;
```

returns the guarded `content_hash` policies in spec order. `PipelineParts` gains `pub
version_verification_rules: Vec<(Key, VersionVerification)>` (default empty);
`MetadataPipelineStore::version_verification(key)` returns the last rule whose prefix covers `key`,
else the unguarded value. `store_factory.rs` `pipeline()` fills both.

Rejected: exact match (Q1); first match (Q2); a `guard-<key>` handler entry instead of the resource
segment (a `/` in the key would need `~I` escaping and it hides the scope in a parameter); globs in
the key (`*` is a legal key character).

Known-issue preflight: none blocking. Command namespace: realm `store_config` only; no command or
registry change.

### Changes

- `liquers-core/src/store/realm.rs`: `compile_plan` (guard), `guarded`, `CompileContext::refuse_step`
  message for `GetAsset`/`GetResource*`, `compile` unchanged in shape, `version_verification_rules`.
- `liquers-core/src/store/pipeline.rs`: `PipelineParts::version_verification_rules`,
  `MetadataPipelineStore` field and `version_verification`.
- `liquers-core/src/store_factory.rs` `pipeline()`: fill the rules.
- Documents: `REALMS.md` (accepted steps table, "the refusals are deliberate places to extend"),
  `STORE_CONFIG_GUIDE.md` §3 ("Only actions" bullet becomes the guard rule), §7 (the validator does
  not check guard placement), §8 (new messages), `STORE_SEMANTICS.md` §14; `STORE-CONFIG-SPEC-…`
  link removed from the guide; History rows and `reviewed:` bumps.

Errors: `Error::configuration_error` through `CompileContext::error`, at build. Sync, as the
compiler.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| A guarded `content_hash` leaves other keys' records versionless while the mount verifies | per-prefix rules (AC-4) | high |
| Wrapping adds a prefix test per step per record | segment compare of a short key; negligible next to I/O | high |
| Message steps after a guard are logged for every key | messages stay unguarded (they are planner notes, not handlers) | medium |
| Existing test asserting the resource refusal | `realm.rs` test updated to the new messages | high |

Recovery: revert; specs without resource segments are unaffected throughout.

## Phase 3: Examples and Tests

### Examples

Validated (`liquers-validate`, filled form):

- `-R-key/docs/-store_config/front_matter` → `UseKeyValue[docs]`, `Action front_matter`
- `-store_config/media_type-fill/file_facts/-R-key/docs/-store_config/front_matter/-R-key/data/-store_config/content_hash-always`
  → two unguarded actions, then two guards (AC-2, AC-4)
- `-R-key/notes/-store_config/require-title` → the Problem Example's validator
- `-R-key/docs/front_matter` → one `UseKeyValue` of key `docs/front_matter`, no action (AC-5's
  missing `/-/`)

### Tests

`liquers-core/src/store/realm.rs` `mod tests`:

- `guard_runs_steps_only_under_its_prefix` — `set-title-T` behind `-R-key/a/b/-/`; keys `a/b`,
  `a/b/c.md`, `a/bc`, `x`: AC-1
- `guards_apply_in_spec_order` — two overlapping guards setting `title`; the later wins for a key
  under both: AC-2
- `guarded_step_needs_no_bytes_outside_its_prefix` — `content_hash-always` guarded; `needs_bytes`
  per key: AC-3
- `guard_mistakes_fail_at_compile` — no handler after a guard, relative key, `-R/…`, `-R-meta/…`,
  a guard in a `storage` spec: AC-5
- `unscoped_specs_compile_as_before` — the default specs of `METADATA_DEFAULTS` produce the same
  results as at HEAD on a fixed record: AC-6
- `version_verification_rules_follow_guards` — AC-4

`liquers-core/src/store/pipeline.rs` `mod tests`: `pipeline_answers_verification_per_key` — AC-4.
`liquers-core/tests/store_adapters.rs`: `key_scoped_validator_and_reader` — the Problem Example over
a memory mount: AC-1, AC-5 (the `-R/` variant fails the build).

Run: `cargo test -p liquers-core --lib store::` and `cargo test -p liquers-core --test store_adapters`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `store/realm.rs` — `guarded`, the guard in `compile_plan`, refusal messages; tests `guard_*`, `unscoped_specs_compile_as_before` — `cargo test -p liquers-core --lib store::realm`. Rollback: revert.
- [ ] 2. `store/realm.rs` `version_verification_rules`; `store/pipeline.rs` `PipelineParts` field and `version_verification`; `store_factory.rs` `pipeline()` — `version_verification_rules_follow_guards`, `pipeline_answers_verification_per_key`. Depends on 1. Rollback: revert step 2 (guards still work; verification stays mount-wide).
- [ ] 3. `tests/store_adapters.rs` `key_scoped_validator_and_reader` — `cargo test -p liquers-core --test store_adapters key_scoped`. Depends on 1-2.
- [ ] 4. Documents: `REALMS.md`, `STORE_CONFIG_GUIDE.md` §3, §7, §8, `STORE_SEMANTICS.md` §14; History rows, `reviewed:`; close `STORE-CONFIG-SPEC-CANNOT-SCOPE-HANDLERS-TO-A-KEY` — `python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-lib --lib --tests` (the lib
entries compile through the same compiler); `bash scripts/check-build-matrix.sh` (wasm32: the
closures use the existing aliases); diff review confined to `liquers-core/src/store*` and the
documents.
