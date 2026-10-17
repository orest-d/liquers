---
id: REGISTRY-QUERY-DEFAULT-TEXT-FORM
kind: design
title: A default link is written and read as query text in registry files
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/commands, core/validate]
issues: [REGISTRY-QUERY-DEFAULT-NOT-WRITABLE-AS-TEXT]
created: 2026-10-17
---
# A default link is written and read as query text in registry files

Produced under [`guides/autonomous_bulk_design.md`](../../guides/autonomous_bulk_design.md) by the
2026-10-17 backlog compaction: Phases 1-4, reviewed without phase approval. Not an approval and not
an implementation.

## Phase 1: High-Level Design

### Purpose

`CommandParameterValue::Query` (`liquers-core/src/command_metadata.rs`) serializes through the
derived `Serialize` of `Query`, i.e. as the parser's AST with source positions. A hand-written
registry overlay cannot give a default link as text, and the exported registry carries the AST.

### Problem Example

```yaml
# overlay.yaml, passed as liquers-validate --registry-file
- {name: cwd, label: cwd, default: !Query "-R-key/.", argument_type: any}
```

Today: `invalid type: string "-R-key/.", expected struct Query`. Also today,
`specs/command_registry.yaml` writes `template/render`'s and `template/expand`'s `cwd` default as a
30-line `segments:` tree with `position: {offset, line, column}` entries. Should be: the overlay
reads as `Query(parse_query("-R-key/."))`, and the exporter writes `default: !Query '-R-key/.'`.

### Scope and Acceptance Criteria

- **AC-1** Text form reads
  - WHEN a registry document gives a default as `!Query "-R-key/."` (YAML) or
    `{"Query": "-R-key/."}` (JSON)
  - THEN it deserializes to `CommandParameterValue::Query` equal to `parse_query("-R-key/.")`
