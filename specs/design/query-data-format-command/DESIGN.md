---
id: QUERY-DATA-FORMAT-COMMAND
kind: design
title: A data_format command lets a query declare the data format and its refinement
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [lib/commands, core/context, core/value]
issues: [QUERY-CANNOT-CHOOSE-A-DATA-FORMAT]
created: 2026-10-26
---
# A data_format command lets a query declare the data format and its refinement

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written, not
approved, nothing implemented. Read `specs/guides/COMMAND_REGISTRATION_GUIDE.md` before
implementing.

## Phase 1: High-Level Design

### Purpose

A query can choose its result's data format only through a trailing filename's extension (level 2
of the seeding cascade, `VALUE_TYPE_SYSTEM.md`), and a refinement such as `csv:semicolon` cannot be
written at all, because `:` is not a legal action-parameter character. Add a root command that
takes the base format and the refinement as two ordinary parameters and declares the asset's data
format (level 3), so a URL can ask for a semicolon-separated CSV or a format its filename does not
name.

### Problem Example

```
-R/data/sales.csv/-/ns-pl/head-5/data.csv              # today: comma CSV; no way to ask for ';'
abc/x-csv:comma                                        # liquers-validate: "Can't parse query completely"
-R/data/sales.csv/-/ns-pl/head-5/data_format-csv-semicolon/data.csv     # proposed (validates, Ok)
```

Today a European spreadsheet user gets commas; the only level-3 route is a Rust caller setting
`MetadataRecord::data_format`. Should be: the third query serves the five rows separated by `;`,
with media type `text/csv` and declared data format `csv:semicolon`.

### Scope and Acceptance Criteria

- **AC-1** A refinement from a URL
  WHEN the problem example's third query is evaluated
  THEN its bytes are `;`-separated CSV, its metadata's declared `data_format` is `csv:semicolon`
  and its effective media type is `text/csv`
- **AC-2** A base format without a filename
  WHEN `-R/data/sales.csv/-/ns-pl/head-5/data_format-parquet` is evaluated
  THEN its bytes are Parquet and its declared `data_format` is `parquet`
- **AC-3** The declaration wins over a filename
  WHEN `-R/data/sales.csv/-/ns-pl/head-5/data_format-csv-semicolon/data.txt` is evaluated
  THEN the bytes are `;`-separated CSV (the filename keeps its name; `set_filename` does not
  replace a declared format)
- **AC-4** An unwritable format is refused at once
  WHEN `data_format-png` follows a DataFrame, or `data_format-` is given an empty format
  THEN the command fails with a `SerializationError` naming the type and its supported formats
- **AC-5** The value passes through
  WHEN any value goes through `data_format`
  THEN the value is unchanged (same identifier, equal content)
- **AC-6** The last declaration wins
  WHEN two `data_format` actions appear, as the last two actions before the filename
  THEN the asset's declared format is the second one's

Non-goals: escaping `:` in query syntax (`QUERY-ESCAPE` territory, a parser change); a format for
an *intermediate* step (a prefix state's `data_format` is cleared by design, `prefix_metadata`);
making every codec accept every refinement (filed:
`RECORD-TABLE-FORMAT-REFUSES-CSV-REFINEMENTS-POLARS-ACCEPTS`); the vocabulary's unknown-format
warnings (`data-format-vocabulary`).

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — a new command (`specs/command_registry.yaml` changes) and
  new `pub` methods on `Context` and `Metadata` in `liquers-core` (rule 4), across two crates (rule 6).
- **Leading issue:** **Open design - name and placement:** `data_format` in the root namespace.
- **Explanation:** the asset's own record is the authoritative metadata of a plan's result
  (`interpreter.rs`, `apply_plan`), and `Context::set_filename` already writes it the same way;
  the write path already accepts a refined format whose base a type declares
  (`TypeInfo::supports_data_format`). Phases 3-4 assume the recommended answers.
- **Open questions:**
  1. **Open design - name and namespace:** recommended `data_format` in the root namespace (the
     metadata field it sets; available after any `ns-…`, since root is a default namespace).
     Alternatives: `to_format` (the issue's sketch; but every `to_*` command converts its value,
     and this one does not), `format` (taken as an argument name by several commands, ambiguous).
  2. **Open design - scope of the declaration:** the result of the whole query (the asset), not
     the following step. Recommended, and the only form the current architecture supports without
     a plan change: intermediate states drop `data_format` by design. Consequence: the command
     belongs at the end, before the filename; placed earlier it may run in a cut-off prefix asset
     and have no effect on the result. Documented in the command's `doc`.
  3. **Proposed resolution - validation:** at command time against the *input's* type
     (`TypeRegistry::supports_data_format`, base-level, as the write path checks), since the
     intended position is last. A refinement a codec rejects (`csv:x` for a record view) still
     fails at serialization, as it would from a Rust caller.
  4. **Proposed resolution - parameters:** `format: String` and `refinement: String = ""`, both
     lowercased; the result is `format` or `format:refinement`. No enum: the formats are
     type-dependent and validated against the registry.

### Design Dependencies

- `data-format-vocabulary` — **overlaps**, weak: provides `is_known_data_format` and typo
  suggestions; once it lands, AC-4's error may append its suggestion. No order (this split out
  of its source feature, point 4).
