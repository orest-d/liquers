# WP-4 Supporting Analysis: The Six Type-Information Concepts

Verified against the current code (2026-07). This document maps how the six related type
descriptors are *produced* and *consumed*, confirms where each is shaky, and proposes a
tightened model. It refines the Phase 1 high-level design and feeds Phase 2.

## The six concepts at a glance

| # | Concept | Layer | Authoritative source | Role |
|---|---------|-------|----------------------|------|
| 1 | Enum variant (internal representation) | value impl | the `Value`/`ExtValue` enum itself | how the value lives in RAM |
| 2 | `type_name` | value + metadata | `ValueInterface::type_name()` | detailed/debug type string |
| 3 | `type_identifier` (`identifier()`) | value + metadata | `ValueInterface::identifier()` | logical type; drives reconstruction |
| 4 | `data_format` | metadata | `metadata.data_format` / `default_data_format()` | serialization codec (may refine extension) |
| 5 | file extension | metadata | `filename` | fallback *guess* for `data_format`/`media_type` |
| 6 | `media_type` | metadata | `metadata.media_type` | web/dataurl MIME type |

---

## 1. Enum variant (internal representation)

**Definition.** `Value = CombinedValue<SimpleValue, ExtValue>`
(`liquers-lib/src/value/mod.rs:222`). `SimpleValue` holds `Text/I32/I64/F64/Bool/Bytes/…`;
`ExtValue` holds `PolarsDataFrame { Arc<DataFrame> }`, `Image { … }`, etc.
(`liquers-lib/src/value/mod.rs:23`). In pure core, `Value` is the enum in
`liquers-core/src/value.rs:361`.

**The "prefer rust-native" rule is a convention, not code.** `try_from_json_value`
(`value.rs:485`) maps JSON into native variants (string→`Text`, number→`I64/F64`), which is
good. But nothing *normalizes* an already-built value: if a richer impl could hold a string as
either `Text` or a `PythonValue` wrapper, there is no `canonicalize()` that collapses it to the
most native variant. So two code paths can produce the same logical string in two different
variants, and everything downstream (`identifier`, serialization) then diverges.

**Problem P1.** The canonical-representation rule is unenforced. There is no
`ValueInterface::canonicalize()` and no normalization boundary at `State` construction.

---

## 2. `type_name` — "detailed type for debugging"

