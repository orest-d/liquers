# Phase 3: Examples & Use-cases — default namespaces for plan building

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | Polars environment from YAML | Configured namespace resolves an unqualified action | AC-1, AC-6 |
| 2 | Example | Escaping the implicit list | Explicit and bare `ns` | AC-3, AC-5 |
| 3 | Test | `liquers_core::plan::tests::implicit_namespace_resolves_unqualified_action` | Same plan as with `ns-pl` | AC-1 |
| 4 | Test | `liquers_core::plan::tests::no_implicit_namespaces_searches_root_only` | Unchanged default | AC-2 |
| 5 | Test | `liquers_core::plan::tests::explicit_ns_replaces_implicit_namespaces` | Replace semantics | AC-3 |
| 6 | Test | `liquers_core::plan::tests::root_searched_after_implicit_namespaces` | Order | AC-4 |
| 7 | Test | `liquers_core::plan::tests::bare_ns_searches_root_only` | Opt-out | AC-5 |
| 8 | Test | `liquers_core::plan::tests::append_action_keeps_ns_under_implicit_namespaces` | `ns-pl` kept; shadowed root `x` gets `ns-root`; both plan to the requested command with and without the list | AC-8 |
| 9 | Test | `liquers_core::environment_config::tests::default_namespaces_round_trip` | Serde | AC-6 |
| 10 | Test | `liquers_core::environment_config::tests::absent_default_namespaces_keeps_builder_setting` | Composition | AC-6 |
| 11 | Test | `liquers_core::command_metadata::tests::implicit_namespace_checks` | Error and warning | AC-7 |
| 12 | Test | `liquers_core::command_metadata::tests::registry_without_implicit_namespaces_serializes_as_before` | Export | AC-11 |
| 13 | Test | `tests/plan_default_namespaces.rs::configured_environment_evaluates_unqualified_query` | End to end, accessor | AC-1, AC-2, AC-6 |
| 14 | Test | `liquers_template::diagnostics::tests::diagnostics_use_implicit_namespaces` | Template agrees | AC-9 |
| 15 | Test | `liquers_lib::utils::tests::next_presets_use_implicit_namespaces` | Presets agree | AC-9 |
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
assert_eq!(envref.get().default_namespaces(), ["pl".to_string()]);

// plans as -R/data/sales.csv/-/ns-pl/head-3
let state = envref.evaluate_immediately("-R/data/sales.csv/-/head-3", None).await?;
```

Lookup list for `head`: `["pl", "", "root"]`. The same document read by `liquers-py` gives the
same result once its builder applies the key (test 18).

## Example 2: Escaping the implicit list

With `default_namespaces: [pl]`:

| Query | `head` / `x` looked up in |
|---|---|
| `-R/data/sales.csv/-/head-3` | `pl`, root |
| `-R/data/sales.csv/-/ns-lui/x` | `lui`, root — not `pl` |
| `-R/data/sales.csv/-/ns/head-3` | root only (fails if `head` is only in `pl`) |
| `-R/data/sales.csv/-/ns-pl/head-3` | `pl`, root — portable to an unconfigured environment |

The last row is what a recipe meant to run anywhere should keep writing.

## Edge and Error Cases

- `default_namespaces: [""]` → `build()` fails, `ConfigurationError` mentioning the empty entry (test 11).
- `default_namespaces: [plx]` (typo) → build succeeds, one warning naming `plx` (test 11).
- `default_namespaces: [pl, root]` → `root` searched second, not duplicated in meaning (test 6).
- `default_namespaces: []` in a document after `with_default_namespaces(["pl"])` → cleared (test 10).
- A two-segment query `-R/a/-/head-3/-/-R/b/-/tail-2`: each transform segment without `ns` uses the
  implicit list independently (test 5 covers a second segment).
- Error message for an unresolved action lists the implicit namespaces too (test 3 asserts it).

## Test Plan

- Unit: `liquers-core/src/plan.rs` tests 3-8, building a registry by hand with `pl/head`, root
  `to_text`, `lui/x`, and `implicit_namespaces` set directly.
- Unit: `liquers-core/src/environment_config.rs` tests 9-10; `command_metadata.rs` tests 11-12;
  `validate` test 16 through the library entry point the binary uses.
- Integration: `liquers-core/tests/plan_default_namespaces.rs` test 13 — `Inline` environment from
  YAML with a memory store, one command registered in namespace `x`.
- Unit: template test 14 and lib test 15.
- Python: `liquers-py/tests/` test 18, run by `bash scripts/test-liquers-py.sh`.
- Command to run: `cargo test -p liquers-core --lib --tests`, `cargo test -p liquers-template`,
  `cargo test -p liquers-lib --lib --tests`.

## Learning Log

- Bare `ns` (no parameters) already parses and plans today; `-R/data/sales.csv/-/ns-pl/head-3/ns/to_text`
  validates, which is what makes it a usable opt-out under replace semantics.
- `liquers-validate` reports `command keys tried: '--head'` for `["", "root"]`, so duplicates in the
  lookup list are already collapsed in the message.
