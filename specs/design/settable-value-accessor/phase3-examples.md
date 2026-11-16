# Phase 3: Examples & Use-cases — settable-value-accessor

## Overview Table
| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | Edit one argument of a recipe in `recipes.yaml` | projection chain from a query, get, describe, set, write-through | AC-1, AC-2, AC-11 |
| 2 | Example | The same edit from Python and JavaScript | the language wrappers, tokens, events | AC-24, AC-5, AC-16 |
| 3 | Test | `liquers-core/src/accessor/{mod,hub}.rs` `tests::*` | tokens; the hub over memory and custom roots | AC-4, AC-5, AC-13, AC-14, AC-15, AC-18, AC-19, AC-22 |
| 4 | Test | `liquers-core/tests/value_accessor.rs` | the first-class roots and the query steps in an environment | AC-1–AC-3, AC-6–AC-8, AC-10, AC-16, AC-20, AC-21, AC-23 |
| 5 | Test | `liquers-lib/tests/value_accessor_commands.rs` | the `acc` commands, the value variant, documents and records | AC-1, AC-9, AC-11, AC-12, AC-15, AC-20 |
| 6 | Test | `liquers-axum/tests/accessor_api.rs` | the `/api/acc` routes and socket | AC-13, AC-17, AC-23 |
| 7 | Test | `liquers-py/tests/test_ACCESSOR_use.py`, `liquers-web/tests/accessor_ACCESSOR.rs` | the language wrappers (`ACCESSOR01`–`ACCESSOR07`) | AC-24 and the scenarios each test names |

## Example 1: Edit one argument of a recipe in `recipes.yaml`

`reports/recipes.yaml` in the store holds a title and one recipe whose query is
`-R/data/sales.csv/-/ns-pl/from_csv/is_in-region-EU-US/head-10/top.csv`. A query editor wants the
`n` of `head`, element 3 of the pipeline (0 is the resource). One query builds the whole chain:

```rust
let q = "-R-key/reports/recipes.yaml/-/ns-acc/accessor/recipe-0/query/action-3/argument-n";
let acc = evaluate(envref.clone(), q, None).await?.value()?.try_into_accessor()?; // ExtValue::Accessor
let snap = acc.get().await?;                                  // Value::I64(10)
let part = acc.describe().await?;                             // type_identifier "I64"; argument_type int,
                                                              // command pl/head, gui_info IntegerField
acc.set(Value::from(20i64), SetOptions::default()).await?;     // writes recipes.yaml once
```

Expected result: `reports/recipes.yaml` now holds the query with `head-20`, the title and every
other key are unchanged (comments are not kept, AC-2 as narrowed), the key's status is `Source`
(it has no recipe of its own), and evaluating `q` again returns a live accessor of the same hub,
not a copy, because the command result is a handle, which is never stored.
`snap.fingerprint.token()` is the token that a later `set_if` or an HTTP `If-Match` would carry.
Every query above validates (`liquers-validate --command acc/accessor --command acc/recipe …`).

## Example 2: The same edit from Python and JavaScript

What it adds: the wrappers of AC-24, the token, and the holder's reaction to events.

```python
acc = env.accessor("-R-key/reports/recipes.yaml/-/ns-acc/accessor/recipe-0/query/action-3/argument-n")
snap = acc.snapshot()                      # snap.value == 10, snap.token is a str
with acc.on(lambda e: seen.append(e.kind)):
    acc.set_if(20, snap.token)             # returns the new token
    acc.process_events()                   # seen == ["changed"]
try:
    acc.set_if(30, snap.token)             # the old token
except lq.StatusConflictError as e:
    assert e.token == acc.info().token     # parsed from the conflict's message
```

```javascript
const acc = await liquers.accessor("-R-key/reports/recipes.yaml/-/ns-acc/accessor/recipe-0/query/action-3/argument-n");
const sub = acc.subscribe(e => seen.push(e.event));
const token = (await acc.snapshot()).token;
await acc.setIf(20, token);               // seen: ["changed"] after the driver ticks
sub.dispose();
```

## Edge and Error Cases

