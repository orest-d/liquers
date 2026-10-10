# Phase 1: High-Level Design — Context parameter at any position

## Purpose

`context` is not a command argument: like the state, it is always available and never comes from
the query. `register_command!` should accept it at **any** position and pass it where the function
declares it; a placement the macro cannot honour is a compile-time error at the offending token.
Merged in: AC-10 to AC-12.

## Where the work stands (2026-10-10)

The 2026-09-02 findings (`specs/archive/2026-09-02-context-param-order-{findings,solution}.md`)
diagnosed an argument-slot index shift; **that fix has landed** in
`CommandSignature::extract_all_parameters` (`liquers-macro/src/registration.rs`). What remains:

| Signature or symptom | Today |
|---|---|
| `fn f(state, a: i64, context, b: String)` | works, but no end-to-end test |
| `fn f(context, state, a: i64)` | ``expected `:` `` at `state`: state keywords are recognised only first |
| `fn f(state, context, a: i64, context)` | accepted, then a moved-value error inside the expansion |
| `fn f(value: String)` (no state) | ``expected `)` ``: `value` taken as the keyword despite the `:` |
| `fn f(a: i64, state)`, `fn f(state, state)` | rejected with the misleading ``expected `:` `` |
| `a: i32 (hint key: "v")` | parsed and discarded; rustc warns ``fields `0` and `1` are never read`` |
| a missing argument | ``Missing argument #0:name`` — 0-based, while `too_many_parameters` says `parameter #1` |

## Problem Example

```rust
async fn load(context: Context<CommandEnvironment>, state: State<Value>, limit: i64)
    -> Result<Value, Error> { /* … */ }
register_command!(cr, async fn load(context, state, limit: i64) -> result)?;
```

**Today:** ``expected `:` `` at `state`; the guides prescribe "context last" as a rule.
**Should:** it compiles, the wrapper calls `load(context, state, limit__par)`, the metadata has one
argument `limit`, and the query `load-5` runs with `limit = 5`.

## Scope and Acceptance Criteria

- **AC-1** Context before the state
  WHEN `fn f(context, state, n: i64)` is registered and the query `f-5` is evaluated
  THEN `f` receives the context, the input state and `n = 5`, and the metadata lists only `n`
- **AC-2** Context between arguments, with an injected argument
  WHEN `fn f(state, a: i64, context, b: T injected, c: String)` is evaluated with `f-1-x`
  THEN `a = 1`, `c = "x"`, `b` is injected, and `f` receives its context third
- **AC-3** Context first without a state
  WHEN `fn f(context, n: i64)` is registered and `f-5` is evaluated
  THEN `f` receives the context and `n = 5`
- **AC-4** Duplicate context is a compile-time error
  WHEN a signature declares `context` (or `Context`) twice
  THEN expansion fails at the second occurrence saying `context` is declared twice
- **AC-5** A first argument named like a state keyword is an argument
  WHEN a command without a state declares `fn f(value: String)` (likewise `text:`, `state:`)
  THEN it registers an argument `value`; a bare `value` / `text` / `state` is still the keyword
- **AC-6** Misplaced or repeated state is a compile-time error
  WHEN the state keyword follows an argument (`fn f(a: i64, state)`) or appears twice
  THEN expansion fails at that keyword saying the state must precede every argument and only `context` may come before it
- **AC-7** Existing commands are unaffected
  WHEN the change lands
  THEN every existing `register_command!` use compiles unchanged and `registry_export` passes without regenerating `specs/command_registry.yaml`
- **AC-8** The documentation states the rule, not the workaround
  WHEN the reference, the guides, `CLAUDE.md` and the `rust-best-practices` skill are read
  THEN they say `context` may appear anywhere and occupies no argument slot; none requires "context last"
- **AC-9** The documentation recommends a conventional position
  WHEN the reference and the guide say where to put `context`
  THEN they recommend last, or immediately before a `multiple` argument, because a Python signature takes no positional parameter after `*args`
