# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** ready
- **Automatic fixing:** not-eligible — renames the `pub` field `Query::absolute` and its serialized
  name (rule 4), in `liquers-core` and `liquers-py` (rule 6)
- **Leading issue:** None
- **Explanation:** Decided (Maintainer decision, 2026-10-10): rename the field to `rooted`, **including
  the serialized (wire) name**, with no `serde(rename)` and no Python alias. Backward compatibility
  is not required at this stage. The documentation correction is still the first step.
- **Open questions:** None.

## What the leading `/` does (corrected 2026-10-10)

A resource key without `.` or `..` is already absolute: `-R/data/x.csv` and `/-R/data/x.csv` both
read the key `data/x.csv`, wherever the query runs. The leading `/` only matters for a resource key
that **is** relative. Inside a recipe folder `reports` (live CWD `reports`):

| Query | Resource key read |
|---|---|
| `-R/./data/x.csv/-/to_text` | `reports/data/x.csv` (`.` is the live CWD) |
| `/-R/./data/x.csv/-/to_text` | `data/x.csv` (`.` is the root, because the query is rooted) |

Links inside the query (`~X~-R/./linked~E`) keep resolving against the live CWD in both cases
(`query.rs` test `cwd_cursor_absolute_query_uses_private_root_without_fallback`).

## Problem (at HEAD)

`liquers-core` uses "absolute" for three things in one module:

| Name | Means | Location |
|---|---|---|
| `Query::absolute` | the text had a leading `/`; **resource segments are rooted at `/`, independent of the CWD** | `query.rs` ≈2224, used by `resolve_query_scoped` ≈2295 and `Plan::absolute_query_resource_step_index` |
| `Key::to_absolute(cwd)` | resolve `.`/`..` against a working directory | `query.rs` ≈1600 |
| `Key::as_absolute()` | assert there is nothing to resolve | `query.rs` ≈1573 |

The module doc (`query.rs` ≈67) and the field doc (≈2220: "independent of relative `.` and `..`
resolution") are stale. They describe the flag as meaningless.

## Expected behaviour and acceptance

1. Documentation (both answers): the module doc and the field doc state that a leading `/` roots
   the query's resource segments at `/`, and that child links keep using the live CWD (as
   `PROJECT_OVERVIEW.md` says).
2. The Rust field is `rooted` and serializes as `"rooted"`. Equality, hashing and `encode` (the
   leading `/` in query text) are unchanged.
3. `liquers-py` exposes `rooted`; the `absolute` getter is removed.
4. `liquers-web` is unaffected (it exposes no such field; verified: `objects.rs` has none).

## Scope

The field, its uses (`query.rs`, `plan.rs` ≈1632/≈2215, `parse.rs` tests, `context.rs` tests),
`liquers-py/src/query.rs` ≈506, and the docs.

## Design Dependencies

None. (`store-key-guard`, complete, filed the issue.)

## Documentation assessment

Code docs in `query.rs`. `specs/reference/PROJECT_OVERVIEW.md` §query: one sentence naming the
field. Per CLAUDE.md, a Query encoding change requires a PROJECT_OVERVIEW update. The encoding does
not change, so only the name is updated.

## Consolidated Findings

- The meaning now exists, so `had_leading_slash` (the issue's first proposal) would describe syntax
  rather than semantics. `rooted` names the semantics.
- Stored metadata is not affected by the wire rename: `MetadataRecord.query`, `LogEntry.query` and
  `AssetInfo.query` serialize the query as encoded text (`query_format` / `option_query_format`).
  Only struct-serialized queries change shape, chiefly serialized `Plan`s (the legacy plan JSON in
  `plan.rs` tests). A stored plan JSON written before the change no longer deserializes; accepted
  by the maintainer decision.
- The earlier example of this design wrongly implied that `/-R/data/x.csv` differs from
  `-R/data/x.csv`; it does not (corrected above, maintainer review 2026-10-10).
