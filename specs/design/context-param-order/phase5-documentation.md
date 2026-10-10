# Phase 5: Documentation - Context parameter at any position

## Completion Preconditions

- [x] Implementation is finished and validated
- [x] All user comments are answered or incorporated
- [x] All review comments are answered or incorporated (no review yet; this is the PR branch)
- [x] Documentation is consistent with the implemented and tested behavior
- [x] Documentation is included in the implementation PR when practical

## Implementation Summary

Implemented as approved, on branch `claude/gracious-cori-wihde7`:

- **`context` at any position (AC-1…AC-7).** `liquers-macro/src/registration.rs` parses a
  signature's parameters as `SignatureItem`s recognised by form, checks their order, and records
  `CommandSignature::state_position`, which `wrapper_arguments` uses to pass the state where it was
  declared (`16055dc`). Valid signatures expand to the same tokens as before; the whole workspace's
  macro uses compile unchanged and the registry export is unchanged.
- **Compile-time diagnostics (AC-4, AC-6)** at the offending token: `context` declared twice, a
  typed `context`, the state keyword twice or after an argument.
- **Keyword by form (AC-5):** `fn f(value: String)` is now a stateless command with an argument
  `value`; it used to fail to parse.
- **Macro tests assert, never print (AC-10, `a849a47`).**
- **Argument `hint` rejected (AC-11):** the option was parsed and discarded — the cause of rustc's
  ``fields `0` and `1` are never read`` warning. It is now an error, and the warning is gone.
- **1-based argument numbers (AC-12, `f72d129`):** `Error::missing_argument` and the
  `CommandArguments` messages read `argument #<n> '<name>'`, `n` from 1; slot indices stay 0-based.
- **Tests:** nine macro unit tests, four `argument_number_tests`, and six end-to-end tests in
  `liquers-core/tests/context_parameter_position.rs` (`82faf8a`).

**Deviations from Phase 2**, both small: `SignatureItem::State` carries the keyword's `syn::Ident`
rather than a bare `Span`, so the AC-6 message can name the keyword; and the typed-`context` check
lives in `CommandParameter::parse`, which `SignatureItem::parse` delegates to, so both parse paths
reject it.

## Documentation Delivered

### New Reference Documents

None. The behaviour belongs in the existing `register_command!` specification.

### New Guide Documents

None. One new section in the existing registration guide.

### Existing Documents Reviewed or Updated

`affects_docs`, each reviewed against the implementation with a History row and `reviewed:` bump:

- `specs/reference/REGISTER_COMMAND_FSD.md` — §Context Parameter rewritten (position rule,
  recommendation and reason, diagnostics); §State Parameter (keywords by form, state first);
  §Parameter Metadata (argument `hint` rejected); §Error Handling (diagnostics, 1-based argument
  numbers); §References (implementation file was `lib.rs`, now `registration.rs`).
- `specs/guides/COMMAND_REGISTRATION_GUIDE.md` — new §1 *Where to put `context`*; the variadic
  section points at it.
- `specs/guides/RECORD_STREAM_GUIDE.md` — "context as its last parameter" restated as the recommendation.

Also updated: `CLAUDE.md` (DSL reference); `.claude/skills/rust-best-practices/SKILL.md` (the hard
rule "context must be last" became an advisory recommendation) and `references/anti-patterns.md`
(the smell is now about state ownership only, with the reason); notes in
`specs/issues/JS-COMMAND-CANNOT-ACCESS-CONTEXT.md`, `MACRO-QUERY-VALIDATION-AND-HINTS.md` and
`PY-MODULES-NOT-DECLARED-IN-LIB.md`. `specs/command_registry.yaml` is unchanged: no signature moved.

### Links and Capability Map

`specs/README.md`: *Context parameter position* moves from `designing` (this folder) to
`documented`, pointing at `reference/REGISTER_COMMAND_FSD.md`.

## Issues Filed

| Issue | Outcome |
|---|---|
| `MACRO-TESTS-PRINT-TO-STDOUT` | Found in Phase 2; merged at the maintainer's request (AC-10); closed |
| `ERROR-MESSAGE-POSITIONS-MIXED-BASE` | Found while scoping AC-12: 0-based positions outside command arguments, e.g. NDJSON `record 0` while CSV says `line 1`. Not eligible for automatic fixing (an open policy question); filed with options and a recommendation |

Seen and already filed, so only annotated: `WORKSPACE-NOT-RUSTFMT-CLEAN` (`cargo fmt -p
liquers-core` rewrites 43 untouched files, so only changed hunks were formatted);
`PY-MODULES-NOT-DECLARED-IN-LIB` (its uncompiled `commands.rs` uses an obsolete
`register_command!` syntax — recorded as further evidence).

## Important Learning

- **The recorded workaround outlived its bug.** The index fix landed months before this design, yet
  three guides, `CLAUDE.md` and a skill's *hard rule* kept teaching "context last". A fix is not
  finished until the guidance that compensated for the bug is retired with it.
- **Recognise keywords by form, not by position.** One cause produced three symptoms (`context`
  before `state`, `value: String` misparsed, a misleading ``expected `:` ``); positional parsing of
  a DSL invites this whenever an element becomes movable.
- **Silent acceptance is worse than rejection.** The `hint` option did nothing for its whole life;
  the only signal was a compiler warning whose wording (fields numbered from 0) obscured it.
- **The recommended position is about portability, not correctness:** last, or before a variadic
  argument, maps directly onto a Python signature.

## Conformance and Remaining Work

Requested: `context` at any position, existing designs reviewed and rewritten as needed — done, with
the old findings archived. Approved scope AC-1…AC-12 — all implemented and proved by the tests in
Phase 3. Nothing remains in this design. Related work stays where it is owned: implementing argument
hints (`COMMAND-METADATA-DESCRIPTIONS-AND-HINTS`), query-literal validation in the macro
(`MACRO-QUERY-VALIDATION-AND-HINTS`), and non-command positions (`ERROR-MESSAGE-POSITIONS-MIXED-BASE`).

## Validation

Pending: the `liquers-lib` suite (AC-7) is still running; results are added here when it finishes.