- **AC-10** The macro's unit tests assert and do not print
  WHEN `registration.rs`'s test module is run or searched
  THEN it has no `println!`, and tests that only printed tokens assert on them (`MACRO-TESTS-PRINT-TO-STDOUT`)
- **AC-11** An argument hint is rejected, not discarded
  WHEN an argument option list contains `hint key: "value"`
  THEN expansion fails at `hint` saying argument hints are not supported, and `liquers-macro` builds with no dead-code warning
- **AC-12** Command-argument errors number arguments from 1
  WHEN an error names a command argument by position (missing, unresolved link or placeholder, a `multiple` argument not resolved as a list)
  THEN it reads `argument #<n> '<name>'` with `n` counted from 1 in the command's declared argument list

**Position rules (decided 2026-10-10).** The state keyword, when present, is the first parameter
other than `context`. `context` may be anywhere, before the state included; recommended last or
just before a `multiple` argument (AC-9). The recommendation is documentation only.

**Non-goals.** `&Context<E>`; renaming the parameter; other state positions; hand-built
`CommandMetadata`; the JavaScript calling convention; *implementing* argument hints (owned by
`COMMAND-METADATA-DESCRIPTIONS-AND-HINTS`); positions outside command arguments
(`ERROR-MESSAGE-POSITIONS-MIXED-BASE`).

## Core Interactions

- **Commands (macro):** the signature parser, `wrapper_arguments`, the argument-option parser.
- **Commands (runtime):** `CommandArguments` error messages and `Error::missing_argument`. No change
  to planning, metadata or the registry.

## Crate Placement

`liquers-macro` (parser, tests); `liquers-core` (`commands.rs`, `error.rs` messages, and end-to-end
tests in `liquers-core/tests/`, where the macro expands against `liquers_core` paths). No crate-flow change.

## Documentation Intent

- **Reference:** `REGISTER_COMMAND_FSD.md` §Context Parameter: position rule, new errors,
  keyword-versus-argument, argument hints unsupported.
- **Guide:** `COMMAND_REGISTRATION_GUIDE.md`: the recommendation, an example before `multiple`, one
  `context`-first example marked allowed but not recommended.
- **Updated:** `RECORD_STREAM_GUIDE.md`; `CLAUDE.md` DSL reference; `rust-best-practices`
  `SKILL.md` and `references/anti-patterns.md`; the note in `JS-COMMAND-CANNOT-ACCESS-CONTEXT`.
- **Not edited:** completed designs that recorded "context last" as honoured; they are history.

## Open Questions

Resolved 2026-10-10: state first among non-`context` parameters, `context` may precede it; `Context`
stays an alias; the issue keeps its title; recommended position per AC-9.

1. **Proposed — reject argument hints (AC-11)** rather than implement them: nothing uses the syntax,
   and implementing needs a `liquers-core` builder that `COMMAND-METADATA-DESCRIPTIONS-AND-HINTS`
   is designing; `MACRO-QUERY-VALIDATION-AND-HINTS` allows either.
2. **Proposed — `#<n>` counts the declared argument list** (as in `command_registry.yaml`, injected
   arguments included), always with the name; it can differ from the n-th query value when an
   injected argument comes first.

## Design Dependencies

Overlaps, none blocking (detail in Phase 2's preflight): `COMMAND-DECLARATION`,
`COMMAND-METADATA-DESCRIPTIONS-AND-HINTS`, `MACRO-QUERY-VALIDATION-AND-HINTS`,
`ERROR-MESSAGE-POSITIONS-MIXED-BASE`, `JS-COMMAND-CANNOT-ACCESS-CONTEXT`,
`VARIADIC-ARGUMENTS-DECLARATION`.

## Scope Changes

All 2026-10-10: rewritten from scratch, old findings archived · AC-5 (same cause as AC-1) · AC-9
after maintainer feedback · AC-10 (`MACRO-TESTS-PRINT-TO-STDOUT` merged, fixed in `a849a47`) ·
AC-11 and AC-12 at the maintainer's request, other positions filed as
`ERROR-MESSAGE-POSITIONS-MIXED-BASE` · converted to the full form as the scope became cross-crate.
