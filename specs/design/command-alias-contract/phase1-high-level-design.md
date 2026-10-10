# Phase 1: High-Level Design — Command alias contract

## Purpose

`CommandDefinition::Alias { command, head_parameters }` rewrites an action into a call of another
command with leading parameters pre-filled. It has no specified contract, no registration API, no
validation and no test, and its only producer (liquers-py) assumes the opposite contract from the
planner that consumes it. Fix the contract, make misuse an error, and give the variant a production
user so it is exercised end to end.

## Problem Example

`pl/head(n = 5)` is, line for line, `pl/slice(offset = 0, length = n)`
(`liquers-lib/src/polars/selection.rs` `head` and `slice` both clamp at zero and call the polars
method of the same name). Declare it as an alias the way liquers-py's
`CommandMetadataRegistry::add_python_command` declares aliases: target `pl/slice`, head parameter
`0`, and `arguments` listing only what the *user* supplies (`n`, default 5). Validated with a registry
overlay (`liquers-validate --registry-file specs/command_registry.yaml --registry-file head.yaml
--allow-overwrite`):

| Query | Today | Expected |
|---|---|---|
| `ns-pl/head` | Ok — `slice [n = 0]`: the head lands on the alias's own `n`, and `slice` gets one parameter instead of two | `slice [offset = 0, length = 5]` |
| `ns-pl/head-10` | Error — *"Too many parameters for command 'head': accepts 0, but parameter #1 '10' was supplied"* | `slice [offset = 0, length = 10]` |
| `ns-pl/slice-0-10` (reference) | Ok — `slice [offset = 0, length = 10]` | unchanged |

The cause is `liquers-core/src/plan.rs` `ResolvedParameterValues::from_action_extended`: it zips
`head_parameters` against the **alias's own** `arguments`, so an alias must re-declare the target's
leading arguments for the heads to occupy, and a head list longer than that is silently truncated by
`zip`. Nothing checks that the target exists or is not itself an alias; volatility, payload and
expiration are read from the alias's metadata only, so an alias can hide a volatile target; and the
plan records dependencies on the target only, so a changed alias leaves cached results stale.

## Scope and Acceptance Criteria

The contract (decision 1, accepted): **an alias's `arguments` are the arguments its user supplies;
the parameters passed to the target are `head_parameters` followed by those.** Heads are named and
typed by the target's leading arguments. The alias's own arguments may carry their own names,
labels and defaults (`n = 5` where the target has `length` with no default).

- **AC-1** Head fills the target's leading argument
  WHEN `ns-pl/head` is planned with `pl/head` an alias of `pl/slice` with head `0`
  THEN the plan holds one `Action` for `pl/slice` with `offset = 0` and `length = 5`
- **AC-2** Alias parameters follow the head
  WHEN `ns-pl/head-10` is planned
  THEN `offset = 0`, `length = 10`; and `ns-pl/head-1-2` fails with *too many parameters … accepts 1*
- **AC-3** Head longer than the target's argument list is an error
  WHEN an alias carries more head parameters than its target has non-injected leading arguments
  THEN registration and planning fail with an error naming the alias and the target; no head is dropped
- **AC-4** A head cannot fill an injected argument
  WHEN a head position corresponds to an injected target argument (e.g. `context`)
  THEN registration and planning fail with an error naming that argument
- **AC-5** Missing target is an error at plan time
  WHEN an alias's target is not in the command registry
  THEN planning fails naming the alias and the missing target, instead of failing at execution
- **AC-6** Aliases do not chain
  WHEN an alias's target is itself an alias
  THEN registration and planning fail with an error naming both
- **AC-7** Variadic arguments resolve through an alias
  WHEN an alias's own last argument is `multiple` and the action supplies three parameters
  THEN the target receives the head followed by one `MultipleParameters` holding all three
- **AC-8** An alias is at least as volatile as its target
  WHEN the target is volatile, requires a payload or expires, and the alias metadata does not say so
  THEN the plan is volatile, requires the payload and carries the target's expiration
- **AC-9** An alias executes its target
  WHEN `ns-pl/head-2` is evaluated on a three-row data frame
  THEN the result equals `ns-pl/slice-0-2` on the same frame: its first two rows
- **AC-10** Registration builds a consistent alias
  WHEN `CommandRegistry::register_alias(alias, target, head, arguments)` is called
  THEN the stored metadata has `definition: Alias`, the given arguments, and the target's state
  argument, volatility, payload and expiration; label and doc are editable afterwards
- **AC-11** `pl/head` is a production alias and round-trips
  WHEN `specs/command_registry.yaml` is regenerated with default features
  THEN `pl/head` has `definition: !Alias` targeting `pl/slice`, the `head` function is gone, and
  `registry_export` passes