| Symptom | Expected behaviour | Test |
|---|---|---|
| Set `n` to `"abc"`, or set an argument `head` does not have | the `ActionEditor` error; `recipes.yaml` is not written | `refused_argument_set_writes_nothing` |
| A step is inserted before element 3 by another accessor | the action accessor's sink fires and its asset expires; the query accessor gets `Changed`; re-evaluating gives the new element 3 | `structural_change_invalidates_and_content_change_keeps`, `structural_edit_expires_action_accessor_asset` |
| A file is written straight to the store | a store root sees it on `refresh`, not by notification (AC-6 as narrowed) | `store_root_sees_a_direct_write_on_refresh` |
| An expired keyed asset is replaced by `get` without `Removed` | the root re-subscribes to the new `AssetRef` (by `id()`) | `expired_asset_get_re_evaluates` |
| A read-back `Expired` or `Error` status | never written back by a set | `keyed_asset_set_overrides_sources_or_creates` |
| A cached accessor keeps the environment alive | it does not: roots hold it weakly | `accessor_does_not_keep_the_environment_alive` |
| Empty memory `get`; a projection set on an absent root; a parent a step cannot parse | `not_available`; `not_available`; `conversion_error` | `hub_error_cases` |
| Two sets based on one fingerprint | the first applies, the second is `StatusConflict` naming the current token | `first_writer_wins_on_one_part` |
| Two sets of different recipes based on one root version | both apply | `disjoint_parts_do_not_conflict` |
| A key that does not exist | status `None`, `get` is not found, a set creates it, `set_if(…, None)` on an existing key conflicts | `store_views_and_a_missing_key`, `keyed_asset_set_overrides_sources_or_creates` |
| Metadata-view set on a missing key | `KeyNotFound`, nothing written | `store_views_and_a_missing_key` |
| A burst of progress hides `Changed` | `info().fingerprint` differs from the drawn one | `progress_and_log_are_relayed` |
| Persisting an accessor value | `NonSerializable`; never stored | `accessor_value_is_never_stored` |
| An accessor crosses Python or JavaScript and back | the same `Arc` (pointer-equal in Rust) | `test_ACCESSOR06…`, `ACCESSOR06` |
| `PUT` without `If-Match` or `If-None-Match` | `428`; read-only accessor `405` | `put_preconditions_and_conflicts` |
| wasm build | the hub spawns nothing; `driver()` is spawned by liquers-web | `check-build-matrix.sh` (wasm rows), `ACCESSOR03` in the Node loop |

## Test Plan

**Core unit, `liquers-core/src/accessor/mod.rs` `mod tests`:**
- `fingerprint_token_round_trips` (AC-13): `from_token(token())`; a malformed token is an error.

**Core unit, `liquers-core/src/accessor/hub.rs` `mod tests`** (memory and test roots and test
`FieldStep` / `ItemStep`, no environment):
- `memory_accessor_is_a_shared_variable` (AC-22): an empty variable has status `None` and `get`
  fails; a set through one clone sends `Changed` to a subscriber of another clone, which reads it.
- `overlapping_projections_stay_in_step` (AC-4): a whole-object and a field projection of one root;
  setting the field notifies the whole-object holder and vice versa.
- `first_writer_wins_on_one_part` (AC-5): two `set_if` with one fingerprint; second is
  `ErrorType::StatusConflict` with the current token; both then read the first value.
- `disjoint_parts_do_not_conflict` (AC-14): two field projections, sets based on one root version.
- `structural_change_invalidates_and_content_change_keeps` (AC-15): item projections over an array;
  content edit → `Changed`; insertion before → `Invalidated` and the sink called once.
- `read_only_and_validated_refuse_sets` (AC-18): `not_supported`; the check's own error; no write.
- `custom_root_gets_projections_conflicts_and_events` (AC-19): a test `AccessorRoot` with
  `poll_event`; projections, conflicts and `refresh`-delivered events work.
- `hub_error_cases` (AC-18, AC-22): the three error rows of the edge table.

**Core integration, `liquers-core/tests/value_accessor.rs`** (`SimpleEnvironment<Value>`, memory
store, `DefaultRecipeProvider`, and test commands `pl/from_csv`, `pl/is_in(column, values: Vec<String> multiple)`
and `pl/head(n: i64 = 10)` registered with `register_command!`):
- `recipe_argument_projection_reads_part_and_hints` (AC-1): the Rust chain `RecipeStep(0)` →
  `QueryFieldStep` → `ActionStep(3)` → `ArgumentStep("n")` reads `I64(10)` and describes argument
  type `int` of `pl/head` with `IntegerField`.
- `recipe_argument_set_writes_through` (AC-2): set `20`; the stored YAML parses to the new query and
  the same title and second recipe.
- `refused_argument_set_writes_nothing` (AC-3): the three refusals; the store version is unchanged.
- `query_steps_work_on_any_query_root` (AC-10): a memory root holding `Value::Query` gives the same
  action and argument parts.
- `outside_writer_is_detected` (AC-6): `set_state` on the key outside the accessor; the keyed-asset
  accessor receives `Changed` and reads the new value.
- `store_root_sees_a_direct_write_on_refresh` (AC-6): an `AsyncStore::set` under a store root; after
  `refresh`, `Changed` and the new value.
- `expiry_and_removal_are_reported` (AC-7): `Expired` status; after `remove`, `Removed`, status
  `None`, `get` not found, a set creates the key again.
- `keyed_asset_set_overrides_sources_or_creates` (AC-8, AC-21): `Override` with a recipe, `Source`
  without, a missing key created as `Source`; a dependent of the overridden key is expired; a set
  after reading an `Expired` status stores `Override`, not `Expired`; `asset_accessor` of a query
  asset and of an unmanaged (`apply`) asset refuses a set.
