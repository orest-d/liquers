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
semantics for cross-*implementation* portability is a **larger, breaking** change recommended as a
**follow-up WP** (see `type-information-model.md` proposal D6; ties to WP-11).

## Normative rules (design owner) → the data-format registry

The intended semantics are pinned by six rules (full text + code reconciliation in
`type-information-model.md` §"Normative rules"):

1. `(type_identifier, data_format)` is the ser/deser key.
2. `data_format` is primary and **normalized** (lowercase + alias-folded, `JPG → jpeg`);
   extension is only a fallback/default; storage under an arbitrary extension is legal;
   deserialization always uses the metadata `data_format`.
3. every extension maps to a normalized `data_format` (total).
4. every `data_format` has a default `type_identifier` ⇒ any file is deserializable from its
   extension (ultimately to `bytes`).
5. roundtrip is **idempotent** from the second roundtrip on; `data_format`/`type_identifier` are
   written to metadata on serialize and preserved thereafter.
6. `media_type` is a function of `data_format`.
7. `type_identifier` is **canonical (most specific)**: each value maps to exactly one identifier
   (a `Value::Text` is always `text`, never `generic`), so deserialization is a left inverse up to
   canonicalization and R5's fixed point holds. `generic` is reserved for values with no
   more-specific identifier. Example: `"xxx"` stored as `(generic, json)` and as `(text, txt)` both
   deserialize to `Value::Text`, whose canonical id is `text` — so only `text` is a stable
   roundtrip; `generic` for a string would break R5.

Rules 2–4 and 6 are all lookups keyed by a normalized `data_format`, so WP-4's backbone becomes a
**data-format registry** (serializer/value-provided, extensible per implementation):
`normalize`, `extension_to_data_format`, `data_format_default_extension`,
`data_format_default_type_identifier`, `data_format_media_type`, `serializer_supports`.
`normalize_type_fields()` and `validate_for_storage()` both consult it; the six fields stop being
derived independently. This resolves FINDINGS Key Gap 3 (ownership = serializer-provided) and the
extension-vs-`data_format` conflict (rule 2: `data_format` wins).

## Open Questions

1. **State-sync item may already be partially done.** WP-4 design item 1 says `State::with_data()`
   should sync `type_identifier`/`type_name` (claiming "today only `with_metadata` does"), but
   current `state.rs:88` already calls `sync_metadata_with_value` in `with_data`. Similarly
   `set_filename`/`set_extension` already sync `media_type` (but not `data_format`). Phase 2 must
   re-audit which FINDINGS gaps are still open vs. already fixed (avoid re-implementing).
2. **`deserialize_from_binary()` — delete or fix?** Confirmed unused; the live store-load path
   `deserialize_stored_value`/`try_fast_track` already uses `get_data_format()`
   (`assets.rs:317`). Prefer deletion over fixing dead code.
3. **Format/type registry ownership:** *resolved* by the normative rules → serializer/value-provided
   data-format registry (see above), not a static core map. FINDINGS "Key Gaps" §3.
4. **Extension-vs-`data_format` conflict policy:** *resolved* by rule 2 → `data_format` always wins;
   extension is a fallback/default only, arbitrary storage extension is legal.
5. **Scope decision for the user:** WP-4 = consistency + validation + the data-format registry +
   idempotence tests (rules 1–6). The cross-*implementation* `identifier`-portability redefinition
   (logical `"dataframe"` vs `"polars_dataframe"`, proposal D6) is recommended as a **separate WP**
   (needs a stored-metadata migration story). Confirm this split.
6. **Registry extensibility mechanism:** how does `liquers-lib` add rows (`csv:*`, `parquet`,
   `jpeg`) to a core-defined registry — trait method returning entries, inventory/`linkme`-style
   registration, or an `Environment`-held registry object? Decide in Phase 2.
7. **Idempotence vs `sync_metadata_with_value`:** *resolved* by R7 — `identifier(deserialized value)`
   equals the stored `type_identifier` by construction (canonical), so `from_value_and_metadata`
   overwriting it (`state.rs:52`) is a safe no-op; a disagreement is an inconsistency that
   `validate_for_storage` normalizes-to-canonical (+warn) or rejects (strict). Latent bug to fix +
   test: the txt codec's `(generic, txt) → Value::Text` path (`value.rs:857`) is non-canonical.

## References

- `specs/metadata-consistency/FINDINGS.md` (candidate invariants, gap inventory)
- `specs/metadata-consistency/PROPOSED_PLAN.md`
- `specs/ASSET_SET_OPERATION.md` (already specifies mandatory `data_format`/`type_identifier`)
- `specs/ISSUES.md` — issue `METADATA-CONSISTENCY`
- `plan20260707.md` — WP-4 (this design turns it into the 4-phase workflow)
