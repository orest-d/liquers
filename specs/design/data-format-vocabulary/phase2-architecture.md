# Phase 2: Solution & Architecture — data-format vocabulary

## Overview

A new `liquers-core/src/data_format.rs` holds one static table of known formats, their aliases
and helpers. `TypeInfo` gains a subset list of write-only formats; `deserialize_stored_value` treats
a stored value in one like the existing "no data format" case (degraded: the fast track recomputes
quietly, a recovery read reports it). `liquers-validate` warns on an unknown filename format and
the write-path refusal gives the same hint. Core `Value` and `SimpleValue` use the constants;
`ExtValue` declares `html` (and, without polars, `parquet`) write-only.

Rejected: per-format capability records (question 2); alias folding in `supports_data_format`
(accepts spellings a codec refuses, e.g. `arrow` for polars); a runtime format registry (no
consumer); a planner `Step::Warning` for unknown extensions (would surface in every evaluation).

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `METADATA-ONLY-ENTRY-RELOADS-AS-CORRUPTED` | closed | P3 | precedent for the quiet branch | no | no | none |
| `STORED-VALUE-WITHOUT-DATA-FORMAT-READ-AS-BIN` | draft | P3 | the load check sees `bin`, never write-only | no | no | none |
| `VALUE-TYPE-DEFINITION-MACRO` | draft | P2 | will consume the vocabulary | no | no | dependency recorded |

## Interfaces

New module, sync, no allocation in the lookups:

```rust
// liquers-core/src/data_format.rs  (pub mod data_format; in lib.rs)
pub const JSON: &str = "json";   // likewise YAML, TOML, TXT, HTML, MD, CSS, JS, PY, RS, CSV, TSV,
                                 // NDJSON, IPC, PARQUET, XLSX, AVRO, PNG, JPG, WEBP, GIF, BMP, TIF,
                                 // ICO, DATAURL, BYTES ("b"), UI, WIDGET, PICKLE
/// The scalar text forms of core `Value` (`md` excluded: it is `Text` only).
pub const TEXT_FORMATS: [&str; 7] = [TXT, HTML, CSS, JS, PY, RS, JSON];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataFormatInfo {
    pub name: &'static str,                 // canonical spelling
    pub aliases: &'static [&'static str],   // other accepted spellings, refinement-free
    pub refinable: bool,                    // `csv:comma`, `csv:;` …
}
pub const DATA_FORMATS: &[DataFormatInfo];

pub fn base_format(data_format: &str) -> &str;                   // moved from type_system (was private)
pub fn data_format_info(data_format: &str) -> Option<&'static DataFormatInfo>; // name or alias of the base
pub fn canonical_data_format(data_format: &str) -> Option<&'static str>;
pub fn is_known_data_format(data_format: &str) -> bool;          // exact, lowercase, base compared
pub fn suggest_data_format(data_format: &str) -> Option<&'static str>; // None if known; else nearest
    // name/alias by case-insensitive edit distance ≤ 2, its canonical name
```

Table contents (canonical: aliases): `json`; `yaml`: `yml`; `toml`; `txt`; `html`; `md`:
`markdown`; `css`; `js`; `py`; `rs`; `csv` (refinable): `csv_comma`, `csv_semicolon`, `csv_pipe`;
`tsv`: `csv_tab`; `ndjson`: `jsonl`; `ipc`: `feather`, `arrow_ipc`, `arrow`; `parquet`; `xlsx`;
`avro`; `png`; `jpg`: `jpeg`, `jpe`; `webp`; `gif`; `bmp`; `tif`: `tiff`; `ico`; `dataurl`; `b`:
`bin`, `bytes`; `ui`; `widget`; `pickle`. A refinement is accepted on a `refinable` base only.

`TypeInfo` (`liquers-core/src/type_system.rs`):

```rust
/// Data formats this type can be **written** in; the write path refuses any other.
pub supported_data_formats: Vec<Cow<'static, str>>,
/// The subset of `supported_data_formats` that cannot be read back (presentation formats). A
/// stored value in one is recomputed, never deserialized.
#[serde(default, skip_serializing_if = "Vec::is_empty")]
pub write_only_data_formats: Vec<Cow<'static, str>>,

impl TypeInfo {
    /// Appends to both lists.
    pub fn with_write_only_data_formats<I, S>(self, formats: I) -> Self
        where I: IntoIterator<Item = S>, S: Into<Cow<'static, str>>;
    /// Writable and not write-only; bases compared, as in `supports_data_format`.
    pub fn can_read_data_format(&self, data_format: &str) -> bool;
}
impl TypeRegistry {
    pub fn can_read_data_format(&self, type_identifier: &str, data_format: &str) -> bool;
}
```

