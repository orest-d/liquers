# Phase 3: Examples & Use-cases — liquers-py-recipes

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | Reports named by a pattern | The Phase 1 problem example in the chain | AC-1, AC-3 |
| 2 | Example | One factory, two entries | SVG to PNG and SVG to JPG from one type | AC-6, AC-12 |
| 3 | Test | `test_RECIPE_python_providers.py` | Protocol, errors, listing, threads, inspection | AC-1-AC-9 |
| 4 | Test | `test_RECIPE_registration.py` | Declaration, repeats, late registration, release | AC-5, AC-10, AC-12 |
| 5 | Test | `liquers-core` `recipes::tests`, `tests/recipe_provider_chain_config.rs` | The duplicate rule, member names, the plan constructor | AC-7, AC-12 |

The struck Phase 1 scenario was replaced by AC-12 and has no test.

## Example 1: Reports named by a pattern

A program names one report per customer; no `recipes.yaml` could list them. `Reports.recipe_opt`
answers for its folder, the chain asks it beside `recipes_yaml`, and the key's recipe runs.

```python
class Reports:
    def __init__(self, folder): self.folder = folder
    def recipe_opt(self, key):
        if str(key.parent()) != self.folder:
            return None                                      # not mine: cheap and silent
        if key.filename() == "fresh.txt":
            return lq.Recipe("hello", volatile=True, expires=lq.Expires("in 5 min"))
        return f"hello/{key.filename()}"                     # cwd: the key's folder

env = lq.Environment(config={"recipes": ["recipes_yaml", {"type": "reports", "folder": "reports"}]})
env.register_recipe_provider_factory("reports", Reports)
register_fixture_commands(env)                               # hello -> "hello"
assert env.evaluate("-R/reports/summary.txt") == "hello"
assert env.recipes.recipe(lq.parse_key("reports/summary.txt")).cwd == "reports"
assert json.loads(env.recipes.recipe_plan(lq.parse_key("reports/fresh.txt")).to_json())["is_volatile"]
with pytest.raises(lq.AssetNotFoundError):
    env.evaluate("-R/elsewhere/x.txt")
```

## Example 2: One factory, two entries

The same type listed twice with different arguments makes two members (question 2). Each lists
what it makes, so the chain's listing and containment agree (`RECIPE02`).

```python
class Convert:
    def __init__(self, folder, ext, command):
        self.folder, self.ext, self.command = folder, ext, command
    def recipe_opt(self, key):
        name = key.filename()
        if str(key.parent()) == self.folder and name.endswith(self.ext):
            return f"-R/src/{name[:-len(self.ext)]}.svg/-/{self.command}"
    def assets_with_recipes(self, folder):
        return [f"logo{self.ext}"] if str(folder) == self.folder else []

env = lq.Environment(config={"recipes": [
    {"type": "convert", "folder": "art", "ext": ".png", "command": "to_png"},
    {"type": "convert", "folder": "art", "ext": ".jpg", "command": "to_jpg"}]})
env.register_recipe_provider_factory("convert", Convert)
assert sorted(map(str, env.recipes.assets_with_recipes(lq.parse_key("art")))) == ["art/logo.jpg", "art/logo.png"]
assert env.recipes.contains(lq.parse_key("art/logo.png"))
```

Listing the first entry twice, unchanged, fails the build with `ConfigurationError` naming the
duplicate; two entries answering one key fail that lookup naming `convert#0` and `convert#1`.

## Edge and Error Cases

| Symptom | Expected | Test |
|---|---|---|
| `recipe_opt` returns a `dict` or an `int` | `ExecutionError` naming the type | `test_RECIPE_return_forms` |
| `recipe_opt` raises `ValueError` / `lq.KeyNotFoundError` | `ExecutionError` with the traceback / `KeyNotFoundError` | `test_RECIPE04_*` |
| Two members answer one key | `ConfigurationError` naming both | `test_RECIPE_not_mine_and_ownership_conflict` |
| Provider without `assets_with_recipes` | lists nothing, `contains` false, its keys evaluate | `test_RECIPE_unlisted_provider_still_resolves` |
| A listed name outside `folder` | `ExecutionError` | same |
| Factory lacks a required argument, gets an unknown one, raises, returns no `recipe_opt`, evaluates its environment | `ConfigurationError` naming the entry, within a timeout | `test_RECIPE_factory_and_protocol_checked` |
| Name taken (`manifest`, `default`, a second `convert`) | refused at registration | same |
| Callback evaluates through its environment (`def`, `async def`) | completes, no deadlock | `test_RECIPE07_*` |
| `env.recipes` after `shutdown()` | `shut_down_error()` | `test_RECIPE_late_registration_and_release` |

