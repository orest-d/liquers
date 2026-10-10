---
id: COMMAND-ALIAS-CONTRACT
kind: design
title: A specified, validated and exercised contract for command aliases
form: compact
workflow: liquers-project
status: in_review
phase: high-level
area: [core/plan, core/commands]
issues: [COMMAND-ALIAS-DEFINITION-UNTESTED]
created: 2026-10-09
---
# A specified, validated and exercised contract for command aliases

## Phase 1: High-Level Design

### Purpose

`CommandDefinition::Alias { command, head_parameters }` rewrites an action into a call of another
command with leading parameters pre-filled. It has no specified contract, no registration API, no
validation and no test, and its only producer (liquers-py) assumes the opposite contract from the
planner that consumes it. Fix the contract, make misuse an error, and give the variant a production
user so it is exercised end to end.

### Problem Example

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

### Scope and Acceptance Criteria

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

**Systems touched and crate placement.** `liquers-core`: `plan.rs` (alias planning, parameter
resolution, dependency scan), `command_metadata.rs` (alias validation), `commands.rs`
(`CommandRegistry::register_alias`), and the step or dependency type chosen in OQ 4. `liquers-lib`:
`polars/selection.rs` (`pl/head` becomes an alias), `specs/command_registry.yaml`. Both respect
the dependency flow; liquers-py needs no change because it already follows the contract.

**Documentation intent.** Reference: a new `specs/reference/COMMAND_ALIASES.md` owning the contract
(argument layout, validation rules, planning and dependency behaviour). Guide: an *Aliases* section in
`specs/guides/COMMAND_REGISTRATION_GUIDE.md`. Update: `specs/reference/COMMAND_DECLARATION.md` §4.1
(`definition` row links the reference); `specs/reference/POLARS_COMMAND_LIBRARY.md` (`head` is an
alias of `slice`). Other: none.

### Open Questions

Resolved with the user on 2026-10-10:

1. **Argument layout — exclusive.** An alias declares only the arguments its user supplies. The
   unit test `plan.rs` `accepted_count_excludes_head_parameters`, which pins the inclusive reading,
   is rewritten.
2. **Chaining — rejected** (AC-6).
3. **Registration surface — `CommandRegistry::register_alias` only**, taking the alias's own
   argument list; no `register_command!` statement.
5. **Production alias — `pl/head` → `pl/slice` with head `0`.** Removes the `head` function.
   Consequence: cached `pl/head` results are invalidated once, because their dependency keys change
   from `pl/head` to `pl/slice` plus the alias.

Open:

4. **Open design — how the plan records the alias (AC-12).** The dependency list is rebuilt from
   the steps by the scan (`plan.rs` `scan_plan` → `plan.dependencies`), so whatever records the
   alias must be in the step list. It should serve two purposes: register the dependency on the
   alias's metadata, and document that the alias caused the mapping. Candidates, under discussion:
   - **(a) Provenance on the action** — `Step::Action` gains an optional `origin` (the alias key);
     the scan derives the dependency from it, and the interpreter names the alias in errors.
   - **(b) `Step::RegisterDependency`** — a general step carrying a `PlanDependency` and a stated
     reason; the alias planner emits one before the action; the scan adds it, the interpreter logs
     the reason. Reusable by any later planner feature that needs a dependency the steps cannot show.
   - **(c) Both** — (b) for the dependency, (a) for provenance.
   - **(d) Defer** — drop AC-12 and file it separately.

### Design Dependencies

- `overlaps` `PY-MODULES-NOT-DECLARED-IN-LIB` — `pycall` is not compiled, so liquers-py's aliases
  cannot run today; this design does not change that, and its contract is the one `pycall` assumes.
- `overlaps` `design/python-wrapper` (complete) — chose alias dispatch; this design specifies it.
- Overlap triage: no open design edits `from_action_extended` or `CommandDefinition`
  (`design/excess-action-parameters-error` is complete and owns the too-many-parameters error this
  design reuses). `COMMAND-DECLARATION-FORMAT` and `POST-INIT-COMMAND-REGISTRATION` overlap weakly.

## Phase 2: Architecture

## Phase 3: Examples and Tests

## Phase 4: Implementation Plan