`validate` (`liquers-core/src/validate/report.rs`): `WarningSource::UnknownDataFormat`.

No new trait, variant or command.

## Integration Points

`liquers-core`:
- `lib.rs` — `pub mod data_format;`.
- `type_system.rs` — field, builder, `can_read_data_format` ×2, doc comment; `base_format`
  imported from `data_format`.
- `assets.rs` — `deserialize_stored_value`: after the "no data formats" branch, `if
  !info.can_read_data_format(data_format) { return Ok(None) }`; `degraded_reason(type_identifier,
  data_format, registry)` returns "stores data format '{f}', which is write-only for this type"
  for that case; its two callers (`try_fast_track`, `get_any_status`) pass `data_format`.
  `validate_metadata_hard`: when the declared format is unsupported and
  `!is_known_data_format(f)`, the message appends "; '{f}' is not a known data format" and, when
  `suggest_data_format` answers, "(did you mean '{s}'?)".
- `value.rs` — `TEXTUAL` replaced by `data_format::TEXT_FORMATS`; string literals in
  `Value::as_bytes`, `deserialize_from_bytes` and `type_descriptions` become the constants
  (`&str` consts are valid match patterns).
- `validate/report.rs` — `collect_step_warnings`: `Step::Filename(name)` gets its own arm; when
  `name.name` has an extension (`rsplit_once('.')`) that `is_known_data_format` rejects, push a
  `ValidationWarning` with `UnknownDataFormat` and the suggestion; the text renderer's match gains
  "Unknown data format:".

`liquers-lib`:
- `value/simple.rs` — the same literal-to-constant change in `as_bytes` / `deserialize_from_bytes`
  (the `toml`, `yaml`, `yml` read arms use `TOML`, `YAML` and the alias literal).
- `value/mod.rs` `ExtValue::type_descriptions` — `RecordView`: `html` moves to
  `.with_write_only_data_formats([data_format::HTML])`; under
  `#[cfg(all(feature = "records-parquet", not(feature = "polars")))]` `parquet` is declared
  write-only instead of plain. Other declarations may use constants; the literal lists stay valid.

`liquers-records`, `liquers-py`, `liquers-web`, `liquers-axum`: no change (builder-built `TypeInfo`).

## Error Handling

- Unknown format at write: the existing `SerializationError`, longer message.
- Write-only at read: `Ok(None)` from `deserialize_stored_value` (degraded), so the fast track
  `eprintln!`s and recomputes; `get_any_status` returns its existing `Error::general_error(…)` with
  the new reason.
- The vocabulary functions are total (no `Result`), no `unwrap`.

## Relevant Commands

None added or changed; `specs/command_registry.yaml` is untouched. Namespaces `rec` and `pl` are
affected only through `RecordView`'s declarations.

## Documentation Architecture

| Path | Change |
|---|---|
| `specs/reference/VALUE_TYPE_SYSTEM.md` | §Refinements: the vocabulary and its helpers; §The registry: `write_only_data_formats`, `can_read_data_format`; §The record identifiers: `html` (and polars-less `parquet`) declared write-only; §Reading: write-only → degraded, recomputed. History row, `reviewed:` |
| `specs/guides/TYPE_SYSTEM_GUIDE.md` | step 4: use `data_format` constants; declare presentation formats with `with_write_only_data_formats`; common problem "not a known data format". History row, `reviewed:` |
| `.claude/skills/liquers-validate/SKILL.md` | the unknown-format warning in the output envelope section |

`affects_docs` as in `DESIGN.md`. No `specs/README.md` link change (no new document).

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | core: `data_format.rs` (new), `lib.rs`, `type_system.rs`, `assets.rs`, `value.rs`, `validate/report.rs`; lib: `value/simple.rs`, `value/mod.rs`; tests in both |
| Workflows affected | reloading record views stored as `html`; `liquers-validate` output |
| Existing tests likely to change | `record_typeinfo.rs` `every_declared_record_view_format_round_trips_or_is_recorded_as_write_only` (use `can_read_data_format` instead of its local list); validator tests asserting `Ok` for a query with an odd extension (none found by grep for `Filename` in `validate/` tests) |
| New validation | Phase 3 tests; `scripts/check-build-matrix.sh` (new `cfg` combination) |
| Compatibility / data | No stored byte or metadata change. `TypeInfo` JSON gains an optional key. A stored `html` view now degrades instead of erroring inside deserialization; the visible difference is the log line and the recovery error text |
| Concurrency / performance / security | static table; a ~30-entry scan on validation and error paths, one short list scan per stored read; no security surface |
| Recovery | each step reverts alone; step order keeps the build green |
| Certainty | high for the slice; questions 1-2 are scope and shape, not feasibility |
