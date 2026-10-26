# Phase 1: High-Level Design — automatic conversion of a command's state and link arguments

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — `L`; changes observable interpreter behaviour for declared
  commands and depends on two unimplemented designs (rules 1, 2, 5).
- **Leading issue:** **Open design - what a refused conversion does** (question 1).
- **Explanation:** the conversion registry's interface is fixed by `value-conversion-registry`
  Phase 2 and the declarations by `command-io-type-declarations` Phase 2; this design only
  consults them at one site, so Phases 3-4 are written against those interfaces.
- **Open questions:**
  1. **Open design - a refused or failed conversion:** when the input's type is not declared and no
     automatic edge reaches a declared type (no edge, or a `Lossy` / `Fallible` one, or an edge
     that fails on this value), the value **passes through unchanged and the asset log gets a
     warning** naming the edge and its class. Recommended: the command then behaves exactly as
     today, and a declaration is never what breaks a pipeline. Alternative: fail with a
     `ConversionError` before the command runs — declared types become a guarantee, but every
     incomplete declaration becomes an outage.
  2. **Open design - scope split:** state and link arguments here; a widget accepting "a table" and
     `Accept: text/csv` negotiation become `UI-WIDGET-DOES-NOT-CONVERT-ITS-VALUE` and
     `HTTP-ACCEPT-HEADER-NOT-NEGOTIATED` (filed). Recommended: those two need purposes and
     serialization choices this design does not make.
  3. **Proposed resolution - declaring is opting in:** no separate flag. A command that declares
     `accepts` on its state or an argument asks for conversion there; an undeclared one is never
     touched (draft: "a command declaring `state: Table` is asking for it").
  4. **Proposed resolution - target choice:** the first declared identifier, in declaration order,
     that the input reaches by an automatic edge. No transitive search.
  5. **Proposed resolution - converted metadata:** the state's type identifier becomes the target's,
     its `data_format` is cleared so the seeding cascade picks the new type's default, and an info
     log entry names source, target, class and command. Key, query and title are kept.
  6. **Proposed resolution - only values are converted:** a link argument's value and the state.
     A parameter written in the query text is parsed by its `ArgumentType`, as today.
  7. **Proposed resolution - purposes deferred** to the widget issue, where "a table" first matters.

## Purpose

Let a command declare the type it works on and receive that type, without writing its own
coercions with its own rules for lossy cases (VISION priority 2, DataFrames and records; "Trust the
result": a conversion is logged and never silently lossy).

## Problem Example

A user (or Python) command `summarize` works on a record view and declares
`accepts: "RecordView"`; the previous step `fetch_people` returns a JSON array of objects (`Array`).

```
fetch_people/summarize
```

Today: `summarize` receives the `Array` and must call the record bridge itself (as every `ns-rec`
command does through `to_record`, with its own rules); a Python command has no such helper at all.
Should be: the interpreter sees that `Array` is not declared, finds `Array → RecordView`
(`Structural`, automatic), converts, logs `converted state Array → RecordView (Structural) for
'summarize'`, and the command receives a `RecordView`. A `RecordView` reaching a command that
accepts only `polars.DataFrame` is *not* converted (that edge is `Lossy`), and the log says so.

## Scope and Acceptance Criteria

- **AC-1** The state is converted by an automatic edge
  WHEN a command declaring `accepts: "I64"` receives an `I32` state
  THEN it runs on the `I64` value and the log has an info entry naming `I32 → I64 (Widening)` and the command
- **AC-2** A declared type is not touched
  WHEN the state's type is in the command's `accepts`
  THEN no conversion runs and nothing is logged
- **AC-3** A refused edge passes through with a warning
  WHEN the only edge to any declared type is `Lossy` or `Fallible`, or there is none, or it fails on this value
  THEN the command receives the value unchanged and the log has a warning naming the types and the reason
- **AC-4** The converted state's metadata follows the value
  WHEN a state is converted
  THEN its type identifier is the target's and its data format is unset
- **AC-5** A link argument is converted
  WHEN an argument declared `(accepts: "I64")` is given a link whose result is `I32`
  THEN the command's argument holds the `I64` value
- **AC-6** Undeclared commands are unchanged
  WHEN a command declares no `accepts`
  THEN its state and arguments reach it exactly as before this change
- **AC-7** The first reachable declared type wins
  WHEN a command accepts `[polars.DataFrame, RecordView]` and the input is an `Array`
  THEN it is converted to `RecordView`, the first declared type with an automatic edge
- **AC-8** Not limited to the macro
  WHEN a command registered through the declaration format declares `accepts`
  THEN the same conversion applies

Non-goals: UI widgets and HTTP negotiation (question 2); purposes; transitive conversion;
converting query-text parameters; converting a command's output; async conversion.

## Core Interactions

Interpreter (`Step::Action`); Value types (conversion registry, metadata of a converted state);
Commands (declared `accepts`); Assets (log entries).

## Crate Placement

`liquers-core/src/interpreter.rs` only, plus tests: the conversion registry is in the environment
(`value-conversion-registry`) and the declarations in the metadata (`command-io-type-declarations`).

## Documentation Intent

- Reference: `VALUE_TYPE_SYSTEM.md` §Conversion gains "Automatic conversion at a command".
- Guide: `COMMAND_REGISTRATION_GUIDE.md` and `TYPE_SYSTEM_GUIDE.md`: what declaring `accepts` buys.
- Other documents: none.

## Design Dependencies

- `command-io-type-declarations` — **requires**: `accepts` on state and arguments.
- `value-conversion-registry` — **requires**: `Environment::get_conversion_registry`,
  `ConversionRegistry::{can_convert, convert}`, `ConversionClass::is_automatic`.
- `command-specialization-by-input-type` — **overlaps**: its `Step::Dispatch` selects a variant
  first; conversion then applies to the chosen action. Either order compiles.
- `schema-argument-structured-link` — **overlaps**: changes `TryFrom<Value> for String`, the
  extraction that runs after this conversion; independent.

## Consolidated Findings

- One site serves every language: `do_step`'s `Step::Action` arm holds the input `State` and the
  resolved link states, before `CommandExecutor::execute_async`; Python and JavaScript commands
  get conversion without binding work.
- The interpreter does not hold the command's metadata today; it looks it up with
  `find_command(realm, ns, action_name)` as `requires_payload` already does (a linear scan of
  ~170 entries per action; acceptable, noted in Phase 2 risks).
- For an alias step the metadata found is the target's, which is the command that runs.
- Under the recommended answer to question 1 no existing pipeline can change outcome from failure
  to success or back, except that a declared command now receives a converted value where an
  automatic edge exists — which is the feature.
- Order: `command-io-type-declarations` and `value-conversion-registry` first.

## References

- `specs/issues/VALUE-NOT-CONVERTED-AUTOMATICALLY.md`; `specs/design/value-type-system/type-conversion-draft.md`
  §"Where automatic conversion applies"; `specs/design/value-conversion-registry/phase2-architecture.md`.
