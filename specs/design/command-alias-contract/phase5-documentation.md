# Phase 5: Documentation - Command alias contract

## Completion Preconditions

- [x] Implementation is finished and validated (Phase 4 §Progress, all ten steps ticked)
- [x] All user comments are answered or incorporated (decisions 1-5, both guides — Phase 1)
- [x] All review comments are answered or incorporated (author review passes; no PR review yet)
- [x] Documentation is consistent with the implemented and tested behavior
- [x] Documentation is included in the implementation branch

## Implementation Summary

`CommandDefinition::Alias` now has a specified, validated and exercised contract.

- **Contract.** The target receives the alias's head parameters followed by the alias's own
  arguments. An alias declares only what its user supplies; heads are named by the target's
  leading arguments (`ResolvedParameterValues::from_alias_action`, replacing
  `from_action_extended`).
- **Validation.** `CommandMetadataRegistry::alias_target` (target registered, not an alias, head
  fits, no head on an injected argument) runs at registration and at planning, so hand-built and
  deserialized aliases obey the same rules. `CommandRegistry::register_alias` also checks the
  positional shape and copies the target's state argument and flags.
- **Planning.** The planner applies the target's volatility, payload requirement and expiration as
  well as the alias's, and emits one target `Step::Action` with `origin: ActionOrigin::Alias`.
- **Dependencies and execution.** The dependency scan adds the alias's metadata key. Errors gain
  *"(via alias '…')"*. `IsVolatile` / `RequiresPayload` consult the alias.
- **Production user.** `pl/head` is an alias of `pl/slice` (head `0`, own `n = 5`). Its
  implementation is deleted, and `specs/command_registry.yaml` has its first `!Alias`.

**Conformance.** All twelve acceptance criteria are met and tested. Deviations from the approved
design, each found while implementing:

1. **The alias diagnostic moved from `steps` to `init_steps`** (Phase 2 said the executed
   `Step::Info` stays). An executed step before the action replaced the input state's metadata, so
   `ns-pl/head` over a supplied CSV state reached `slice` as `bin` data. `origin` already records
   the alias in the plan, so nothing is lost. The underlying behaviour is filed (below).
2. **Added rule:** a head may not fill a `multiple` target argument either, since the executor
   expects a list there (`alias_target`).
3. **Test 11 is a unit test in `interpreter.rs`, not in `tests/command_alias.rs`**, because
   `IsVolatile` is crate-private.
4. **`pl/head`'s `n` shows `gui_info: IntegerField`**, the default of
   `ArgumentInfo::integer_argument`, instead of the macro's `TextField 40`.

## Documentation Delivered

### New Reference Documents

- `specs/reference/COMMAND_ALIASES.md` — representation, argument contract, validation table,
  planning with `ActionOrigin`, dependencies and execution.

### New Guide Documents

None. Both guides were extended instead (below), as Phase 2 planned.

### Existing Documents Reviewed or Updated

`affects_docs` (authoritative):

- `specs/guides/COMMAND_REGISTRATION_GUIDE.md` — Quick Reference row and §2 *Registering an alias*.
- `specs/guides/COMMAND_DESIGN_GUIDE.md` — new §Aliases: convenience wrapper (`pl/head`), bridge
  (`pycall`), when not to, consequences; introduction widened.
- `specs/reference/COMMAND_DECLARATION.md` — §4.1 `definition` row.
- `specs/reference/POLARS_COMMAND_LIBRARY.md` — `head` row; the pattern example uses `tail`.
- `specs/reference/api/DOC_08_RECIPES_PLANS.md` — planning contract, `dependencies`, `origin`
  serialization.

Each has a 2026-10-10 History row and `reviewed:` bump. Discarded area candidates:
`REGISTER_COMMAND_FSD.md` (the macro gained no alias statement) and `PROJECT_OVERVIEW.md` (query
and key encoding untouched).

### Links and Capability Map

`specs/README.md` *Command aliases* now reads "documented" and links `reference/COMMAND_ALIASES.md`,
with the design linked after it. The new reference links both guides, and both guides link it.

## Issues Filed

- `CONTEXT-STEP-REPLACES-INPUT-STATE-METADATA` (P2, M) — a pass-through step replaces a supplied
  input state's metadata with the context's. Filed without a design: the expected behaviour (keep
  the input's metadata, or merge the context's log into it) is a decision for its owner, no
  user-visible failure is known after this design's workaround, and it is not eligible for an
  automatic fix.

Closed: `COMMAND-ALIAS-DEFINITION-UNTESTED`.

## Important Learning

- `zip` silently truncated over-long heads; any length rule must come before a zip.
- The no-`_ =>` rule protects exhaustive matches only. `if let` / `matches!` "is this an action"
  tests (`Plan::last_action_index`, which recipe overrides use) are why a new `Step` variant was the
  wrong vehicle for aliased actions, and a field was the right one.
- `CommandMetadataRegistry::add_command` replaces an entry with the same key; it does not fail.
- `apply_plan` rebuilds the state from the context after every step, so executed diagnostics belong
  in `init_steps` unless they must run at a position.

## Conformance and Remaining Work

Requested (the issue's fix direction): a decided contract, tests for each case, and a production
alias in the registry. Approved: the same, plus action provenance and both guides. Implemented: all
of it, with the four deviations above. Nothing remains in this design. Argument *mapping* is a
stated non-goal and future design; `pycall` compiling is `PY-MODULES-NOT-DECLARED-IN-LIB`.

## Validation

- `cargo test -p liquers-core --lib --tests` — 39 suites, 0 failures (1043 unit tests).
- `cargo test -p liquers-lib --lib --tests` — 35 suites, 0 failures; again with
  `--no-default-features`, 0 failures.
- `cargo check -p liquers-py` — passes with `PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1`
  (`PY-PYO3-REJECTS-PYTHON-3-13`).
- `liquers-validate` on every query in the new documentation: `ns-pl/head`, `ns-pl/head-10`,
  `ns-pl/head-2` and `ns-pl/slice-0-2` plan; `ns-pl/head-1-2` fails with *accepts 1*.
- `python3 scripts/docs_index.py --check` — 0 errors.
