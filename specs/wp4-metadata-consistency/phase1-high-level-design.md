# Phase 1: High-Level Design - WP-4 Metadata Format/Type Consistency

## Feature Name

Metadata Format/Type Consistency (WP-4, review finding F-5)

## Purpose

Guarantee that a stored asset's metadata always describes its bytes truthfully: the
`data_format` used to *serialize* a `State` is provably the one used to *deserialize* it,
`State` mutations keep the metadata type fields (`type_identifier`/`type_name`) in sync with
the value, and metadata arriving through external writes (`set`/`set_state`) is validated so
inconsistency is caught at **write** time instead of surfacing as a corrupt read later.
Policy is warn-first (normalize + log) with an opt-in strict mode that rejects.

## Example of the Issue (why this matters)

`State::as_bytes()` serializes using `metadata.get_data_format()` (`state.rs`), but the read
helper `deserialize_from_binary()` chooses the codec from the **filename extension**
(`assets.rs`, via `metadata.extension()`), not from `data_format`. So a state carrying
`data_format = "json"` but `filename = "weird.txt"` is written as JSON yet a naive reader keys
off `.txt` and parses it as plain text — the format used to write is not provably the one used
to read. Compounding this, `AssetManager::set()`/`set_state()` persist caller-supplied metadata
with **no validation**: a client can store metadata whose `data_format` is not supported by the
active serializer, and the failure only appears much later, at deserialization, far from the
write that caused it (delayed, hard-to-attribute failure). WP-4 closes both the read/write
asymmetry and the unvalidated-write gap. (`specs/ASSET_SET_OPERATION.md` already *demands* this
validation but the code does not enforce it.)

## Core Interactions

### Query System
No query-language or Key-encoding changes. Consistency is enforced on the value/metadata layer,
which every query result flows through.

### Store System
Serialize (write) and deserialize (read) paths must agree on `data_format`. The read path stops
selecting a codec by filename extension. `set()`/`set_state()` validate metadata before persisting.

### Command System
No new user-facing commands. Commands that call `context.set_filename()`/`set_extension()`
benefit automatically because those setters will sync `media_type`/`data_format`.

### Asset System
`AssetManager::set()`/`set_state()` gain metadata validation; `DefaultAssetManager` gains a
`strict_metadata: bool` mode (default `false` = normalize + warn into the metadata log; `true`
= reject with `Error`).

### Value Types
No new `ExtValue` variants. Uses existing `ValueInterface::identifier()`/`type_name()` and the
serializer's supported-format set for the capability check.

### Web/API
No new endpoints. `liquers-axum` write handlers inherit the validation policy; must be audited
so a rejected write returns a clean error rather than a 500.

### UI
Not applicable.

## Crate Placement

**liquers-core** — primary and only crate: `state.rs` (State mutators), `metadata.rs`
(`set_filename`/`set_extension` sync + `validate_for_storage`), `assets.rs` (read-path fix,
`set`/`set_state` validation, `strict_metadata` flag). `cargo check -p liquers-py` after any
public core-type change (CLAUDE.md rule). Spec update in `specs/ASSET_SET_OPERATION.md`.

## The six type-information concepts (scope clarification)

WP-4 touches six overlapping type descriptors that today are individually useful but mutually
inconsistent. A full code-verified analysis — how each is produced/consumed and where it is
shaky — is in the companion `type-information-model.md`. Summary:

1. **enum variant** (internal representation) — the "prefer most rust-native variant" rule is a
   convention, unenforced (no `canonicalize()`).
2. **`type_name`** — "detailed/debug" type; but for rich types it equals `identifier`
   (`"polars_dataframe"`), so its "more detailed" contract is not uniformly honored.
3. **`type_identifier`** — the reconstruction key, but it (a) bakes the *representation* into the
   logical type (`"polars_dataframe"` not `"dataframe"`), breaking cross-implementation
   portability, and (b) is lossy for primitives (`bool/i64/… → "generic"`, which the text codec
   turns back into a string).
4. **`data_format`** — serialization codec, can be finer than the extension (`"csv:tab"`), but
   `default_data_format()` falls back to the bare extension so that specificity is lost by default;
   no capability check that the serializer actually supports it.
5. **file extension** — the fallback *guess* for `data_format`/`media_type` when metadata is
   absent; today it feeds three independent derivation chains with no single entry point.
6. **`media_type`** — web/dataurl MIME; derived from the extension in parallel to `data_format`,
   so the two can drift.

This clarifies WP-4's boundary: WP-4 delivers the **consistency + validation** layer (single
normalization routine, `validate_for_storage`, serializer capability registry, kill the dead
extension-based read helper, derive `media_type` from `data_format`). Redefining `identifier`
semantics for portability and adding `canonicalize()` are **larger, breaking** changes recommended
as a **follow-up WP** (see `type-information-model.md` proposals D6–D8; ties to WP-11).

## Open Questions

1. **State-sync item may already be partially done.** WP-4 design item 1 says `State::with_data()`
   should sync `type_identifier`/`type_name` (claiming "today only `with_metadata` does"), but
   current `state.rs:88` already calls `sync_metadata_with_value` in `with_data`. Similarly
   `set_filename`/`set_extension` already sync `media_type` (but not `data_format`). Phase 2 must
   re-audit which FINDINGS gaps are still open vs. already fixed (avoid re-implementing).
2. **`deserialize_from_binary()` — delete or fix?** Confirmed unused; the live store-load path
   `deserialize_stored_value`/`try_fast_track` already uses `get_data_format()`
   (`assets.rs:317`). Prefer deletion over fixing dead code.
3. **Format/type registry ownership:** static supported-format map in `liquers-core` vs. a
   serializer-provided capability check (`supports(type_identifier, data_format)`). Recommend
   serializer-provided. FINDINGS "Key Gaps" §3.
4. **Extension-vs-`data_format` conflict policy:** canonicalize filename to match `data_format`,
   or keep both and only warn? (FINDINGS candidate invariant 4.) Proposed precedence: explicit
   `data_format` > explicit extension > value default; `media_type` derived from `data_format`.
5. **Scope decision for the user:** does WP-4 stay purely consistency/validation (D1–D5), or does
   it also absorb the `identifier`-portability redefinition (D6)? The latter needs a stored-metadata
   migration story and is recommended as a separate WP.

## References

- `specs/metadata-consistency/FINDINGS.md` (candidate invariants, gap inventory)
- `specs/metadata-consistency/PROPOSED_PLAN.md`
- `specs/ASSET_SET_OPERATION.md` (already specifies mandatory `data_format`/`type_identifier`)
- `specs/ISSUES.md` — issue `METADATA-CONSISTENCY`
- `plan20260707.md` — WP-4 (this design turns it into the 4-phase workflow)
