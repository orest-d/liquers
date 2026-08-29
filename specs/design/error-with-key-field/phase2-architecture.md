# Phase 2: Solution and Architecture - Error::with_key Populates the Key Field

## Overview

Change `Error::with_key` to assign `self.key = Some(key.encode())`. Keep `with_query` unchanged,
and review constructors that currently set both fields before deciding whether they intentionally
represent query context or only copied an old mistake.

## Known-Issue Preflight

| Issue | Status | Priority | Relevance and action | Blocking? |
|---|---|---|---|---|
| `LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT` | accepted | P3 | Related to error transport but does not constrain key/query separation. | no |
| `WEB-LIQUERSERROR-NOT-CONSTRUCTIBLE` | accepted | P3 | Web exposes error fields; this fix improves consumed data but does not require JS constructor work. | no |

## Files and Symbols

Primary file: `liquers-core/src/error.rs`, method `Error::with_key` and any constructors that set
`query` from a key. Focused tests should be added in the same module's test section or an existing
core error test target.

## Data, Ownership, Serialization and Errors

No new data field. The existing owned `String` from `Key::encode()` moves from the `query` option
to the `key` option. Serialized shape is stable; values under keys become more correct.

## Sync, Async and API Effects

Pure synchronous builder change. Public API signatures do not change; observable behaviour changes
only for callers reading `Error.key`, `Error.query` or serialized `Metadata::error_data`.

## Alternatives

Rejected: set both `query` and `key` in `with_key`; this preserves the ambiguity and violates the
separate accessor contract. Rejected: add a migration layer; persisted historical errors are
diagnostic records and can remain as written.

## Risk Assessment

| Assessment | Record |
|---|---|
| Files | 1 source file and 1 focused test location, plus specs/index. |
| Impact area | Error enrichment and serialized error metadata. |
| Module/crate reach | Confined to `liquers-core`; web impact is consumption only. |
| Existing-test breakage | Low; tests that asserted the bug would fail, none expected. |
| New validation | Unit tests for `with_key`, `with_query`, and one constructor with key context. |
| Behavioural risk | Persistence compatibility is shape-compatible but field values differ for future errors; no concurrency/performance/security impact. |
| Recovery | Revert one assignment and tests. |
| Certainty | High for builder bug; medium for dependency constructor intent until call sites are checked. |

## Rust Review

No ownership complexity, no new errors, no unwraps, no async and no crate dependency change. The
design keeps existing typed constructors and serialized fields.
