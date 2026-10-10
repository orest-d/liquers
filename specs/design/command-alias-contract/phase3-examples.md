# Phase 3: Examples & Use-cases — Command alias contract

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | `pl/head` as an alias of `pl/slice` | Registration and use | AC-1, AC-2, AC-10, AC-11 |
| 2 | Example | Reading an aliased plan | Provenance and dependencies | AC-12 |
| 3 | Test | `command_alias::alias_head_fills_target_leading_argument` | Head named by the target | AC-1 |
| 4 | Test | `command_alias::alias_parameters_follow_head` | Own parameters, arity error | AC-2 |
| 5 | Test | `command_alias::alias_executes_target` | Alias result = written-out target | AC-9 |
| 6 | Test | `command_alias::alias_variadic_collects_after_head` | `multiple` through an alias | AC-7 |
| 7 | Test | `command_alias::alias_execution_error_names_alias` | `with_alias` in errors | AC-9, AC-12 |
| 8 | Test | `command_alias::alias_plan_depends_on_alias_metadata` | Dependency keys | AC-12 |
| 9 | Test | `command_alias::register_alias_copies_target_flags` | Flags, state argument, definition | AC-10, AC-8 |
| 10 | Test | `command_alias::register_alias_rejects_inconsistent_shape` | Registration errors | AC-10, AC-3, AC-5, AC-6 |
| 11 | Test | `command_alias::step_is_volatile_when_alias_is_volatile` | `IsVolatile` reads the alias | AC-8 |
| 12 | Test | `command_alias::hand_built_alias_is_planned_without_shape_check` | Deserialized aliases (liquers-py layout) | AC-1 |
| 13 | Test | `command_metadata::tests::alias_target_*` (5 tests) | Shared validation | AC-3, AC-4, AC-5, AC-6 |
| 14 | Test | `plan::tests::alias_planning_inherits_target_volatility` | Volatility, payload, expiry union | AC-8 |
| 15 | Test | `plan::tests::alias_errors_surface_at_plan_time` | Plan-time failure | AC-5 |
| 16 | Test | `plan::tests::accepted_count_excludes_head_parameters` (rewritten) | Excess count is the alias's | AC-2 |
| 17 | Test | `plan::tests::action_origin_serialization` | Omitted when `Direct`, round-trips | AC-12 |
| 18 | Test | `polars_commands::test_head_alias_matches_slice` | Production alias executes | AC-1, AC-9 |
| 19 | Test | `registry_export::pl_head_is_exported_as_alias_of_slice` | Exported, round-trips | AC-11 |

## Example 1: `pl/head` as an alias of `pl/slice`

`pl/head(n = 5)` stops being an implementation and becomes a binding: `slice` with `offset = 0`.
In `register_polars_selection_commands!`, after `slice` is registered:

```rust
let head = $cr.register_alias(
    CommandKey::new("", "pl", "head"),
    CommandKey::new("", "pl", "slice"),
    vec![CommandParameterValue::Value(0.into())],
    vec![ArgumentInfo::integer_argument("n", false).with_default(5)],
)?;
head.with_label("Get first rows").with_doc("Return first N rows (default: 5)");
```

| Query | Plans to |
|---|---|
| `ns-pl/head` | `slice [offset = 0, length = 5]`, origin `Alias pl/head` |
| `ns-pl/head-2` | `slice [offset = 0, length = 2]` — same rows as `ns-pl/slice-0-2` |
| `ns-pl/head-1-2` | Error: *Too many parameters for command 'head': accepts 1, but parameter #2 '2' was supplied* |

All three parse as written (`liquers-validate --no-registry`); the first two plan today against an
overlay only with the wrong result shown in Phase 1, which is what tests 3, 4 and 18 pin.

## Example 2: Reading an aliased plan

`make_plan(envref, "text-abcdef/first-3")` in the core test environment, where `first` is an alias
of `pick` with head `0`. The action step, as JSON:

```json
{"Action": {"realm": "", "ns": "", "action_name": "pick", "position": {"offset": 12, "…": "…"},
  "parameters": [{"DefaultValue": ["offset", 0]},
                 {"ParameterValue": ["n", 3, {"offset": 18, "…": "…"}]}],
  "origin": {"Alias": {"command": {"realm": "", "namespace": "", "name": "first"}}}}}
```

