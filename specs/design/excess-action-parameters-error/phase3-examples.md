# Phase 3: Examples & Use-cases - Excess Action Parameters Error

## Introduction

Phase 1 asked for an error that names *where* the surplus parameter is, so an editor can point at
it. Everything below tests exactly that pair of claims: the plan stops being built, and the failure
carries the position of the offending parameter.

The examples run from the everyday case (a user writes one parameter too many) through the two
shapes that decide whether the rule is right — the exemptions that must *not* fire, and the
resource header, which the design makes strict in one place while deliberately leaving the adjacent
warning alone. The pitfalls section covers what a reader is most likely to get wrong afterwards:
`select_columns-a-b` now fails, and the fix is an escape, not a retreat.

**Examples are runnable.** Nothing here needs a store, an async runtime, a fixture file or a
registered command implementation — `ResolvedParameterValues::from_action` takes a hand-built
`CommandMetadata`, so every case is a plain `#[test]` in `liquers-core/src/plan.rs` beside the
existing `test_resolved_parameter_values`. Conceptual sketches would be strictly worse here: the
whole design is about a boundary condition, and boundary conditions are what compile-checked tests
are for.

## Overview Table

| # | Example / test | What it demonstrates or checks | Where |
|---|---|---|---|
| **E1** | One parameter too many | The primary workflow: `to_text-extra` errors instead of building a plan; the error is `TooManyParameters` and its position is the position of `extra` | `plan.rs` unit test |
| **E2** | Exemptions must not fire | Exactly-saturated, under-supplied, variadic, and injected actions all still build | `plan.rs` unit test |
| **E3** | The resource header, both halves | Surplus header parameters error; the ignored header *name* still only warns | `plan.rs` unit test |
| **P1** | `select_columns-a-b` | The pitfall a user meets first, and the `~_` escape that resolves it | doc + validate transcript |
| **T1** | `too_many_parameters` message and fields | Constructor sets `ErrorType::TooManyParameters`, preserves position, names index and value | `error.rs` unit test |
| **T2** | First excess only | Three surplus parameters report the *first*, not the last or a count | `plan.rs` unit test |
| **T3** | Link parameter in excess position | `ActionParameter::Link` reports as `~X~…~E` via `encode()`, not a debug form | `plan.rs` unit test |
| **T4** | `accepted` excludes injected arguments | A command with one real and one injected argument reports `accepts 1`, not 2 | `plan.rs` unit test |
| **T5** | `accepted` excludes alias head parameters | `from_action_extended` with head parameters reports what the *action* may supply | `plan.rs` unit test |
| **T6** | Recipe path is equally strict | `allow_placeholders = true` does not soften the check (decision 1) | `plan.rs` unit test |
| **T7** | Unknown header instruction | Corrected message, and it now carries a position (decision 5) | `plan.rs` unit test |
| **T8** | End-to-end through `PlanBuilder` | A full query fails at plan build, not only at the helper | `plan.rs` unit test |
| **T9** | Validator contract | An over-supplied query reports `status: Error` and exit 1 | `liquers-core/src/validate` test |
| **T10** | Committed material still builds | The three suites pass unchanged | `cargo test`, Phase 4 |

## Example 1 — One parameter too many

The primary case, and the one the issue opened with. `to_text` declares no arguments.

```rust
#[test]
fn excess_action_parameter_is_rejected() -> Result<(), Error> {
    let cm = CommandMetadata::new("to_text");           // no arguments declared
    let action = "to_text-extra".try_to_query()?.action().ok_or_else(
        || Error::general_error("expected an action".to_string()))?;

    let err = ResolvedParameterValues::from_action(&action, &cm, false)
        .expect_err("an excess parameter must not build");

    assert_eq!(err.error_type, ErrorType::TooManyParameters);
    // The position is the point of the whole design: it is what an editor highlights.
    assert_eq!(err.position, action.parameters[0].position());
    assert!(err.message.contains("to_text"));
    assert!(err.message.contains("extra"));
    Ok(())
}
```

