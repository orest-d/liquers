---
id: ARGUMENT-DECLARATION-IS-ALL-OR-NOTHING
kind: feature
title: Declared arguments replace inferred ones instead of augmenting them
status: closed
priority: P3
complexity: M
area: [core/commands, web, py]
design:
created: 2026-08-29
github:
---
## Problem

A host that registers a command from a callable has two sources of argument information: what it can
read from the callable itself, and what the author supplies. Today these are mutually exclusive.

`JsCommandSpec::parse` (`liquers-web/src/command/spec.rs:171-174`) infers arguments **only** when no
`arguments` key is present:

```rust
let (arguments, arguments_inferred) = match get(spec, "arguments") {
    Some(declared) => (parse_arguments(&declared, &name)?, false),
    None => (infer_arguments(&run, state_mode, &name)?, true),
};
```

So supplying any argument metadata switches inference off entirely, and the author must restate
every argument in full.

## Why it matters

For JavaScript this is defensible: `infer_arguments` parses function source, requires plain
identifiers and an exact `Function.length` match, and refuses defaults and destructuring outright, so
what it can infer is thin and an author overriding it usually has to supply everything anyway.

For a language with real reflection it is the wrong default. Python's `inspect.signature` yields
parameter names, type annotations, defaults and kinds exactly. An author who wants to add a widget
hint to one argument should not have to restate the types and defaults that were already known:

```python
@command(label="Repeat text", arguments={"count": {"label": "Repeat count", "gui": ...}})
def repeat(state, count: int = 2): ...
```

Under the all-or-nothing rule that declaration discards `int` and `2`. The same applies to Starlark,
whose `def` carries parameter type annotations, and to Rhai, whose `get_fn_metadata_list` reports
parameter names for script functions — three of the six languages assessed in
`design/command-declaration/portability-analysis.md`.

## Fix direction

Make the merge explicit rather than implicit, in whichever layer ends up owning declaration parsing:

1. **Per-argument merge by name.** A declared argument entry augments the inferred one with the same
   name; fields the entry omits keep their inferred values. An entry naming an argument the callable
   does not have is an error, not a silent addition.
2. **Ordering.** Inference establishes the argument list and its order; declaration may not reorder
   it. This keeps positional binding stable, which matters because Liquers binds query parameters
   positionally.
3. **Opting out.** A host that wants today's replace-everything behaviour needs a way to say so —
   either a flag, or the convention that a declared list containing an argument the callable does
   not have means "these are authoritative". Decide deliberately rather than inheriting.
4. Keep JavaScript's current behaviour unless it is changed on purpose: `command05` asserts on the
   refusal messages, and `describeCommand` reports whether arguments were inferred.

## Related

- `COMMAND-DECLARATION-FORMAT` — found while validating that design's portability. Not a blocker for
  it: its `arguments_declared` boolean matches today's JavaScript behaviour exactly, and a Python
  binding can merge before handing metadata over. This issue is about whether the merge belongs in
  the shared layer instead of being re-implemented per host.
- `COMMAND-METADATA-ENHANCEMENTS` — per-argument enum and IO typing would be supplied the same way,
  so the merge semantics should be settled before that lands.

## Verification

A command whose callable declares `count: int = 2`, registered with argument metadata supplying only
a label for `count`, has metadata carrying **all three** of the label, the `int` type and the default
`2`.

## Resolution (2026-08-30)

**Closed — resolved by `design/command-declaration/`** (PR [#50](https://github.com/orest-d/liquers/pull/50)). The shared declaration pipeline merges
declared arguments over discovered ones **by name**: an entry naming a discovered argument augments
it field by field, leaving what it omits alone. The exact case this issue describes — adding a
widget hint to one argument without discarding its type and default — is `merge05` in
`liquers-core::command_declaration`.

Two rules came with it, both tested. An entry naming an argument the callable does not have is
**rejected** (`merge06`) rather than appended, because Liquers binds query parameters positionally
and a typo would silently misbind. And the reject rule stands down when no introspection ran
(`merge07`), so a plain document can still establish its own argument list.

`liquers-web` keeps today's all-or-nothing behaviour, which is correct for it: its inference parses
function source and refuses anything it cannot read exactly, so an author overriding it generally
has to supply everything anyway.
