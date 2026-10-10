# Phase 2: Solution & Architecture — Command alias contract

## Overview

Aliases are resolved entirely at plan time. The planner looks up the target, validates the alias
against it, resolves `head ++ own arguments`, and emits one `Step::Action` for the target whose
`origin` names the alias. Everything downstream — dependency scan, volatility, the interpreter,
recipe overrides — sees an ordinary action plus its provenance. Validation lives in one function on
`CommandMetadataRegistry`, shared by registration and planning, so a hand-built or deserialized alias
(liquers-py) is held to the same rules as one built by `register_alias`.

Rejected:
- Validating only at registration — deserialized registries (liquers-py, a YAML overlay) bypass it.
- Resolving the target at execution — errors surface late, and the plan cannot be inspected.
- Keeping `from_action_extended` with new semantics — a silent change to a public function; it is
  replaced by a function with a new name.
- Plan-recording alternatives — Phase 1, Open Question 4.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `POLARS-COMMAND-TESTS-BYPASS-COMMANDS` | draft | P2 | AC-9 evaluates `pl/head` through the command path, a first such test | No | No | Mention in that issue's Phase 5 note |
| `STATE-ARGUMENT-CONSTRUCTOR-SERDE-DEFAULT-DISAGREE` | draft | P2 | `register_alias` copies the target's `state_argument` value instead of constructing one | No | No | None |
| `POST-INIT-COMMAND-REGISTRATION` | accepted | P3 | An alias is registered like any command, before `to_ref` | No | No | None |
| `PY-MODULES-NOT-DECLARED-IN-LIB` | draft | P2 | liquers-py's aliases cannot run until `pycall` compiles; not required here | No | No | None |

## Interfaces

**`liquers-core/src/plan.rs`** (sync: plan building does no I/O)

```rust
/// Why a `Step::Action` calls the command it names.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
pub enum ActionOrigin {
    /// The query named this command.
    #[default]
    Direct,
    /// The query named `command`, an alias whose definition resolved to this action.
    Alias { command: CommandKey },
}

impl ActionOrigin {
    pub fn is_direct(&self) -> bool;
}

// Step::Action gains a last field:
    #[serde(default, skip_serializing_if = "ActionOrigin::is_direct")]
    origin: ActionOrigin,

impl ResolvedParameterValues {
    /// Replaces `from_action_extended`. Heads are named by `target`'s leading arguments; the rest
    /// resolve against `alias`'s own arguments, so the excess count and the names are the alias's.
    pub fn from_alias_action(
        action_request: &ActionRequest,
        alias: &CommandMetadata,
        target: &CommandMetadata,
        head_parameters: &[CommandParameterValue],
        allow_placeholders: bool,
    ) -> Result<Self, Error>;
}
```

`from_action` keeps its signature. `from_action_extended` is removed; its only caller is
`PlanBuilder`.

**`liquers-core/src/command_metadata.rs`** (sync)

```rust
impl CommandMetadataRegistry {
    /// The target of an alias, after checking AC-3..AC-6. `Ok(None)` for a `Registered` command.
    pub fn alias_target(&self, alias: &CommandMetadata) -> Result<Option<CommandMetadata>, Error>;
}
```

Checks, in order: target registered (AC-5); target not itself an alias (AC-6); `head.len()` ≤ the
target's argument count (AC-3); no head position is an injected target argument (AC-4). Returns an
owned value, as `find_command` does.

**`liquers-core/src/commands.rs`** (sync)

```rust
impl<E: Environment> CommandRegistry<E> {
    /// Registers metadata only: the target's executor runs. Copies the target's state argument,
    /// volatile, payload_required, expires and is_async; sets `definition: Alias`.
    pub fn register_alias(
        &mut self,
        alias: CommandKey,
        target: CommandKey,
        head_parameters: Vec<CommandParameterValue>,
        arguments: Vec<ArgumentInfo>,
    ) -> Result<&mut CommandMetadata, Error>;
}
```

Beyond `alias_target`, it checks the shape the executor reads positionally:
`head.len() + arguments.len() == target.arguments.len()`; `injected` and `multiple` agree position
by position; alias key ≠ target key (AC-6). Like `register_command`, a later registration under the
same key replaces the earlier one (`CommandMetadataRegistry::add_command`).

**`liquers-core/src/error.rs`** — see Error Handling.

**Value types, traits, `ExtValue`:** none.

## Integration Points

- `liquers-core/src/plan.rs`
  - `PlanBuilder` alias arm: `self.command_registry.alias_target(&metadata)?`; run the volatility,
    payload and expiration checks for the target key as well as the alias key (AC-8); resolve with
    `from_alias_action`; push the action with `origin: ActionOrigin::Alias { command:
    metadata.key() }`. The existing `Step::Info("Alias command …")` stays: it is the line a user
    reads in the asset log.
  - Registered arm and every other `Step::Action` construction: `origin: ActionOrigin::Direct`.
  - `scan_plan`, action arm: for `Alias`, also insert `DependencyKey::for_command_metadata(alias)`
    with `DependencyRelation::CommandMetadata`. No implementation key: an alias has no
    implementation.
  - Rewrite `accepted_count_excludes_head_parameters` for `from_alias_action`.
