---
id: PLAN-EXCESS-ACTION-PARAMETERS-DROPPED
kind: issue
title: Plan builder silently drops excess action parameters
status: closed
priority: P0
complexity: M
area: [core/plan]
design: excess-action-parameters-error
created: 2026-08-08
github:
---
## Problem

An action that supplies more parameters than its command declares builds a plan
successfully, and the surplus parameters are discarded without an error or a warning.

`ResolvedParameterValues::from_action_extended` (`liquers-core/src/plan.rs:759`) drives its
loop from the *command metadata's* argument list:

```rust
for a in command_metadata.arguments.iter().skip(n) {
    let pv = ParameterValue::pop_value(a, &mut parameters, allow_placeholders)?;
    values.push(pv);
}
```

It pops one value per declared argument and returns. Whether the `ActionParameterIterator`
still holds unconsumed parameters is never checked, so anything beyond the declared arity is
dropped silently. Arguments marked `multiple` are unaffected — they consume the remainder by
design.

Observed with `liquers-validate` against the committed 95-command registry:

| Query | Resolved plan | Expected |
|---|---|---|
| `to_text-extra-args` | `action to_text()`, status `Ok` | warning: 2 extra parameters |
| `-R/d/s.csv/-/ns-pl/head-10-99` | `action pl/head(n=10)`, status `Ok` | warning: `99` ignored |
| `-R/d/s.csv/-/ns-pl/select_columns-name-price` | `action pl/select_columns(columns="name")`, status `Ok` | warning: `price` ignored |

The third is the realistic one: `pl/select_columns` is documented as "Select columns by name
(separated by dashes)" but declares a single `string` argument, so the dash-separated form the
doc invites silently keeps only the first column.

This also bounds what query validation can promise. A validator built on `PlanBuilder` reports
`status: Ok` and exit 0 for all three, so "the plan built" does not imply "every parameter you
wrote was used" — the resolved parameters have to be read.

## Expected behavior

Leftover parameters should be reported rather than dropped. The plan builder already
establishes the convention for the resource-header path a few hundred lines below
(`plan.rs:1093`):

```rust
self.plan.init_warning(format!(
    "Resource header has too many parameters: {}, extra parameters are ignored",
    header.parameters.len()
));
```

The action path should warn equivalently, naming the action and the ignored values. A warning
rather than an error keeps it consistent with the header case and with the general rule that a
plan carrying a warning still validates (exit 0, `status: Warning`).

## Fix direction

`from_action_extended` cannot call `init_warning` — it has no handle on the `Plan`. Two options:

1. Have it return the unconsumed parameters (or their count and positions) alongside the
   resolved values, and let `PlanBuilder` raise the warning where it owns the plan; or
2. Move the leftover check into `PlanBuilder` after the call, where the `ActionRequest` and the
   `CommandMetadata` are both in scope — the surplus is `action_request.parameters.len()`
   beyond the number of declared non-`multiple` arguments.

Option 1 keeps the arity knowledge in one place; option 2 avoids changing a public signature.

## Verification

Unit tests in `liquers-core/src/plan.rs`: an action with more parameters than declared
arguments produces exactly one warning naming the ignored values; an action whose last
argument is `multiple` produces none; an exactly-saturated action produces none. Add a
`liquers-core/src/validate` test asserting such a query reports `status: Warning` with exit 0,
which is what pins the validator's contract.

## Resolution

Closed by `specs/design/excess-action-parameters-error/`. Surplus parameters now raise an error
carrying the position of the first surplus parameter, on both the action path
(`ResolvedParameterValues::from_action_extended`) and the resource-header path
(`PlanBuilder::process_resource_query`).

**The resolution is an error, not the warning proposed above, and the reason is structural.** A
planning warning is `Step::Warning(String)` — a bare message with no `Position` field — whereas
`Error` carries `position: Position`. Naming *which* parameter is excess is therefore inexpressible
as a warning without first extending the diagnostic type. The position is what feeds the query
console's existing highlight path (`StyledQuery::from_query` -> `to_highlight_if_matching` -> an
underlined token), so an error is what makes the surplus parameter highlightable while editing.

Consequences for the "Verification" section above: the validator reports `status: Error` and exits
**1** for such a query, not `status: Warning` and exit 0.

The `fix direction` section is superseded: neither option was needed. Because the resolution is an
error, the check returns `Err` from `from_action_extended`, which already returns `Result` — so
there is no signature change and no `Plan` handle required.

Two related defects were filed rather than absorbed:
`COMMAND-VARIADIC-ARGUMENTS-NOT-DECLARABLE` (the `multiple` escape hatch this issue names cannot
actually be declared) and `VARIADIC-ARGUMENT-STARVES-LATER-ARGUMENTS`.