**Definition.** `ValueInterface::type_name()` (`value.rs:197`, doc: "more detailed than
identifier … serves for information and debugging"; carries a `// TODO: Rename to
detailed_type_identifier?`). Persisted as `MetadataRecord.type_name`
(`metadata.rs:621`, `#[serde(default)]` — recently added).

**Consumed by.** Error/diagnostic messages (`conversion_error(value.type_name(), …)`,
`value.rs:760`, and the serializer error arms), and UI/debug surfaces.

**Problem P2 — granularity contract is not honored uniformly.**
- For `SimpleValue`, `type_name` *is* more detailed than `identifier`: it distinguishes
  `bool/i32/i64/f64/array/object` (`value.rs:381`) where `identifier` collapses them (see §3).
- For `ExtValue`, `identifier() == type_name() == "polars_dataframe"`
  (`mod.rs:117` and `mod.rs:130`) — identical, so `type_name` adds nothing exactly where the
  representation detail would matter most.

So `type_name`'s promise ("always more detailed than identifier") holds for primitives and is
empty for rich types. Its meaning is effectively undefined.

---

## 3. `type_identifier` — "the true type identifier"

**Definition.** `ValueInterface::identifier()` (`value.rs:192`, `// TODO: Rename to
type_identifier?`). Persisted as `MetadataRecord.type_identifier`. It is the field the
deserializer uses to decide **which value to reconstruct**:
`deserialize_from_bytes(b, type_identifier, data_format)` (`value.rs:789`) — `data_format`
picks the codec, `type_identifier` picks the target type.

**Two opposite failures, both confirmed in code:**

**Problem P3a — representation leaks into the identifier (the user's dataframe case).**
`ExtValue::PolarsDataFrame` reports identifier `"polars_dataframe"` (`mod.rs:117`), and
`deserialize_from_bytes` matches literally on `"polars_dataframe"` (`mod.rs:207`). The identifier
therefore encodes the *internal representation* (`polars`), not the logical type (`dataframe`).
Consequence: a second Liquers deployment that represents a dataframe as a list-of-dicts (`object`)
would stamp a *different* identifier for the same CSV, and could not reconstruct the first
deployment's stored asset by identifier — even though both "understand dataframes." Stored
metadata is **not portable across value-type implementations.** This is exactly the ambiguity
the review flagged.

**Problem P3b — identifier is lossy for primitives.** `Value::identifier()` collapses
`None/Bool/I32/I64/F64/Array` all to `"generic"` (`value.rs:361`). On the text codec,
`"generic"` deserializes to `Text` (`value.rs:857`), so a `Bool`/`I64` round-tripped through
`txt`+`generic` comes back a **string**. Round-tripping only survives via `json` (serde ignores
the identifier). So the identifier is simultaneously *too coarse* (`generic`) for primitives and
*too specific* (`polars_dataframe`) for rich types.

---

## 4. `data_format` — serialization codec

**Definition.** `MetadataRecord.data_format: Option<String>` (`metadata.rs:626`); the serializer
contract is `as_bytes(data_format)` / `deserialize_from_bytes(…, data_format)` (`value.rs:784`).
It can be finer than the extension: Polars parses `"csv:comma"`, `"csv:tab"`, `"csv:semicolon"`,
`"csv:pipe"` (`liquers-lib/src/polars/serde.rs:25`) — confirming the user's point 4.

**Problem P4a — the default throws the extra specificity away.**
`default_data_format()` defaults to `default_extension()` (`value.rs:209`); for a dataframe the
default extension is `"csv"` (`mod.rs:143`). So unless a command explicitly sets `data_format`,
the separator information the field exists to carry is lost at creation time —
`get_data_format()` then returns the bare extension `"csv"`.

**Problem P4b — no capability check.** The serializer supports a finite, hardcoded set
(`value.rs:798` json/txt-family/bytes; `mod.rs:181` image/polars), but metadata accepts an
arbitrary `data_format` string with no validation, so an unsupported value is stored happily and
only fails at read (`Error::"Unsupported format"`). This is the core WP-4 gap.

**Read/write path (WP-4 headline).** Write uses `get_data_format()` (`state.rs:75` via
`State::as_bytes`). The *live* store-load path `deserialize_stored_value` → `try_fast_track`
already uses `get_data_format()` (`assets.rs:317`, `assets.rs:519`) — so the active path is
symmetric. The asymmetric helper `deserialize_from_binary()` (extension-based) is **dead**
(no callers) and should be deleted, not fixed (FINDINGS §2).

---

## 5. File extension — a *guess* when metadata is absent

**Definition.** Parsed from `filename` (`metadata.rs:1130`). It is the fallback for both
`get_data_format()` (extension → else `"bin"`, `metadata.rs:1169`) and `get_media_type()`
(extension → else `octet-stream`, `metadata.rs:1155`). This is the right idea for reading a raw
file from a mounted store that has no sidecar metadata (user point 5).

**Problem P5 — one string, three independent derivations, no single entry point.** The extension
feeds `data_format`, `media_type`, and display name through three separate fallback chains. There
is no single "infer metadata from a bare filename" function; the inference is smeared across
getters, so the guesses can disagree and there is no one place to make the extension→format
mapping authoritative.

---

## 6. `media_type` — web / dataurl MIME

**Definition.** `MetadataRecord.media_type` (`metadata.rs:636`); `get_media_type()` returns the
stored value else an extension→MIME lookup (`file_extension_to_media_type`, `metadata.rs:1155`).

**Consumed by.** Axum `Content-Type` (`liquers-axum/src/axum_integration.rs:52`); image dataurl
(`data:{mime};base64,…`, `liquers-lib/src/image/format.rs:98`); egui asset label
(`egui/widgets.rs:854`).

**Problem P6 — parallel, drift-prone derivation.** `media_type` is derived from the *extension*
via a mapping that is entirely separate from the extension→`data_format` mapping, so the two can
disagree (e.g. `data_format="csv:tab"` but `media_type` still `text/csv`). `media_type` is really
a function of `data_format`, but it is stored as an independent field. Setters are inconsistent:
`set_filename`/`set_extension` now sync `media_type` (`metadata.rs:1125`, `:1153`) but **not**
`data_format`; `with_filename` (`metadata.rs:708`) does set `data_format` from the extension.

---

## Cross-cutting diagnosis

1. **Four independently stored, mutually derivable fields.** `type_identifier`, `type_name`,
   `data_format`, `media_type` (plus `filename`) are each stored, each with its own fallback, and
   no single function guarantees they agree. WP-4's setter-sync work treats symptoms; the root
   cause is the absence of one normalization routine and a defined precedence order.
2. **`identifier` conflates logical type and physical representation** (P3a) while being lossy for
   primitives (P3b). This is a *semantics* problem, larger than WP-4's sync/validation scope.
3. **No serializer capability registry.** `(type_identifier, data_format)` validity is knowable
   only by attempting (de)serialization. FINDINGS "Key Gap 3" is unresolved.
4. **`media_type` and `type_name` are non-authoritative** yet stored as if authoritative.

---

## Proposed model

### Precedence (single normalization DAG)
```
value ─┬─► default identifier   (logical type)
       ├─► default type_name    (detailed/debug)
       ├─► default data_format  (may be finer than extension)
       └─► default media_type

filename ─► extension ─► (guessed data_format, guessed media_type)   [only when nothing explicit]

explicit data_format  overrides  extension guess
media_type          derived-from  data_format (data_format→MIME map), extension only last resort
```
Every `State`/metadata mutator (`with_data`, `with_metadata`, `set_filename`, `set_extension`,
external `set`/`set_state`) routes through **one** `normalize_type_fields()` that applies this
DAG. This subsumes and generalizes WP-4's scattered setter fixes.

### Proposals, tagged by scope

**In WP-4 scope (sync + validation — do now):**
- **D1.** Add `Metadata::normalize_type_fields(value_defaults, explicit_overrides)` and route all
  setters through it; make `set_filename`/`set_extension` also sync `data_format` (closing the
  remaining half of FINDINGS §6b — `media_type` is already synced there today).
- **D2.** Add `Metadata::validate_for_storage(strict) -> Result<Vec<Warning>, Error>`: non-empty
  `type_identifier`, effective `data_format` non-empty **and supported by the serializer**,
  media/format agreement. `DefaultAssetManager.strict_metadata: bool` (default false = normalize
  + warn to log; true = reject). (WP-4 design items 3–4.)
- **D3.** Delete the dead extension-based `deserialize_from_binary()`; the live path already uses
  `get_data_format()` (WP-4 design item 2 — confirmed; prefer deletion).
- **D4.** Derive `media_type` from `data_format` in normalization (add a `data_format→MIME` map),
  demoting extension→MIME to last-resort. Removes the P6 drift source.

**Serializer capability registry (WP-4-adjacent; resolves Key Gap 3):**
- **D5.** Extend the serializer trait with `supported_data_formats()` / `supports(type_identifier,
  data_format) -> bool`; `validate_for_storage` consults it instead of hardcoding. Ownership:
  serializer-provided capability, not a static core map.

**Beyond WP-4 (semantics; recommend a follow-up WP, ties to WP-11 ValueInterface split):**
- **D6 (P3a portability).** Split logical type from representation: make `identifier()` return the
  *logical, portable* type (`"dataframe"`), match `deserialize_from_bytes` on the logical
  identifier, and let each implementation reconstruct into its own native representation. Keep the
  representation detail in `type_name` (`"polars_dataframe"`). Requires an alias/migration table
  for existing `"polars_dataframe"` metadata. This is what makes stored assets exchangeable
  between deployments.
- **D7 (P3b).** Make primitive identifiers granular enough that `(identifier, data_format)`
  uniquely determines reconstruction, or forbid the lossy `"generic"→Text` text-codec path.
- **D8 (P1).** Add `ValueInterface::canonicalize()` and apply it at `State` construction so the
  "most rust-native variant" rule is enforced, not merely documented.

### Recommended split
- **WP-4 (this design):** D1–D5 — mechanical consistency, validation, capability registry.
- **Follow-up WP (new, before/with WP-11):** D6–D8 — redefine identifier semantics, portability,
  canonicalization. These change public meaning of `identifier()`/`type_name()` and need their
  own red→green plan and a stored-metadata migration story.

---

## Normative rules (authoritative — from design owner)

These are the target invariants WP-4 (and its follow-up) must realize. They resolve the earlier
open questions.

- **R1 — (type_identifier, data_format) is the ser/deser key.** Serialization is a function of the
  value + `data_format`; deserialization is a function of `(type_identifier, data_format)`.
- **R2 — data_format is primary; extension is a fallback and a default.** `data_format` determines
  the *default* file extension, but data may be stored under an arbitrary extension. Deserialization
  relies on the `data_format` recorded in metadata, never on the stored extension. `data_format` is
  **normalized**: lowercased and alias-folded (e.g. `JPG → jpeg`, whose default extension is `jpg`).
- **R3 — every extension maps to a (normalized) data_format.** `extension → data_format` is total.
- **R4 — every data_format has a default type_identifier.** Therefore any file is deserializable
  from its extension alone (extension → data_format → default type_identifier), and in the limit
  every file is deserializable to `bytes`.
- **R5 — roundtrip is idempotent.** `state → serialize → deserialize` need not return the *same*
  state, but from the second roundtrip on it is stable: `deserialize = deserialize∘serialize∘
  deserialize`. `data_format` and `type_identifier` are written into metadata during serialization
  and preserved across every subsequent roundtrip.
- **R6 — media_type is a function of data_format.** Concretely realized as a data_format (≈ default
  extension) → media_type mapping.

### The artifact these rules imply: a **data-format registry**

R2–R4 and R6 are all lookups keyed by a normalized `data_format`. They call for one registry
(populated per value-type implementation, i.e. serializer-provided, resolving FINDINGS Key Gap 3):

```
normalize(data_format)                -> data_format          (lowercase + alias fold; R2)
extension_to_data_format(ext)         -> data_format          (total; R3)
data_format_default_extension(df)     -> extension            (R2)
data_format_default_type_identifier(df) -> type_identifier    (R4)
data_format_media_type(df)            -> media_type           (R6)
serializer_supports(type_id, df)      -> bool                 (validation; D5)
```

Core ships the primitive/json/txt/bytes rows; `liquers-lib` extends it with `csv:*`, `parquet`,
`jpeg`, … Every derivation in §4–§6 (currently ad-hoc extension string handling) routes through it.

### Reconciliation: rules vs. current code

| Rule | Today | Gap → WP-4 action |
|------|-------|-------------------|
| R1 | `deserialize_from_bytes(b, type_id, data_format)` already takes both (`value.rs:789`) | Honored; keep. Ensure the serialize path *records* both in metadata (R5). |
| R2 normalize | **No normalization** — `get_data_format()` returns the raw stored/extension string (`metadata.rs:1169`); `JPG` stays `JPG` | Add `normalize()`; apply in `get_data_format` and `validate_for_storage`. |
| R2 df→ext | `default_extension()` is value-derived; `default_data_format()` collapses *to* extension (`value.rs:209`) — inverted | Registry `data_format_default_extension`; stop deriving df from ext by default. |
| R3 ext→df | Extension is used *as* df verbatim; `jpg` never folds to `jpeg` | Registry `extension_to_data_format` (total). |
| R4 df→default type_id | **Missing** — deserialize needs a caller-supplied `type_identifier`; unknown `""` only special-cased in the txt arm (`value.rs:855`) | Registry `data_format_default_type_identifier`; use it when metadata lacks a type_id; bytes as ultimate fallback (partly present via `deserialize_stored_value`, `assets.rs:322`). |
| R5 idempotence | Not stated/tested; deserialize already yields native variants (deterministic) so idempotence likely holds, but is unproven and `sync_metadata_with_value` **overwrites** stored `type_identifier` with `value.identifier()` on `from_value_and_metadata` (`state.rs:52`) — safe only if `identifier()` round-trips exactly | Pin R5 with roundtrip tests; document that `identifier()` must be a stable pure function of the value; ensure serialize writes `(type_id, data_format)` into metadata. |
| R6 media←df | `get_media_type()` derives from **extension** (`metadata.rs:1155`), parallel to df | Registry `data_format_media_type`; derive media_type from df in normalization (D4). |

### Effect on scope decisions
- **OQ3 (registry ownership): resolved** → serializer/value-provided registry (above).
- **OQ4 (extension vs data_format conflict): resolved** by R2 → `data_format` always wins;
  extension is only a fallback/default, arbitrary storage extension is legal.
- **OQ5 (D6 portability): still a follow-up.** R1/R5 make single-system idempotence a WP-4
  invariant, which the registry + roundtrip tests deliver. Cross-*implementation* portability
  (logical `"dataframe"` vs `"polars_dataframe"`) is the multi-system generalization and remains
  a separate WP; R4's "default type_identifier per data_format" is the seam where it would later
  plug in.

### Registry proposal folds into the WP-4 set
The registry (D1–D6 above generalized) becomes WP-4's backbone: `normalize_type_fields` and
`validate_for_storage` both consult it, and the six fields stop being independently derived.

## References
- `liquers-core/src/value.rs` (`ValueInterface`, `DefaultValueSerializer`, `Value` impls)
- `liquers-core/src/metadata.rs` (`MetadataRecord` fields, getters/setters)
- `liquers-core/src/state.rs` (`sync_metadata_with_value`, `as_bytes`)
- `liquers-core/src/assets.rs:317` (`deserialize_stored_value`, live read path)
- `liquers-lib/src/value/mod.rs`, `liquers-lib/src/polars/serde.rs` (rich types, csv formats)
- `specs/metadata-consistency/FINDINGS.md`
