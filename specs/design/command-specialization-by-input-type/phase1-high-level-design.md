# Phase 1: High-Level Design — a generic command dispatches by its input's type

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — `L`; a new `CommandDefinition` and `Step` variant, a new
  `pub` registration method and a new root command (rules 1, 3, 4).
- **Leading issue:** **Open design - the model** (question 1).
- **Explanation:** the mechanism reuses the alias machinery (an entry in the metadata registry with
  a `definition`, resolved by the planner, recorded in the step's origin) and needs only the
  declarations of `command-io-type-declarations`. Phases 3-4 assume the answers below.
- **Open questions:**
  1. **Open design - the model:** a *generic* command is its own metadata entry,
     `CommandDefinition::Generic { variants }`, listing existing commands (registered or aliases)
     in preference order; `CommandRegistry::register_generic` builds and checks it. Recommended
     over each variant declaring `specializes: head`: one place lists the candidates and their
     order, the registry export shows it, and no `CommandKey` change is needed (variants keep their
     own keys, executors and namespaces).
  2. **Open design - when the variant is chosen:** at plan time when the declared type flowing into
     the action selects one variant (the plan then names it, origin `Specialized`); otherwise at run
     time, through a `Step::Dispatch` that lists every variant's fully planned step and is resolved
     by the input value's type identifier. Recommended: resources have no plan-time type, so run
     time is needed, and plan time keeps the common typed case explainable.
  3. **Open design - selection rule:** (a) the variant whose *principal* (first declared) type is
     the input's type; else (b) the first variant, in the generic's order, whose `accepts` contains
     it; else (c) the one undeclared variant, if any; else a `ConversionError` naming the generic,
     the input type and each variant with its types. Principals must be unique and at most one
     variant may be undeclared — checked at registration, so selection is never ambiguous.
     Declarations may overlap, which honest declarations need (`pl/head` and `rec/head` both take
     CSV text). No conversion is tried during selection.
  4. **Open design - which generics ship:** only `head` (root namespace: `pl/head`, then
     `rec/head`) as the reference generic. Recommended; others follow on demand.
  5. **Proposed resolution - variants share the argument shape:** the same count, names,
     `argument_type`, `injected` and `multiple`, so one query parameter list fits every variant;
     defaults and labels may differ (each variant's step is resolved with its own metadata).
  6. **Proposed resolution - limits:** a variant is not itself generic; generic and variants are in
     one realm; an alias whose target is a generic is refused (as an alias of an alias is).
  7. **Proposed resolution - error type:** no new `ErrorType`; a no-match is a `ConversionError`.

## Purpose

Let a query author write `head-5` once for any table-like input instead of knowing whether it is a
Polars frame or a record view (VISION priority 2, DataFrames and records; "one query language"; an
agent can compose pipelines without tracking types by hand).

## Problem Example

```
-R/data/sales.csv/-/head-5
-R/data/sales.csv/-/ns-rec/to_record/ns-root/head-5
```

Today both fail to plan: `Action 'head' not registered`. The author must pick `ns-pl/head` or
`ns-rec/head`. Should be: the first runs `pl/head` (the file's value is `Bytes` or `Text`, so the
variant is chosen at run time and logged); the second plans `rec/head` directly, because `to_record` declares it produces a
`RecordView`, and the plan records that `head` was specialized.

## Scope and Acceptance Criteria

- **AC-1** Run-time dispatch
  WHEN `-R/data/sales.csv/-/head-5` is evaluated with the shipped `head`
  THEN `pl/head` runs on the CSV, the result is a five-row frame, and the log names `head`, `pl/head` and the input's type (`Bytes` or `Text`, as the store reads the file)
- **AC-2** Plan-time specialization
  WHEN the input's declared type selects one variant (`…/ns-rec/to_record/ns-root/head-5`)
  THEN the plan's step runs `rec/head` and its origin is `Specialized { command: head, variant: rec/head }`
- **AC-3** Principal first, then order, then fallback
  WHEN the input is a variant's principal type, or only accepted by a later variant, or accepted by none but one variant is undeclared
  THEN that variant is chosen, in that precedence
- **AC-4** No match is a typed error
  WHEN no variant accepts the input and none is undeclared
  THEN evaluation fails with a `ConversionError` naming the generic, the input type and each variant's accepted types
- **AC-5** Registration checks
  WHEN `register_generic` gets an unknown, generic, source or other-realm variant, mismatched arguments, a repeated principal or two undeclared variants
  THEN it returns an error and registers nothing
- **AC-6** Flags and dependencies are conservative
  WHEN a plan contains a run-time dispatch
  THEN it is volatile, payload-requiring or uncached if any variant is, and it depends on every variant's metadata and implementation
- **AC-7** It round-trips and validates
  WHEN the registry is exported and read by `liquers-validate`
  THEN the generic appears with its variants and the problem example validates
- **AC-8** Namespaced names are unchanged
  WHEN a query says `ns-pl/head-5`
  THEN it plans `pl/head` exactly as before

Non-goals: dispatch on argument types; converting the input to reach a variant
(`automatic-value-conversion` converts after selection); cross-realm variants
(`multi-realm-dispatch`); generics other than `head`.

## Core Interactions

Commands (registry, definition, registration); Plan (selection, new step, origin, flags,
dependencies); Interpreter (run-time selection); validation and registry export.

## Crate Placement

Mechanism in `liquers-core` (`command_metadata.rs`, `commands.rs`, `plan.rs`, `interpreter.rs`,
`error.rs`, `validate/report.rs`); the `head` generic in `liquers-lib` (`commands.rs` registration
after `ns-pl` and `ns-rec`); `liquers-mcp` gets the new `Step` arm.

## Documentation Intent

- Reference: `COMMAND_ALIASES.md` (a sibling section "Generic commands"), `RECIPES_AND_PLANS.md`
  (`Step::Dispatch`, `ActionOrigin::Specialized`), `COMMAND_DECLARATION.md` (`definition: Generic`).
- Guide: `COMMAND_REGISTRATION_GUIDE.md` "Registering a generic command".
- Other documents: `specs/command_registry.yaml` regenerated.

## Design Dependencies

- `command-io-type-declarations` — **requires**: `accepts`, `principal`, and `PlanBuilder::flowing`.
- `automatic-value-conversion` — **overlaps**: runs on the chosen variant; no shared code.
- `multi-realm-dispatch` — **overlaps**: another rewriting of what a step runs; variants stay in
  one realm here.
- `global-enum-registration` — **overlaps**: also extends the exported registry; independent.

## Consolidated Findings

- `pl/head` is an alias of `pl/slice` (head parameter 0), so variants must be allowed to be aliases;
  this is why run-time dispatch carries fully planned per-variant steps rather than one parameter
  list for an executor lookup.
- `ActionOrigin`'s doc comment already anticipates a specialization variant; the dependency scan,
  `last_action_command` and the interpreter's error naming read the origin and need the new arm.
- Adding `Step::Dispatch` touches every exhaustive `Step` match (about fifteen, including
  `liquers-mcp` and `store/realm.rs`); compile errors find them.
- Selection never converts, so the run-time choice is a pure function of the input's identifier.
- A plan-time choice trusts `produces`; a wrong declaration picks the wrong variant, which is why
  `command-io-type-declarations` makes declarations honest (its question 4).
- Order: `command-io-type-declarations` first; then this design (either order with conversion).

## References

- `specs/issues/COMMAND-SPECIALIZATION-BY-INPUT-TYPE-MISSING.md`; `specs/reference/COMMAND_ALIASES.md`.