## Test Plan

Python (`liquers-py/tests/`):

- `test_RECIPE_python_providers.py`
  - `test_RECIPE06_end_to_end_keyed_evaluation` (AC-1): Example 1's query and the unowned key.
  - `test_RECIPE_not_mine_and_ownership_conflict` (AC-2): `None` defers to `recipes_yaml`; two
    providers claiming `reports/x.txt` raise naming both.
  - `test_RECIPE_return_forms` (AC-3): `lq.Recipe`, a string, a recipe with its own `cwd`; the
    refused types.
  - `test_RECIPE04_provider_error_maps_through_error` (AC-4): error types and the traceback text.
  - `test_RECIPE02_list_and_contains_are_consistent` (AC-6): Example 2's listing; every listed key
    is contained and has a recipe; an unlisted `art/other.png` is addressable.
  - `test_RECIPE_unlisted_provider_still_resolves` (AC-6): question 1.
  - `test_RECIPE03_recipe_produces_a_valid_plan` (AC-7): `recipe_plan` of `reports/summary.txt`
    has a `SetCwd`, the `hello` action and `Filename[summary.txt]`.
  - `test_RECIPE05_volatility_and_expiration_metadata_survive` (AC-7): the `fresh.txt` recipe
    keeps `volatile` and `expires`; its plan is volatile; the evaluated asset's metadata shows an
    expiry.
  - `test_RECIPE07_nested_environment_use_follows_policy[def|async]` (AC-8): `recipe_opt`
    evaluates `hello` through its own `lq.Environment` before answering; `timeout=5`.
  - `test_RECIPE01_found_and_missing_recipe` (AC-9): `recipe_opt` found and `None`.
  - `test_RECIPE_inspection_methods_and_async_twins` (AC-9): `recipe`, `recipe_plan`,
    `assets_with_recipes`, `contains`, `can_make` and each `_async` twin under `run_async`.
- `test_RECIPE_registration.py`
  - `test_RECIPE_factory_and_protocol_checked` (AC-5): the table's failures and refused names.
  - `test_RECIPE_late_registration_and_release` (AC-10): a document naming `reports` fails; after
    registering it the next `evaluate` builds, with an earlier `env.store.set` kept;
    `live_handle_count()` back to baseline after `shutdown()`.
  - `test_RECIPE_one_factory_several_entries` (AC-12): Example 2, the identical-entry refusal and
    the `convert#0` / `convert#1` naming.
- `liquers-core` (`src/recipes.rs` tests; `tests/recipe_provider_chain_config.rs`):
  - `expand_recipes_refuses_an_identical_entry` (AC-12) — the former "listed more than once" test.
  - `the_same_type_with_different_arguments_makes_two_members` (AC-12).
  - `repeated_type_members_are_named_by_position` (AC-12); `named_chain_reports_member_names`
    unchanged.
  - `an_appended_entry_is_skipped_when_its_type_is_listed` (AC-12): `with_records_recipe_provider`
    after a document listing `manifest` keeps one member (`liquers-lib/tests/`).
  - `plan_with_dependencies_matches_the_default_provider` (AC-7): the same recipe gives equal plans
    through `DefaultRecipeProvider::recipe_plan` and `Recipe::plan_with_dependencies`.
- Commands: `cargo test -p liquers-core recipes`, `cargo test -p liquers-lib --test
  recipe_provider_chain_config`, `bash scripts/test-liquers-py.sh -k RECIPE`.

Queries validated (`--command hello --command to_png --command to_jpg`): `-R/reports/summary.txt`,
`hello/summary.txt`, `-R/src/logo.svg/-/to_png`, `-R/art/logo.png`, `-R/art/logo.jpg`.

## Learning Log

- `lq.Plan` exposes only JSON; tests read `is_volatile` and `expires` from `to_json()`. A typed
  accessor is not in scope.
- A provider answering `None` for foreign keys is what keeps the chain usable: every member is
  asked for every key, so the guide's first rule is "answer `None` fast".
- Appendix A's `p.list(folder)` is `assets_with_recipes` here, Orest's name for the listing method.
