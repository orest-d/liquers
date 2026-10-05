# Phase 2: Solution and Architecture - Documenting `payload: required`

## Chosen Solution

### `specs/reference/REGISTER_COMMAND_FSD.md`

- §Metadata Statements example block: add `payload: required` after `volatile: true`.
- Table row, after `volatile`:

  | Statement | Type | Description |
  |---|---|---|
  | `payload: required` / `payload: none` | Identifier | `required` marks the command as needing the evaluation payload (`CommandMetadata::payload_required = PayloadRequirement::Required`) and **also sets `volatile`**; the requirement propagates to the plan so nested evaluation forwards the payload. `none` is the default and emits nothing. A string or bool is a compile error. See `PAYLOAD_GUIDE.md`. |

- §Injected Parameters: one sentence after "Built-in injectable: `E::Payload`": "A command that
  reads the payload should also declare `payload: required` — without it the command runs, but
  receives no payload when evaluated as a nested dependency (`PAYLOAD_GUIDE.md`, *Declare it, or
  lose it*)."
- `## History` row; `reviewed: 2026-10-04` (or the implementation date).

### `CLAUDE.md`

DSL Syntax Reference, metadata bullet: append `` `payload:` `` (bare `required`/`none`;
`required` implies volatile).

### `specs/guides/COMMAND_REGISTRATION_GUIDE.md`

- §Macro DSL Syntax bullet "Metadata statements (label, doc, namespace, realm, etc.)" →
  "(label, doc, namespace, realm, volatile, payload, etc.)".
- New short subsection under §1, "Commands that need the payload": the snippet from Phase 3,
  two sentences, link to `PAYLOAD_GUIDE.md`.
- `## History` row; `reviewed:` bump.

## Rejected Alternatives

- **Copy the payload inheritance rules into the FSD and guide** — duplicates `PAYLOAD_GUIDE.md`.
- **Also document `expires:`/`version:` here** — different source; filed separately.

## Files

The three documents above. No code, no generated files (`command_registry.yaml` unchanged).

## Risk Table

| Aspect | Assessment |
|---|---|
| Likely files | `REGISTER_COMMAND_FSD.md`, `COMMAND_REGISTRATION_GUIDE.md`, `CLAUDE.md` |
| Affected workflows | command authoring guidance |
| Existing-test impact | none; `docs_index.py --check` link check covers new relative links |
| New validation | snippet compile check (Phase 3) |
| Compatibility/data/security | none |
| Recovery | revert the text |
| Certainty | high |

## Review

Against Phase 1: criteria 1-4 map to the edits; 5 to Phase 3. Against code: parser and emitter
read at HEAD; the existing test uses exactly `payload: required` after `namespace: "test"`.
