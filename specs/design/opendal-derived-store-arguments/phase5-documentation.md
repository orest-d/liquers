# Phase 5: Documentation - Derived OpenDAL Store Arguments

**Status: executed 2026-10-07**, after implementation (Wave 3 step 23 of
`archive/2026-10-06-p2-p3-s-implementation-order-revised.md`). Awaiting approval.

## Completion Preconditions

- [x] Phase 4 steps 1-5 complete; tests and build matrix pass
- [x] User and review comments answered or incorporated

## Documentation Plan

### New Reference Documents

None.

### New Guide Documents

None.

### Existing Documents to Update (`affects_docs`)

| Document | Planned change |
|---|---|
| `reference/STORE_CONFIG_FSD.md` | §How complete is an argument list? (≈643): OpenDAL arguments are derived from the linked service's config for compiled-in services, with Liquers' docs merged on; still `Partial`; an uncompiled service shows the hand-written list. History row + `reviewed:` |
| `guides/STORE_FACTORY_GUIDE.md` | §Describing arguments you do not own (≈140): when the backend exposes a `Default + Serialize` config, derive with `StoreArgumentInfo::derived` and merge hand docs by name; link the OpenDAL factory as the worked example. History row + `reviewed:` |

### Candidates Considered and Discarded

By area (`store/backends`, `store/config`): `STORE_SEMANTICS.md`, `STORE_IMPLEMENTATION_GUIDE.md`
(behavioural contract, not descriptions); `ENVIRONMENT_CONFIG.md` (no argument lists).

### Links and Capability Map

None: the Stores section already points at `STORE_CONFIG_FSD.md`.

### Issues to Close

`STORE-OPENDAL-ARGUMENTS-NOT-DERIVED` → `status: closed`, resolution naming the six tests.

## Implementation Summary

All in `liquers-store/src/store_factory.rs`, as Phase 2 planned:

- `derived_arguments::<C: Configurator + Default>()` serializes `C::default()` and maps each field
  through `StoreArgumentInfo::derived`.
- `service_arguments(store_type)` has one `#[cfg]`-gated arm per `OPENDAL_STORE_TYPES` entry (21
  types, 20 configs; `http`/`https` share `HttpConfig`; `sftp` gated on
  `all(opendal, any(unix, services-sftp))`).
- `common_arguments` is renamed `hand_written_arguments`; its "for now" section is replaced by the
  merge rule. `OpendalStoreFactory::arguments` merges hand over derived by name
  (`merge_argument`: name/default derived, doc/label/required hand, type hand only when derived is
  `Any`); hand names absent from the config are dropped; uncompiled services keep the hand list.
- `type_info` still marks every type `Partial` against `OPENDAL_DOCS`.
- Tests `derive01`–`derive04`, `s3_01_arguments_and_uri_agree`,
  `s3_02_missing_region_fails_at_construction` (the last two gated on `services-s3` and
  `async_store`, since they construct a store). `coverage02`'s comment no longer says
  `atomic_write_dir` is undescribed.

## Documentation Delivered

| Document | Change |
|---|---|
| `reference/STORE_CONFIG_FSD.md` | §How complete is an argument list?: derived lists, merge rule, still `Partial`, uncompiled fallback. History row, `reviewed: 2026-10-07` |
| `guides/STORE_FACTORY_GUIDE.md` | §Describing arguments you do not own: derive from a `Default + Serialize` config and merge by name; OpenDAL factory as worked example. History row, `reviewed: 2026-10-07` |

`STORE-OPENDAL-ARGUMENTS-NOT-DERIVED` closed.

## Issues Filed

None.

## Important Learning

OpenDAL's S3 builder silently reads the AWS environment and profile unless `disable_config_load`
is set, so an offline test of a missing argument must set it; `s3_02` passes with and without
`AWS_REGION=eu-west-1` in the environment.

## Conformance and Remaining Work

Conforms to Phases 2-4. No remaining work.

## Validation

- `cargo test -p liquers-store --lib store_factory`: 24 passed
- `AWS_REGION=eu-west-1 cargo test -p liquers-store --lib s3_02`: passed
- `cargo test -p liquers-store --no-default-features --features opendal,services-fs --lib store_factory`: 17 passed
- `bash scripts/check-build-matrix.sh` (run with the wave)
- `python3 scripts/docs_index.py --check`
