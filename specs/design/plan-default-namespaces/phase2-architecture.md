# Phase 2: Solution & Architecture — default namespaces for plan building

## Overview

A new registry field, `CommandMetadataRegistry::implicit_namespaces`, holds the configured list.
One new registry method, `lookup_namespaces`, states the whole search rule — the last `ns` of the
segment if there is one, otherwise the implicit list, then the existing `default_namespaces` root
fallback — and the four places that rebuild that rule today call it instead. The builder writes the
field at `build()` from `with_default_namespaces` or the configuration's `default_namespaces:` key;
`Environment::default_namespaces()` reads it back.

The field lives on the registry, not on `GenericEnvironment`, because every resolver — `PlanBuilder`,
template diagnostics, `next` presets, `liquers-validate`, a published registry read by a browser —
is handed a `&CommandMetadataRegistry` and nothing else. Putting it there makes all of them agree
with no new parameter, and an exported registry carries the rule with it.

Rejected:
- **Prepend to `default_namespaces`.** No new field, but an explicit `ns` could never escape the
  configured list (the *extend* answer to Phase 1 question 1), and the root fallback would be
  rewritten by configuration.
- **Field on `GenericEnvironment` passed into `PlanBuilder`.** Every resolver that has only the
  registry (template, lib presets, validation, web import) would need a new argument and would
  silently disagree when one is missed.
- **Rewriting each query to insert `ns-…`.** Changes query text, cache identity and error positions.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `TEMPLATE-DIAGNOSTICS-RESOLVE-NAMESPACES-FROM-THE-WHOLE-QUERY` | closed | P3 | Its per-segment rule is the one `lookup_namespaces` centralises | no | no | Keep its tests green |
| `WEB-COMMAND-LOOKUP-IGNORES-NAMESPACE` | draft | P2 | Unrelated lookup by bare name; not a namespace search | no | no | None |

## Interfaces

```rust
// liquers-core/src/command_metadata.rs
pub struct CommandMetadataRegistry {
    pub commands: Vec<CommandMetadata>,
    pub default_namespaces: Vec<String>,          // unchanged: the root fallback, ["", "root"]
    /// Searched, in order, for an action with no `ns` before it in its transform segment, ahead of
    /// `default_namespaces`. Empty by default. Set by `EnvironmentBuilder`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub implicit_namespaces: Vec<String>,
    pub global_enums: HashMap<String, EnumArgument>,
}

impl CommandMetadataRegistry {
    /// The namespaces an action is looked up in: `explicit` (the parameters of the last `ns` before
    /// it in its segment) when `Some`, else `implicit_namespaces`; then `default_namespaces`.
    pub fn lookup_namespaces(&self, explicit: Option<&[String]>) -> Vec<String>;
}

// liquers-core/src/context.rs — provided method, so no implementor changes
pub trait Environment {
    /// The namespaces searched when a query names none (empty: root only).
    fn default_namespaces(&self) -> &[String] {
        &self.get_command_metadata_registry().implicit_namespaces
    }
}

// liquers-core/src/environment_builder.rs
pub struct EnvironmentBuilder<…> { …, default_namespaces: Option<Vec<String>>, … }
impl EnvironmentBuilder<…> {
    pub fn with_default_namespaces<S: Into<String>>(self, namespaces: impl IntoIterator<Item = S>) -> Self;
}

// liquers-core/src/environment_config.rs
pub struct EnvironmentConfig {
    …
    /// Absent: leave the builder's setting alone. `[]`: none (root only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_namespaces: Option<Vec<String>>,
}
```

All synchronous: planning is synchronous and the list is fixed at `build()`.

Naming: the user-facing name is **default namespaces** (config key, setter, accessor). The registry
field is `implicit_namespaces` because `default_namespaces` there is already the serialized name of
the root fallback; renaming it would change every exported registry and is out of scope.

## Integration Points

