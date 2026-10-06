# Phase 3: Examples and Tests - Derived OpenDAL Store Arguments

## Example

`store_types()` entry for `s3`, before and after (abridged):

| | Arguments |
|---|---|
| Before | `bucket` (required, doc), `root`, `region`, `endpoint`, `access_key_id` |
| After | `bucket` (required, doc), `root`, `region`, `endpoint`, `access_key_id` (all hand docs), then every other `S3Config` field alphabetically — `allow_anonymous` (boolean, default `false`), `disable_config_load`, `secret_access_key`, `session_token`, … |

For `fs`: before `root`, `access_key_id`; after `root` (hand doc), `atomic_write_dir`.

## Tests (`liquers-store/src/store_factory.rs`, `mod tests`)

| Test | Gate | Asserts | Criterion |
|---|---|---|---|
| `derive01_s3_reports_stable_fields_with_hand_docs` | `services-s3` | `bucket`, `region`, `root`, `endpoint` present; `bucket.required`; `bucket.doc` equals the hand-written doc; `allow_anonymous` present with type `Boolean` | 1, 5 |
| `derive02_hand_written_names_absent_from_the_config_are_dropped` | `services-fs` | `fs` has `root` and no `access_key_id` | 2 |
| `derive03_an_uncompiled_service_keeps_the_hand_written_list` | `not(feature = "services-redis")` | `redis` arguments equal `hand_written_arguments("redis")` | 3 |
| `derive04_coverage_stays_partial` | `opendal` | every OpenDAL type's coverage is `Partial` with `OPENDAL_DOCS` | 4 |
| `s3_01_arguments_and_uri_agree` | `services-s3` | from `design/store-factories-in-core/phase3-examples.md` ≈498 verbatim: config `bucket`, `root`, `region`, `allow_anonymous`, `disable_config_load` builds offline, `key_prefix` is `remote`, and `Operator::from_uri` with the same values succeeds with the same root | 6 |
| `s3_02_missing_region_fails_at_construction` | `services-s3` | config `bucket` **and `disable_config_load: true`**, no `region` → `create` is `Err` whose message contains `region` | 7 |

All presence assertions; no test fixes the full list (criterion 5). No network: OpenDAL builders
are lazy.

## Commands

```bash
cargo test -p liquers-store --lib store_factory
cargo test -p liquers-store --no-default-features --features opendal,services-fs --lib store_factory
bash scripts/check-build-matrix.sh
```
