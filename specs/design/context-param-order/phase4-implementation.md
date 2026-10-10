# Phase 4: Implementation Plan — Context parameter at any position

## Overview

Two independent tracks, both on `claude/gracious-cori-wihde7`: the macro parser (Steps 2–4) and
the runtime messages (Step 5); end-to-end tests (Step 6) need the parser; documents (Step 8) come
last so they describe tested behaviour. No prerequisite issue. Step 1 (AC-10) is done.

## Progress

- [x] Step 1: Macro tests assert instead of printing (AC-10) — a849a47
- [x] Step 2: Signature items, order checks and `state_position` (AC-1…AC-6) — 16055dc
- [x] Step 3: Reject the argument `hint` option (AC-11) — 16055dc
- [x] Step 4: Macro unit tests for Steps 2–3 — 16055dc
- [x] Step 5: 1-based argument numbers in runtime errors, with unit tests (AC-12) — f72d129
- [x] Step 6: End-to-end tests `context_parameter_position.rs` — 82faf8a
- [x] Step 7: Workspace validation (AC-7, AC-10, AC-11 checks) — Phase 5 §Validation
- [x] Step 8: Documents (AC-8, AC-9) — 18ba63f

## Implementation Steps

### Step 1: Macro tests assert instead of printing (AC-10)
Done in `a849a47`; see Phase 2 and `MACRO-TESTS-PRINT-TO-STDOUT`.

### Step 2: Signature items, order checks and `state_position`
- Files / symbols: `liquers-macro/src/registration.rs` — new `enum SignatureItem` + `impl Parse`,
  `StateParameter::from_keyword`, `is_context_keyword`; `impl Parse for CommandSignature`;
  `CommandSignature::state_position`; `CommandSignature::wrapper_arguments`; remove
  `impl Parse for StateParameter`.
- Change: as Phase 2 *Integration Points*. Keyword-by-form test: `input.fork()`, parse the ident,
  then `fork.peek(syn::Token![:])` (a `::` path never appears here). The order checks are one
  `match` over `SignatureItem` with no `_` arm.
- Depends on: none.
- Proof: `cargo test -p liquers-macro` (existing tests unchanged and passing).
- Rollback: revert the file.

### Step 3: Reject the argument `hint` option
- Files / symbols: `registration.rs` — `CommandParameterStatement` (drop `Hint`), its `Parse` impl
  (`"hint"` arm returns the AC-11 error at the `hint` ident), the option loop in
  `impl Parse for CommandParameter` (drop the `Hint(_, _)` arm).
- Depends on: none (parallel with Step 2).
- Proof: `cargo build -p liquers-macro` shows no `warning:`.
- Rollback: revert the file.

### Step 4: Macro unit tests
- Files / symbols: `registration.rs` `mod tests` — the nine `signature_*` / `argument_hint_*` tests
  in Phase 3. Error assertions compare `err.to_string()`.
- Depends on: Steps 2, 3.
- Proof: `cargo test -p liquers-macro signature_ argument_hint_` — all pass.
- Rollback: drop the tests with their step.

### Step 5: 1-based argument numbers in runtime errors
- Files / symbols: `liquers-core/src/error.rs` `Error::missing_argument` (message
  `Missing argument #{i + 1} '{name}'`; doc comment says `i` is the 0-based slot);
  `liquers-core/src/commands.rs` `CommandArguments::{get_value, get, get_multiple}`,
  `convert_multiple_element` (`argument #{i + 1} '{name}'` wording, Phase 2 *Error Handling*); new
  `#[cfg(test)] mod argument_number_tests` at the end of `commands.rs` with the four Phase 3 tests.
- Depends on: none (parallel with Steps 2–4).
- Proof: `cargo test -p liquers-core --lib argument_number_tests`, then `cargo test -p liquers-core --lib`.
- Rollback: revert both files.

### Step 6: End-to-end tests
- Files / symbols: new `liquers-core/tests/context_parameter_position.rs` with the six Phase 3
  tests (`liquers-macro` is already a `liquers-core` dev-dependency).
- Depends on: Step 2.
- Proof: `cargo test -p liquers-core --test context_parameter_position`.
- Rollback: delete the file.

### Step 7: Workspace validation
- Proof: `cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-lib --lib --tests`
  (every macro use compiles, AC-7); `cargo test -p liquers-lib --test registry_export`;
  `grep -c 'println!' liquers-macro/src/registration.rs` → 0 (AC-10); `cargo build -p liquers-macro`
  without `warning:` (AC-11); `cargo fmt --check -p liquers-macro`, and `rustfmt --check` on the new test file (`liquers-core`
  as a whole is not rustfmt-clean, `WORKSPACE-NOT-RUSTFMT-CLEAN`).
- Rollback: n/a (checks only).

### Step 8: Documents
- Files: as *Documentation Updates* below.
- Proof: the AC-8 grep in Phase 3 returns nothing; `python3 scripts/docs_index.py --check` has 0 errors.
- Rollback: revert the documents.

## Testing Plan

Steps 2–5 each run their crate's tests on completion; Step 6 its file; Step 7 the full proportionate
run. `scripts/check-build-matrix.sh` is not needed: no `cfg(feature)`, optional dependency or
`ExtValue` match changes. Run with `CARGO_INCREMENTAL=0` (CLAUDE.md, disk budget).

## Rollback Plan

Each step is its own commit, so any step reverts alone. Steps 2–4 and Step 5 are independent; if
only one track lands, the other's ACs stay open and Phase 5 records them as an issue (§5.6).

## Documentation Updates

| Document | Change | History / `reviewed:` |
|---|---|---|
| `specs/reference/REGISTER_COMMAND_FSD.md` | §Context Parameter: any position, recommendation and reason, diagnostics, keyword-versus-argument, argument hints rejected | row + bump |
| `specs/guides/COMMAND_REGISTRATION_GUIDE.md` | Recommendation with an example before `multiple`; `context`-first marked allowed | row + bump |
| `specs/guides/RECORD_STREAM_GUIDE.md` | "context as its last parameter" → recommendation | row + bump |
| `CLAUDE.md` | DSL reference: `context` anywhere, recommended last or before `multiple` | — |
| `.claude/skills/rust-best-practices/SKILL.md`, `references/anti-patterns.md` | hard rule → recommendation | — |
| `specs/issues/JS-COMMAND-CANNOT-ACCESS-CONTEXT.md` | note updated | — |
| `specs/index.csv`, `specs/index.md` | regenerate with `python3 scripts/docs_index.py` | — |

`specs/command_registry.yaml` is not regenerated: no command signature or `version: auto` body changes.

## Phase 5 Entry Criteria

- [x] Implementation finished and validated (all Progress items ticked)
- [x] User and review comments answered
- [x] Documentation checkable against implemented and tested behaviour
