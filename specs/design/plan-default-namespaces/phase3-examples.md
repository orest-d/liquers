# Phase 3: Examples & Use-cases — default namespaces for plan building

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | Polars environment from YAML | Configured namespace resolves an unqualified action | AC-1, AC-6 |
| 2 | Example | Chains in the registry, per realm | An environment built in code | AC-3, AC-12 |
| 3 | Test | `liquers_core::plan::tests::configured_namespace_resolves_unqualified_action` | Same plan as with `ns-pl` | AC-1 |
| 4 | Test | `liquers_core::plan::tests::default_chain_is_unchanged` | Unchanged default | AC-2 |
| 5 | Test | `liquers_core::plan::tests::explicit_ns_is_searched_before_defaults` | `lui`, then `pl`, then root; first match | AC-3 |
| 6 | Test | `liquers_core::plan::tests::root_searched_after_configured_namespaces` | Order | AC-4 |
| 7 | Test | `liquers_core::plan::tests::realm_chain_overrides_main_chain` | Per-realm lookup | AC-12 |
| 8 | Test | `liquers_core::plan::tests::append_action_keeps_ns_under_configured_chain` | `ns-pl` kept; shadowed root `x` gets `ns-root`; both resolve with either chain | AC-8 |
| 9 | Test | `liquers_core::environment_config::tests::default_namespaces_round_trip` | Serde | AC-6 |
| 10 | Test | `liquers_core::environment_config::tests::configured_namespaces_take_precedence_over_registry` | Present key wins, absent key keeps the registry chain | AC-6 |
| 11 | Test | `liquers_core::command_metadata::tests::namespace_chain_checks` | Error and warning | AC-7 |
| 12 | Test | `liquers_core::command_metadata::tests::registry_without_realm_chains_serializes_as_before` | Export | AC-11 |
| 13 | Test | `tests/plan_default_namespaces.rs::configured_environment_evaluates_unqualified_query` | End to end, accessor | AC-1, AC-2, AC-6 |
| 14 | Test | `liquers_template::diagnostics::tests::diagnostics_use_realm_chain` | Template agrees | AC-9 |
| 15 | Test | `liquers_lib::utils::tests::next_presets_use_realm_chain` | Presets agree | AC-9 |
| 16 | Test | `liquers_core::validate::tests::default_namespace_flag_resolves_unqualified_action` | CLI flag | AC-10 |
| 17 | Test | `liquers-lib/tests/registry_export.rs` (existing) | Committed registry still fresh | AC-11 |
| 18 | Test | `liquers-py/tests/test_ENVIRON_environment.py::test_default_namespaces_from_config` | Python builder applies the key | AC-6 |

## Example 1: Polars environment from YAML

An application serving mostly DataFrame queries configures `pl` once.

```yaml
store:
  stores:
    - type: filesystem
      prefix: data
      config: { path: ./data }
default_namespaces: [pl]
```

```rust,ignore
let config = EnvironmentConfig::from_yaml(&yaml)?;
let mut builder = EnvironmentBuilder::<Value>::new().with_config(config, Box::new(default_store_factory()));
register_my_commands(&mut builder.command_registry)?;      // registers pl/head among others
let envref = builder.build()?;
assert_eq!(envref.get().default_namespaces(""), ["pl", "", "root"]);

// plans as -R/data/sales.csv/-/ns-pl/head-3
let state = envref.evaluate_immediately("-R/data/sales.csv/-/head-3", None).await?;
```

The same document read by `liquers-py` gives the same result once its builder applies the key
(test 18).

## Example 2: Chains in the registry, per realm

An environment assembled in code, with no `EnvironmentConfig`, sets chains on the registry
directly:

```rust,ignore
let cmr = &mut builder.command_registry.command_metadata_registry;
cmr.set_realm_namespaces("", vec!["pl".into(), "".into(), "root".into()]);   // main realm
cmr.set_realm_namespaces("py", vec!["img".into(), "".into(), "root".into()]); // realm `py`
```

| Query (main chain `[pl, "", root]`) | Action looked up in, first match wins |
|---|---|
| `-R/data/sales.csv/-/head-3` | `pl`, root |
| `-R/data/sales.csv/-/ns-lui/x` | `lui`, `pl`, root |
| `-R/data/sales.csv/-/ns-pl/head-3` | `pl`, `pl`, root — portable to an unconfigured environment |
| an action in a `-py/…` segment | (its `ns`, then) `img`, root |

A configuration document applied to this builder with `default_namespaces: [lui]` would replace the
main chain with `[lui, "", root]` and leave the `py` chain alone.

## Edge and Error Cases

- `default_namespaces: [""]` → `build()` fails, `ConfigurationError` naming the empty entry (test 11).
- `default_namespaces: [plx]` (typo) → build succeeds, one warning naming `plx` (test 11).
- `default_namespaces: [pl, root]` → root second, nothing appended (test 6).
- `default_namespaces: []` → the main chain becomes `["", "root"]` (test 10).
- A two-segment query: each transform segment uses its own last `ns` and its own realm's chain
  (test 7 covers a second segment).
- An unresolved action's error lists the whole chain tried (test 3).

## Test Plan

- Unit: `liquers-core/src/plan.rs` tests 3-8, building a registry by hand with `pl/head`, root
  `to_text`, `lui/x`, a root and a `pl` `x`, and chains set with `set_realm_namespaces`.
- Unit: `liquers-core/src/environment_config.rs` tests 9-10; `command_metadata.rs` tests 11-12;
  `validate` test 16 through the library entry point the binary uses.
- Integration: `liquers-core/tests/plan_default_namespaces.rs` test 13 — `Inline` environment from
  YAML with a memory store, one command registered in namespace `x`.
- Unit: template test 14 and lib test 15.
- Python: test 18, run by `bash scripts/test-liquers-py.sh`.
- Command to run: `cargo test -p liquers-core --lib --tests`, `cargo test -p liquers-template`,
  `cargo test -p liquers-lib --lib --tests`, `bash scripts/test-liquers-py.sh`.

## Learning Log

- The planner already resolves along a first-match chain, `ns` first then
  `default_namespaces` (`namespaces_for_query`, `find_command_in_namespaces`); only the source of
  the chain changes.
- `liquers-validate` reports `command keys tried: '--head'` for `["", "root"]`, so duplicates in the
  chain are already collapsed in the message.