- **AC-12** The plan records the alias
  WHEN a plan uses an alias
  THEN its dependencies include the alias's command-metadata key as well as the target's keys, and
  the plan states which alias produced the target call (mechanism: Open Question 4)

Non-goals: a `register_command!` statement for aliases (decision 3); argument *mapping* (reordering,
dropping or computing target arguments) — planned later, and the contract here must not preclude
it; compiling liquers-py's `pycall` (`PY-MODULES-NOT-DECLARED-IN-LIB`); serializable bindings for
host-declared commands (`POST-INIT-COMMAND-REGISTRATION`).

## Core Interactions

- **Query / plans:** alias resolution moves to a validated, plan-time step; `Step::Action` records
  its origin; the dependency scan reads it.
- **Commands:** alias registration and validation; `pl/head` becomes an alias.
- **Assets:** cached results depend on the alias's metadata version.
- **Bindings:** liquers-py already produces aliases in the chosen layout; no change.

## Crate Placement

`liquers-core` — `plan.rs`, `command_metadata.rs`, `commands.rs`, `error.rs`, `interpreter.rs`.
`liquers-lib` — `polars/selection.rs` and the generated `specs/command_registry.yaml`. Both respect
the dependency flow.

## Documentation Intent

- Reference: new `specs/reference/COMMAND_ALIASES.md` — contract, validation, planning and
  dependency behaviour.
- Guides:
  - `specs/guides/COMMAND_REGISTRATION_GUIDE.md` — *how* to register an alias.
  - `specs/guides/COMMAND_DESIGN_GUIDE.md` — *when* to design a command as an alias: a convenience
    wrapper over a general command (`pl/head` over `pl/slice`), or a bridge that routes many
    declared commands through one executor (liquers-py's `pycall`); and when not to.
- Other documents: none.
- Documents to update: `specs/reference/COMMAND_DECLARATION.md` §4.1, `specs/reference/POLARS_COMMAND_LIBRARY.md`, `specs/reference/api/DOC_08_RECIPES_PLANS.md`.

## Open Questions

None open. Resolved with the user on 2026-10-10:

1. **Argument layout — exclusive.** An alias declares only what its user supplies; the test
   `plan.rs` `accepted_count_excludes_head_parameters`, which pins the old reading, is rewritten.
2. **Chaining — rejected** (AC-6).
3. **Registration — `CommandRegistry::register_alias` only**, taking the alias's own arguments.
4. **Recording the alias — `ActionOrigin` on `Step::Action`** (AC-12): the plan shows which alias
   produced each target call, and the dependency scan derives the alias dependency from it. A future
   argument-mapping alias stays `Alias` (its rules are in the alias's versioned metadata). Rejected:
   `Step::RegisterDependency` and builder-populated `plan.dependencies` (no link to the action); a
   `Step::AliasedAction` variant (non-exhaustive "is this an action" tests such as
   `Plan::last_action_index`, which recipe overrides use, would skip it silently); deferring.
5. **Production alias — `pl/head` → `pl/slice` with head `0`**; cached `pl/head` results are
   invalidated once.

## Design Dependencies

- `overlaps` `PY-MODULES-NOT-DECLARED-IN-LIB` — `pycall` is not compiled, so liquers-py's aliases
  cannot run today; this design does not change that, and its contract is the one `pycall` assumes.
- `overlaps` `design/python-wrapper` (complete) — chose alias dispatch; this design specifies it.
- Overlap triage: no open design edits `from_action_extended` or `CommandDefinition`
  (`design/excess-action-parameters-error` is complete and owns the too-many-parameters error this
  design reuses). `COMMAND-DECLARATION-FORMAT` and `POST-INIT-COMMAND-REGISTRATION` overlap weakly.

## Scope Changes

- **2026-10-10 — compact form converted to full form.** Started as a compact design from the issue's
  `M`. Decisions 4 (action provenance with `ActionOrigin`) and 5 (`pl/head`) added a serialized
  plan-format change, an interpreter change and a production command migration; Phase 2 alone took
  the single file past its size limit. Complexity raised to `L`; acceptance criteria unchanged.
  Phases 1-2 moved verbatim into their phase files.
- **2026-10-10 — both command guides document aliases** (requested at the Phase 3 gate). The
  registration guide gets the how-to; the design guide gets when to use an alias (convenience
  wrapper, bridge) and when not to. No acceptance criterion changes; Phases 1 and 2 updated
  (documentation intent and architecture), and Phase 4 will carry the steps.
