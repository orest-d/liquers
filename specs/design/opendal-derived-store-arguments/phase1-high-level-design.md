# Phase 1: High-Level Design - Derived OpenDAL Store Arguments

## Design Readiness

- **Readiness:** ready
- **Automatic fixing:** not-eligible — changes the advertised OpenDAL store arguments, part of the
  config schema (rule 4), across two crates (rule 6)
- **Leading issue:** None
- **Explanation:** The prerequisite (`STORE-OPENDAL-SERVICES-NOT-ENABLED`) is closed; OpenDAL 0.55's
  service configs were inspected (2026-10-05): each is `Default + Serialize`, `#[serde(default)]`,
  `#[non_exhaustive]`, exported as `opendal::services::<Name>Config` behind `services-<name>`, and
  none uses `skip_serializing_if`. So `serde_json::to_value(C::default())` names every field. The
  plan below is complete. Rewritten after the post-Phase-4 review, which found the first version
  to be template text.
- **Open questions:** None

## Problem and Evidence

`OpendalStoreFactory::common_arguments` (`liquers-store/src/store_factory.rs` ≈103) hand-writes
three to five arguments per OpenDAL store type, and adds `access_key_id` to every type — including
`fs` and `http`, which have no such option. The real option set is OpenDAL's and changes on its
release schedule. Two tests specified by `design/store-factories-in-core/` Phase 3 (`s3_01`,
`s3_02`) were never added. `StoreArgumentInfo::derived` (`liquers-core/src/store_factory.rs` ≈114)
exists for this and has one user, a round-trip test.

## Expected Behaviour and Acceptance Criteria

1. For every `OPENDAL_STORE_TYPES` entry whose service is compiled into this build, the reported
   arguments are the service config's fields (from `C::default()`), with the hand-written `doc`,
   `required` and type merged onto entries of the same name.
2. A hand-written argument whose name the service config does not have is **not** reported for
   that type (no more `access_key_id` on `fs`).
3. For a store type whose service is not compiled in, the hand-written list is reported unchanged.
4. Coverage stays `ArgumentCoverage::Partial` with the OpenDAL authority: a default value says
   nothing about which options are required or how they interact.
5. Tests assert the **presence** of long-stable names (`bucket`, `region`, `root` for `s3`), never
   that a list is exhaustive.
6. `s3_01`: an `s3` store built from arguments and the same configuration as a URI agree, offline.
7. `s3_02`: an `s3` store without `region` fails at construction, offline and independently of
   the machine's AWS environment.
8. Every feature configuration in `scripts/check-build-matrix.sh` still builds.

## Affected Users

Anyone reading store-type descriptions: the `store_types` listing, coding agents, a future
configuration UI.

## Scope and Non-Goals

Non-goals: `ArgumentCoverage::Complete` for OpenDAL types; per-field documentation beyond what is
hand-written today; the `opendal_<scheme>` escape hatch (no table entry, no description).

## Compatibility

Additive in content: descriptions grow and lose the wrong `access_key_id` entries. No API change.

## Documentation Assessment

Update `reference/STORE_CONFIG_FSD.md` §How complete is an argument list? and
`guides/STORE_FACTORY_GUIDE.md` §Describing arguments you do not own (derive, then merge docs).

## Design Dependencies

| Relationship | Target | Effect |
|---|---|---|
| owns | `STORE-OPENDAL-ARGUMENTS-NOT-DERIVED` | — |
| overlaps | design `store-factories-in-core` (complete) | its deferred steps 9 and 11 are carried out here |
| overlaps | design `opendal-list-option-config` (implemented) | list-valued fields default to `null` (type `Any`); the merged doc for such a field says a YAML list is comma-joined (`STORE_CONFIG_FSD.md` §Configuration values and the OpenDAL string boundary) |
| requires (done) | `STORE-OPENDAL-SERVICES-NOT-ENABLED` (closed) | service configs are nameable |