- **AC-2** Legacy AST form still reads
  - WHEN the default is the derived AST form (today's `specs/command_registry.yaml`)
  - THEN it deserializes to the same `Query` as before
- **AC-3** Text form is written
  - WHEN a `CommandParameterValue::Query(q)` is serialized
  - THEN it is written as the tagged string `q.encode()`, and `Value` / `None` are written as today
- **AC-4** Bad text is refused with the parse error
  - WHEN the text does not parse (`!Query "-R-"` style garbage)
  - THEN deserialization fails with a message containing the parse error
- **AC-5** The committed registry uses the text form
  - WHEN `specs/command_registry.yaml` is regenerated
  - THEN both `cwd` defaults read `!Query '-R-key/.'` and `registry_export` and
    `int01_command_registry_yaml_is_byte_identical_after_parse_and_re_serialize` pass

Non-goals: changing `Query`'s own derived serde (plans and recipes keep the AST form).

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — changes a serialized format and the generated
  `specs/command_registry.yaml` (rule 4).
- **Leading issue:** **Proposed resolution - write side:** also *write* text (recommended), or only
  *accept* text on read and leave the exporter's AST form alone.
- **Explanation:** Both read paths already exist in `CommandParameterValueVisitor`; the change is
  one helper and a hand-written `Serialize`. Writing text also removes source positions from
  `calculate_metadata_version`'s input, so the metadata version stops depending on where a default
  query's tokens happen to sit.
- **Open questions:**
  1. **Proposed resolution - write side:** recommended: write text. Consequence: the two template
     commands' `metadata_version` changes once, so their stored results are recomputed once; an
     older `liquers-validate` cannot read the new file. Read-only alternative: eligible for
     automatic fixing, but leaves the verbose AST and the position-dependent metadata version.

### Design Dependencies

- `overlaps` `cwd-default-link-silent-root` (weak: same `-R-key/.` default, independent change site).

### Consolidated Findings

`int01` compares bytes after parse and re-serialize, so the regenerated registry must land in the
same commit as the `Serialize` change. Query equality ignores positions and `source`, so
`registry_export`'s structural comparison is unaffected by the switch.

## Phase 2: Architecture

### Solution

In `liquers-core/src/command_metadata.rs`:

1. Replace `#[derive(Serialize)]` on `CommandParameterValue` with a manual `impl Serialize`:
   `Value(v)` → `serialize_newtype_variant(.., "Value", v)`, `Query(q)` →
   `serialize_newtype_variant(.., "Query", &q.encode())`, `None` → `serialize_unit_variant(.., "None")`
   (identical to the derive for the other two variants).
2. A private `fn query_default_from_json<E: serde::de::Error>(v: serde_json::Value) -> Result<Query, E>`:
   a JSON string goes through `crate::parse::parse_query` (error mapped with `E::custom`), anything
   else through `serde_json::from_value::<Query>` (the legacy AST).
3. In `CommandParameterValueVisitor::visit_map` (`"Query"` key) and `visit_enum` (`"Query"` tag),
   read the payload as `serde_json::Value` and call the helper. No new struct; `serde_json` is
   already a core dependency, and `serde_json::Value` deserializes from YAML too.
4. Update the visitor's doc table: `!Query "a/b"` is the text form; the AST form is accepted.

Rejected: a custom `Serialize`/`Deserialize` on `Query` itself (plans, recipes and stored metadata
carry it); a private `QueryText` newtype (equivalent, but a new type for no gain).

Known-issue preflight: none blocks. `liquers-py` does not convert `CommandParameterValue`
(`liquers-py/src/command_metadata.rs` TODO). `liquers-web` reads defaults through the same visitor,
so a JavaScript `{Query: "…"}` default starts working as a side effect.

### Changes

`command_metadata.rs` as above; `specs/command_registry.yaml` regenerated with a CHANGELOG line;
the doc comment of `ArgumentInfo::with_default_link` unchanged. No signature change.

### Risks

| Risk | Handling |
|---|---|
| `encode()` does not round-trip some programmatic query | AC-3 test round-trips every link default the full registry registers |
| Metadata version of the two template commands changes | Accepted (open question 1); CHANGELOG line says so |
| Other JSON consumers of command metadata (axum command listing) | They see a string instead of an object: simpler; no in-tree consumer parses the AST |

Recovery: revert the commit and regenerate the registry. Certainty: high.

## Phase 3: Examples and Tests

### Examples

The Problem Example; secondary: `{"Query": {"segments": [...], "rooted": false, "source": "Unspecified"}}`
still reads (AC-2).

### Tests

Unit tests in `liquers-core/src/command_metadata.rs` `tests`:

- `query_default_reads_from_yaml_and_json_text` — AC-1
- `query_default_reads_legacy_ast_form` — AC-2 (serialize `parse_query("-R-key/.")` with
  `serde_json::to_value`, wrap as `{"Query": …}`)
- `query_default_serializes_as_encoded_text` — AC-3 (YAML `!Query '-R-key/.'`, JSON
  `{"Query":"-R-key/."}`, `Value`/`None` unchanged)
- `query_default_with_bad_text_is_refused` — AC-4

Existing: `liquers-lib/tests/registry_export.rs` (`committed_registry_is_fresh`,
`committed_registry_impl_versions_are_fresh`) and `liquers-core/tests/command_declaration.rs`
`int01_…` — AC-5. Commands: `cargo test -p liquers-core --lib command_metadata`,
`cargo test -p liquers-core --test command_declaration`, `cargo test -p liquers-lib --test registry_export`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `command_metadata.rs` helper + visitor arms + four unit tests — `cargo test -p liquers-core --lib command_metadata`
- [ ] 2. Manual `impl Serialize for CommandParameterValue` — same command
- [ ] 3. Regenerate `specs/command_registry.yaml` (`export-command-registry`, CLAUDE.md) with a dated CHANGELOG line naming this design and the metadata-version bump — `cargo test -p liquers-lib --test registry_export` and `cargo test -p liquers-core --test command_declaration`
- [ ] 4. `liquers-validate --registry-file specs/command_registry.yaml --registry-file overlay.yaml --allow-overwrite -- 'ns-template/render-~X~-R/t.md~E'` with the Problem Example overlay — exit 0
- [ ] 5. Close the issue; `python3 scripts/docs_index.py --check`

### Validation

`cargo test -p liquers-core --lib --tests`, `cargo test -p liquers-lib --lib --tests`. Rollback:
revert steps 2-3 together (they must ship together, see Consolidated Findings).
