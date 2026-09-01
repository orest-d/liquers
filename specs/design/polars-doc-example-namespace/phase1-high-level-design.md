# Phase 1: High-Level Design - Polars Reference Examples Select the pl Namespace

## Design Readiness

- **Readiness:** ready
- **Leading issue:** None
- **Explanation:** The registered namespace and validator path are explicit; no runtime contract or
  user choice is needed to correct the examples.
- **Open questions:** None

## Problem and Evidence

`specs/reference/POLARS_COMMAND_LIBRARY.md` states the `pl` namespace but still shows example
queries such as `from_csv/head-10` without the `ns-pl` instruction, so validation reports
`ActionNotRegistered` for copied examples.

## Expected Behaviour and Acceptance Criteria

Every runnable query example in the Polars reference resolves against the committed command
registry. The reference states once that Polars commands require `ns-pl` unless the namespace is
already selected.

## Affected Systems

Documentation and command-library examples are affected. Runtime Polars command registration,
query parsing and value behaviour are unchanged.

## Scope and Non-Goals

Scope is correcting examples and adding an offline validation recipe. Do not rename commands,
change namespaces, or implement new Polars commands.

## Compatibility, Assumptions and Questions

The reference must remain internally consistent with variadic Polars examples from
`variadic-arguments-declaration`. Assumption: examples are intended to be executable unless clearly
marked as snippets.

## Documentation Assessment

Update `specs/reference/POLARS_COMMAND_LIBRARY.md` and its History row. No new guide is needed, but
the validation command may be mentioned if concise.

## Design Dependencies

- `overlaps` `variadic-arguments-declaration`: completed variadic Polars examples must retain their
  parameter spelling while gaining namespace context.

## Consolidated Findings

Qualify complete runnable queries with `ns-pl` before the first Polars action, but do not alter
isolated command-name tables or fragments that intentionally assume an established namespace.
Replace or repair the malformed resource-transform example. Validation should extract an explicit
curated set of runnable examples and use the committed registry; no Rust source change is planned.

## Review

This is docs-only, directly tied to an existing reference, and acceptance can be checked with
`liquers-validate`.
