# Phase 1: Consistent State Argument Default

## Design Readiness

- **Readiness:** needs-decision
- **Leading issue:** **Proposed resolution - omission semantics:** Omitted `state_argument` should
  mean the conventional transforming command; source commands must write explicit `null`.
- **Explanation:** This preserves both constructors and existing registration defaults, but omission
  changes planning semantics and therefore remains a system-design choice.
- **Open questions:** **Proposed resolution - omission semantics:** Approve conventional-state
  omission rather than changing constructors so omission means a source command.

## Problem and Evidence

`CommandMetadata::new` and `from_key` create a state argument, while serde omission yields `None`.
The same metadata therefore consumes its predecessor or acts as a source solely according to how it
was constructed.

## Expected Behaviour and Acceptance Criteria

Omitted `state_argument` deserializes to `Some(ArgumentInfo::any_argument("state"))`; explicit
`null` remains `None` and serializes back as null. Constructors, JSON, and YAML agree, and planning
tests distinguish the two cases. Existing registry documents with explicit fields round-trip
byte-equivalently.

## Scope, Compatibility, and Non-Goals

Scope is one serde default and tests. Do not change command-declaration convention inference or
remove source-command support. Old documents that relied on omission meaning source change
semantics; explicit `null` is the migration spelling. No security or persistent binary format is
affected, but command metadata is a serialized contract.

## Design Dependencies

- `overlaps` `argument-gui-info-default`: neighbouring omitted defaults share parity validation but
  are independently implementable.
- `overlaps` `command-declaration`: declarations already make state/source intent explicit and
  provide compatibility cases to preserve.

## Documentation Assessment

Update `specs/reference/COMMAND_DECLARATION.md` to state omitted-versus-null behaviour if it exposes
raw `CommandMetadata`; otherwise document it on the serde field and in the issue resolution.

## Consolidated Findings

The proposed default preserves all Rust registration callers and makes absence mean the common
transforming command. Explicit null must be serialized, not skipped, to preserve source commands
across round-trips. The principal risk is changing old hand-authored metadata that omitted the
field; test JSON/YAML omission, null, constructor parity, declaration conventions, registry
round-trip, and planner consumption.