The head is named by the target (`offset`); the user's parameter keeps the alias's name (`n`) and
its query position. `plan.dependencies` holds `ns-dep/…/first` (metadata) and `pick`'s metadata and
implementation keys. The same plan with `pick-0-3` written out has no `origin` key at all.

## Edge and Error Cases

| Symptom | Expected | Test |
|---|---|---|
| Head longer than the target's arguments | `ParameterError` naming alias and target, at registration and planning | 10, 13 |
| Head on an injected argument | `ParameterError` naming the argument | 13 |
| Target missing | `ActionNotRegistered` at plan time, not at execution | 10, 13, 15 |
| Alias of an alias; alias key = target key | `NotSupported` | 10, 13 |
| Alias metadata says non-volatile, target is volatile | Plan volatile, payload and expiry taken from target | 14 |
| Alias volatile, target not (hand-built) | Step and plan volatile | 11 |
| Deserialized alias whose arguments do not line up (liquers-py) | Planned; positional mismatch is the executor's concern | 12 |
| Plan serialized before this change | Loads with `origin: Direct` | 17 |
| Unaliased plan | Serializes without `origin` | 17 |
| Feature `polars` off | Tests 18-19 compile out (file- and test-level gates) | build matrix |

## Test Plan

- **Unit, `liquers-core/src/command_metadata.rs`:** `alias_target_is_none_for_registered`,
  `alias_target_rejects_missing_target` (AC-5), `alias_target_rejects_chained_alias` (AC-6),
  `alias_target_rejects_overlong_head` (AC-3), `alias_target_rejects_head_on_injected_argument`
  (AC-4). Registries are built with `add_command`, as a deserialized one would be.
- **Unit, `liquers-core/src/plan.rs`:** tests 14-17 above. Test 14 builds a target with
  `volatile`, `payload_required: Required` and an expiry, and an alias with none, then asserts
  `plan.is_volatile`, `plan.payload_required` and `plan.expires`.
- **Integration, `liquers-core/tests/command_alias.rs` (new):** `SimpleEnvironment<Value>` with test
  commands `text(value: String)`, `pick(state, offset: i32, length: i32)` (character range),
  `tag(state, prefix: String, items: Vec<String> multiple)`, `fail(state, reason: String)` and a
  volatile `ticker()`; aliases `first` → `pick` (head `0`, own `n = 2`), `tagged` → `tag` (head
  `"#"`), `broken` → `fail` (head `"boom"`). Queries: `text-abcdef/first`, `text-abcdef/first-3`,
  `text-abcdef/first-1-2`, `text-abcdef/pick-0-3`, `text-x/tagged-a-b-c`, `text-x/broken` — each
  parse-checked with `liquers-validate --no-registry`. Tests 3-12.
- **Integration, `liquers-lib/tests/polars_commands.rs`:** `test_head_alias_matches_slice` (AC-1,
  AC-9): `eval_over_csv(…, "head-2")` equals `"slice-0-2"`; `"head"` on three rows returns three.
- **Integration, `liquers-lib/tests/registry_export.rs`:** `pl_head_is_exported_as_alias_of_slice`
  (AC-11), `#[cfg(feature = "polars")]`: definition, head and arguments survive a YAML round trip;
  the existing `committed_registry_is_fresh` and `committed_registry_impl_versions_are_fresh` pass
  after regeneration.
- **Commands:**
  `cargo test -p liquers-core --lib alias` ·
  `cargo test -p liquers-core --test command_alias` ·
  `cargo test -p liquers-lib --test polars_commands --test registry_export` ·
  `bash scripts/check-build-matrix.sh`

## Learning Log

- `zip` made the old over-long-head case silent; a length check must come before any zip.
- `CommandMetadataRegistry::add_command` replaces an entry with the same key; it does not fail.
- The repo's no-`_ =>` rule protects exhaustive matches only. `if let` / `matches!` "is this an
  action" tests are why a new `Step` variant was unsafe for aliased actions.
- Existing `polars_commands.rs` tests mostly call polars directly; `eval_over_csv` is the helper
  that goes through the command path, and the alias test uses it.
