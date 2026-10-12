# Phase 3: Examples & Use-cases — Argument overrides over HTTP and WebSocket

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | A search with overrides over HTTP | Query string and body forms; an override replacing a path value | AC-1, AC-2, AC-5 |
| 2 | Example | Follow an overridden evaluation on the WebSocket, then cancel another | `arguments`, `subscription_id`, `managed`, `cancel` | AC-17, AC-18, AC-19, AC-20 |
| 3 | Example | `q/submit` for a side effect | Unmanaged submit: no result, note, `q/info` 404 | AC-16 |
| 4 | Test | `liquers-core/tests/recipe_override_conversion.rs` (ROC01–ROC05) | `Recipe::to_plan` conversion, warnings, errors | AC-4, AC-6, AC-12 |
| 5 | Test | `liquers-core/tests/apply_queued.rs` (AQ01–AQ03) | Queued and inline `apply_queued` | AC-14, AC-16, AC-17 |
| 6 | Test | `liquers-axum/src/api_core/overrides.rs` unit tests | Parsing: empties, duplicates, reserved names, links | AC-3, AC-7, AC-9, AC-10, AC-11 |
| 7 | Test | `liquers-axum/tests/argument_overrides_http.rs` (AOH01–AOH19) | Every HTTP route in scope | AC-1–AC-16, AC-20, AC-21 |
| 8 | Test | `liquers-axum/tests/argument_overrides_ws.rs` (AOW01–AOW07) | WebSocket `arguments`, ids, cancel | AC-7, AC-17–AC-21 |
| 9 | Test | `liquers-axum/tests/entry_format_option.rs` (EFO01–EFO06) | `_entry_format` on every entry route | AC-13, AC-22 |

**Fixture commands** (registered in each axum suite with `register_command` plus `ArgumentInfo`
builders, since `liquers-axum` has no `liquers-macro` dependency): `echo_args(query: string = "",
rank: bool = true, limit: int = 50, fields: multiple string)` returns
`"query=<q>;rank=<b>;limit=<n>;fields=<a,b>"`; `make_text` returns `"generated"`; `fail_with`
fails; `slow_count(delay_ms: int = 50)` sleeps `delay_ms` (async), then increments a shared
`AtomicUsize` and returns it. Every fixture
query below was checked with `liquers-validate` against an overlay declaring these signatures
(`echo_args`, `echo_args----20`, `make_text/echo_args`, `echo_args-hello-f-3-a-b`,
`echo_args-~X~make_text~E`: all `Ok`).

## Example 1: A search with overrides over HTTP

The motivating request (`ns-search/search`, Phase 1). The path query is validated; the overrides
travel beside it.

```bash
Q='-R/docs/catalog.csv/-/ns-search/search'
curl -i "$B/q/$Q?query=expiration%20safety&limit=20"            # Query API, GET
curl -i -X POST -H 'Content-Type: application/json' \
     -d '{"query": "expiration safety", "limit": 20}' "$B/q/$Q"  # Query API, POST
curl -i "$A/q/data/${Q}----20?query=expiration%20safety"         # AC-5: path gives limit, name gives query
```

Each answers the search's value with `x-liquers-managed: false`. The same requests without
overrides (`$B/q/$Q`, `$B/q/$Q?`, a POST with `{}`) answer `x-liquers-managed: true` and share the
cached asset, exactly as today (AC-7). In the suites the fixture `echo_args` stands in for the
search: `GET /q/echo_args?query=a%20b&limit=20&fields=x&fields=y` →
`query=a b;rank=true;limit=20;fields=x,y` (AOH01).

## Example 2: Follow an overridden evaluation on the WebSocket, then cancel another

```json
→ {"action":"subscribe","query":"echo_args","arguments":{"query":"a","limit":3}}
← {"type":"Initial","asset_id":1042,"query":"echo_args","subscription_id":"7c9e…","managed":false,"info":{"status":"Submitted",…}}
← {"type":"JobFinished","subscription_id":"7c9e…","managed":false,"info":{"status":"Ready",…}}
→ {"action":"subscribe","query":"slow_count","arguments":{"delay_ms":5000}}
→ {"action":"cancel","subscription_id":"<the second id>"}
← … {"type":"StatusChanged","status":"Cancelled",…}
→ {"action":"subscribe","query":"echo_args","arguments":{}}
← {"type":"Initial","query":"echo_args","managed":true,…}          (no subscription_id: AC-7)
```