- `liquers-core/src/interpreter.rs` — each `Step::Action` pattern that lists all fields gains
  `origin`: the CWD rewrite carries it over; `apply_step` calls `Error::with_alias` on failure when
  the origin is `Alias`; both `IsVolatile` impls also test the alias's own `volatile`.
- `liquers-core/src/validate/report.rs` and tests that build `Step::Action` literals: add
  `origin: ActionOrigin::Direct`.
- `liquers-lib/src/polars/selection.rs` — delete `head` and its `register_command!`. After `slice` is
  registered: `$cr.register_alias(CommandKey::new("", "pl", "head"), CommandKey::new("", "pl",
  "slice"), vec![CommandParameterValue::Value(0.into())], vec![ArgumentInfo::integer_argument("n",
  false).with_default(5)])?`, then set label and doc on the returned metadata.
- `specs/command_registry.yaml` — regenerate, with a CHANGELOG line.

## Error Handling

All are typed constructors on `Error`. No new `ErrorType`, so the exhaustive matches on it
(`assets.rs`, liquers-py, axum status mapping) are untouched.

| Case | Constructor | `ErrorType` |
|---|---|---|
| Target not registered (AC-5) | `alias_target_not_registered(alias, target)` | `ActionNotRegistered` |
| Target is an alias, or alias = target (AC-6) | `alias_chain_not_supported(alias, target)` | `NotSupported` |
| Head too long, head on injected argument, shape mismatch (AC-3, AC-4, AC-10) | `invalid_alias(alias, target, problem: &str)` | `ParameterError` |
| Too many parameters through an alias (AC-2) | existing `too_many_parameters`, subject = the alias | `TooManyParameters` |
| Execution failure of the target | existing error + `with_alias(&alias)` appending `" (via alias '<key>')"` once | unchanged |

## Relevant Commands

No new command. `pl/head` changes from a registered command to an alias of `pl/slice`; its query
interface (`ns-pl/head`, `ns-pl/head-<n>`) is unchanged. Namespaces involved: `pl`.

## Documentation Architecture

| Document | Kind | Change |
|---|---|---|
| `specs/reference/COMMAND_ALIASES.md` | reference, new | Contract; validation table (AC-3..6, shape); planning (`ActionOrigin`, volatility union, dependencies); errors |
| `specs/guides/COMMAND_REGISTRATION_GUIDE.md` | guide | Quick Reference row; §2 *Registering an alias*: `register_alias` call for `pl/head`, the argument layout (head then the alias's own arguments), what is copied from the target, the registration errors, testing an alias. Links the reference |
| `specs/guides/COMMAND_DESIGN_GUIDE.md` | guide | Intro no longer "cancellation only"; new `## Aliases` section: an alias is a binding, not an implementation. **When to use** — a convenience wrapper that fixes leading arguments of a general command (`pl/head` = `pl/slice` with `offset = 0`); a bridge that dispatches declared commands to one generic executor, the head carrying the identity (`pycall` with module and function). **When not to** — the behaviour differs, not just the arguments (`rec/head` materializes, `rec/slice` does not); the target's arguments need reordering or computing (argument mapping is a later design); a chain of aliases. **Design consequences** — the alias's arguments are its public interface; volatility, payload and expiry come from the target; errors name the target "via alias"; cached results depend on the alias's metadata; changing a registered command into an alias invalidates its cache once |
| `specs/reference/COMMAND_DECLARATION.md` | reference | §4.1 `definition` row links the new reference |
| `specs/reference/POLARS_COMMAND_LIBRARY.md` | reference | `head` is an alias of `slice` |
| `specs/reference/api/DOC_08_RECIPES_PLANS.md` | reference | `Step::Action` shape gains `origin` |

`affects_docs` is the five existing documents. `specs/README.md`: link the new reference from the
*Command aliases* line when the design completes.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `plan.rs`, `command_metadata.rs`, `commands.rs`, `error.rs`, `interpreter.rs`, `validate/report.rs`; `polars/selection.rs`; `command_registry.yaml` |
| Crates and workflows affected | liquers-core, liquers-lib; registry regeneration |
| Existing tests likely to change | `accepted_count_excludes_head_parameters`; tests that build `Step::Action` literals (`plan.rs`, `interpreter.rs`, `tests/validate_integration.rs`) |
| New validation | Phase 3 tests; `registry_export`; build matrix (the `polars` feature gates the alias) |
| Compatibility / data | Plans gain a defaulted field, omitted when `Direct`: old plans load, unaliased plans serialize unchanged. `from_action_extended` leaves the public API (no outside caller). Cached `pl/head` assets are invalidated once; `pl/head` loses its `impl_version`, correctly. |
| Concurrency / performance / security | None: planning does one more registry lookup per aliased action |
| Behaviour | Heads are not type-checked at registration; the executor converts them like any default, and a wrong type fails at execution with the alias named |
| Recovery | Revert the `pl/head` registration to restore the old command; the core changes are additive except the removed function |
| Certainty and open questions | High: every touched symbol was opened at HEAD. No open questions. |
