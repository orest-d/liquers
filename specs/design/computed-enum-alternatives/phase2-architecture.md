# Phase 2: Solution & Architecture — computed enum alternatives

## Overview

Re-checked at HEAD (2026-10-26):

- `ArgumentType::GlobalEnum(name)` is resolved to `ArgumentType::Enum(..)` by
  `CommandMetadata::resolve_global_enums` in `PlanBuilder::get_command_metadata` (`plan.rs`
  ≈1554), then `ParameterValue::from_string` (`plan.rs` ≈635) expands an alias or, with
  `others_allowed`, parses the text by `value_type` (`parse_other_enum_value`, ≈493).
- `make_plan_with_cwd` (`interpreter.rs` ≈177) is async with the `EnvRef`: `pb.build()` then
  `analyze_plan_dependencies` (`plan.rs` ≈3505).
- The interpreter's `Step::Action` arm (`interpreter.rs` ≈724) materializes parameters, resolves
  links, then calls `envref.get_command_executor().execute_async(&command_key, input, …)`.
- `CommandRegistry<E>` (`commands.rs` ≈622) holds executor maps; `CommandExecutor<E>` is the
  environment's executor trait (`#[async_trait]`, `?Send` on wasm32).
- `EnvironmentBuilder::validate` (`environment_builder.rs` ≈440) is
  `command_metadata_registry.check()`, and `build` fails on errors.

Chosen solution: metadata says *that* an enum is computed and *when* it can be checked; the
executable side holds providers; two async checks (plan, execution) and one listing function use
them. Rejected: a provider called inside `PlanBuilder` (it is synchronous and stateless);
input-scoped checks at plan time (no input exists there); a provider per argument instead of per
global enum (the same column list serves many commands).

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `COMMAND-METADATA-ENHANCEMENTS` (`global-enum-registration`) | accepted | P2 | `add_global_enum`, resolved-enum versioning | yes | no | `requires` |
| `ENUM-ALTERNATIVES-HAVE-NO-LABEL` | draft | P3 | readable computed alternatives | no | no | `overlaps` |
| `GLOBAL-ENUMS-NOT-IMPORTED-BY-LANGUAGE-INTEGRATIONS` | draft | P3 | computed enums from JS/Python have no provider | no | no | note |
| `COMMAND-INPUT-OUTPUT-TYPES-ARE-UNDECLARED` (`command-io-type-declarations`) | draft | P2 | typed provider input | no | no | note |

## Interfaces

`liquers-core/src/command_metadata.rs`:

```rust
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnumProviderScope {
    /// Alternatives depend on the environment only; checked when the query is planned.
    #[serde(rename = "environment")] Environment,
    /// Alternatives depend on the action's input state; checked before the command runs.
    #[serde(rename = "input")] Input,
}
pub struct EnumArgument {
    // … existing fields …
    /// Set when the alternatives are computed by the provider registered under `name`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub computed: Option<EnumProviderScope>,
}
impl EnumArgument { pub fn computed(self, scope: EnumProviderScope) -> Self; }
```

New module `liquers-core/src/enum_alternatives.rs` (re-exported from `lib.rs`):

```rust
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait EnumAlternativesProvider<E: Environment>: MaybeSend + MaybeSync {
    /// `input` is `None` for an environment-scoped enum, and for a listing made without a state.
    async fn alternatives(&self, envref: EnvRef<E>, input: Option<&State<E::Value>>)
        -> Result<Vec<EnumArgumentAlternative>, Error>;
}

pub struct EnumAlternatives {
    pub alternatives: Vec<EnumArgumentAlternative>,
    /// False when computed alternatives could not be obtained (no input, no provider, or failure).
    pub complete: bool,
}

/// What a UI or agent calls: static alternatives, then computed ones (aliases de-duplicated).
pub async fn enum_alternatives<E: Environment>(envref: EnvRef<E>, key: &CommandKey,
    argument: &str, input: Option<&State<E::Value>>) -> Result<EnumAlternatives, Error>;

pub(crate) async fn check_environment_enums<E: Environment>(envref: EnvRef<E>, plan: &mut Plan)
    -> Result<(), Error>;
pub(crate) async fn check_input_enums<E: Environment>(envref: &EnvRef<E>, key: &CommandKey,
    parameters: &ResolvedParameterValues, input: &State<E::Value>, context: &Context<E>)
    -> Result<(), Error>;
```

