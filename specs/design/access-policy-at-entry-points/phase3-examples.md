# Phase 3: Examples & Use-cases — Principals and an access policy at the entry points

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | Two users, one server | the problem example, configured | AC-1, AC-2, AC-4, AC-6 |
| 2 | Example | Publishing through a recipe | Q5 in practice | AC-4 |
| 3 | Test | `access::tests::rule_policy_first_match_wins` | rule order, default | AC-1, AC-4 |
| 4 | Test | `access::tests::key_patterns_match_by_element` | `notes` covers `notes/a`, not `notes2/a` | AC-1, AC-2 |
| 5 | Test | `access::tests::principal_selectors` | `*`, `anonymous`, `system`, `user:*`, `user:alice` | AC-6 |
| 6 | Test | `access::tests::plan_access_requests_collects_keys_commands_and_links` | the walk | AC-2, AC-3 |
| 7 | Test | `access::tests::rule_policy_from_yaml` | YAML in, malformed → `ConfigurationError` | AC-1 |
| 8 | Test | `acc01_store_put_refused_for_other_user` | 403, nothing written | AC-1 |
| 9 | Test | `acc02_query_reading_forbidden_key_refused_before_evaluation` | 403, command never ran | AC-2 |
| 10 | Test | `acc03_link_override_naming_forbidden_key_refused` | `_link_` overrides walked | AC-2 |
| 11 | Test | `acc04_command_without_evaluate_rule_refused` | 403 naming the command | AC-3 |
| 12 | Test | `acc05_allowed_query_is_one_asset_for_two_principals` | same asset id, evaluated once | AC-4 |
| 13 | Test | `acc06_principal_from_extension_else_anonymous` | the seam | AC-6 |
| 14 | Test | `acc07_listings_omit_unreadable_entries` | Store, Assets, Recipes listings | AC-7 |
| 15 | Test | `acc08_admin_routes_need_admin` | 403 then 200 | AC-8 |
| 16 | Test | `acc09_websocket_subscribe_refused` | `Error` message, no `Initial` | AC-2 |
| 17 | Test | existing `liquers-axum` suites, no policy | unchanged | AC-5 |
| 18 | Test | `keyabs15b` (extended), `liquers-py` and `liquers-web` error mapping tests | `AccessDenied` round-trips, 403 | AC-9 |

## Example 1: Two users, one server

```rust
let policy: Arc<dyn AccessPolicy> = Arc::new(RulePolicy::from_yaml(POLICY_YAML)?); // Phase 1 YAML
let app = Router::new()
    .merge(QueryApiBuilder::new("/liquer/q").with_access_policy(policy.clone()).build())
    .merge(StoreApiBuilder::new("/liquer/api/store").with_access_policy(policy.clone()).build())
    .merge(AssetsApiBuilder::new("/liquer/api/assets").with_access_policy(policy).build())
    .layer(axum::middleware::from_fn(bearer_to_user))   // the deployment's auth: inserts Extension(User)
    .with_state(envref);
```

```text
GET /liquer/q/-R/public/readme.md/-/to_text         (anonymous)   → 200
GET /liquer/q/-R/notes/alice/todo.md/-/to_text      (user:bob)    → 403 {"error":{"type":"AccessDenied",
      "message":"user:bob may not read 'notes/alice/todo.md'","key":"notes/alice/todo.md"}}
PUT /liquer/api/store/data/notes/alice/todo.md      (user:alice)  → 200
```

## Example 2: Publishing through a recipe

`public/recipes.yaml` declares `stats.json` from `-R/notes/alice/log.csv/-/stats`. Anyone may
`GET key/data/public/stats.json`: the request names `public/stats.json` only, and the recipe is
server configuration (Q5). `GET q/data/-R/notes/alice/log.csv/-/stats` by bob is refused: the
request itself names `notes/alice/log.csv`.

## Edge and Error Cases

- **Overrides and links.** `q/data/…?_link_x=-R/notes/alice/b.txt` → planned through
  `OverrideRequest::to_recipe` → refused (Test 10). A link inside the path,
  `-R/public/a.txt/-/concat-~X~-R/notes/alice/b.txt~E`, plans as `GetAsset[public, a.txt]` plus a
  `ParameterLink` → refused (Test 6).
- **Relative keys.** A key step left relative by the planner (no cwd) is checked as written after
  `Key::as_absolute()`; one that is not absolute is refused, never matched by prefix (Test 6).
- **Cancel of someone else's query.** `q/cancel` uses the query check: a principal who may run it
  may cancel it (documented; finer ownership is the unmanaged registry's concern).
- **No policy.** `AccessGuard(None)` answers `Ok` without planning, so no added cost (Test 17).
- **Policy errors.** Malformed YAML → `ConfigurationError` at startup (Test 7).

## Test Plan

- Unit: `liquers-core/src/access.rs` — Tests 3-7. Test 6 plans
  `-R/public/a.txt/-/concat-~X~-R/notes/alice/b.txt~E` with a registry holding `concat` and expects
  `[Read public/a.txt, Evaluate concat, Read notes/alice/b.txt]` (order-insensitive).
- Unit: `liquers-axum/src/api_core/error.rs` — `keyabs15b` gains `AccessDenied`; new
  `access_denied_is_403` (AC-9). `liquers-py/src/error.rs` and `liquers-web/src/error.rs` mapping
  tests extended (AC-9).
- Integration: `liquers-axum/tests/access_policy.rs` (new) — Tests 8-16, fixture: memory store with
  `public/readme.md`, `notes/alice/todo.md`, `notes/alice/b.txt`; commands `to_text`, `concat`,
  and `count_calls` (increments an `AtomicUsize`, for "never ran"); principal inserted by a test
  layer reading an `x-test-user` header.
- Regression: `cargo test -p liquers-axum --lib --tests` with no policy (AC-5).
- Commands: `cargo test -p liquers-core --lib access`, `cargo test -p liquers-axum --lib --tests`,
  `bash scripts/test-liquers-py.sh`, `cargo test -p liquers-web --target wasm32-unknown-unknown
  --features debug-handles` (after `cargo clean`).

Queries validated with `liquers-validate` (`--command concat`, `--command stats` where needed):
`-R/public/readme.md/-/to_text`, `-R/notes/alice/todo.md/-/to_text`,
`-R/public/a.txt/-/concat-~X~-R/notes/alice/b.txt~E`, `-R/notes/alice/log.csv/-/stats`.

## Learning Log

- `liquers-mcp` `check_allowed` already walks plans for commands; adding key steps to the same walk
  is what turns it into a policy check, and the MCP server can later call the core version.