Two `subscribe`s of `echo_args` with different `arguments` run and finish independently (AC-18);
`cancel` with an id this connection never received is `Error` "unknown subscription id" (AC-19).

## Example 3: `q/submit` for a side effect

`export_rows` is illustrative (a command writing to a database):

```bash
curl -X POST -H 'Content-Type: application/json' -d '{"table":"events"}' "$A/q/submit/export_rows"
# {"status":"OK","result":{"status":"Submitted",…,"managed":false,
#   "note":"This evaluation is unmanaged: its status and result cannot be read later. …"}}
curl "$A/q/info/export_rows"          # 404 NotAvailable — the overridden evaluation is in no map
```

The export still runs to completion; only its result is unreachable. In the suite, `slow_count`
proves it ran: the counter increases although no route can read the asset (AOH17).

## Edge and Error Cases

| Symptom | Expected | Test |
|---|---|---|
| Zero-length POST body with `Content-Type: application/json` | no overrides; managed path | `parse_empty_body_with_content_type_is_none`, AOH09 |
| `?` alone, `{}`, `_entry_format` only, WS `arguments: {}` | managed, as today | AOH09, AOW03 |
| Same name in query string and body | 400 `ParameterError` | `parse_duplicate_name_rejected`, AOH11 |
| `_foo` | 400 reserved | `parse_reserved_name_rejected`, AOH12 |
| `_link_query` and `query` together; unparsable link | 400 `ParameterError`; 400 `ParseError` | `parse_value_and_link_rejected`, AOH13 |
| `?limit=abc`, `{"limit":"abc"}`, `{"limit":null}` | 422 `ConversionError` | AOH08, ROC03 |
| Name that is not an argument; injected argument; query with no action (`-R/notes/todo.txt?x=1`) | 400 `ParameterError` | AOH06, ROC02 |
| Inline manager: overridden evaluation fails | the failed asset is returned, its log reaches `q/entry` | AQ02, AOH16 |
| Cancel before the asset exists (inline manager mid-evaluation; queued manager mid-submit) | cancelled, not lost | AOW07 |
| A volatile query submitted without overrides | `managed: false`, note | AOH18 |
| Old `?format=json` on `key/entry` | 400 naming `_entry_format` | EFO02 |

## Test Plan

**Core** — `cargo test -p liquers-core --test recipe_override_conversion --test apply_queued` and
`cargo test -p liquers-core --lib plan::tests::last_action_command`:
- `recipe_override_conversion.rs`
  - `roc01_mismatched_json_converts_with_a_warning` (AC-12): `"20"`→20 for `int`, `20`→`"20"` for `string`, `"yes"`→true, `3.0`→3; each adds one `Step::Warning` to `plan.init_steps` naming argument and types; matching types add none.
  - `roc02_unknown_injected_or_actionless_name_is_parameter_error` (AC-4): three cases, `ErrorType::ParameterError`.
  - `roc03_unconvertible_value_is_conversion_error` (AC-6): `"abc"`, `null`, `2.5` for `int`.
  - `roc04_overrides_apply_in_sorted_order` (AC-12): two mismatched arguments; warnings in name order on every run.
  - `roc05_enum_alias_is_expanded` (AC-12): alias string → enum value; query alias → link override.
- `apply_queued.rs`
  - `aq01_default_manager_returns_before_completion_and_maps_nothing` (AC-16, AC-17): status not finished on return; `lookup_query_asset` is `None`; `get` later yields the value.
  - `aq02_inline_manager_returns_the_failed_asset` (AC-14): `Ok(asset)` with `Status::Error` and the error in its metadata.
  - `aq03_queued_asset_uses_its_recipe_arguments` (AC-17): the override reaches the command.
- `plan.rs` unit: `last_action_command_returns_alias_target` (AC-4).

