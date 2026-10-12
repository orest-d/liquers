# Phase 3: Examples & Use-cases — Argument overrides over HTTP and WebSocket

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | A search with overrides over HTTP | Query string and body forms; override replacing a path value | AC-1, AC-2, AC-5 |
| 2 | Example | Follow an overridden evaluation on the WebSocket; cancel another | `arguments`, `subscription_id`, `managed`, `cancel` | AC-17, AC-18, AC-19, AC-20 |
| 3 | Example | `q/submit` for a side effect | Unmanaged submit: no result, `note`, `q/info` 404 | AC-16 |
| 4 | Test | `liquers-core/tests/recipe_override_conversion.rs` (ROC01–ROC06) | `Recipe::to_plan` conversion, warnings, errors, aliases | AC-4, AC-6, AC-12 |
| 5 | Test | `liquers-core/tests/apply_queued.rs` (AQ01–AQ03) | Queued and inline `apply_queued` | AC-14, AC-16, AC-17 |
| 6 | Test | `liquers-axum/src/api_core/overrides.rs` unit tests | Parsing | AC-3, AC-7, AC-9, AC-10, AC-11 |
| 7 | Test | `liquers-axum/tests/argument_overrides_http.rs` (AOH01–AOH21) | Every HTTP route in scope | AC-1–AC-16, AC-20, AC-21 |
| 8 | Test | `liquers-axum/tests/argument_overrides_ws.rs` (AOW01–AOW13) | WebSocket `arguments`, ids, cancel, limits | AC-7, AC-17–AC-21 |
| 9 | Test | `liquers-axum/tests/entry_format_option.rs` (EFO01–EFO06) | `_entry_format` on every entry route | AC-13, AC-22 |

**Fixtures**, registered with `register_command` / `register_async_command` (plus
`.with_async(true)`) and `ArgumentInfo` builders (`string_argument`, `integer_argument`,
`boolean_argument`, `with_default`, `set_multiple`, `set_injected`; `command_metadata.rs:619-721`)
— `liquers-axum` has no `liquers-macro` dependency:

| Fixture | Signature | Returns / does |
|---|---|---|
| `echo_args` | `query: string = ""`, `rank: bool = true`, `case_sensitive: bool = false`, `limit: int = 50`, `fields: multiple string`, `ctx` injected (never read) | `"query=<q>;rank=<b>;case_sensitive=<b>;limit=<n>;fields=<a,b>"` — the shape of `ns-search/search` |
| `make_text` | — | `"generated"` |
| `fail_with` | — | an error |
| `gated` | `tag: string = ""` (async) | awaits a shared `tokio::sync::Notify`, increments a shared `AtomicUsize`, returns `"<tag>:<count>"` |
| `gated_volatile` | as `gated`, `.volatile = true` | as `gated` |
| `echo_alias` | alias of `echo_args` with head `query = "fixed"`; own arguments `flag: bool` (fills `rank`) | — |