- `expired_asset_get_re_evaluates` (AC-16): `Expired`, then `get` re-evaluates and `Changed`
  follows; a second expiry is seen too (re-subscription by `id()`); with `ExpiryPolicy::Reload` the
  `Changed` arrives with no `get`.
- `store_views_and_a_missing_key` (AC-20): the three views of a stored YAML; a missing key's status
  `None`; creation through the binary view, after which all three views read it; a value-view set
  of an existing key keeps its metadata's title, a metadata-view set keeps its bytes; the
  metadata-view refusal for a missing key.
- `progress_and_log_are_relayed` (AC-23): a recipe command that reports progress
  (`Context::progress`, `secondary_progress`) and logs (`Context::info`), then waits on a
  `tokio::sync::Notify`; while it waits, `info()` shows the progress and `log_len`, and `log(0)` the
  message. Events are checked only for kind, because `watch` keeps the latest.
- `accessor_does_not_keep_the_environment_alive`: after the environment is dropped, a held root
  accessor's `get` fails instead of keeping it alive (pattern of `tests/environment_teardown.rs`).

**Lib, `liquers-lib/tests/value_accessor_commands.rs`:**
- `accessor_query_projects_recipe_argument` (AC-1, AC-11): Example 1 through `evaluate`; the
  second evaluation returns an accessor of the same hub.
- `accessor_value_is_never_stored` (AC-11): persisting the state is `NonSerializable`.
- `accessor_address_resolves_in_a_second_environment` (AC-12): the address encoded, parsed back,
  and evaluated in a second environment over the same store reads and sets the same part.
- `structural_edit_expires_action_accessor_asset` (AC-15): evaluate `…/recipe-0/query/action-3`
  and `…/recipe-0/query`; insert a step through the query accessor; the action accessor's asset is
  `Expired`, and re-evaluating its query gives the new element 3.
- `document_field_projection` (AC-9): `…/ns-acc/accessor/field-title` on a JSON and a YAML file;
  a set changes that field only.
- `record_row_column_and_cell_projections` (AC-9, `records` feature): `RowStep`, `ColumnStep` and
  `CellStep` read and set their part; the other rows and columns read unchanged.
- `store_command_views` (AC-20): `ns-acc/store-binary`, `store-metadata`, `store-value`.
- `ext_value_accessor_hooks_round_trip`: `from_accessor` / `try_into_accessor` through
  `CombinedValue`.

**Axum, `liquers-axum/tests/accessor_api.rs`:**
- `get_returns_value_and_etag` (AC-13); `put_preconditions_and_conflicts` (AC-13, AC-20, AC-21):
  `200`, `412` with the current `ETag`, `428`, `If-None-Match: *` create, `405` read-only.
- `info_log_and_describe_routes` (AC-23); `websocket_relays_accessor_events` (AC-17): status,
  changed, invalidated and removed frames in the asset socket's shape.

**Python, `liquers-py/tests/test_ACCESSOR_use.py`, and JavaScript, `liquers-web/tests/accessor_ACCESSOR.rs`**
(same IDs, the `ACCESSOR` feature of the language integration guide):
- `ACCESSOR01_get_and_set_with_a_token` (AC-24); `ACCESSOR02_project_and_describe` (AC-24, AC-1);
  `ACCESSOR03_events_reach_callbacks` (AC-24, AC-16); `ACCESSOR04_conflict_is_status_conflict`
  (AC-24, AC-5); `ACCESSOR05_memory_accessor_is_shared` (AC-24, AC-22);
  `ACCESSOR06_accessor_crosses_back_unchanged` (AC-24, AC-11); `ACCESSOR07_info_and_log` (AC-24,
  AC-23). Python names them `test_ACCESSOR01_get_and_set_with_a_token` and so on, and
  `test_LAYOUT_submodules.py` gains `accessors`.

Commands to run:
```bash
cargo test -p liquers-core accessor --lib && cargo test -p liquers-core --test value_accessor
cargo test -p liquers-lib --test value_accessor_commands --test registry_export
cargo test -p liquers-axum --test accessor_api
bash scripts/test-liquers-py.sh                          # includes test_ACCESSOR_use.py
cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles --test accessor_ACCESSOR
bash scripts/check-build-matrix.sh
```

## Learning Log

- `AssetNotificationMessage::LogMessage` carries no entry: the log is read from
  `AssetRef::get_metadata`, so `Log` events carry only the count.
- There is no manager-level event stream yet (`external-index-sync` is not implemented), so a store
  root sees other writers on `get` and `refresh` only.
- `Version::unknown()` *matches* any version (`Version::matches`); fingerprints compare with `==`.
- The `head` of Example 1 is element 3, not 2: `PipelineEditor` counts the resource as element 0.
- `ActionEditor::set` takes JSON, so an argument part converts through `try_into_json_value`.
