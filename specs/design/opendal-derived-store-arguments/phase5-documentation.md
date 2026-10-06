# Phase 5: Documentation - Derived OpenDAL Store Arguments

**Status: plan.** Written on 2026-10-05; executed after implementation.

## Completion Preconditions

- [ ] Phase 4 steps 1-5 complete; tests and build matrix pass
- [ ] User and review comments answered or incorporated

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

*Pending.*

## Documentation Delivered

*Pending.*

## Issues Filed

*Pending.*

## Important Learning

*Pending.* Seed: OpenDAL's S3 builder silently reads the AWS environment unless
`disable_config_load` is set, so offline tests must set it.

## Conformance and Remaining Work

*Pending.*

## Validation

*Pending.* Planned: `python3 scripts/docs_index.py --check`.
