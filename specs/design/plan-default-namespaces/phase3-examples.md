# Phase 3: Examples & Use-cases — default namespaces for plan building

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | Polars environment from YAML | Configured namespace resolves an unqualified action | AC-1, AC-6 |
| 2 | Example | Chain in the registry | An environment built in code | AC-3 |
| 3 | Test | `liquers_core::plan::tests::configured_namespace_resolves_unqualified_action` | Same plan as with `ns-pl` | AC-1 |
| 4 | Test | `liquers_core::plan::tests::default_chain_is_unchanged` | Unchanged default | AC-2 |
| 5 | Test | `liquers_core::plan::tests::explicit_ns_is_searched_before_defaults` | `lui`, then `pl`, then root; first match | AC-3 |
| 6 | Test | `liquers_core::plan::tests::root_searched_after_configured_namespaces` | Order | AC-4 |
| 7 | Test | `liquers_core::plan::tests::second_segment_uses_its_own_ns_then_defaults` | Per-segment `ns`, shared chain | AC-3 |
| 8 | Test | `liquers_core::plan::tests::append_action_keeps_ns_under_configured_chain` | `ns-pl` kept; shadowed root `x` gets `ns-root`; both resolve with either chain | AC-8 |
| 9 | Test | `liquers_core::environment_config::tests::default_namespaces_round_trip` | Serde | AC-6 |
| 10 | Test | `liquers_core::environment_config::tests::configured_namespaces_take_precedence_over_registry` | Present key wins, absent key keeps the registry chain | AC-6 |
| 11 | Test | `liquers_core::command_metadata::tests::namespace_chain_checks` | Error and warning | AC-7 |
| 12 | Test | `liquers_core::command_metadata::tests::registry_export_carries_configured_chain` | Export | AC-11 |
| 13 | Test | `tests/plan_default_namespaces.rs::configured_environment_evaluates_unqualified_query` | End to end, accessor | AC-1, AC-2, AC-6 |
| 14 | Test | `liquers_template::diagnostics::tests::diagnostics_use_configured_chain` | Template agrees | AC-9 |
| 15 | Test | `liquers_lib::utils::tests::next_presets_use_configured_chain` | Presets agree | AC-9 |
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

## Example 2: Chain in the registry

An environment assembled in code, with no `EnvironmentConfig`, sets the chain on the registry
directly, as it can today:

```rust,ignore
builder.command_registry.command_metadata_registry.default_namespaces =
    vec!["pl".into(), "".into(), "root".into()];
```

| Query (chain `[pl, "", root]`) | Action looked up in, first match wins |
|---|---|
| `-R/data/sales.csv/-/head-3` | `pl`, root |
| `-R/data/sales.csv/-/ns-lui/x` | `lui`, `pl`, root |
| `-R/data/sales.csv/-/ns-pl/head-3` | `pl`, `pl`, root — portable to an unconfigured environment |

A configuration document applied to this builder with `default_namespaces: [lui]` replaces the
chain with `[lui, "", root]`; without the key, the chain above stands.

## Edge and Error Cases

- `default_namespaces: [""]` → `build()` fails, `ConfigurationError` naming the empty entry (test 11).
- `default_namespaces: [plx]` (typo) → build succeeds, one warning naming `plx` (test 11).
- `default_namespaces: [pl, root]` → root second, nothing appended (test 6).
- `default_namespaces: []` → the main chain becomes `["", "root"]` (test 10).
- A two-segment query: each transform segment uses its own last `ns`, then the shared chain (test 7).
- An unresolved action's error lists the whole chain tried (test 3).

## Test Plan

- Unit: `liquers-core/src/plan.rs` tests 3-8, building a registry by hand with `pl/head`, root
  `to_text`, `lui/x`, a root and a `pl` `x`, and `default_namespaces` set directly.
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
