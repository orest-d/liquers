---
id: ENUM-ALTERNATIVE-LABELS
kind: design
title: An enum alternative carries an optional label and description
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/commands, macro]
issues: [ENUM-ALTERNATIVES-HAVE-NO-LABEL]
created: 2026-10-26
---
# An enum alternative carries an optional label and description

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written, not
approved, nothing implemented.

## Phase 1: High-Level Design

### Purpose

An enum argument tells a UI or an agent which values are allowed, but only by their query spelling
(`catmullrom`, `lanczos3`). A person choosing a resize method, or an agent reading an MCP tool
schema, needs a readable name and a sentence on what each choice does. This adds both, optional, to
each alternative, with a `register_command!` spelling, and shows them where alternatives are shown.

### Problem Example

Verified at HEAD. `ns-img/resize` declares `method` with
`enum: ["nearest", "triangle", "catmullrom", "gaussian", "lanczos3"]`. `liquers-validate` on
`ns-img/resize-100-100-lanczos` reports *"Undefined enum value for argument method. Valid values:
nearest, triangle, catmullrom, gaussian, lanczos3"*. That is all any consumer can show:

- `EnumArgumentAlternative` (`liquers-core/src/command_metadata.rs` ≈62) has `alias` and `value`
  only, under `// TODO: Label and description for alternatives`;
- the MCP schema (`liquers-mcp/src/schema.rs` `enum_schema`) is `{"enum": ["nearest", …]}`;
- the egui command view (`liquers-lib/src/egui/widgets.rs` ≈785) prints `alias => value`;
- the macro (`liquers-macro/src/registration.rs` `parse_inline_enum_parameter_spec`) accepts no
  per-alternative metadata.

Should be: `enum: ["catmullrom" (label: "Catmull-Rom", description: "Cubic, sharper than
triangle"), …]` registers, the MCP schema gives each alias its `title` and `description`, and the
egui view shows `catmullrom — Catmull-Rom`.

### Scope and Acceptance Criteria

- **AC-1** Labels are registered from the macro
  WHEN a command declares `enum: ["a" (label: "A", description: "first"), "b"]` or the map form
  `{"hq" => "lanczos3" (label: "High quality")}`
  THEN the registered alternatives carry those labels and descriptions, and an alternative without
  them has empty strings
- **AC-2** Unlabelled metadata is unchanged
  WHEN an enum whose alternatives have no label or description is serialized
  THEN the output is byte-identical to today's (no `label` / `description` keys), so
  `specs/command_registry.yaml` and every `metadata_version` stay the same
- **AC-3** Labels round-trip
  WHEN metadata with labelled alternatives is serialized to JSON or YAML and read back (registry
  files, JavaScript and Python declarations)
  THEN the labels and descriptions are preserved, and a document without them still reads
- **AC-4** MCP shows them
  WHEN a tool schema is built for an argument whose enum has at least one label or description
  THEN the schema is `{"oneOf": [{"const": alias, "title": …, "description": …}, …]}` (title and
  description omitted where empty), wrapped in `anyOf` with the value schema when `others` is
  allowed; with no labels the schema is today's `{"enum": […]}`
- **AC-5** The macro refuses a malformed option
  WHEN an alternative's options name anything other than `label` or `description`, or repeat one
  THEN compilation fails with a message naming the option
- **AC-6** The egui command view shows them
  WHEN an alternative has a label
  THEN the "Enum alternatives" list shows `alias — label`, with the description as hover text

Non-goals: labelling the built-in commands' enums (a later, per-command change that regenerates
the registry); computed alternatives (`COMMAND-ENUM-ALTERNATIVES-CANNOT-BE-COMPUTED`); a rendered
enum selector widget (none exists at HEAD); translating labels.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — adds two `pub` fields and `pub` builder methods in
  `liquers-core` and a macro syntax (rules 3, 4), across `liquers-core`, `liquers-macro`,
  `liquers-mcp` and `liquers-lib` (rule 6).
- **Leading issue:** **Open design - macro spelling:** a parenthesised option list after each
  alternative.
- **Explanation:** the fields are additive with `skip_serializing_if`, so nothing serialized today
  changes; every consumer that reads alternatives was found. The open choices are spelling and the
  MCP schema form; both have a recommended answer and the plan is written around it.
- **Open questions:**
  1. **Open design - macro spelling.** Recommended: `"alias" (label: "…", description: "…")` in
     the list form and `"alias" => value (label: "…", …)` in the map form, the same option syntax
     as a parameter's own metadata (`REGISTER_COMMAND_FSD.md` §Parameter Metadata). Alternative: a
     separate statement `enum_labels: {"alias" => "Label"}`, which repeats every alias and cannot
     carry descriptions without a second statement.
  2. **Open design - MCP schema form.** Recommended: switch to `oneOf`/`const` only when some
     alternative has a label or description (AC-4), so every current schema is unchanged.
     Alternative: keep `enum` and append the labels to the argument's `description` text; simpler,
     but not machine-readable.
  3. **Proposed resolution - labels count in the metadata version.** A label is part of the
     serialized command, so adding one changes that command's `metadata_version` and recomputes
     its cached results once. This is already true of `ArgumentInfo::label` and `doc`; excluding
     cosmetic fields from the hash is a separate question, not this design's.

