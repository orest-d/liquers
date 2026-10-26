# Phase 1: High-Level Design — checking a value at a realm boundary at plan time

## Purpose

When one query spans a browser and a server (`VISION.md`, client/server), a value produced in one
realm is consumed in another. Some values cannot cross at all (a JavaScript closure), some need a
conversion first (a Polars frame for a page without Polars), and the rest must cross in a format
both sides read. Planning should know this, so the query is refused or adjusted before anything
runs, rather than failing when the receiver cannot decode the bytes.

## Problem Example

A page has imported the server's realm as `server` (`registry-publication`) and binds it remotely
(`multi-realm-dispatch`). The server's `heavy_stats` returns a `polars.DataFrame`; the page's
registry has no `polars.DataFrame`.

```
-R/data/sales.csv/-server/heavy_stats/-/to_text
```

(Validated with `--command server//heavy_stats`: `GetResource[data, sales.csv]`,
`Action{server/heavy_stats}`, `Action{to_text}`.)

- **Today:** nothing can run it (no dispatch). Even with dispatch, `PlanBuilder`
  (`liquers-core/src/plan.rs`) does not know `heavy_stats` returns a frame (command metadata
  records no output type), nor that `TypeInfo` declares a transfer policy (`realm-type-registry`,
  not yet built). The page would receive bytes it cannot decode, and fail late, or degrade them to
  `Bytes`.
- **Should:** the plan places a conversion on the server side (`polars.DataFrame` declares
  `ConvertTo RecordView`, which the page reads), or, if no route exists, planning fails with
  *"polars.DataFrame produced in realm 'server' cannot cross to realm '': no common format and no
  conversion"*. A `js.Value` sent toward the server fails planning as `Untransferable`.

## Scope and Acceptance Criteria

- **AC-1** An untransferable value is a planning error
  WHEN a value whose type declares `Untransferable` would cross a realm boundary
  THEN planning fails with an error naming the type and both realms
- **AC-2** A conversion is placed where it can run
  WHEN the crossing type declares `ConvertTo { type_identifier, data_format }`
  THEN the plan converts on the side that has the conversion edge (`value-conversion-registry`),
  and the receiving realm gets `type_identifier`
- **AC-3** A serializable value crosses in a common format
  WHEN the type declares `Serialize`
  THEN the plan fixes a format supported for that type in both realms (the sender's default when
  it qualifies), or refuses the plan when there is none
- **AC-4** An unknown type is reported, not guessed
  WHEN the crossing step's output type is unknown at plan time
  THEN the plan carries a warning naming the step, and the crossing is left to run-time decoding
- **AC-5** Single-realm plans are unchanged
  WHEN no step crosses a realm bound to another host
  THEN plans are identical to HEAD

Non-goals: dispatching a realm (`multi-realm-dispatch`); performing conversions
(`value-conversion-registry`); publishing registries (`registry-publication`); choosing a host for
a realm-less step; streaming large values.

## Core Interactions

Planning (a boundary pass over the plan), value types (transfer policy per realm), commands
(declared output types), the conversion registry. Store and assets untouched.

## Crate Placement

`liquers-core`: the pass belongs to planning and reads only core registries.

## Documentation Intent

- Reference: `REALMS.md` (boundaries), `VALUE_TYPE_SYSTEM.md` (transfer policy in planning).
- Guide: none.
- Other documents: none.
- Documents to update: decided once Phases 3-4 can be written.

## Design Readiness

- **Readiness:** phase2-blocked
- **Automatic fixing:** not-eligible — `L`, core planning semantics, and blocked (rules 1, 4, 5).
- **Leading issue:** **Blocking - where a value crosses.**
- **Explanation:** the check is simple once its inputs exist. The crossing *type* now has a
  proposed contract (`command-io-type-declarations`), but *where* a crossing happens depends on
  `multi-realm-dispatch`'s unresolved dispatch unit, which changes the crossing set and the errors.
- **Open questions:**
  1. **Blocking - where a value crosses.** Decided by `multi-realm-dispatch` question 1. With
     *prefix delegation* (its recommendation), a delegated prefix runs wholly on the server and
     only its result crosses back: one remote→local crossing per delegated segment. With
     *per-action RPC*, the input state is uploaded too: in
     `-R/local/notes.csv/-server/heavy_stats`, a page-store value crosses local→remote, and a
     `js.Value` input becomes AC-1's error instead of dispatch's "prefix not resolvable remotely".
     The two answers produce different crossing sets and different errors for the same query.
  2. **Open design - a crossing type that is a set.** `command-io-type-declarations` proposes
     `CommandMetadata::produces: TypeSet` (ordered identifiers, the first principal; empty =
     undeclared). Recommended: check every member; refuse only when no member can cross (all
     `Untransferable`, or no format and no conversion), warn for the members that cannot, and
     choose formats for the principal type. Alternative: check the principal only, which misses a
     command that sometimes returns an untransferable type.
  3. **Open design - unknown type.** Recommended: warn and let run-time decoding decide (AC-4),
     because refusing would refuse nearly every query until output types are widespread.
  4. **Open design - which side converts.** Recommended: the sender, when it has the edge (a
     server converts its frame), else the receiver after decoding a `Serialize`d form.
  5. **Proposed resolution - format choice.** Intersection of `supported_data_formats` of the type
     in both realms; the sender's default if present; otherwise the first in the sender's order.

## Design Dependencies

- `multi-realm-dispatch` (`CORE-MULTI-REALM-INTERPRETER`, `phase2-blocked`) — **requires**: where
  steps run and how a crossing is expressed in the plan (its `Delegate` step sketch). Blocking.
- `command-io-type-declarations` (`COMMAND-INPUT-OUTPUT-TYPES-ARE-UNDECLARED`, `needs-decision`)
  — **requires**: `CommandMetadata::produces`, the output types of a step at plan time. Its
  contract is written but not built, and AC-4 covers commands that leave it undeclared.
- `realm-type-registry` (`TYPE-REGISTRY-NOT-REALM-AWARE`) — **requires**: `TransferPolicy` and
  per-realm `TypeInfo`.
- `registry-publication` (`REGISTRIES-NOT-PUBLISHED-ACROSS-REALMS`) — **requires**: the remote
  realm's commands and types on the planning side.
- `value-conversion-registry` (`VALUE-CONVERSION-CAPABILITY`) — **requires**: `can_convert` and
  the `convert-TARGET` command that AC-2 places.
- `command-specialization-by-input-type` (`COMMAND-SPECIALIZATION-BY-INPUT-TYPE-MISSING`) —
  **overlaps**, weak: both propagate types through a plan; a shared type-flow pass is likely.
- Overlap judgement: no strong overlap. The issue was split from `realm-type-registry` on purpose;
  this design consumes four others' contracts and owns none of them.
