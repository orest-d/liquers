# Phase 1: High-Level Design - Excess Action Parameters Error

## Feature Name

Excess action parameter rejection (`PLAN-EXCESS-ACTION-PARAMETERS-DROPPED`)

## Purpose

An action that supplies more parameters than its command declares currently builds a plan
successfully and the surplus is discarded in silence, so `select_columns-name-price` quietly
selects one column. Plan building must instead fail with an error that names the action, the
excess parameter and **its position in the query text**, so the mistake surfaces where it was
written rather than as a wrong result later. The resource-header path, which today warns and
ignores its own surplus, becomes an error on the same terms.

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
its last argument `multiple` — which requires *making that declarable first*, see Q3 below.

### Asset System
Recipes and assets that name an over-supplied action now fail at plan build instead of computing a
truncated result. `ErrorType::TooManyParameters` is already classified in `assets.rs`.

### Value Types / Store / Web / UI
None. `liquers-axum` already maps `TooManyParameters` to HTTP 400 and `liquers-web` / `liquers-py`
already round-trip the variant; only the constructor is missing.

## Crate Placement

`liquers-core` — `src/plan.rs` (both checks: the action path and the resource-header path) and
`src/error.rs` (a constructor for the existing, never-constructed `ErrorType::TooManyParameters`).

`liquers-macro` — `src/registration.rs`, to make `multiple` declarable in the `register_command!`
argument DSL. Currently the generated `ArgumentInfo` hardcodes `multiple: false` (`:718`, `:2336`,
`:2406`), so no registered command can be variadic.

`liquers-lib` — `src/polars/selection.rs`, to declare `select_columns` and `drop_columns` variadic
once the macro allows it.

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
short subsection, not a repeatable workflow. `specs/reference/REGISTER_COMMAND_FSD.md` must also
document the new `multiple` DSL flag, and `CLAUDE.md`'s DSL syntax summary lists argument
attributes, so it gains `multiple` alongside `injected`.

**Other documents to create:** None. The change is a single rule with no new concepts.

**Specific documents to update:** `specs/issues/PLAN-EXCESS-ACTION-PARAMETERS-DROPPED.md` (close,
recording that the resolution is an error rather than the warning the issue proposed);
`specs/README.md` (design folder link); `.claude/skills/liquers-validate/` references if they state
that a built plan may silently drop parameters.

Audience: command authors and query writers. After this, a reader should learn from the reference
that arity is enforced, and from the guide how to declare a variadic argument — without reading
this design.

## Resolved Decisions

1. **The error fires regardless of `allow_placeholders`.** A recipe that over-supplies an action is
   as wrong as a query that does; placeholders concern *missing* arguments, not surplus ones.
2. **No opt-out mode.** The break is accepted outright. Should a policy knob ever be wanted, it
   belongs in `CORE-PLAN-POLICY-AND-DEFAULTS`, not here.
3. **Both paths error; no asymmetry.** The resource header (`plan.rs:1242`) stops warning-and-
   ignoring and raises the same error for surplus parameters. This keeps one rule to document:
   *every parameter written must be consumed*.
4. **`pl/select_columns` and `pl/drop_columns` become variadic** (`multiple`), so
   `select_columns-a-b` selects two columns — the behaviour their documentation always claimed.
   This is the prerequisite work described in Q3.

## Open Questions

1. **Q3 — `multiple` is not declarable today, and this is the largest piece of the work.**
   `ArgumentInfo::set_multiple()` exists (`command_metadata.rs:550`) and the *runtime* fully
   supports variadic arguments — `pop_value` collects them (`plan.rs:679`), `commands.rs:289`
   materialises them into a `Vec<T>`, and the interpreter handles them in five places. But
   `register_command!` has no syntax for it and hardcodes `multiple: false`, so **no command in the
   workspace is variadic** and `multiple: true` appears nowhere outside one `plan.rs` unit test.
   Making the polars commands variadic therefore means: adding a `multiple` flag to the argument
   DSL (a close sibling of the existing `injected` flag, `registration.rs:1564`), changing the two
   command signatures from `columns: String` to `Vec<String>`, and regenerating
   `specs/command_registry.yaml`. Confirm this scope is wanted inside this design rather than split
   into its own.
2. **Must a `multiple` argument be last?** It consumes the iterator's remainder, so any argument
   declared after one is unreachable. Nothing enforces this today because nothing uses `multiple`.
   Enforce at registration (macro or registry validation) or leave as an author's hazard?
3. **The header's `_` arm reports the wrong thing.** An unrecognised instruction such as
   `-R-xyz/data` returns "Resource header parameters must be string or link" (`plan.rs:1296`),
   which describes a different failure. Fix the message while touching this code — and it wants a
   position too.
4. **Does the header's ignored *name* stay a warning?** `plan.rs:1237` warns that a non-empty
   header name is ignored. Decision 3 covers surplus *parameters* only; the name is a separate
   ignored input and is left warning unless stated otherwise.
5. How much existing test, example and recipe material relies on the current silent truncation? To
   be measured in Phase 2 by running the suites and validating the committed registry.

## References

- `specs/issues/PLAN-EXCESS-ACTION-PARAMETERS-DROPPED.md` — the issue
- `liquers-core/src/plan.rs:871` `ResolvedParameterValues::from_action_extended` — the loop that drops
- `liquers-core/src/error.rs:19` `ErrorType::TooManyParameters` — the unused variant this fills
- `specs/issues/PARAMETER-ESCAPING-INCOMPLETE.md` — related; `~_` is the escape that lets a
  dash-bearing value reach a scalar argument
- `specs/issues/CORE-PLAN-POLICY-AND-DEFAULTS.md` — where a plan-builder policy knob would live