The sequence this exercises: `from_action` → `from_action_extended` serves every declared argument
(here, none) → the leftover check asks `ActionParameterIterator::next()` → `Some(extra)` → the error
is built from `encode()`, `position()` and `parameter_number`.

## Example 2 — The exemptions, which must not fire

The rule is only correct if it stays silent in four cases. This is the test that would catch an
over-eager implementation.

```rust
#[test]
fn arity_boundaries_still_build() -> Result<(), Error> {
    let mut cm = CommandMetadata::new("cmd");
    cm.with_argument(ArgumentInfo::string_argument("a").to_owned());
    cm.with_argument(ArgumentInfo::string_argument("b").with_default("bee").to_owned());

    // Exactly saturated — the boundary itself.
    assert!(ResolvedParameterValues::from_action(
        &"cmd-x-y".try_to_query()?.action().unwrap(), &cm, false).is_ok());

    // Under-supplied — `b` falls back to its default. Unchanged behaviour.
    assert!(ResolvedParameterValues::from_action(
        &"cmd-x".try_to_query()?.action().unwrap(), &cm, false).is_ok());

    // Variadic — `multiple` drains the iterator, so nothing is ever left over.
    let mut vcm = CommandMetadata::new("vcmd");
    vcm.with_argument(ArgumentInfo::string_argument("items").set_multiple());
    assert!(ResolvedParameterValues::from_action(
        &"vcmd-a-b-c-d".try_to_query()?.action().unwrap(), &vcm, false).is_ok());

    Ok(())
}
```

The variadic case is the important one: it must pass *without* a special case in the check, because
`pop_value` has already emptied the iterator. If a future refactor computes surplus from
`action.parameters.len()` against `arguments.len()` instead of asking the iterator, this test fails
— which is precisely why it is written against behaviour rather than against the count.

## Example 3 — The resource header, and the warning that stays

The design makes one of the header's two warn-and-ignore paths strict. This test pins both halves so
nobody later "finishes the job" and makes the other one strict too.

```rust
#[test]
fn header_surplus_errors_but_ignored_name_only_warns() -> Result<(), Error> {
    // Surplus header parameters: now an error.
    let err = build_plan("-R-meta-extra/data/x.txt").expect_err("surplus must not build");
    assert_eq!(err.error_type, ErrorType::TooManyParameters);
    assert!(!err.position.is_unknown());

    // A header *name* is reserved for a future realm interpretation (plan.rs:1238),
    // so it must keep warning rather than failing.
    let plan = build_plan("-Rname/data/x.txt")?;
    assert!(plan.has_warning());
    assert!(!plan.has_error());
    Ok(())
}
```

## Common Pitfalls

### P1 — `select_columns-a-b` stops working, and the fix is an escape

The symptom a user hits first. `pl/select_columns` is documented as taking "column names separated
by dashes", but `-` is the *parameter* separator, so that spelling was two parameters against a
one-argument command all along. It used to select only `a`; now it fails.

| Written | Before | After |
|---|---|---|
| `ns-pl/select_columns-a-b` | selects `a`, silently | **error** at parameter #2 |
| `ns-pl/select_columns-a~_b` | selects `a` and `b` | unchanged — selects `a` and `b` |

Verified against the committed 95-command registry:

```
ns-pl/select_columns-name~_price  ->  pl/select_columns(columns = "name-price")   status Ok
```

The command splits that single argument on `-` internally, which is why the escaped form does what
the documentation promised. The unescaped form becomes expressible again when
`COMMAND-VARIADIC-ARGUMENTS-NOT-DECLARABLE` lands.

### P2 — "The plan built" now means more than it did

Before this change a clean `liquers-validate` run did not imply that every parameter written was
used. It does now, for action and header parameters alike. Anything that treated `status: Ok` as
proof of a *correct* query still needs to read the resolved parameters for meaning — a query can
still mean something other than intended — but it can no longer silently discard input.

