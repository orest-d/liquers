---
id: LINK-IN-VARIADIC-DOES-NOT-EXPAND
kind: issue
title: A link inside a variadic argument yields one element even when it resolves to an array
status: draft
priority: P3
complexity: M
area: [core/plan, core/commands]
design: variadic-arguments-declaration
created: 2026-08-25
github:
---
## Problem

A variadic argument holds a list. A query link resolving to a JSON **array** is the natural way to
supply that list from elsewhere — a stored `cols.json` containing `["date", "amount"]`, say. It does
not work: the link becomes **one** element whose value is the whole array, not one element per
entry.

Both spellings hit it:

```
ns-pl/select_columns-~X~-R/config/cols.json~E     a link element in the query
```

```yaml
# recipes.yaml
links:
  columns: -R/config/cols.json
```

In each case the element is materialised by `materialize_nested_parameter`
(`liquers-core/src/interpreter.rs:344`) into a single `ParameterValue` holding
`["date", "amount"]`. `CommandArguments::get_multiple` then calls
`String::from_parameter_value` on that element, which fails: a JSON array is not a string.

So the argument is either a conversion error, or — for a command declared `Vec<Value>` — a
one-element list containing an array, which is not what the caller meant either.

## Why it is shaped this way

Nothing decided it. `pop_value`'s variadic branch treats a link like any other parameter and pushes
one element for it (`plan.rs:777-783`); `override_link` on a variadic slot likewise produces a
one-element list. Expansion would have to happen *after* materialisation, which is where the array
first becomes visible, and no layer does anything there today.

## Expected behaviour

Undecided, and that is the substance of the issue. Two coherent answers:

1. **Expand.** A link element whose materialised value is an array becomes one element per entry.
   Natural for the motivating case; makes the element count depend on runtime data, which nothing
   else in plan resolution does.
2. **Do not expand, but convert.** Keep one element and let `get_multiple` accept an array-valued
   element by flattening it during conversion. Keeps planning static; moves list-ness into
   retrieval.

Option 2 interacts with the existing `impl<V: ValueInterface> FromParameterValue<Vec<V>>`
(`commands.rs:269`), which already reads a JSON array out of a single parameter — that is arguably
the same feature, reachable only for `Vec<Value>` and only through hand-built registrations.
Settle whether these are one mechanism before implementing either.

## Impact

Low today: no in-repo command or recipe supplies a variadic argument by link. It becomes visible
the first time someone tries, and the failure is a conversion error some distance from the cause.

## Discovery

Found while fixing recipe overrides of variadic arguments
(`specs/design/variadic-arguments-declaration/`, from a Codex review finding on PR #38). The fix
made `override_link` preserve the variadic slot as a one-element list; whether that element should
expand when it resolves to an array is this issue, deliberately left out of that change.
