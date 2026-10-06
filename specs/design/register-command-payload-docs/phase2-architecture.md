# Phase 2: Solution and Architecture - Documenting `payload:`, `expires:` and `version:`

## Chosen Solution

### `specs/reference/REGISTER_COMMAND_FSD.md`

- §Metadata Statements example block: add, after `volatile: true`,
  `payload: required`, `expires: "in 5 min"` and `version: auto` (with
  `#[liquers_macro::command_version]` shown on the function definition above the block).
- Table rows, after `volatile`:

  | Statement | Type | Description |
  |---|---|---|
  | `payload: required` / `payload: none` | Identifier | `required` marks the command as needing the evaluation payload (`PayloadRequirement::Required`) and **also sets `volatile`**; the requirement propagates to the plan so nested evaluation forwards the payload. `none` is the default and emits nothing. A string or bool is a compile error. See `PAYLOAD_GUIDE.md`. |
  | `expires: "..."` | String | Default expiration of the command's results, e.g. `"in 5 min"`, `"immediately"`, `"never"`. Parsed when the command is registered: an invalid spec makes `register_command!` return `Err`. Grammar: `DOC_08_RECIPES_PLANS.md` §Finalization and expiration. |
  | `version: auto` / `now` / `"..."` / integer | Identifier, String or Integer | The command's implementation version (`impl_version`), which feeds dependency freshness. `auto`: a hash of the function's source; requires `#[liquers_macro::command_version]` on the function. `now`: the registration time — changes on every start, so dependents re-evaluate after each restart. A string: its BLAKE3 hash. An integer: used as is; bump it by hand. Omitted: unversioned. |

- §Injected Parameters, after "Built-in injectable: `E::Payload`": one sentence linking
  `payload: required` and `PAYLOAD_GUIDE.md` §"Declare it, or lose it".
- New short subsection "Implementation versions" after §Metadata Statements: what
  `#[liquers_macro::command_version]` generates (`<fn>__VERSION_() -> u128`, a hash of the item's
  tokens) and that `version: auto` calls it.
- `## History` row; `reviewed:` bump.

### `CLAUDE.md`

DSL Syntax Reference, metadata bullet: append `` `payload:` `` (bare `required`/`none`; `required`
implies volatile), `` `expires:` `` (string, checked at registration) and `` `version:` `` (`auto`
with `#[command_version]`, `now`, string or integer).

### `specs/guides/COMMAND_REGISTRATION_GUIDE.md`

- §Macro DSL Syntax: the metadata bullet becomes "(label, doc, namespace, realm, filename, volatile,
  payload, expires, version, preset, next)".
- Two short subsections under §1: "Commands that need the payload" and "Versioning a command so its
  results expire when its code changes" (Phase 3 snippets), each linking the FSD row.
- `## History` row; `reviewed:` bump.

## Rejected Alternatives

- **Copy the payload inheritance rules or the expiration grammar into the FSD and guide** —
  duplicates `PAYLOAD_GUIDE.md` and `DOC_08`.
- **Keep the expires/version issue separate** — rejected by the maintainer on 2026-10-05: same
  table, same files.

## Files

The three documents above and the two issue files. No code; `command_registry.yaml` unchanged.

## Risk Table

| Aspect | Assessment |
|---|---|
| Likely files | `REGISTER_COMMAND_FSD.md`, `COMMAND_REGISTRATION_GUIDE.md`, `CLAUDE.md` |
| Existing-test impact | none; `docs_index.py --check` link check covers new relative links |
| New validation | snippet compile check against existing tests (Phase 3) |
| Recovery | revert the text |
| Certainty | high |
