# Phase 3: Examples & Use-cases — Manager status and a guarded removal

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | Clearing derived summaries safely | the problem example, fixed | AC-1, AC-2 |
| 2 | Example | Reading the manager mode | `GET manager` | AC-6 |
| 3 | Test | `amr70_remove_computed_drops_ready_keeps_version` | computed value dropped like `remove` | AC-1 |
| 4 | Test | `amr71_remove_computed_refuses_source_and_override` | user values untouched, 409 type | AC-2 |
| 5 | Test | `amr72_remove_computed_refuses_key_without_recipe` | no recipe = user data | AC-2 |
| 6 | Test | `amr73_remove_computed_races_set_binary` | no lost user value | AC-3 |
| 7 | Test | `amr74_remove_computed_recipe_without_value_and_directory` | nothing to do; directory and unknown key | AC-4, AC-5 |
| 8 | Test | `aae93_post_remove_computed_ready_returns_recipe` | HTTP 200 envelope | AC-1 |
| 9 | Test | `aae94_post_remove_computed_override_is_409` | HTTP 409, value still served | AC-2 |
| 10 | Test | `aae95_remove_computed_unknown_and_directory` | 404 and 409 with hint | AC-5 |
| 11 | Test | `aae96_get_manager_reports_mode_and_started` | queued and inline managers | AC-6 |
| 12 | Test | `route_presence_for_every_switch_combination` (extended) | switches | AC-7 |

## Example 1: Clearing derived summaries safely

Server with `recipes.yaml` in `notes/` declaring `a.summary.md` from `-R/notes/a.md/-/summarize`.

```text
GET  key/data/notes/a.summary.md            → 200, computed, X-Liquers-Status: Ready
POST key/remove_computed/notes/a.summary.md → 200 {"result":{"removed":true,"new_status":"Recipe"}}
POST key/data/notes/a.summary.md  (body "my own")   → 201, status Override
POST key/remove_computed/notes/a.summary.md → 409 {"error":{"type":"StatusConflict",
      "message":"Cannot remove_computed 'notes/a.summary.md': asset status is Override; use DELETE key/data to remove a user value"}}
GET  key/data/notes/a.summary.md            → 200 "my own"
```

## Example 2: Reading the manager mode

```text
GET manager → 200 {"status":"OK","result":{"eval_mode":"Queued","is_started":true},"message":"Asset manager"}
```

With `ImmediateEnvironment` (`EvalMode::Inline`) the result reads `"Inline"`, telling a client that
`submit` answers with the final status (§5.2 "Inline managers").

## Edge and Error Cases

- **In-flight computed value.** `Processing` is computed: cancelled, then dropped, as `remove` does
  (covered by `amr70` with a slow command, same fixture as `amr04`).
- **Live `None`/`Recipe` placeholder with a stored `Override`.** The stored status decides
  (`live_status_defers_to_store`), so it is refused — `amr71` sets the override, then requests the
  key without awaiting it, then calls `remove_computed`.
- **Legacy metadata.** A stored legacy record's status still decides; no special case.
- **`read_only()`.** The route is absent, `GET manager` present → test 12.
- **Concurrency.** `amr73` runs 50 iterations of `join!(set_binary(user value), remove_computed)`
  on a `Ready` recipe key and asserts the key never ends without the user value when `set_binary`
  returned `Ok`.

## Test Plan

- Integration (core): `liquers-core/tests/asset_manager_remove_expire_describe.rs` — `amr70`
  (AC-1: data gone, stored record `Recipe` with the old version, dependents not expired);
  `amr71` (AC-2: `Source` and `Override`, `ErrorType::StatusConflict`, value and dependents
  unchanged, live asset not cancelled); `amr72` (AC-2: stored `Source` with no recipe);
  `amr73` (AC-3); `amr74` (AC-4, AC-5: recipe key without value → `Ok` and nothing evaluated;
  directory → `StatusConflict`; unknown → `KeyNotFound`).
- Integration (axum): `liquers-axum/tests/assets_api_endpoints.rs` — `aae93` (AC-1),
  `aae94` (AC-2, the hint text and a following `key/data` read), `aae95` (AC-5),
  `aae96` (AC-6, `SimpleEnvironment` → `Queued`, `ImmediateEnvironment` → `Inline`, both
  `is_started: true`).
- Unit (axum): `liquers-axum/src/assets/builder.rs` `route_presence_for_every_switch_combination`
  gains `POST key/remove_computed/x`, `GET key/remove_computed/x` and `GET manager` rows (AC-7).
- Regression: every existing `amr*` and `aae*` test (the moved `remove` body).
- Commands: `cargo test -p liquers-core --test asset_manager_remove_expire_describe`,
  `cargo test -p liquers-axum --lib --tests`.

No Liquers query is written by a client in these examples; the recipe query
`-R/notes/a.md/-/summarize` is fixture text whose command the test registers.

## Learning Log

- A guard that must be atomic with a status decision belongs where the lock is; this is the second
  time (`remove` itself was moved into core by `axum-assets-endpoints` for the same reason).