### Design Dependencies

- `global-enum-registration` (`COMMAND-METADATA-ENHANCEMENTS`, Phase 4 written) — **overlaps**,
  weak: it edits `EnumArgument` checks and the version hash, not `EnumArgumentAlternative`. Either
  order; its `definition_issues` need not look at labels.
- `computed-enum-alternatives` (`COMMAND-ENUM-ALTERNATIVES-CANNOT-BE-COMPUTED`) — **required-by**,
  weak: computed alternatives are returned as `EnumArgumentAlternative`, and benefit from labels
  (column names with their types as descriptions). Not a precondition.
- `GLOBAL-ENUMS-NOT-IMPORTED-BY-LANGUAGE-INTEGRATIONS` (issue) — **overlaps**, weak: labels in an
  imported global enum come for free through serde.
- Overlap judgement: no T1-T4 with any open item; `global-enum-registration` was the nearest (same
  file, different symbols, T3 does not hold).

### Consolidated Findings

- Struct literals of `EnumArgumentAlternative` exist only in `command_metadata.rs` builders and in
  the macro's generated code (`registration.rs` `to_argument_type_expression`); both change in
  this design. Code outside the workspace that builds the struct literally breaks; the new
  `EnumArgumentAlternative::new` is the migration.
- JavaScript (`command_declaration.rs`, `liquers-web` import) and Python (`liquers-py` wraps the
  core type and reads JSON) need no change: they deserialize `ArgumentType` with serde.
- `liquers-lib/src/search/command_catalog.rs`, `liquers-mcp/src/server.rs` `type_name` and the
  plan's "Valid values" error keep showing aliases, which is what a query must contain.
- No query syntax changes; the examples' queries were validated.

## Phase 2: Architecture

### Solution

`liquers-core/src/command_metadata.rs`:

```rust
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct EnumArgumentAlternative {
    pub alias: String,
    pub value: CommandParameterValue,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}
impl EnumArgumentAlternative {
    pub fn new(alias: &str, value: CommandParameterValue) -> Self;   // empty label, description
    pub fn with_label(self, label: &str) -> Self;
    pub fn with_description(self, description: &str) -> Self;
    /// The label, or the alias when the label is empty.
    pub fn display_label(&self) -> &str;
}
impl EnumArgument {
    /// Appends a fully built alternative (labelled or not); `value_type` is left as it is.
    pub fn with_enum_alternative(self, alternative: EnumArgumentAlternative) -> Self;
}
```

The existing builders (`with_string_value`, `with_int_value`, `with_value`, `with_link`) build
through `EnumArgumentAlternative::new`; their signatures do not change. `String` with
`skip_serializing_if` mirrors `CommandPreset::ns` and keeps AC-2.

`liquers-macro/src/registration.rs`: `EnumAlternativeSpec` gains `label: Option<String>`,
`description: Option<String>`. `parse_inline_enum_parameter_spec` calls a new
`parse_enum_alternative_options(&content)` after each list alias and after each map value when
`content.peek(syn::token::Paren)`; it accepts `label:` and `description:` string literals, refuses
unknown or repeated keys (AC-5). `to_argument_type_expression` emits the literal with all four
fields (`label: #label.to_string()`, empty when `None`).

`liquers-mcp/src/schema.rs` `enum_schema`: when any alternative has a non-empty label or
description, build `oneOf` of `{"const": alias}` objects with `title` / `description` inserted
only when non-empty; else today's `enum`. The `others_allowed` `anyOf` wrapper is kept around
either form.

`liquers-lib/src/egui/widgets.rs` (≈785): label text `alias — label` when the label is non-empty,
`.on_hover_text(description)` when the description is.

Rejected: an `Option<String>` field (two empty states, and `ArgumentInfo` already uses `String`);
a separate `labels` map on `EnumArgument` (alternatives and their labels could drift apart).

### Changes

- `liquers-core/src/command_metadata.rs`: `EnumArgumentAlternative` fields and methods,
  `EnumArgument::with_enum_alternative`, builders via `new`; TODO comment removed.
- `liquers-macro/src/registration.rs`: `EnumAlternativeSpec`, `parse_enum_alternative_options`,
  `parse_inline_enum_parameter_spec`, `to_argument_type_expression`.
