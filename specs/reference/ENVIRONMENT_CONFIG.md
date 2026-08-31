---
title: Environment Configuration
kind: reference
audience: both
area: [core/context, core/store, core/assets]
reviewed: 2026-08-31
---
# Environment Configuration

`EnvironmentConfig` (`liquers-core/src/environment_config.rs`) describes an environment's services
in one serde document, so an application or a language binding can be set up from a file rather
than from Rust. It embeds `StoreRouterConfig` verbatim, so one document configures the environment
**and** its store.

For the task-oriented walkthrough see
[Building and Configuring an Environment](../guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md); this page is
the field-by-field reference.

## Scope: services, not commands

A configuration configures *services*. Commands are Rust functions registered by a macro and no
document can name one, so command registration stays in code. `EnvironmentBuilder` splits along
exactly that line — the `with_*` setters are the config-drivable half, the public
`command_registry` field is the code-only half.

## Format

```yaml
store:                          # StoreRouterConfig, verbatim
  stores:
    - type: filesystem
      prefix: data
      config:
        path: ${LIQUERS_DATA}
    - type: memory
      prefix: tmp
recipes: default                # default | trivial
assets:
  job_capacity: 8               # queued managers only
```

| Field | Type | Default when absent | Meaning |
|---|---|---|---|
| `store` | `StoreRouterConfig` | empty router | Store list and routing prefixes. See [Store Configuration](./STORE_CONFIG_FSD.md); the format is not restated here. |
| `recipes` | `RecipeProviderChoice` | **`default`** | `default` reads recipes through the store; `trivial` resolves none. Aliases `none` and `no_recipes` are accepted for `trivial`. |
| `assets` | `AssetManagerOptions` | all unset | Per-manager settings. `job_capacity` sets the queued manager's job-queue size; **must be at least 1**. |

Every field has a serde default, so a document may configure one section and omit the rest, and a
field added later does not break an existing document. Unknown keys are currently **ignored**
(`deny_unknown_fields` is not set).

## Constructors

| Method | Notes |
|---|---|
| `from_yaml`, `from_json` | Always available |
| `from_toml` | Behind the `toml` feature, matching `StoreRouterConfig` |
| `to_yaml`, `to_json` | Round-trip |
| `expand_env_vars` | Expands `${VAR}` in the store section; called by `build()` |

## Applying it

```rust,ignore
let config = EnvironmentConfig::from_yaml(&yaml)?;

let mut builder = EnvironmentBuilder::<Value>::new()
    .with_config(config, Box::new(default_store_factory()));
register_my_commands(&mut builder.command_registry)?;
let envref = builder.build()?;
```

`with_config` is equivalent to `with_store_config` + `with_recipe_provider_choice` +
`with_asset_manager_options`, and reads in the same direction as every other setter, so a document
and hand-written configuration compose in either order — document first then overridden in code, or
the reverse.

Store construction and `${VAR}` expansion are **deferred to `build()`**. That is what keeps the
setters infallible and chainable; three failures surface at `build()` instead:

| Failure | Error |
|---|---|
| Store type no factory in the chain claims | Names the type, and lists the types the chain supports |
| `${VAR}` referencing an unset variable | The expander has no default-value syntax, so this is an error rather than an empty value |
| Both a store and a store configuration supplied | Rejected rather than resolved by a silent precedence rule |
| `assets.job_capacity: 0` | Rejected. The queue starts work only while `running_count < capacity`, so zero would accept every evaluation and run none — a hang with no error, which is strictly worse than a rejected configuration |
| `assets.job_capacity` set against an inline kind | Rejected. `Inline` has no job queue, and silently ignoring the setting would hide the mistake |

Use `with_store_config_unexpanded` where there are no environment variables to expand — a browser
page — mirroring `StoreRouterBuilder::build_without_env_expansion`.

## Two deliberate omissions

**The asset-manager kind is not a field.** A string cannot select a type: `"queued"` and
`"inline"` produce two different concrete environment types, and `Environment` is not object-safe
(associated types, `Sized`), so they cannot be erased behind a `dyn`. The choice is a *build* fact
rather than a deployment one — wasm has no choice at all, and natively `Inline` exists for
deterministic testing rather than production tuning. `DefaultKind` gets it right on both targets.
An application that genuinely wants runtime selection monomorphizes its own tail with an explicit
match.

**The store factories are not a field.** Which backends exist is a build fact for the same reason:
`liquers-core` supplies memory and filesystem, `liquers-store` chains OpenDAL onto them, and
`liquers-web` chains its own. The factory reaches the builder as an argument, and the document
names store *types* the chain is expected to resolve.

## The `recipes` default is not the builder's default

This is the one field whose absence changes behaviour in a way worth stating twice.

| Situation | Recipe provider |
|---|---|
| `EnvironmentBuilder::new()` with no configuration | `Trivial` — resolves no recipes |
| `EnvironmentConfig` with no `recipes:` key, applied | **`Default`** — reads recipes through the store |
| `liquers_lib::default_environment_builder()` | `Default` |

`RecipeProviderChoice`'s `#[default]` is the *document* default, chosen on the grounds that a
configuration saying nothing about recipes most plausibly wants them to work. `liquers-core`'s
builder has no opinion and resolves nothing. So applying even an empty configuration is an explicit
act that changes how `-R/` queries resolve. Pinned by
`environment_config::tests::an_absent_recipes_key_means_default_not_trivial`.

## Related

- Guide: [Building and Configuring an Environment](../guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md)
- Reference: [Store Configuration](./STORE_CONFIG_FSD.md)
- Reference: [DOC-04 Environment, Context and Evaluation](./api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md)
- Design: [`design/environment-builder/`](../design/environment-builder/)

## History

| Date | Change | Source |
|---|---|---|
| 2026-08-31 | Created with `EnvironmentConfig`: fields, constructors, deferred failures, the two deliberate omissions, and the `recipes`-absent asymmetry. | `design/environment-builder/phase-5` |
