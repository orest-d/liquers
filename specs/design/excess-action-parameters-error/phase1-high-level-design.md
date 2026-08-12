# Phase 1: High-Level Design - Excess Action Parameters Error

## Feature Name

Excess action parameter rejection (`PLAN-EXCESS-ACTION-PARAMETERS-DROPPED`)

## Purpose

An action that supplies more parameters than its command declares currently builds a plan
successfully and the surplus is discarded in silence, so `select_columns-name-price` quietly
selects one column. Plan building must instead fail with an error that names the action, the
excess parameter and **its position in the query text**, so the mistake surfaces where it was
written rather than as a wrong result later.

## Why an error rather than the warning the issue proposed

The deciding fact is structural, not stylistic: **the warning channel cannot carry a position.**
A planning warning is `Step::Warning(String)` (`plan.rs:1712`, `init_warning`) — a bare message with
no `Position` field — whereas `Error` carries `position: Position` and `.with_position()`. Naming
*which* parameter is excess is therefore not merely better served by an error; as a warning it is
inexpressible without first extending the diagnostic type.

That matters because the editor's highlight path is already built and is driven entirely by a
`Position`: `StyledQuery::from_query(x, &position)` (`query.rs:253`) → `to_highlight_if_matching`
(`query.rs:319`) → `StyledQueryToken::Highlight` → red underline in egui
(`liquers-lib/src/egui/widgets.rs:411`). An `Error` supplies the one input that path needs; a
warning supplies nothing. The interactive query console can then underline the offending parameter
while the user types.

## Core Interactions

### Query System
No grammar change. The check is on arity, at plan build time; `Position` already carried by
`ActionParameter` supplies the reported location.

### Command System
Command arity becomes binding. A command that wants a variable-length parameter list must declare
its last argument `multiple` (already supported and already exempt from the check).

### Asset System
Recipes and assets that name an over-supplied action now fail at plan build instead of computing a
truncated result. `ErrorType::TooManyParameters` is already classified in `assets.rs`.

### Value Types / Store / Web / UI
None. `liquers-axum` already maps `TooManyParameters` to HTTP 400 and `liquers-web` / `liquers-py`
already round-trip the variant; only the constructor is missing.

## Crate Placement

`liquers-core` — `src/plan.rs` (the check) and `src/error.rs` (a constructor for the existing,
never-constructed `ErrorType::TooManyParameters`). Possible follow-on in `liquers-lib`
(`src/polars/selection.rs`) for commands whose documented usage relies on the dropped parameters.

## Documentation Intent

**Reference:** Extend `specs/reference/api/DOC_02_QUERY_LANGUAGE_REFERENCE.md` — the arity rule
("every action parameter must be consumed by a declared argument") is one paragraph, and it belongs
directly beside the existing statement at `:293` that *resource-header* extra parameters are
ignored with a warning; leaving the two apart is what makes the difference look accidental. Not a
document of its own. Also correct `specs/reference/POLARS_COMMAND_LIBRARY.md:451`, which offers
`select_columns-col1-col2-col3` as the documented form that this change turns into a hard error.
No `PROJECT_OVERVIEW.md` change: the query/key *encoding* is untouched.

**Guide:** Extend `specs/guides/COMMAND_REGISTRATION_GUIDE.md` — command authors now need to know
that `multiple` is the only way to accept a variable-length list. No new guide; the addition is a
short subsection, not a repeatable workflow.

**Other documents to create:** None. The change is a single rule with no new concepts.

**Specific documents to update:** `specs/issues/PLAN-EXCESS-ACTION-PARAMETERS-DROPPED.md` (close,
recording that the resolution is an error rather than the warning the issue proposed);
`specs/README.md` (design folder link); `.claude/skills/liquers-validate/` references if they state
that a built plan may silently drop parameters.

Audience: command authors and query writers. After this, a reader should learn from the reference
that arity is enforced, and from the guide how to declare a variadic argument — without reading
this design.

## Open Questions

1. Should the error also fire when `allow_placeholders` is set (the recipe path)? Excess is excess
   regardless, so the intent is yes — confirm in Phase 2.
2. Is any lenient/opt-out mode wanted for backward compatibility, or is the break accepted
   outright? Intent: no opt-out; `CORE-PLAN-POLICY-AND-DEFAULTS` is where a policy knob would
   belong if one is ever needed. The cost is real and one-sided — queries that build a plan today
   stop building one — so this is the question the break hinges on.
2b. The resource-header path (`plan.rs:1242`) keeps warning-and-ignoring for its own excess
   parameters, so the two paths will disagree. Accept and document the asymmetry, or align them
   later? Aligning is out of scope here; the reference wording must not pretend they match.
3. `pl/select_columns` and `pl/drop_columns` document "separated by dashes" but declare one
   `String`; after this change `select_columns-a-b` errors. Declare them `multiple`, or leave them
   scalar and fix the documentation to the `~_` escape (`select_columns-a~_b`)?
4. How much existing test, example and recipe material relies on the current silent truncation?
   To be measured in Phase 2 by running the suites and validating the committed registry.

## References

- `specs/issues/PLAN-EXCESS-ACTION-PARAMETERS-DROPPED.md` — the issue
- `liquers-core/src/plan.rs:871` `ResolvedParameterValues::from_action_extended` — the loop that drops
- `liquers-core/src/error.rs:19` `ErrorType::TooManyParameters` — the unused variant this fills
- `specs/issues/PARAMETER-ESCAPING-INCOMPLETE.md` — related; `~_` is the escape that lets a
  dash-bearing value reach a scalar argument
- `specs/issues/CORE-PLAN-POLICY-AND-DEFAULTS.md` — where a plan-builder policy knob would live