### P3 — Don't reach for a warning to soften the break

`Step::Warning` carries a `String` and no `Position` (`plan.rs:1712`), so a warning could not name
the offending parameter even if the rule were relaxed. Softening the error would discard the feature
Phase 1 asked for, not just its severity.

## Corner Cases

| Case | Expected | Covered by |
|---|---|---|
| Link (`~X~q~E`) in excess position | Reported via `encode()` as `~X~q~E`, not a `Debug` form | T3 |
| Three surplus parameters | First is reported; not the last, not a count | T2 |
| Injected argument among the declared ones | Not counted in `accepted`; consumes no parameter | T4 |
| Alias with head parameters | `accepted` counts only what the action may supply | T5 |
| `allow_placeholders = true` (recipes) | Errors identically — decision 1 | T6 |
| Variadic argument | Never errors; iterator already drained | E2 |
| Empty parameter (`cmd-`) against a zero-argument command | Errors — an empty string is still a parameter, and `parse.rs` documents `action-` as one empty parameter | T2 (variant) |
| Header with exactly one parameter | Unchanged; no error | E3 |
| Header name non-empty | Warning only, never an error | E3 |
| Unknown header instruction | `NotSupported`, corrected message, now positioned | T7 |
| Zero-argument command, zero parameters | Builds, as always | E2 |

**Concurrency, memory and serialization:** not applicable, and deliberately so. The change adds no
state, no allocation outside the error path, no `Send`/`Sync` surface and no serialized field —
`ErrorType::TooManyParameters` is an existing variant already covered by the derives and already
mapped in `liquers-py`, `liquers-axum` and `liquers-web`. There is nothing here for a concurrency or
round-trip test to exercise.

## Test Plan

### Unit tests — `liquers-core/src/plan.rs`

Placed in the existing `#[cfg(test)] mod tests`, beside `test_resolved_parameter_values` (`:2639`),
which is the closest existing test and the one whose fixtures they reuse.

| Test | Asserts |
|---|---|
| `excess_action_parameter_is_rejected` | E1 — error type, position, message content |
| `arity_boundaries_still_build` | E2 — saturated, under-supplied, variadic all `Ok` |
| `excess_reports_first_surplus_only` | T2 — with `cmd-a-b-c-d` against one argument, position is `b`'s |
| `excess_link_parameter_encodes` | T3 — message contains `~X~`, no panic |
| `accepted_count_excludes_injected` | T4 — message says `accepts 1` for one real + one injected |
| `accepted_count_excludes_head_parameters` | T5 — via `from_action_extended` with a non-empty `head_parameters` |
| `excess_errors_under_allow_placeholders` | T6 — same error with the flag set |
| `header_surplus_errors_but_ignored_name_only_warns` | E3 — both halves |
| `unknown_header_instruction_is_positioned` | T7 — message names the instruction, position known |
| `plan_builder_rejects_excess_end_to_end` | T8 — through `PlanBuilder::build`, not just the helper |

### Unit tests — `liquers-core/src/error.rs`

| Test | Asserts |
|---|---|
| `too_many_parameters_constructor` | T1 — `ErrorType::TooManyParameters`; position preserved; message contains subject, accepted count, 1-based index and the excess value |

### Validator test — `liquers-core/src/validate`

| Test | Asserts |
|---|---|
| `over_supplied_query_reports_error` | T9 — `ValidationStatus::Error` for `to_text-extra` |

This is the test that pins the *contract*, and it is worth stating what changed. The issue
anticipated `status: Warning` with exit 0. Because the resolution is an error, the query reports
`ValidationStatus::Error` and the CLI exits **1**. `ValidationStatus::Warning` is untouched by this
design and keeps its meaning for the header-name case and the other `init_warning` sources.

### Regression — the suites

