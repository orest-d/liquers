---
title: Command Aliases
kind: reference
audience: internal
area: [core/commands, core/plan]
reviewed: 2026-10-10
---
# Command Aliases

An **alias** is a command with no implementation of its own: a name, an argument interface and a
binding to another command — its **target** — with some of the target's leading arguments fixed.
`pl/head` is the production example: it is `pl/slice` with `offset = 0`.

The how-to is in [`guides/COMMAND_REGISTRATION_GUIDE.md`](../guides/COMMAND_REGISTRATION_GUIDE.md)
§2 *Registering an alias*; when to design a command as an alias is in
[`guides/COMMAND_DESIGN_GUIDE.md`](../guides/COMMAND_DESIGN_GUIDE.md) §Aliases.

## 1. Representation

`CommandMetadata.definition` (`liquers-core/src/command_metadata.rs`):

```rust
pub enum CommandDefinition {
    Registered,
    Alias { command: CommandKey, head_parameters: Vec<CommandParameterValue> },
}
```

In YAML (`specs/command_registry.yaml`):

```yaml
- namespace: pl
  name: head
  arguments:
  - name: n
    default: !Value 5
    argument_type: int
  definition: !Alias
    command: { realm: '', namespace: pl, name: slice }
    head_parameters:
    - !Value 0
```

An alias has no executor and no `impl_version`.

## 2. The argument contract

**The parameters passed to the target are `head_parameters` followed by the alias's own
arguments.** An alias's `arguments` are exactly what its user supplies; they never include the
positions the heads fill.

| Query | Target call |
|---|---|
| `ns-pl/head` | `slice [offset = 0, length ← n = 5]` |
| `ns-pl/head-10` | `slice [offset = 0, length ← n = 10]` |
| `ns-pl/head-1-2` | error: *Too many parameters for command 'head': accepts 1, but parameter #2 '2' was supplied* |

- Heads are named by the target's leading arguments (`offset`); the alias's own parameters keep the
  alias's names (`n`), labels, defaults and query positions. Recipe overrides by name therefore use
  the alias's names.
- The target's executor reads parameters **by position**, so an alias's arguments must line up with
  the target's arguments after the heads: same count, same `injected` and `multiple` flags.
- Too-many-parameters errors count the alias's own non-injected arguments and name the alias.

## 3. Validation

`CommandMetadataRegistry::alias_target(&alias)` applies these rules. Both registration and planning
call it, so an alias that was hand-built or deserialized (liquers-py, a YAML overlay) is held to the
same rules as one built by `CommandRegistry::register_alias`.

| Rule | Error (`ErrorType`) |
|---|---|
| The target is registered | `Error::alias_target_not_registered` (`ActionNotRegistered`) |
| The target is not an alias, and is not the alias itself | `Error::alias_chain_not_supported` (`NotSupported`) |
| `head_parameters.len()` ≤ the target's argument count | `Error::invalid_alias` (`ParameterError`) |
| No head fills an injected or a `multiple` target argument | `Error::invalid_alias` (`ParameterError`) |

`register_alias` additionally checks the positional shape of §2 (count and flags), and copies the
target's `state_argument`, `volatile`, `payload_required`, `expires` and `is_async`. Registered
under the key of an existing command, it replaces that command entirely, executors and
`impl_version` included; a refused registration changes nothing. A deserialized
alias is planned without the shape check; a mismatch then surfaces when the target's executor reads
its parameters.

## 4. Planning

Aliases are resolved entirely at plan time (`liquers-core/src/plan.rs` `PlanBuilder`):

1. The alias is validated (§3); a failure is a planning error, never an execution one.
2. The volatility, payload requirement and expiration of **both** the alias and the target apply
   to the plan. An alias can never make a volatile command look stable.
3. Parameters are resolved with `ResolvedParameterValues::from_alias_action` (§2).
4. One `Step::Action` for the target is emitted, carrying its provenance:

```rust
pub enum ActionOrigin { Direct, Alias { command: CommandKey } }
// Step::Action { realm, ns, action_name, position, parameters, origin }
```

`origin` is omitted from a serialized plan when `Direct`, so unaliased plans are unchanged and plans
written before the field existed load as `Direct`. A planning diagnostic, *"Command '…' is an alias
of '…'"*, goes to `plan.init_steps` (the asset log), not to the executed steps.

Aliases do not chain, and a later argument-*mapping* form of alias (reordering or computing target
arguments) would still be `ActionOrigin::Alias`: its rules live in the alias's metadata.

## 5. Dependencies and execution

- The dependency scan records, for an aliased action, the target's command-metadata and
  command-implementation keys **and the alias's command-metadata key**. Changing an alias — its
  head, its arguments, its target — changes its `metadata_version`, which invalidates results
  computed through it.
- Turning a registered command into an alias changes the dependency keys of its results, which are
  recomputed once.
- The target's executor runs. When it fails, the error keeps the target as `command_key` and its
  message gains *" (via alias '<alias>')"* (`Error::with_alias`).
- `IsVolatile` and `RequiresPayload` for an aliased step consult the alias's metadata as well as
  the target's.

## 6. Related documents

- [`COMMAND_DECLARATION.md`](COMMAND_DECLARATION.md) §4.1 — `definition` as a declarable key.
- [`POLARS_COMMAND_LIBRARY.md`](POLARS_COMMAND_LIBRARY.md) — `pl/head`.
- [`api/DOC_08_RECIPES_PLANS.md`](api/DOC_08_RECIPES_PLANS.md) — plan steps.
- Design: [`design/command-alias-contract/`](../design/command-alias-contract/).

## History

| Date | Change | Source |
|---|---|---|
| 2026-10-10 | Created: the argument contract, validation (including replacement of a registered command), planning with `ActionOrigin`, dependencies and execution, as implemented and tested (`liquers-core/tests/command_alias.rs`, `pl/head`). | `design/command-alias-contract/` |