Shared helpers for the new suites go in a new `liquers-axum/tests/common/mod.rs`: `env_with` generic over
`SimpleEnvironment` / `ImmediateEnvironment` and able to set `Recipe.arguments`; `start_server`,
`connect`, `send_json`; and `recv_matching(ws, predicate)`, which keeps unmatched messages
(today's `recv_by_type` drops them, `assets_websocket.rs:101`). Gated commands make every
"while running" assertion deterministic; waits are bounded `tokio::time::timeout` polls, never
fixed sleeps. Fixture queries validated with `liquers-validate` against an overlay declaring these
signatures: `echo_args----20` plans `query=""`, `rank`/`case_sensitive` default, `limit=20`,
`fields=[]`; `make_text/echo_args`, `echo_args-hello-f-f-3-a-b`, `echo_args-~X~make_text~E` all `Ok`.

## Example 1: A search with overrides over HTTP

```bash
Q='-R/docs/catalog.csv/-/ns-search/search'
curl -i "$B/q/$Q?query=expiration%20safety&limit=20"            # Query API, GET
curl -i -X POST -H 'Content-Type: application/json' \
     -d '{"query": "expiration safety", "limit": 20}' "$B/q/$Q"  # Query API, POST
curl -i "$A/q/data/${Q}----20?query=expiration%20safety"         # AC-5: path gives limit, name gives query
```

Each answers the value with `x-liquers-managed: false`. Without overrides (`$B/q/$Q`, `$B/q/$Q?`,
a POST with `{}`) the answer is `x-liquers-managed: true` and the cached asset is shared, as today
(AC-7). In the suites `echo_args` stands in: `GET /q/echo_args?query=a%20b&limit=20&fields=x&fields=y`
→ `query=a b;rank=true;case_sensitive=false;limit=20;fields=x,y` (AOH01).

## Example 2: Follow an overridden evaluation on the WebSocket; cancel another

```json
→ {"action":"subscribe","query":"echo_args","arguments":{"query":"a","limit":3}}
← {"type":"Initial","asset_id":1042,"query":"echo_args","subscription_id":"7c9e…","managed":false,"info":{"status":"Submitted",…}}
← {"type":"JobFinished","asset_id":1042,"query":"echo_args","subscription_id":"7c9e…","managed":false,"info":{"status":"Ready",…}}
→ {"action":"subscribe","query":"gated","arguments":{"tag":"b"}}
← {"type":"Initial",…,"subscription_id":"41d2…","managed":false,…}
→ {"action":"cancel","subscription_id":"41d2…"}
← {"type":"StatusChanged","query":"gated","subscription_id":"41d2…","managed":false,"status":"Cancelled",…}
→ {"action":"subscribe","query":"echo_args","arguments":{}}
← {"type":"Initial","query":"echo_args","managed":true,…}          (no subscription_id: AC-7)
```

`cancel` with an id this connection never received is `Error` "unknown subscription id" (AC-19).

## Example 3: `q/submit` for a side effect

`export_rows` is illustrative (a command writing to a database):

```bash
curl -X POST -H 'Content-Type: application/json' -d '{"table":"events"}' "$A/q/submit/export_rows"
# {"status":"OK","result":{"status":"Submitted",…,"managed":false,
#   "note":"This evaluation is unmanaged: its status and result cannot be read later. …"}}
curl "$A/q/info/export_rows"          # 404 NotAvailable — the overridden evaluation is in no map
```

The export still runs; only its result is unreachable. AOH17 proves it with `gated`: after the
gate opens the counter rises, though no route can read the asset.

## Edge and Error Cases

| Symptom | Expected | Test |
|---|---|---|
| Zero-length POST body with `Content-Type: application/json`; `?`; `{}`; `_entry_format` only on `q/entry`; WS `arguments: {}` | managed, as today | `parse_empty_body_with_content_type_is_none`, AOH09, AOW03 |
| Same name in query string and body; body not an object | 400 `ParameterError` | `parse_duplicate_name_rejected`, AOH11 |
| `_foo`; `_entry_format` on the Query API | 400 reserved | `parse_reserved_name_rejected`, AOH12 |
| `_link_query` and `query` together; unparsable link | 400 `ParameterError`; 400 `ParseError` | `parse_value_and_link_rejected`, AOH13 |
| `?limit=abc`, `{"limit":"abc"}`, `{"limit":null}` | 422 `ConversionError` | AOH08, ROC03 |
| Unknown name; injected `ctx`; query with no action (`-R/notes/todo.txt?x=1`); an alias's head `query` | 400 `ParameterError` | AOH06, ROC02, ROC06 |
| Evaluation fails, with and without overrides, on both managers | the asset's log reaches `q/entry`'s `error.metadata` (without overrides on the inline manager the asset is found by `lookup_query_asset` when `get_asset` returns `Err`) | AQ02, AOH16 |
| Cancel before the asset exists | not lost | AOW07 |
| Volatile query without overrides: `q/submit`; WS `subscribe` | `managed: false`, `note`; `subscription_id`, cancellable | AOH18, AOW11 |
| `read_only()` builder | `POST q/data`, `POST q/entry` still served | AOH20 |
| Old `?format=json` on `key/entry` | 400 naming `_entry_format` | EFO02 |

## Test Plan

**Core** — `cargo test -p liquers-core --test recipe_override_conversion --test apply_queued`;
`cargo test -p liquers-core --lib plan::tests::last_action_command`:
- `roc01_mismatched_json_converts_with_a_warning` (AC-12): `"20"`→20 (`int`), `20`→`"20"` (`string`), `"yes"`→true, `3.0`→3; one `Step::Warning` each naming argument and types; matching types add none.
- `roc02_unknown_injected_or_actionless_name_is_parameter_error` (AC-4).
- `roc03_unconvertible_value_is_conversion_error` (AC-6): `"abc"`, `null`, `2.5` for `int`.
- `roc04_overrides_apply_in_sorted_order` (AC-12): warnings in name order on every run.
- `roc05_enum_alias_is_expanded` (AC-12, Phase 2 table): alias string → value; query alias → link.
- `roc06_alias_overrides_use_alias_names_heads_refused` (AC-4): `flag` overrides `rank`; `query` (head) is `ParameterError`.
- `aq01_default_manager_returns_before_completion_and_maps_nothing` (AC-16, AC-17): `gated`; status in {None, Submitted, Processing} on return; `lookup_query_asset` is `None`; gate opened; `get` yields the value.
- `aq02_inline_manager_returns_the_failed_asset` (AC-14): `Ok(asset)`, `Status::Error`, error in metadata.
- `aq03_queued_asset_uses_its_recipe_arguments` (AC-17).
- `plan.rs` unit `last_action_command_returns_alias_metadata` (AC-4).

**axum** — `cargo test -p liquers-axum`:
- `overrides.rs` unit: `parse_query_string_repeated_keys_keep_order` (AC-3); `parse_empty_body_with_content_type_is_none`, `parse_empty_object_and_options_only_is_empty` (AC-7); `parse_body_not_object_rejected`, `parse_duplicate_name_rejected` (AC-9); `parse_reserved_name_rejected`, `parse_entry_format_only_with_entry_options` (AC-10); `parse_link_option`, `parse_value_and_link_rejected` (AC-11).
- `argument_overrides_http.rs` (`build_app_with_query_api`, both routers merged):
  - `aoh01_query_api_get_query_string` (AC-1, AC-3, AC-20): value; header `false`.
  - `aoh02_q_data_and_q_entry_get_query_string` (AC-1, AC-15, AC-20).
  - `aoh03_query_api_post_json_body` (AC-2, AC-20); `aoh04_q_entry_post_json_body` (AC-2, AC-15, AC-20).
  - `aoh05_variadic_repeated_key_and_json_array` (AC-3).
  - `aoh06_unknown_injected_or_actionless_name_400` (AC-4): `nope`, `ctx`, `-R/notes/todo.txt?x=1`.
  - `aoh07_override_replaces_path_value` (AC-5): `echo_args----20?query=a` → `query=a;…;limit=20;fields=`.
  - `aoh08_unconvertible_value_422` (AC-6).
  - `aoh09_no_override_forms_stay_managed` (AC-7, AC-20): no query string, `?`, empty body with JSON content type, `{}`, `_entry_format` only (on `q/entry`) — header `true`, `q/info` finds the asset; a managed `q/submit` has `managed: true` and no `note` key.
  - `aoh10_different_overrides_not_shared` (AC-8): concurrent `echo_args?query=a` and `?query=b`, each its own value; the same overrides twice evaluate twice (`gated` counter 2).
  - `aoh11_malformed_body_or_duplicate_name_400` (AC-9); `aoh12_reserved_name_400` (AC-10).
  - `aoh13_link_override` (AC-11): `_link_query=make_text` → `query=generated`; `_link_query=-R/` → 400 `ParseError`; `query` with `_link_query` → 400 `ParameterError`.
  - `aoh14_body_type_mismatch_logged_url_text_not` (AC-12): `q/entry` log has a warning naming `limit` for `{"limit":"20"}`; none naming `limit` for `?limit=20`.
  - `aoh15_q_entry_answers_data_and_metadata` (AC-13).
  - `aoh16_q_entry_failure_carries_metadata` (AC-14): with and without overrides, both managers.
  - `aoh17_q_submit_with_overrides_is_unmanaged` (AC-16, AC-20): `managed: false`, `note`, header, `q/info` 404; gate opened; counter observed by a bounded poll.
  - `aoh18_q_submit_volatile_without_overrides_is_unmanaged` (AC-16, AC-20).
  - `aoh19_recipe_with_arguments_keyed_asset_managed` (AC-21): recipe with `arguments` in `recipes.yaml`; `key/submit`, `key/info` reports it; header `true` on the key-addressed `q/data`.
  - `aoh20_post_read_routes_served_when_read_only` (AC-15).
  - `aoh21_q_data_post_query_string_and_body_combined` (AC-1, AC-2, AC-15): `?limit=3` with `{"query":"a"}`.
- `argument_overrides_ws.rs`:
  - `aow01_subscribe_with_arguments_follows_to_ready` (AC-17, AC-20): every message carries `subscription_id`, `managed: false`.
  - `aow02_two_subscriptions_same_query_different_arguments` (AC-18): both sent at once, told apart by the `Subscribed` acknowledgements' `request_id` (Q16); two distinct `asset_id`s each reach `Ready`.
  - `aow03_empty_arguments_is_managed_without_id` (AC-7, AC-20).
  - `aow04_cancel_by_id` (AC-19): `gated`; `Cancelled`; counter unchanged after the gate opens.
  - `aow05_unknown_or_managed_id_is_generic_error` (AC-19): a made-up id and a guessed id give the same `Error` text.
  - `aow06_ws_key_subscribe_recipe_managed` (AC-21).
  - `aow07_cancel_before_asset_exists_is_not_lost` (AC-19): inline — `gated` holds `apply_queued`, `cancel` aborts, gate opens, counter unchanged (deterministic); queued — `subscribe` and `cancel` sent in one burst, outcome asserted whichever branch ran (`Cancelled`, counter unchanged); coverage of the `cancel_requested` branch there is probabilistic, and stated so in the test.
  - `aow08_unsubscribe_by_id_stops_notifications` (AC-19).
  - `aow09_disconnect_does_not_cancel` (AC-19), queued manager: close the socket, open the gate,
    counter rises (on the inline manager the evaluation runs in the aborted task — Phase 2).
  - `aow10_arguments_on_ws_key_rejected` (AC-17): `Error`, `ParameterError`.
  - `aow11_volatile_subscribe_without_arguments_gets_id` (AC-20): `gated_volatile`; `managed: false`, `subscription_id`; `cancel` by it works.
  - `aow12_resubscribe_same_address_replaces` (AC-7): one stream of notifications remains.
  - `aow13_override_subscriptions_count_toward_limit` (AC-17): `max_subscriptions = 2`; the third is `Error`.
- `entry_format_option.rs`:
  - `efo01_q_entry_entry_format_then_accept_then_cbor` (AC-13, AC-22).
  - `efo02_old_format_on_key_entry_is_400` (AC-22): message names `_entry_format`.
  - `efo03_store_and_recipes_entry_accept_entry_format` (AC-22); `efo04_unknown_entry_format_value_400` (AC-22).
  - `efo05_post_key_entry_decodes_by_entry_format_then_content_type` (AC-22).
  - `efo06_format_on_q_entry_is_an_override` (AC-22): on `echo_args` (no `format` argument) → 400 `ParameterError` unknown name.
- Existing suites updated, not weakened: every `?format=` → `?_entry_format=`. (`query_api_routes.rs` has no POST-with-body case; AOH03 is the first.) New suites use `tests/common`; existing suites keep their private helpers.

## Learning Log

- `liquers-validate --command` accepts arguments as one variadic list, so `cmd----x` fails to plan;
  an overlay with the real signature (`--registry-file`; needs `commands`, `default_namespaces`,
  `global_enums`) works. Filed `VALIDATE-COMMAND-STUB-REJECTS-POSITIONAL-EMPTIES`.
- **A query that validates can still mean something else**: `echo_args----20` first filled
  `fields`, not `limit`, because the fixture had one argument fewer than `search`. Read the plan's
  parameters, not only `status: Ok`.
- `apply` waits for the evaluation and, on the inline manager, returns `Err` instead of the failed
  asset — both reasons for `apply_queued`.
- An alias's head slots carry the *target's* names and its own slots the *alias's* names
  (`ResolvedParameterValues::from_alias_action`): overrides must resolve against the alias's
  metadata, and heads are not overridable (Phase 2 amended).
- `CommandArguments::get` reads an override with `as_i64`, which rejects `3.0`: an integral float
  must be converted.
