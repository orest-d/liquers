# Phase 2: Solution & Architecture — checking a value at a realm boundary at plan time

## Overview

Re-checked at HEAD (2026-10-26):

- `make_plan_with_cwd` (`liquers-core/src/interpreter.rs` ≈177) runs the synchronous
  `PlanBuilder::build` and then async passes with the `EnvRef` (`analyze_plan_dependencies`,
  `plan.rs` ≈3505). A boundary pass fits there, after dispatch has marked remote segments.
- `Step::Action { realm, .. }` carries the realm; nothing binds a realm to another host
  (`REALMS.md` "How a realm is executed").
- `TypeRegistry` (`type_system.rs`) is keyed by `TypeKey { realm, type_identifier }` and has
  `get_in_realm`; no `TransferPolicy` yet (`realm-type-registry` step 1).
- `CommandMetadata` has no output type at HEAD; `command-io-type-declarations` proposes
  `produces: TypeSet` (empty = undeclared). `Plan` steps record none.
- No conversion registry exists (`value-conversion-registry`, `in_review`).

**Sketch under the recommended answers** (prefix delegation; `produces` checked member by member). Not committed: it changes if either blocking question is answered otherwise.

```rust
// liquers-core/src/plan.rs — sketch
pub(crate) async fn check_realm_boundaries<E: Environment>(envref: EnvRef<E>, plan: &mut Plan)
    -> Result<(), Error>;
```

For each `Step::Delegate { realm, query }` (`multi-realm-dispatch` seam B):

1. `outs = produces` of the last action of `query`, from the imported metadata of `realm`
   (`command-io-type-declarations`); `is_any()` → `plan.init_warning` and continue (AC-4).
   Steps 2-3 run for each member `out`; the plan is refused only when no member can cross.
2. `info = types.get_in_realm(realm, out)`; `match info.transfer_mode()` (exhaustive):
   - `Untransferable` → `Err`, naming `out`, `realm` and the local realm (AC-1);
   - `ConvertTo { type_identifier, .. }` → if the remote has the edge, append
     `convert-<type_identifier>` to `query` (sender side); else keep `query` and insert a local
     `convert` action after the step; then continue with the converted type (AC-2);
   - `Serialize` → intersect `supported_data_formats` of `out` in `realm` and in `""`; empty →
     `Err`; otherwise fix the format on the delegation (the remote answers that format) (AC-3).

Rejected: checking at execution (late, the failure this issue exists to prevent); inferring output
types from a sample run (evaluates during planning); a type field on `Step` written by every
planner branch (that is `command-io-type-declarations`' decision, not this one's).

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `CORE-MULTI-REALM-INTERPRETER` (`multi-realm-dispatch`) | accepted | P3 | where crossings are | yes | yes | `requires` |
| `COMMAND-INPUT-OUTPUT-TYPES-ARE-UNDECLARED` (`command-io-type-declarations`) | draft | P2 | crossing type | yes | no | `requires` |
| `TYPE-REGISTRY-NOT-REALM-AWARE` (`realm-type-registry`) | draft | P2 | transfer policy | yes | no | `requires` |
| `REGISTRIES-NOT-PUBLISHED-ACROSS-REALMS` (`registry-publication`) | draft | P3 | remote metadata and types | yes | no | `requires` |
| `VALUE-CONVERSION-CAPABILITY` (`value-conversion-registry`) | draft | P2 | the conversion step | yes, for AC-2 | no | `requires` |
| `VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE` | draft | P2 | large crossings | no | no | note |

## Interfaces

None committed. The sketch above is the only new function; how the chosen format reaches the
transport (a `Filename` step on the delegated query, or a field of the delegation step) is
`multi-realm-dispatch`'s plan format.

## Integration Points

- `interpreter.rs` `make_plan_with_cwd`: call the pass after dispatch has emitted delegation steps.
- `store/realm.rs` `compile_plan`: unaffected (a `store_config` plan never delegates).
- `value-conversion-registry`: `can_convert(from, to)` on each side; the `convert-TARGET`
  command. A remote side's edges must be known locally — published with the registry, or assumed
  from `ConvertTo` (the declaring side promises the edge). Recommended: the latter.

## Error Handling

Planning errors are `Error::general_error` today; a dedicated type is not proposed until the
plan-error vocabulary of `multi-realm-dispatch` exists. Messages name the type, the source realm,
the target realm and the step position.

## Relevant Commands

`convert-TARGET` from `value-conversion-registry` is placed by the planner; no new command.

## Documentation Architecture

`REALMS.md` gains "Values at a realm boundary"; `VALUE_TYPE_SYSTEM.md` links transfer policy to
planning. Written with Phases 3-4.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `plan.rs`, `interpreter.rs`; nothing else in this design |
| Crates and workflows affected | `liquers-core` planning; every query with a remote segment |
| Existing tests likely to change | none (AC-5); single-realm plans skip the pass |
| New validation | in-process two-environment tests once dispatch has an in-process binding |
| Compatibility / data | an inserted `convert` changes the remote query and so its cache key; the same query always plans the same way, so caching stays coherent |
| Concurrency / performance | one registry lookup per crossing |
| Security | none beyond dispatch's |
| Recovery | the pass can be disabled to fall back to run-time decoding |
| Certainty | low until the dispatch unit is decided; the sketch is sound for prefix delegation with `TypeSet` outputs |
