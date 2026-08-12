---
id: COMMAND-VARIADIC-ARGUMENTS-NOT-DECLARABLE
kind: issue
title: Variadic command arguments cannot be declared or retrieved
status: draft
priority: P1
complexity: M
area: [macro, core/commands, lib/commands]
design: 
created: 2026-08-12
github:
---
## Problem

`ArgumentInfo.multiple` marks an argument that consumes every remaining action parameter. The
plan builder and the interpreter implement it fully, but a command author cannot declare one and the
command framework cannot hand one to a function. **No command in the workspace is variadic**, and
`multiple: true` appears nowhere outside a single `plan.rs` unit test.

| Layer | State |
|---|---|
| `ArgumentInfo.multiple` and `set_multiple()` | works — `liquers-core/src/command_metadata.rs:385`, `:550` |
| `pop_value` collects the iterator remainder | works — `liquers-core/src/plan.rs:679-729` |
| Interpreter materialises `MultipleParameters` | works — `interpreter.rs:106`, `:335`, `:362`, `:454`, `:1368` |
| `register_command!` DSL flag | **absent** — `ArgumentInfo` is generated with `multiple: false` hardcoded at `liquers-macro/src/registration.rs:718`, `:2336`, `:2406` |
| `FromParameterValue<Vec<T>>` for a scalar `T` | **absent** — the only `Vec` implementation is `impl<V: ValueInterface> FromParameterValue<Vec<V>> for Vec<V>` (`liquers-core/src/commands.rs:269`), so `Vec<String>` cannot be produced |
| `Vec<_>: TryFrom<Value, Error = Error>` | **absent** — `TryFrom<Value>` is implemented for scalars only (`liquers-core/src/value.rs:599-752`) |

The last two rows are the substantive blockage. `Arguments::get` (`commands.rs:102`) is bounded

```rust
pub fn get<T: FromParameterValue<T> + TryFrom<E::Value, Error = Error>>(&self, i: usize, name: &str) -> Result<T, Error>
```

so `arguments.get::<Vec<String>>(i, name)` — the call the macro would generate — satisfies neither
bound, and `Vec<Value>` fails the second. Adding
`impl<T: FromParameterValue<T>> FromParameterValue<Vec<T>> for Vec<T>` does not work either: it
overlaps the existing `Vec<V: ValueInterface>` implementation and is rejected by coherence.

## Impact

A command that wants a variable-length parameter list has no way to say so. The visible casualties
are `pl/select_columns` and `pl/drop_columns`, documented as taking "column names separated by
dashes" while declaring a single `String`: because `-` separates parameters, the dash-separated
spelling never reached the command as written.

### Affected commands — fixing these is part of this issue

Both live in `liquers-lib/src/polars/selection.rs` and both work around the missing feature the same
way: they declare one `String` argument and split it on `-` internally.

| Command | Declaration | Internal workaround | Registration |
|---|---|---|---|
| `pl/select_columns` | `columns: String` (`selection.rs:15`) | `columns.split('-')` (`:18`) | `:105` |
| `pl/drop_columns` | `columns: String` (`selection.rs:37`) | `columns.split('-')` (`:40`) | `:114` |

These two are the whole set. Verified by searching `liquers-lib/src/` for dash-splitting and for
list-shaped documentation: no other command in any namespace takes a delimited list in a single
argument, and no other `polars` module (`aggregation`, `filtering`, `info`, `io`, `sorting`) has a
multi-value argument.

The internal `split('-')` is the workaround, not the feature, and it must be **removed** when the
arguments become variadic — leaving it would make `select_columns-a~_b` (one escaped argument) and
`select_columns-a-b` (two arguments) both work but by two different mechanisms, and would silently
keep splitting a legitimate column name that contains a dash.

Until `PLAN-EXCESS-ACTION-PARAMETERS-DROPPED` was fixed this failed silently — `select_columns-a-b`
selected only `a`. It now raises a positioned error instead, so the defect is loud rather than
silent, and the working spelling is the escaped `select_columns-a~_b`, which resolves to the single
argument `"a-b"` and splits correctly inside the command.

`multiple` is also the sanctioned exemption from that arity check, so this issue is what stops an
author from taking the documented way out.

## Fix direction

Add an accessor that does not route through the blocked bounds:

```rust
// liquers-core/src/commands.rs, beside `get` and `get_injected`
pub fn get_multiple<T: FromParameterValue<T>>(&self, i: usize, name: &str) -> Result<Vec<T>, Error>
```

It walks `ParameterValue::MultipleParameters` and calls `T::from_parameter_value` per element. This
needs **no new trait implementation**, so there is no coherence conflict, and it drops the
`TryFrom<E::Value>` bound, which exists only for the pre-materialised fast path that a variadic
argument never uses.

Then, in `liquers-macro/src/registration.rs`:

- accept a `multiple` flag in the argument DSL and propagate it to the generated `ArgumentInfo`;
- emit `get_multiple` rather than `get` for such an argument;
- **reject unknown flags while doing so.** The current parser (`:1564`) parses any identifier and
  compares it to `"injected"`, so `fn f(state, a: i32 foobar)` silently swallows `foobar`. Adding a
  second flag without this fix makes a typo between `multiple` and `injected` fail silently.

Finally — and this is required for the issue to be closed, not optional follow-up — fix the two
affected commands listed above:

1. declare `columns` variadic in both `register_command!` invocations (`selection.rs:105`, `:114`);
2. change both signatures to `columns: Vec<String>` and **delete** the internal `split('-')`;
3. update both doc comments and the `doc:` metadata, which currently describe the workaround
   ("separated by dashes") rather than the behaviour;
4. regenerate `specs/command_registry.yaml` — `cargo test -p liquers-lib --test registry_export`
   enforces this;
5. correct `specs/reference/POLARS_COMMAND_LIBRARY.md`, which
   `specs/design/excess-action-parameters-error/` will by then have set to the escaped `a~_b`
   spelling; it becomes the plain `a-b` spelling again.

A column name that genuinely contains a dash is expressible after this and is not today: as a
variadic argument each element is escaped independently, so `select_columns-a~_b` selects the single
column `a-b` while `select_columns-a-b` selects two. Under the current `split('-')` those two are
indistinguishable. Worth a test.

`VARIADIC-ARGUMENT-STARVES-LATER-ARGUMENTS` should be fixed in the same effort or immediately
after: it becomes reachable the moment this one lands.

Documentation: `specs/reference/REGISTER_COMMAND_FSD.md` and the DSL summary in `CLAUDE.md` both
list argument attributes and would gain `multiple`.

## Discovery

Split out of `specs/design/excess-action-parameters-error/` at Phase 2. That design initially
planned to make the polars commands variadic; the trait-layer blockage above was found while
specifying it, and the work was deferred here as a three-crate change with a trait-design decision
inside it.