**axum** — `cargo test -p liquers-axum`:
- `src/api_core/overrides.rs` unit: `parse_query_string_repeated_keys_keep_order` (AC-3); `parse_empty_body_with_content_type_is_none`, `parse_empty_object_and_options_only_is_empty` (AC-7); `parse_body_not_object_rejected`, `parse_duplicate_name_rejected` (AC-9); `parse_reserved_name_rejected`, `parse_entry_format_only_with_entry_options` (AC-10); `parse_link_option_and_value_and_link_rejected` (AC-11).
- `tests/argument_overrides_http.rs`:
  - `aoh01_query_api_get_query_string` (AC-1, AC-3); `aoh02_q_data_get_query_string` (AC-1, AC-15);
  - `aoh03_query_api_post_json_body` (AC-2); `aoh04_q_entry_post_json_body` (AC-2, AC-15);
  - `aoh05_variadic_repeated_key_and_json_array` (AC-3); `aoh06_unknown_injected_or_actionless_name_400` (AC-4);
  - `aoh07_override_replaces_path_value` (AC-5); `aoh08_unconvertible_value_422` (AC-6);
  - `aoh09_no_override_forms_stay_managed` (AC-7, AC-20): no query string, `?`, empty body with JSON content type, `{}`, `_entry_format` only — header `true`, `q/info` finds the asset;
  - `aoh10_different_overrides_not_shared` (AC-8): two `slow_count`-backed requests, both evaluated, distinct values;
  - `aoh11_malformed_body_or_duplicate_name_400` (AC-9); `aoh12_reserved_name_400` (AC-10); `aoh13_link_override` (AC-11);
  - `aoh14_body_type_mismatch_logged_url_text_not` (AC-12): `q/entry` metadata log has the warning for `{"limit":"20"}`, none for `?limit=20`;
  - `aoh15_q_entry_answers_data_and_metadata` (AC-13); `aoh16_q_entry_failure_carries_metadata` (AC-14), on both managers;
  - `aoh17_q_submit_with_overrides_is_unmanaged` (AC-16, AC-20): `managed: false`, `note`, header, `q/info` 404, side effect observed;
  - `aoh18_q_submit_volatile_without_overrides_is_unmanaged` (AC-16, AC-20);
  - `aoh19_recipe_with_arguments_keyed_asset_managed` (AC-21): `key/submit`, then `key/info` reports it; header `true` on `key`-addressed `q/data`.
- `tests/argument_overrides_ws.rs`:
  - `aow01_subscribe_with_arguments_follows_to_ready` (AC-17, AC-20); `aow02_two_subscriptions_same_query_different_arguments` (AC-18);
  - `aow03_empty_arguments_is_managed_without_id` (AC-7, AC-20); `aow04_cancel_by_id` (AC-19);
  - `aow05_unknown_or_managed_id_is_generic_error` (AC-19); `aow06_ws_key_subscribe_recipe_managed` (AC-21);
  - `aow07_cancel_before_asset_exists_is_not_lost` (AC-19), inline and queued managers.
- `tests/entry_format_option.rs`:
  - `efo01_q_entry_entry_format_json` (AC-13, AC-22); `efo02_old_format_on_key_entry_is_400` (AC-22);
  - `efo03_store_and_recipes_entry_accept_entry_format` (AC-22); `efo04_unknown_entry_format_value_400` (AC-22);
  - `efo05_post_key_entry_decodes_by_entry_format` (AC-22); `efo06_format_on_q_entry_is_an_override` (AC-22).
- Existing suites updated, not weakened: every `?format=` becomes `?_entry_format=`; `query_api_routes.rs` POST-body case now expects the body to be used.

## Learning Log

- `liquers-validate --command NAME` accepts *any* arguments as one variadic list, so positional
  forms such as `echo_args----20` fail ("Multiple parameters not supported inside vector argument").
  A registry overlay with the real signature (`--registry-file`, which needs `commands`,
  `default_namespaces` and `global_enums`) validates them. Filed as
  `VALIDATE-COMMAND-STUB-REJECTS-POSITIONAL-EMPTIES`.
- `apply` waits for the evaluation (`AssetRef::run`) and, on the inline manager, returns `Err`
  instead of the failed asset — both reasons for `apply_queued`.
- `CommandArguments::get` reads an override with `as_i64`, which rejects `3.0`: an integral float
  must be converted, not passed through.