```bash
cargo test -p liquers-lib --lib --tests     # the default loop; builds core, macro, store
cargo test -p liquers-core                  # the crate that changes
```

`liquers-web` and the browser suites need no separate run: this design touches neither, and its
error variant was already mapped there. Run them only if the full pre-merge sweep is wanted, after
`cargo clean`, per `CLAUDE.md`.

### Measured breakage in committed material

Run before writing any code, so the answer is not coloured by the change.

**Method.** 176 query literals harvested from `liquers-core`, `liquers-lib` and `liquers-axum`
sources and run through `liquers-validate` against the committed 95-command registry. For every
resolved action, the written parameter count was compared against the number of resolved
non-`Injected` argument slots, skipping variadic actions.

**Result: one hit, and it does not break.**

| Query | Location | Verdict |
|---|---|---|
| `to_text-~X~-R/data/report/-/to_text~E` | `parse.rs:236` (doc example), `parse.rs:2008` (test) | **Safe** — both are parse-level; `parse_query(...).is_ok()` never builds a plan |

**Coverage and its limit, stated honestly.** 44 of the 176 resolved against the registry; 123 failed
with `ActionNotRegistered` because they use commands registered locally inside test setups, and 7
were not queries at all. Registry-backed scanning cannot check those 123, so the 31 with two or more
parameters were inspected by hand. The two that reach parameter resolution are both safe:

- `plan.rs:2614` `hello-testarg-123` — calls `pop_value` directly, never `from_action_extended`, so
  the leftover check is not on its path;
- `plan.rs:2651` `testcommand-xxx-234` — exactly saturated against two declared arguments.

The residual risk is therefore confined to test-local commands whose queries were not harvested, and
the three suites in Phase 4 are what closes it. This measurement narrows the expectation from
"unknown" to "expect zero failures"; it does not replace running them.

## Documentation and Learning Log

### Guide-worthy material

| Destination | Content | Executable evidence |
|---|---|---|
| `specs/reference/PROJECT_OVERVIEW.md` | The arity rule: every action parameter must be consumed by a declared argument; `multiple` consumes the remainder; surplus is an error carrying the parameter's position. **Must distinguish the header's two ignored inputs** — surplus parameters error, a reserved name warns — rather than calling the header strict | E3 |
| `specs/guides/COMMAND_REGISTRATION_GUIDE.md` | A command that accepts a variable-length list needs a `multiple` argument — plus the note that the flag is not yet declarable, linking `COMMAND-VARIADIC-ARGUMENTS-NOT-DECLARABLE` | E2's variadic case |
| `specs/reference/POLARS_COMMAND_LIBRARY.md` | The `~_` spelling for `select_columns` / `drop_columns`, with the reason: `-` separates parameters | P1 |

The P1 table (before/after/escape) is the snippet worth lifting verbatim into the polars reference —
it answers "why did my query stop working" in three lines.

### Learning to carry into Phase 5

1. **The warning channel cannot carry a position.** `Step::Warning(String)` has no `Position` field,
   so "which parameter" is inexpressible as a warning. This is the load-bearing reason the
   resolution is an error rather than the warning the issue proposed, and it is not obvious from
   either the issue or the code.
2. **Two ignored inputs, two correct treatments.** A *reserved* input (the header name, awaiting
   realm support) must warn; an *unconsumable* one (surplus parameters) must error. Consistency
   between them would be the wrong goal.
3. **`accepted` is not `arguments.len()`.** Injected arguments and alias head parameters both
   subtract from what a query may supply. Easy to get wrong, and wrong only in the message — which
   is exactly the kind of defect that survives review.
4. **The variadic exemption is structural, not special-cased.** It falls out of `pop_value` draining
   the iterator. Worth stating so nobody adds a redundant guard.
5. **The documented escape hatch was undeclarable.** `multiple` had a full runtime and no way to
   declare it — the feature was half-built in a way no test would reveal, because nothing used it.