- `RECORD-TABLE-FORMAT-REFUSES-CSV-REFINEMENTS-POLARS-ACCEPTS` (issue, filed by this design) —
  **overlaps**: decides which refinements a record view can be asked for.
- `SOFT-CONSISTENCY-CHECK-MISREADS-A-REFINED-DATA-FORMAT` (issue, filed by this design) —
  **overlaps**: a refined format stored through `set_state` gets a spurious media-type warning;
  this command makes refined formats common.
- `polars-dataframe-json` — **overlaps**, weak: adds formats this command can then request.

### Consolidated Findings

- `MetadataRecord::set_filename` seeds `data_format` only when none is declared, so a filename
  after the command keeps the declaration (AC-3), with no ordering code.
- `MetadataRecord::effective_media_type` strips the refinement before mapping, so `csv:semicolon`
  serves as `text/csv` (AC-1).
- `cached: false` is right: the output equals the input, so caching it would store a second copy of
  a possibly large frame.

## Phase 2: Architecture

### Solution

1. `liquers-core/src/metadata.rs`: `impl Metadata { pub fn set_data_format(&mut self, data_format:
   &str) -> Result<&mut Self, Error> }` — sets `MetadataRecord::data_format = Some(..)`;
   `LegacyMetadata` → `Error::general_error("Cannot set data format on legacy metadata")`, as
   `set_filename` does.
2. `liquers-core/src/context.rs`: `pub async fn set_data_format(&self, data_format: &str) ->
   Result<(), Error>` — writes the current asset's record under its lock, as `set_filename`.
3. `liquers-lib/src/commands.rs`: a generic command

   ```rust
   #[liquers_macro::command_version]
   pub async fn data_format<E: Environment>(state: State<E::Value>, format: String,
       refinement: String, context: Context<E>) -> Result<E::Value, Error>
   ```

   — `let value = state.value()?` (a terminal input propagates); build `format[:refinement]`,
   trimmed and lowercased; empty `format` → error; look up `value.identifier()` in
   `context.get_envref().get_type_registry()`: unknown identifier, or a type declaring formats none
   of which matches → `Error::from_error(ErrorType::SerializationError, "Type '…' cannot be
   serialized as '…'; supported formats: […]")` (the wording of `validate_metadata_hard`); a type
   declaring *no* formats is refused the same way. Then `context.set_data_format(&fmt).await?`,
   an info log line, and `Ok((*value).clone())`.
4. Registered in `register_core_commands!`:

   ```rust
   register_command!($cr,
       async fn data_format(state, format: String, refinement: String = "", context) -> result
       label: "Data format"
       doc: "Declare the data format (and optional refinement) the query's result is written in; place it last, before the filename"
       cached: false
       version: auto
   )?;
   ```

Rejected: query syntax for `:` (parser and key-encoding change, `PROJECT_OVERVIEW.md`); a
filename-only convention such as `data.csv_semicolon` (works today for polars, `csv_semicolon` is a
parser alias, but not for record views and not for a format whose extension differs); per-step
format state (contradicts `prefix_metadata`).

### Changes

- Core: `metadata.rs` (`Metadata::set_data_format`), `context.rs` (`Context::set_data_format`).
- Lib: `commands.rs` (function and registration). `specs/command_registry.yaml` regenerated with a
  CHANGELOG line.
