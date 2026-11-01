# Phase 2: Solution & Architecture — default namespaces for plan building

## Overview

The registry keeps `default_namespaces` as the one chain searched after any `ns`, for every realm,
with its current meaning and default (`["", "root"]`). One new method,
`CommandMetadataRegistry::lookup_namespaces(explicit)`, states the whole rule — the last `ns`'s
namespaces, then `default_namespaces` — and the four places that rebuild the rule today call it.
The builder overwrites `default_namespaces` at `build()` when the configuration (or
`with_default_namespaces`) supplies a list, appending root when it is not listed.
`Environment::default_namespaces()` reads the chain back. The planner's lookup itself (first match
along the chain, `find_command_in_namespaces`) does not change, and the registry gains no field.

The chain lives on the registry because every resolver — `PlanBuilder`, template diagnostics,
`next` presets, `liquers-validate`, a published registry read by a browser — is handed a
`&CommandMetadataRegistry` and nothing else, and because an environment not built from
`EnvironmentConfig` still has a registry. When the registry is organised by realm
(`COMMAND-REGISTRY-NOT-ORGANIZED-BY-REALM`), `lookup_namespaces` gains a realm and the field moves;
the callers already pass through one method.

Rejected:
- **A replacing `implicit_namespaces` list** (first draft): the user chose extend semantics, which
  is today's `ns` rule, so a second list adds nothing.
- **A `realm_default_namespaces` side map** (second draft): deferred by the user to the registry
  reorganisation by realm.
- **Field on `GenericEnvironment` passed into `PlanBuilder`.** Every resolver holding only the
  registry would need a new argument and would silently disagree when one is missed.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `TEMPLATE-DIAGNOSTICS-RESOLVE-NAMESPACES-FROM-THE-WHOLE-QUERY` | closed | P3 | Its per-segment rule is the one `lookup_namespaces` centralises | no | no | Keep its tests green |
| `COMMAND-REGISTRY-NOT-ORGANIZED-BY-REALM` | draft | P3 | Will make the chain per realm; this design keeps one chain behind `lookup_namespaces` so that change stays local | no | no | None |

## Interfaces

```rust
// liquers-core/src/command_metadata.rs — no new field
impl CommandMetadataRegistry {
    /// The namespaces an action is looked up in, highest priority first: `explicit` (the
    /// parameters of the last `ns` before it in its segment), then `default_namespaces`.
    pub fn lookup_namespaces(&self, explicit: &[String]) -> Vec<String>;
}

// liquers-core/src/context.rs — provided method, so no implementor changes
pub trait Environment {
    fn default_namespaces(&self) -> &[String] {
        &self.get_command_metadata_registry().default_namespaces
    }
}

// liquers-core/src/environment_builder.rs
impl EnvironmentBuilder<…> {
    /// Namespaces searched after any `ns`; root is appended when not listed.
    /// Replaces the registry's `default_namespaces` at `build()`.
    pub fn with_default_namespaces<S: Into<String>>(self, namespaces: impl IntoIterator<Item = S>) -> Self;
}

// liquers-core/src/environment_config.rs
pub struct EnvironmentConfig {
    …
    /// Absent: the registry's chain stands. Present: it replaces it (root appended if absent).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_namespaces: Option<Vec<String>>,
}
```

All synchronous: planning is synchronous and the chain is fixed at `build()`.

## Integration Points

- `liquers-core/src/plan.rs` `namespaces_for_query`: keeps its `ns` parsing (non-string is
  `NotSupported`) and calls `lookup_namespaces`. `append_action` returns the
  plain query only when it resolves to the requested namespace with the environment's chain **and**
  with `["", "root"]`; otherwise `ns-<ns>`, or `ns-root` for a requested root command that a
  configured namespace shadows, instead of today's unconditional plain return for root. With the
  default chain both checks coincide and output is unchanged (AC-8).
- `liquers-core/src/environment_builder.rs`: field `default_namespaces: Option<Vec<String>>`;
  `build()` applies it to the registry before `validate()`. `with_config` (in
  `environment_config.rs`) calls `with_default_namespaces` only when the document has the key.
- `liquers-core/src/command_metadata.rs` `CommandMetadataRegistry::check`: empty entry in
  `default_namespaces` → error issue; a namespace used by no command (`root` and `""` exempt) → warning.
- `liquers-core/src/bin/liquers_validate.rs`: repeatable `--default-namespace <ns>` sets the merged
  registry's chain by the builder's rule (the overlay merge already unions `default_namespaces`).
- `liquers-template/src/diagnostics.rs` `lookup_namespaces` and
  `liquers-template/src/query/instantiate.rs` (override lookup): call the registry method.
- `liquers-lib/src/utils.rs` `namespaces_from`: call the registry method.
- `liquers-py/src/environment.rs` `EnvCell::configured_builder`: applies `assets` and `recipes` one
  by one rather than through `with_config`, so add `.with_default_namespaces(…)` when
  `self.config.default_namespaces` is `Some`.

## Error Handling

| Case | Result |
|---|---|
| Empty-string entry | `build()` fails: the registry check's error issue through `IssueReport::to_error`, as for other registry errors (`ErrorType::ConfigurationError` via `Error::configuration_error` where the builder rejects the list itself) |
| Namespace used by no command | Warning issue, emitted at build; build succeeds |
| Non-string `ns` parameter | Unchanged: `Error::not_supported` |
| Action found nowhere | Unchanged `Error::action_not_registered(…, &namespaces)`, listing the chain tried |

## Relevant Commands

No command added or changed; `specs/command_registry.yaml` is unchanged (no new field). Namespaces this interacts with: any non-root one — `pl`, `lui`, `img`, `rec`, `template`,
`mem`.

## Documentation Architecture

| Path | Change |
|---|---|
| `reference/ENVIRONMENT_CONFIG.md` | `default_namespaces` row (precedence over the registry, root appended), YAML sample, `with_config` sentence, build-failure table; History row, `reviewed:` |
| `reference/RECIPES_AND_PLANS.md` | `ns` paragraph: first-match chain, `ns` first then the registry's chain, set from the configuration; History row, `reviewed:` |
| `guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md` | "Default namespaces": configuration vs registry, the portability caveat; History row, `reviewed:` |
| `CLAUDE.md` | `--default-namespace` in the `liquers-validate` table |

`affects_docs` as in `DESIGN.md`. `specs/README.md`: capability line under "Query and evaluation".

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `command_metadata.rs`, `plan.rs`, `context.rs`, `environment_builder.rs`, `environment_config.rs`, `validate/registry.rs`, `bin/liquers_validate.rs`, template `diagnostics.rs` + `query/instantiate.rs`, lib `utils.rs`, py `environment.rs` |
| Crates and workflows affected | core, template, lib, py (one builder call) |
| Existing tests likely to change | Struct-literal constructions of `CommandMetadataRegistry` outside `new()`, if any |
| New validation | Registry check of `default_namespaces` |
| Compatibility | No new registry field; `default_namespaces` keeps its meaning; additive config key. A query's meaning depends on the environment, as recipes already depend on registered commands |
| Persistence | A stored recipe result records the command it ran, but the recipe text does not change with configuration; after a reconfiguration that resolves the query to another command, an `explicit` dependency audit can still accept the stored value. Not verified by test; documented (Phase 1 question 5) |
| Certainty | High: the planner's chain is unchanged, only where the chain comes from |
