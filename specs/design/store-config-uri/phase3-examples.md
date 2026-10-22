# Phase 3: Examples & Use-cases — Configuring a store from a URI

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | S3 from a URI, secrets in `config:` | the short form with a credential kept out of the string | AC-1, AC-6 |
| 2 | Example | Two core stores from URIs | `file://` and `memory://` | AC-5 |
| 3 | Test | `store_config::tests::uri_*` | parsing, scheme, variable expansion | AC-6 |
| 4 | Test | `store_factory::tests::uri_*` (core) | exclusivity, chain first-wins, unclaimed scheme, core schemes | AC-2, AC-3, AC-4, AC-5 |
| 5 | Test | `store_factory::tests::uri_*` (liquers-store) | OpenDAL equivalence, overrides, no URI form | AC-1, AC-6, AC-7 |

## Example 1: S3 from a URI, secrets in `config:`

```yaml
stores:
  - uri: s3://${DATA_BUCKET}/datasets?region=eu-central-1
    prefix: remote
    config:
      access_key_id: ${AWS_ACCESS_KEY_ID}
      secret_access_key: ${AWS_SECRET_ACCESS_KEY}
```

`build_store` expands `DATA_BUCKET` into the URI percent-encoded and the two keys per value as today;
`OpendalStoreFactory` builds `OperatorUri::new(uri, {access_key_id, secret_access_key})` and
`S3Config::from_uri`. The resulting operator's name is the bucket and its root `/datasets/`, as for
the `type: s3` entry. Liquers queries are unaffected: `-R/remote/a.csv` addresses the mount as
before (the query is unchanged by this design, so no new query needs validating).

## Example 2: Two core stores from URIs

```yaml
stores:
  - { uri: "file:///srv/liquers/data", prefix: data }
  - { uri: "memory://", prefix: scratch }
```

`core_store_factory` resolves both (first in `default_store_factory`): an `AsyncFileStore` rooted at
`/srv/liquers/data` and an `AsyncMemoryStore`. `uri: "fs:///srv/liquers/data"` would instead build
OpenDAL's `fs` service — the two are not interchangeable (`STORE-OPENDAL-SLASH-HANDLING`).

## Corner Cases

| Symptom | Expected | Test |
|---|---|---|
| `type: s3` and `uri: s3://b` | configuration error naming the prefix | `uri_and_type_are_exclusive` |
| no `type`, no `uri` | configuration error naming the prefix | `entry_needs_type_or_uri` |
| `uri: "bucket/data"` | configuration error: no scheme | `uri_without_scheme_is_refused` |
| `uri: nosuch://x` | not supported, lists `file`, `memory`, `s3`, … | `uri_unclaimed_scheme_lists_schemes` |
| two chained factories declare `demo` | the first builds | `uri_scheme_first_in_chain_wins` |
| `${R}` = `a&b=c` in a query value | the service sees `a&b=c` | `uri_variables_are_percent_encoded` |
| `config: { region: x }` with `?region=y` | `x` wins | `uri_config_overrides_query` |
| `file://host/path`, `file://relative` | configuration error | `file_uri_must_be_local_and_absolute` |
| service without `from_uri` | not supported, says write `type` + `config` | `uri_service_without_uri_form` |
| an error from a URI holding `?secret=…` | the message omits the query | `uri_errors_omit_the_query` |
| round trip through YAML | `uri` kept; absent `uri` not written | `uri_round_trips` |

## Test Plan

- Unit, `liquers-core/src/store_config.rs` `mod tests`: `uri_scheme_is_lower_cased`,
  `store_uri_parses_and_decodes`, `uri_variables_are_percent_encoded` (AC-6),
  `uri_without_scheme_is_refused`, `uri_round_trips`.
- Unit, `liquers-core/src/store_factory.rs` `mod tests`: `uri_and_type_are_exclusive`,
  `entry_needs_type_or_uri` (AC-2); `uri_scheme_first_in_chain_wins` (AC-3, two `StoreTypeMap`s
  declaring one scheme); `uri_unclaimed_scheme_lists_schemes` (AC-4); `file_uri_builds_filesystem`,
  `memory_uri_builds_memory`, `file_uri_must_be_local_and_absolute` (AC-5);
  `uri_errors_omit_the_query`.
- Unit, `liquers-store/src/store_factory.rs` `mod tests`: `uri_s3_matches_arguments` (AC-1, beside
  `s3_01_…`, gated on `services-s3`), `uri_config_overrides_query` (AC-6), `fs_uri_is_opendal`
  (AC-5, `services-fs`), `uri_service_without_uri_form` (AC-7, a service whose config keeps the
  default `from_uri`), `uri_schemes_are_declared` (every available type lists one scheme).
- Commands: `cargo test -p liquers-core --lib store_`, `cargo test -p liquers-store --lib
  store_factory`, `bash scripts/check-build-matrix.sh`.

## Documentation and Learning Log

- The audit's one breaking requirement (`resolve`) landed before this design resumed; everything
  left is additive.
- `OperatorUri::new` percent-decodes the path and query and lets extra options override query
  pairs — which is what makes percent-encoded variables and `config:` beside `uri` work.