`liquers-core/src/commands.rs`:

```rust
pub trait CommandExecutor<E: Environment> {
    // … existing methods …
    fn enum_provider(&self, _name: &str) -> Option<Arc<dyn EnumAlternativesProvider<E>>> { None }
}
impl<E: Environment> CommandRegistry<E> {
    /// Marks `enum_argument` computed with `scope`, registers it as a global enum
    /// (`add_global_enum`) and its provider under its name; replaces an earlier one.
    pub fn register_enum_provider(&mut self, enum_argument: EnumArgument,
        scope: EnumProviderScope, provider: Arc<dyn EnumAlternativesProvider<E>>)
        -> Result<&mut Self, Error>;
    /// One warning per computed global enum without a provider, per inline computed enum.
    pub fn enum_provider_issues(&self) -> IssueReport;
}
```

`CommandRegistry` gains a private `enum_providers: HashMap<String, Arc<dyn …>>` and overrides
`enum_provider`. A closure adapter (`EnumProviderFn`) is an implementation detail of step 3.

## Integration Points

- `plan.rs` `ParameterValue::from_string`, `Enum` arm: when the alias is not static and
  `e.computed.is_some()`, parse with `parse_other_enum_value` whatever `others_allowed` says
  (AC-3, AC-6). Static aliases still expand.
- `interpreter.rs` `make_plan_with_cwd`: after `analyze_plan_dependencies`, call
  `check_environment_enums(envref, &mut plan)`. It walks `Step::Action` (and nested `Step::Plan`),
  looks the command up, resolves global enums, and for each `ParameterValue::ParameterValue`,
  `OverrideValue` or element of `MultipleParameters` whose enum is `Environment`-computed and not
  `others_allowed`, compares the text with the provider's aliases (one provider call per enum per
  plan).
- `interpreter.rs` `Step::Action` arm: before `execute_async`, `check_input_enums(…)` for
  `Input`-computed arguments, with the step's input state.
- `environment_builder.rs` `validate`: append `self.command_registry.enum_provider_issues()`.
- `liquers-macro/src/registration.rs` `to_argument_type_expression`: emit `computed: None` in the
  `EnumArgument` literal. `enum_ref:` syntax is unchanged.

## Error Handling

An unlisted value: `Error::conversion_error_with_message(value, &enum.name, "Undefined enum value
'{value}' for argument {arg}. Valid values: {aliases}")` — the static enum's message, with the
value quoted. At plan time it fails `make_plan`; at execution it becomes the action's error with
the command key and position attached by the existing `map_err`. A provider error: `plan.init_warning`
(plan time) or `context.warning` (execution), naming the enum and the error, and the value is
accepted (AC-5). `register_enum_provider` returns `add_global_enum`'s error for an invalid enum.

## Relevant Commands

No command changes. Converting `ns-pl/sort`, `select_columns` and `drop_columns` to an input-scoped
`pl.columns` enum is a `liquers-lib` follow-up that would regenerate `specs/command_registry.yaml`.

## Documentation Architecture

`REGISTER_COMMAND_FSD.md` (reference, `macro`): `enum_ref` may name a computed enum.
`COMMAND_DECLARATION.md` (reference): the `computed` field. `COMMAND_REGISTRATION_GUIDE.md` (guide):
how to register a provider, both scopes, the validate-only rule. Proposed `affects_docs` as in
`DESIGN.md`.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `command_metadata.rs`, new `enum_alternatives.rs`, `commands.rs`, `plan.rs`, `interpreter.rs`, `environment_builder.rs`, `lib.rs`, macro `registration.rs` |
| Crates and workflows affected | `liquers-core`, `liquers-macro`; every evaluation passes one extra plan walk (no provider call unless a computed enum is present) |
| Existing tests likely to change | none: the field is optional and the new branches only trigger on `computed` |
| New validation | unit tests per check, an integration test with an environment and both scopes |
| Compatibility / data | additive serialized field; an older reader ignores it and treats the enum as static, which would reject computed values — so a registry with computed enums needs a current reader |
| Concurrency / performance | provider calls are async and per plan or per action; a slow provider slows its queries only |
| Security | a provider listing store keys reveals them to whoever can list alternatives; providers run with the environment's rights, like commands |
| Recovery | revert; no stored data depends on it |
| Certainty | high on the hooks; the validate-only rule (question 1) is the decision the plan rests on |