- `liquers-core/src/plan.rs` `namespaces_for_query`: keep the `ns` parameter parsing (non-string is
  `NotSupported`), then `cmr.lookup_namespaces(explicit.as_deref())`. `append_action` resolves the
  "plain" candidate with `lookup_namespaces` on a clone whose `implicit_namespaces` is empty — in
  practice a private helper taking a flag — so the emitted query carries `ns-<ns>` (AC-8).
- `liquers-core/src/environment_builder.rs` `build()`: before `validate()`, copy
  `self.default_namespaces` into `command_registry.command_metadata_registry.implicit_namespaces`
  when `Some`. `with_config` (in `environment_config.rs`) calls `with_default_namespaces` only when
  the document has the key.
- `liquers-core/src/command_metadata.rs` `CommandMetadataRegistry::check`: empty entry → error
  issue; entry used by no command's `namespace` in any realm (`root` and `""` exempt) → warning.
- `liquers-core/src/validate/registry.rs` merge: union `implicit_namespaces` in order, as for
  `default_namespaces`; `liquers-core/src/bin/liquers_validate.rs`: repeatable
  `--default-namespace <ns>`, appended to the merged registry's `implicit_namespaces`.
- `liquers-template/src/diagnostics.rs` `lookup_namespaces` and
  `liquers-template/src/query/instantiate.rs` (override lookup): call the registry method.
- `liquers-lib/src/utils.rs` `namespaces_from`: call the registry method.
- `liquers-py`: none — `EnvironmentConfig` documents pass through.

## Error Handling

| Case | Result |
|---|---|
| Empty-string entry | `build()` fails: the registry check's error issue, surfaced through `IssueReport::to_error` as today, plus `Error::configuration_error` where the config is applied directly |
| Entry naming no registered namespace | Warning issue, emitted at build; build succeeds |
| Non-string `ns` parameter | Unchanged: `Error::not_supported` |
| Action found nowhere | Unchanged `Error::action_not_registered(…, &namespaces)`, whose list now includes the implicit namespaces |

## Relevant Commands

No command added or changed; `specs/command_registry.yaml` is unchanged (the new field is skipped
when empty). Namespaces this interacts with: any non-root one — `pl`, `lui`, `img`, `rec`,
`template`, `mem`.

## Documentation Architecture

| Path | Change |
|---|---|
| `reference/ENVIRONMENT_CONFIG.md` | `default_namespaces` row in the field table, the YAML sample, the `with_config` equivalence sentence, the build-failure table; History row, `reviewed:` |
| `reference/RECIPES_AND_PLANS.md` | `ns` paragraph: the implicit list, replace semantics, bare `ns`, root last; History row, `reviewed:` |
| `guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md` | Short section "Default namespaces", including the portability caveat; History row, `reviewed:` |
| `CLAUDE.md` | `--default-namespace` in the `liquers-validate` table |

`affects_docs` as in `DESIGN.md`. `specs/README.md`: capability line under "Query and evaluation".

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `command_metadata.rs`, `plan.rs`, `context.rs`, `environment_builder.rs`, `environment_config.rs`, `validate/registry.rs`, `bin/liquers_validate.rs`, template `diagnostics.rs` + `query/instantiate.rs`, lib `utils.rs` |
| Crates and workflows affected | core, template, lib; Python only through config |
| Existing tests likely to change | Struct-literal constructions of `CommandMetadataRegistry`, if any outside `new()`; validate report tests that print the registry |
| New validation | Registry check for implicit namespaces |
| Compatibility | Additive serde field with default and skip; no change when unset. A query's meaning now depends on the environment, as recipes already depend on registered commands |
| Persistence | A stored recipe result records the command it ran (`Step::Action` keeps the resolved `ns`), but the recipe text does not change with configuration: after a reconfiguration that resolves the query to another command, an `explicit` dependency audit can still accept the stored value. Not verified by test; documented rather than prevented (Phase 1 question 5) |
| Performance | One `Vec` clone per lookup, as today |
| Recovery | Remove the key from the configuration |
| Certainty | High for the mechanism; question 1 changes only `lookup_namespaces`' branch |
