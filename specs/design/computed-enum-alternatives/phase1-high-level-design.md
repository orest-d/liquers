# Phase 1: High-Level Design — computed enum alternatives

## Purpose

Data-dependent arguments (a column of the input frame, a stored table, an installed font) are plain
strings today: a UI or an agent cannot be offered the valid choices, and a misspelling fails inside
the command. Let such an argument name a computed enum, so the choices can be listed and a wrong
value is refused with the valid ones named, as a static enum already does (`VISION.md`: agents and
UIs work from command metadata).

## Problem Example

Verified at HEAD. `ns-pl/sort` takes `column: String`. `liquers-validate` accepts
`-R/data/sales.csv/-/ns-pl/sort-amont` (status `Ok`); evaluation fails inside Polars on the
unknown column. `EnumArgument` (`liquers-core/src/command_metadata.rs`) holds a fixed `values`
list, and `CommandMetadataRegistry` carries `// TODO: support for dynamic global enums`.

Should be, for a command whose `column` is declared `enum_ref: "pl.columns"` and a provider of
scope *input* registered under `pl.columns`:

- the query still plans (the planner has no input frame);
- before the command runs, the value is checked against the frame's columns, and evaluation fails
  with *"Undefined enum value 'amont' for argument column. Valid values: region, amount, date"*;
- a UI asks `enum_alternatives(envref, key, "column", Some(&state))` and gets the three columns.

For a provider of scope *environment* (say `demo.tables`, the keys under `tables/`), the wrong
value is refused when the query is planned.

## Scope and Acceptance Criteria

- **AC-1** Environment-scoped alternatives are checked at plan time
  WHEN a query gives a value that the environment-scoped provider does not list, and the enum does
  not allow others
  THEN `make_plan` fails with a `ConversionError` naming the argument, the value and the valid
  aliases
- **AC-2** Input-scoped alternatives are checked before execution
  WHEN an action's value is not among the input-scoped provider's alternatives for its input state
  THEN the action fails with the same error before the command function is called, and the query
  plans without error
- **AC-3** Valid values pass unchanged
  WHEN the value is listed (or the enum allows others)
  THEN the command receives the query text as the value, exactly as with `others: true` today
- **AC-4** Alternatives can be listed
  WHEN `enum_alternatives` is called for an argument
  THEN it returns static alternatives followed by computed ones, with `complete: false` when an
  input-scoped provider was not given an input or has no provider in this environment
- **AC-5** Provider failure is not a query failure
  WHEN a provider returns an error
  THEN planning (or execution) continues, the value is accepted, and a warning naming the enum and
  the error is logged
- **AC-6** No provider, no check
  WHEN metadata declares a computed enum but no provider is registered (`liquers-validate`, an
  imported registry)
  THEN any value of the enum's type plans, and `EnvironmentBuilder::validate` reports a warning
- **AC-7** Static enums are unchanged
  WHEN an enum is not computed
  THEN planning, serialization, `metadata_version` and `specs/command_registry.yaml` are as today
- **AC-8** Variadic elements are checked one by one
  WHEN a `multiple` argument refers to a computed enum
  THEN each element is checked, and the error names the first unlisted one

Non-goals: rewriting a value through a computed alias; checking linked or default values; computed
alternatives in MCP schemas; declaring `ns-pl` column arguments computed (a `liquers-lib`
follow-up); providers in Python or JavaScript.

## Core Interactions

Commands (metadata and the command registry), planning (`make_plan`), the interpreter's action step,
the environment builder's validation, and the read API a UI calls. Store, assets and value types
are untouched.

## Crate Placement

`liquers-core` only: the metadata field, the provider trait, both checks and the listing function.
`liquers-macro` needs one generated field (`computed: None`). Providers themselves live with their
commands (`liquers-lib`).

## Documentation Intent

- Reference: extend `REGISTER_COMMAND_FSD.md` (the `enum_ref` row: may name a computed enum) and
  `COMMAND_DECLARATION.md` (the `computed` field of an enum).
- Guide: `COMMAND_REGISTRATION_GUIDE.md` §Enum Parameters gains "Computed alternatives".
- Other documents: none.
- Documents to update: the three above.

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — `L`, a new `pub` trait, enum and field in `liquers-core`
  (rules 1, 3, 4).
- **Leading issue:** **Open design - computed alternatives validate only.**
- **Explanation:** every hook exists (async `make_plan_with_cwd`, the input state in the
  `Step::Action` arm, global enums by name). Phases 3-4 assume the recommendations below.
- **Open questions:**
  1. **Open design - computed alternatives validate only.** The query text is the value; a
     provider's `value` field is ignored. Recommended: this removes the caching question the issue
     raises, because a result depends on the query and the input, never on the alternatives at the
     time. Alternative: let a computed alias expand to another value; then every result would
     depend on provider output that no key or version records, and the command would have to be
     volatile.
  2. **Open design - provider failure.** Recommended: warn and accept (AC-5); the command still
     validates its own input. Alternative: fail the query, which turns a store hiccup in a
     *listing* into a failure of a query that may not need the listing.
  3. **Proposed resolution - scopes.** Two, declared in metadata so a planner without providers
     knows what it cannot check: `environment` (plan time, `input: None`) and `input` (execution
     time). Async, because a listing reads a store.
  4. **Proposed resolution - computed enums are global.** Only a global enum (`enum_ref:`) can be
     computed; the provider is registered under the enum's name. An inline enum with `computed`
     set is a check warning, which also covers commands imported from another realm.
  5. **Proposed resolution - no caching in core.** Each check calls the provider; a provider that
     is expensive caches for itself.

## Design Dependencies

- `global-enum-registration` (`COMMAND-METADATA-ENHANCEMENTS`) — **requires**: providers register
  their enum with its `add_global_enum`; its versioning then hashes `computed` too. First.
- `enum-alternative-labels` (`ENUM-ALTERNATIVES-HAVE-NO-LABEL`) — **overlaps**, weak: labelled
  computed alternatives. Either order.
- `GLOBAL-ENUMS-NOT-IMPORTED-BY-LANGUAGE-INTEGRATIONS`, `registry-publication` — **overlaps**,
  weak: a computed enum reaching JavaScript, Python or a client has no provider there (AC-6).
- `command-io-type-declarations` (`COMMAND-INPUT-OUTPUT-TYPES-ARE-UNDECLARED`) — **overlaps**, weak: typed
  inputs would let an input-scoped provider refuse a wrong type early.
- Overlap judgement: no T1-T4 with any open item; `global-enum-registration` is a shared
  prerequisite with a finished Phase 4, so `requires`, not a merge.

## Consolidated Findings

- `PlanBuilder` stays synchronous: it treats a computed enum like `others: true`; the environment
  check is an async pass after `analyze_plan_dependencies` in `make_plan_with_cwd`. Synchronous
  paths (`Recipe::to_plan`) skip it; evaluation does not.
- Providers are found through a defaulted `CommandExecutor::enum_provider`, so other executors
  compile unchanged.
- A cached asset is served without re-planning, so a value accepted once stays accepted for that
  key — a consequence of question 1, documented.
- Metadata gains only an optional `computed` field; the macro's `EnumArgument` literal gains
  `computed: None`. AC-7 holds.
