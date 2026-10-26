# Phase 2: Solution & Architecture — a generic command dispatches by its input's type

## Overview

A generic is a metadata entry whose `definition` is `Generic { variants }`; it has no executor.
`register_generic` checks the variants and derives the generic's argument list and declared types.
When the planner meets a generic it plans every variant's step with the existing registered/alias
code; if the declared flowing type (`command-io-type-declarations`) picks one variant, that step
is emitted with origin `Specialized`; otherwise a `Step::Dispatch` holding all of them is emitted and
the interpreter picks one by the input value's identifier. Selection is one function used by both.

Rejected: a `CommandKey` extended with a type (every key, map and error changes; an alias variant
has no place to live); variants declaring `specializes:` (no single preference order; question 1);
run-time-only dispatch through an executor lookup (cannot run alias variants: their head parameters
are applied at plan time); plan-time-only selection (a resource's type is not known then).

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `COMMAND-INPUT-OUTPUT-TYPES-ARE-UNDECLARED` (`command-io-type-declarations`) | draft | P2 | Provides `accepts`, `flowing` | yes | yes (order) | `requires` |
| `CORE-MULTI-REALM-INTERPRETER` (`multi-realm-dispatch`) | accepted | P3 | Also rewrites what a step runs | no | no | Same realm only |
| `VALUE-NOT-CONVERTED-AUTOMATICALLY` (`automatic-value-conversion`) | draft | P2 | Converts after selection | no | no | `overlaps` |

## Interfaces

```rust
// liquers-core/src/command_metadata.rs
pub enum CommandDefinition {
    #[default] Registered,
    Alias { command: CommandKey, head_parameters: Vec<CommandParameterValue> },
    /// Variants in preference order; chosen by the input's type (see `select_variant`).
    Generic { variants: Vec<CommandKey> },
}
impl CommandMetadataRegistry {
    /// `Ok(None)` unless `generic` is `Generic`; otherwise its variants' metadata after the checks
    /// of AC-5. Registration and planning both call it, as both call `alias_target`.
    pub fn generic_variants(&self, generic: &CommandMetadata)
        -> Result<Option<Vec<CommandMetadata>>, Error>;
}
/// Principal match, then first accepting in order, then the undeclared variant.
/// `variants` pairs each variant's key with its state's `accepts`.
pub fn select_variant<'a>(generic: &CommandKey, variants: &'a [(CommandKey, TypeSet)],
                          type_identifier: &str) -> Result<&'a CommandKey, Error>;

// liquers-core/src/commands.rs
impl<E: Environment> CommandRegistry<E> {
    /// Registers `generic` over existing `variants`. Arguments are the first variant's;
    /// `accepts` is the variants' identifiers in order (any if one is undeclared); `produces`
    /// likewise; `volatile` = any, `payload_required` = join, `cached` = `Some(false)` if any
    /// variant is uncached. Replaces an existing entry like `register_alias`; nothing changes on error.
    pub fn register_generic(&mut self, generic: CommandKey, variants: Vec<CommandKey>)
        -> Result<&mut CommandMetadata, Error>;
}

// liquers-core/src/plan.rs
pub enum ActionOrigin {
    Direct,
    Alias { command: CommandKey },
    /// The query named generic `command`; the planner chose `variant` (which may be an alias).
    Specialized { command: CommandKey, variant: CommandKey },
}
pub enum Step {
    // …existing…
    /// A generic whose variant is chosen at run time by the input value's type identifier.
    Dispatch { command: CommandKey, position: Position, variants: Vec<DispatchVariant> },
}
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct DispatchVariant { pub variant: CommandKey, pub accepts: TypeSet, pub step: Step }

// liquers-core/src/error.rs
impl Error {
    /// `ErrorType::ConversionError`: "no variant of 'head' accepts 'Image'; variants: pl/head
    /// (polars.DataFrame, RecordView, Text, Bytes), rec/head (…)".
    pub fn no_command_variant(generic: &CommandKey, type_identifier: &str,
                              variants: &[(CommandKey, TypeSet)]) -> Self;
}
```

Planner (`PlanBuilder::process_action`): the body of the `Registered` and `Alias` arms moves into a
private `fn action_step(&mut self, metadata, action_request, prefix) -> Result<Step, Error>`
(applying flags, resolving parameters, building `Step::Action`). The new `Generic` arm: call
`generic_variants`, `apply_command_flags` for the generic, build each variant's step with
`action_step` (a variant's alias target flags apply as today), then:
- if `flowing` is a declared `TypeSet` whose every identifier selects the same variant, push that
  step with its origin replaced by `Specialized { command, variant }` and
  `init_info("Command 'head' specialized to 'rec/head' for RecordView")`; `flowing` becomes the
  variant's `produces`;
- otherwise push `Step::Dispatch` and `init_info("Command 'head' is chosen at run time among …")`;
  `flowing` becomes the generic's `produces`.

Interpreter (`do_step`, `do_step_state`): `Step::Dispatch` selects with
`select_variant(&command, …, &input.type_identifier())`, logs
`context.info("dispatched 'head' to 'pl/head' for Bytes")`, then evaluates the chosen `step` by the
same function (boxed recursion, as `Step::Plan` does); a no-match error gets `with_position` and
`with_command_key(command)`.

## Integration Points

- `command_metadata.rs`: enum arm; `alias_target` refuses a `Generic` target; `generic_variants`;
  `select_variant`. `commands.rs`: `register_generic`; the `head_count` match in `register_alias`.
- `plan.rs`: `ActionOrigin::Specialized` (+ `is_direct`, `alias`, and a new `named()` for error
  naming); `Step::Dispatch` arms in `is_log`, `is_action` (true), `is_context_modifier`,
  `freeze_cwd_with` (recurse into each variant step), `is_bare_key_read`, `to_metadata_record`,
  `update_metadata_record`, `last_action_index` / `last_action_command` / `override_value`
  (a `Dispatch` counts as the last action; an override is applied to every variant step),
  `scan_plan` (metadata + implementation of every variant, metadata of the generic; for
  `Specialized`, metadata of generic and variant), `requires_payload` in `interpreter.rs`.
- `interpreter.rs`: `schedule_plan_dependencies_from`, `resolve_absolute_query_resource_step`,
  `log_init_steps`, `do_step`, `do_step_state`.
- `validate/report.rs` `collect_step_warnings`; `store/realm.rs` (three matches);
  `liquers-mcp/src/server.rs` (one match).
- `liquers-py/src/command_metadata.rs`: the definition wrapper renders `Generic`.
- `liquers-lib/src/commands.rs`: register `head` after `ns-pl` and `ns-rec`, gated
  `#[cfg(any(feature = "polars", feature = "records"))]`, with the variants present in the build.

## Error Handling

Registration: `Error::general_error` naming the generic and the failing rule (unknown variant uses
`alias_target_not_registered`'s wording style). Planning: the same errors from `generic_variants`
(a deserialized registry is checked, not trusted). Run time: `Error::no_command_variant`.

## Relevant Commands

New root command `head` (generic): variants `pl/head` (alias of `pl/slice`), then `rec/head`;
arguments `n: int = 5`. Registered with
`cr.register_generic(CommandKey::new("", "", "head"), vec![CommandKey::new("", "pl", "head"),
CommandKey::new("", "rec", "head")])?.with_label("Head").with_doc("First n rows of a table-like input")`.
`specs/command_registry.yaml` regenerated (`definition: !Generic`).

## Documentation Architecture

| Path | Change |
|---|---|
| `specs/reference/COMMAND_ALIASES.md` | "Generic commands": rules, selection, errors; History, `reviewed:` |
| `specs/reference/RECIPES_AND_PLANS.md` | `Step::Dispatch`, `ActionOrigin::Specialized`; History, `reviewed:` |
| `specs/reference/COMMAND_DECLARATION.md` | `definition: Generic`; History, `reviewed:` |
| `specs/guides/COMMAND_REGISTRATION_GUIDE.md` | "Registering a generic command"; History, `reviewed:` |

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | listed above; `specs/command_registry.yaml` |
| Crates and workflows affected | core, lib, mcp, py; plan serialization gains a step and an origin |
| Existing tests likely to change | none by behaviour; exhaustive matches in tests that build `Step` lists compile unchanged |
| New validation | Phase 3; `check-build-matrix.sh` for the gated registration; `cargo test -p liquers-mcp` |
| Compatibility / data | Serialized plans and registries gain variants; an older reader fails on `!Generic` / `Dispatch` rather than misreading them. Existing queries are unaffected (AC-8): `head` was unregistered in the root namespace |
| Concurrency / performance | Planning a generic plans each variant (two for `head`); run-time selection is a slice scan |
| Security | none |
| Recovery | Revert; the generic's registration is one call in `liquers-lib` |
| Certainty | High for the mechanism; question 4 decides what ships |