- Documents: `specs/reference/VALUE_TYPE_SYSTEM.md` §seeding cascade (level 3 is reachable from a
  query through `data_format`); `specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md`
  (`Context::set_data_format`); `specs/guides/COMMAND_REGISTRATION_GUIDE.md` §"Reading the input's
  description" (one line: the produced asset's format is set with `set_data_format`). History rows,
  `reviewed:`.

### Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `metadata.rs`, `context.rs`, `commands.rs`, the registry, three documents |
| Existing tests | `registry_export` fails until the registry is regenerated (intended) |
| Semantics | misplaced (not last) the command may have no effect (Q2); documented, not detected |
| Compatibility | additive: a new command and two new methods |
| Python/web | a new root command is visible to every binding's command list; no binding code changes |
| Recovery | revert the commits; regenerate the registry |
| Certainty | High for the mechanism; the name is a decision |

## Phase 3: Examples and Tests

### Examples

Validated with `liquers-validate --command data_format` (all `Ok`, `data_format` resolved in the
root namespace after `ns-pl`):

```
-R/data/sales.csv/-/ns-pl/head-5/data_format-csv-semicolon/data.csv
-R/data/sales.csv/-/ns-pl/head-5/data_format-parquet
-R/data/sales.csv/-/ns-pl/head-5/data_format-csv-semicolon/data.txt
```

### Tests

Unit, `liquers-core/src/metadata.rs` `mod tests`:
- `set_data_format_declares_level_three` — then `set_filename("x.json")` keeps it; legacy metadata
  errors: AC-3

Integration, new `liquers-lib/tests/data_format_command.rs`; `DefaultEnvironment<Value>` with all
commands, an `AsyncMemoryStore` holding `data/sales.csv` (6 rows, 2 columns):
- `data_format_passes_text_through` (no feature gate) — a recipe producing `Text`, `…/data_format-html`:
  value equal, declared `html`, media type `text/html`: AC-5
- `refined_csv_from_url` — `#[cfg(feature = "polars")]`, AC-1 query: bytes contain `;` and no `,`,
  `data_format` `csv:semicolon`, `effective_media_type` `text/csv`: AC-1
- `base_format_without_filename` — `polars`, AC-2 query; bytes begin with `PAR1`: AC-2
- `declaration_wins_over_filename` — `polars`, AC-3 query: AC-3
- `unwritable_format_is_refused` — `polars`, `…/data_format-png`, and `data_format-` (empty):
  `ErrorType::SerializationError`, message names `polars.DataFrame` and `csv`: AC-4
- `last_declaration_wins` — `polars`, `…/head-5/data_format-csv/data_format-parquet`: declared
  `parquet`: AC-6

Registry: `cargo test -p liquers-lib --test registry_export` after regeneration.

Run: `cargo test -p liquers-lib --test data_format_command`,
`cargo test -p liquers-core --lib metadata::tests`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-core/src/metadata.rs` — `Metadata::set_data_format` and its test — `cargo test -p liquers-core --lib metadata::tests`.
- [ ] 2. `liquers-core/src/context.rs` — `Context::set_data_format` — `cargo check -p liquers-core`. Depends on 1.
- [ ] 3. `liquers-lib/src/commands.rs` — `data_format` and its registration in `register_core_commands!` — `cargo check -p liquers-lib`. Depends on 2.
- [ ] 4. `liquers-lib/tests/data_format_command.rs` — the six tests — `cargo test -p liquers-lib --test data_format_command`; `cargo test -p liquers-lib --no-default-features --lib --tests` (the text test runs without polars).
- [ ] 5. Regenerate `specs/command_registry.yaml` (exporter command in `CLAUDE.md`), CHANGELOG line — `cargo test -p liquers-lib --test registry_export`.
- [ ] 6. Documents (three, History rows, `reviewed:`); close `QUERY-CANNOT-CHOOSE-A-DATA-FORMAT` — `python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-lib --lib --tests`; `CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib --tests`;
`bash scripts/check-build-matrix.sh`; `cargo check -p liquers-py`; validate the three example
queries with `liquers-validate` against the regenerated registry (no `--command`); diff review.
