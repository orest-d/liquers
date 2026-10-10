---
id: MACRO-TESTS-PRINT-TO-STDOUT
kind: issue
title: liquers-macro unit tests print generated tokens to stdout
status: in_progress
priority: P3
complexity: S
area: [macro]
design: context-param-order
created: 2026-10-10
github:
---
## Problem

Four unit tests in `liquers-macro/src/registration.rs` (`mod tests`) print generated token streams
with `println!` (lines 2691, 2762, 2869, 2886 at the time of filing: two `"Generated tokens: {}"`
lines, `test_nostate_command_registration2` and `test_config_command_registration`). The
blanket rule in `CLAUDE.md` ("Diagnostic Output") forbids `println!` in library code, `#[cfg(test)]`
modules included. Several of these tests also assert nothing: they only build tokens and print them.

## Impact

Low. Test output on stdout under `--nocapture` only; no binary links this module. The real cost is
that tests which print instead of asserting pass whatever the macro generates.

## Expected behaviour

Replace `println!` with `eprintln!`, or better, give each such test an assertion on the generated
tokens and drop the print. The sibling `STORE-TESTS-PRINT-TO-STDOUT` (closed) did the same for
`liquers-store`.

## Discovery

Found on 2026-10-10 while reading the macro's test module for the `context-param-order` design
(Phase 2). Not part of that design: different cause, no shared contract.

## Resolution

Merged into `design/context-param-order/` (AC-10) on 2026-10-10 at the maintainer's request and
fixed on its branch: the prints are gone and the four tests that only built tokens now assert on
them. Closed by that design's Phase 5.
