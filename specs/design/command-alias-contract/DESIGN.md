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

An alias `lui/add_child` for `lui/add` (`add(state, position_word, reference_word = "current",
context)`), with head parameter `"child"`, declared the way
`liquers-py/src/command_metadata.rs` `CommandMetadataRegistry::add_python_command` declares aliases:
its `arguments` list only what the *user* supplies (`reference_word`). Validated with a registry
overlay (`liquers-validate --registry-file specs/command_registry.yaml --registry-file alias.yaml`):

| Query | Today | Expected |
|---|---|---|
| `ns-lui/add_child` | Ok — `add [reference_word = "child"]`: the head lands on the wrong argument, `position_word` is never supplied | `add [position_word = "child", reference_word = "current"]` |
| `ns-lui/add_child-parent` | Error — *"Too many parameters for command 'add_child': accepts 0"* | `add [position_word = "child", reference_word = "parent"]` |
| `ns-lui/add-child` (reference) | Ok — `add [position_word = "child", reference_word = "current"]` | unchanged |

The cause is `liquers-core/src/plan.rs` `ResolvedParameterValues::from_action_extended`: it zips
`head_parameters` against the **alias's own** `arguments`, so an alias must re-declare the target's
leading arguments for the heads to occupy, and a head list longer than that is silently truncated by
`zip`. Nothing checks that the alias's arguments match the target, that the target exists, or that
it is not itself an alias; and volatility, payload and expiration are read from the alias's metadata
only, so an alias can hide a volatile target.

### Scope and Acceptance Criteria

The contract, recommended in Open Question 1: **an alias's `arguments` are the arguments its user
supplies; the parameters passed to the target are `head_parameters` followed by those.** Heads are
named and typed by the target's leading arguments.

- **AC-1** Head fills the target's leading argument
  WHEN `ns-lui/add_child` is planned with the alias above
  THEN the plan holds one `Action` for `lui/add` with `position_word = "child"` and `reference_word = "current"`
- **AC-2** Alias parameters follow the head
  WHEN `ns-lui/add_child-parent` is planned
  THEN `reference_word = "parent"`; and `ns-lui/add_child-a-b` fails with *too many parameters … accepts 1*
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
  WHEN a query using an alias is evaluated in an environment with the target registered
  THEN the result equals evaluating the target with the head parameters written out
- **AC-10** Registration builds a consistent alias
  WHEN `CommandRegistry::register_alias(alias, target, head)` is called
  THEN the stored metadata has `definition: Alias`, the target's state argument, the target's
  arguments after the heads, and the target's volatility, payload and expiration; label and doc are editable afterwards
- **AC-11** A production alias is exported and round-trips
  WHEN `specs/command_registry.yaml` is regenerated with default features
  THEN it contains `lui/add_child` with `definition: !Alias`, and `registry_export` passes
- **AC-12** Changing an alias invalidates results that used it
  WHEN a plan uses an alias
  THEN its dependencies include the alias's command-metadata key as well as the target's keys

Non-goals: a `register_command!` statement for aliases (OQ 3); compiling liquers-py's `pycall`
(`PY-MODULES-NOT-DECLARED-IN-LIB`); serializable bindings for host-declared commands
(`POST-INIT-COMMAND-REGISTRATION`).

**Systems touched and crate placement.** `liquers-core`: `plan.rs` (alias planning, parameter
resolution, dependency scan), `command_metadata.rs` (alias validation), `commands.rs`
(`CommandRegistry::register_alias`). `liquers-lib`: `ui/commands.rs` (`lui/add_child`),
`specs/command_registry.yaml`. Both respect the dependency flow; liquers-py needs no change because
it already follows the recommended contract.

**Documentation intent.** Reference: a new `specs/reference/COMMAND_ALIASES.md` owning the contract
(argument layout, validation rules, planning and dependency behaviour). Guide: an *Aliases* section in
`specs/guides/COMMAND_REGISTRATION_GUIDE.md`. Update: `specs/reference/COMMAND_DECLARATION.md` §4.1
(`definition` row links the reference). Other: none.

### Open Questions

1. **Open design — argument layout (contract).** *Exclusive* (alias `arguments` = what the user
   supplies; recommended) or *inclusive* (alias `arguments` mirror the target, heads fill the first
   slots; what `from_action_extended` does today). Exclusive matches liquers-py's producer,
   `pycall`'s positional reading (`skip(3)`), `design/python-wrapper` §10, and `commands_doc`, which
   lists an alias's arguments as user-facing. Inclusive needs no planner change but makes every alias
   restate arguments it cannot receive and breaks the existing producer. Consequence of exclusive:
   the unit test `plan.rs` `accepted_count_excludes_head_parameters`, which pins the inclusive
   reading, is rewritten.
2. **Proposed resolution — chaining.** Reject (AC-6). Resolving chains is possible but composes head
   lists across levels and needs cycle detection, for no current user.
3. **Proposed resolution — registration surface.** A `CommandRegistry::register_alias` method only;
   no `register_command!` DSL statement until a second production alias exists.
4. **Open design — dependency recording (AC-12).** Today a plan records only the target's keys, so
   editing an alias's head leaves cached results stale. Recommended: `Step::Action` gains an
   optional, serde-defaulted `alias: Option<CommandKey>` that the dependency scan reads. Alternative:
   drop AC-12 and file it separately — smaller, but ships a known staleness.
5. **Proposed resolution — production alias.** `lui/add_child` → `lui/add` with head `"child"`. It
   exercises an injected trailing argument, a volatile payload-requiring async target, and the
   exported registry. Alternative: a `pl` alias (polars-gated, no volatility to inherit).

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