- `liquers-mcp/src/schema.rs`: `enum_schema`.
- `liquers-lib/src/egui/widgets.rs`: the enum alternatives list (feature `egui`).
- Documents: `reference/REGISTER_COMMAND_FSD.md` §Parameter Metadata (enum syntax),
  `guides/COMMAND_REGISTRATION_GUIDE.md` §Enum Parameters, `reference/COMMAND_DECLARATION.md`
  (the alternative's JSON shape, if it lists fields). Each gets a History row and a `reviewed:`
  bump.

Sync only. No command signature changes, so `specs/command_registry.yaml` is not regenerated.

### Risks

| Assessment | Finding |
|---|---|
| Files likely to change | four source files in four crates, three documents |
| Existing tests | macro tests matching `alias : \"nearest\"` in token strings still match; `schema_maps_types_defaults_and_hidden_arguments` keeps its unlabelled enum. `registry_export` stays green (AC-2) |
| Compatibility | serialized form unchanged unless a label is set; struct literals outside the workspace break (migration: `new`) |
| Build matrix | the egui change is inside an `egui`-gated module; no new `cfg` |
| Recovery | revert; stored metadata without labels reads either way |
| Certainty | high; every reader of `EnumArgumentAlternative` was grepped (core, macro, mcp, lib egui and search, py, web) |

## Phase 3: Examples and Tests

### Examples

```rust
register_command!(cr,
    fn resize(state, width: u32, height: u32,
        method: String = "lanczos3" (enum: [
            "nearest" (label: "Nearest neighbour", description: "Fastest; blocky"),
            "catmullrom" (label: "Catmull-Rom"),
            "lanczos3" (label: "Lanczos 3", description: "Slowest; sharpest")]))
    -> result
    namespace: "img")?;
```

The query is unchanged: `ns-img/resize-100-100-lanczos3` (validated: plans with
`method="lanczos3"`). The MCP schema of `method` becomes
`{"oneOf": [{"const": "nearest", "title": "Nearest neighbour", "description": "Fastest; blocky"},
{"const": "catmullrom", "title": "Catmull-Rom"}, {"const": "lanczos3", …}]}`.

Rust builder: `EnumArgument::new("img.method").with_enum_alternative(EnumArgumentAlternative::new(
"hq", CommandParameterValue::from_value(Value::from("lanczos3"))).with_label("High quality"))`.

### Tests

Unit, `liquers-macro/src/registration.rs` `tests`:
- `test_parse_enum_list_with_labels` (AC-1): token string contains `label : "Nearest neighbour"`.
- `test_parse_enum_map_with_labels` (AC-1).
- `test_enum_alternative_rejects_unknown_option` and `…_rejects_repeated_label` (AC-5).

Unit, `liquers-core/src/command_metadata.rs` `tests`:
- `enum_alternative_without_label_serializes_as_before` (AC-2): JSON has no `label` key, and the
  `metadata_version` of a command built with the old builders is unchanged across the edit (pin
  one hash computed before the change).
- `enum_alternative_labels_round_trip` (AC-3): JSON and YAML; a document without the keys reads
  with empty strings.

Integration, `liquers-core/tests/register_command_option.rs`:
`enum_alternative_labels_registered` (AC-1): register through the macro, read the metadata back.

Unit, `liquers-mcp/src/schema.rs` `tests`: `labelled_enum_schema_uses_one_of` and
`unlabelled_enum_schema_is_unchanged` (AC-4), including the `others` wrapper.

AC-6 is a display change in an egui-gated view with no test harness for that widget; verified by
`cargo check -p liquers-lib --features egui` and a manual look (stated as such).

Run: `cargo test -p liquers-macro`, `cargo test -p liquers-core --lib enum_alternative`,
`cargo test -p liquers-core --test register_command_option`, `cargo test -p liquers-mcp`,
`cargo test -p liquers-lib --test registry_export`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. Record the answers to questions 1-2 in Phase 1 — review only
- [ ] 2. `command_metadata.rs` fields, `new`, `with_label`, `with_description`, `display_label`,
  `with_enum_alternative`; builders through `new` — AC-2 / AC-3 unit tests —
  `cargo test -p liquers-core --lib enum_alternative`
- [ ] 3. `registration.rs` option parsing and generated literal — macro tests, then
  `cargo test -p liquers-core --test register_command_option`
- [ ] 4. `liquers-mcp/src/schema.rs` `enum_schema` — `cargo test -p liquers-mcp`
- [ ] 5. `liquers-lib/src/egui/widgets.rs` — `cargo check -p liquers-lib --features egui`
- [ ] 6. Documents (FSD, guide, COMMAND_DECLARATION) with History rows; close the issue; this
  design to `complete` — `python3 scripts/docs_index.py` then `--check`

### Validation

`cargo fmt --check`; `cargo test -p liquers-lib --lib --tests` (every registration still
compiles); `cargo test -p liquers-lib --test registry_export` green with no regeneration;
`bash scripts/check-build-matrix.sh` (egui-gated edit). Diff review: no change to
`specs/command_registry.yaml`, no `println!`.
