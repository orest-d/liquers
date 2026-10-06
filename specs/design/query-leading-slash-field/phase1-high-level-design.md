# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** needs-decision
- **Leading issue:** **Open design question — rename the public field `Query::absolute`.** Since
  the issue was filed, the leading `/` has gained meaning: it roots the query's resource segments
  at the logical root, independent of the live CWD. The name collision with `Key::as_absolute` /
  `Key::to_absolute` remains, but the field is no longer a mere syntactic flag.
- **Explanation:** Both answers are small. The documentation correction is needed under either
  answer and is specified as a separate first step.
- **Open questions:**
  1. **Proposed resolution — rename to `rooted`, keep the wire name.** The Rust field becomes
     `rooted` with `#[serde(rename = "absolute")]`, so stored and transmitted queries are
     unchanged (they are persisted inside metadata, e.g. `MetadataRecord.query`). Python keeps an
     `absolute` getter as a deprecated alias of a new `rooted` getter.
  2. **Alternative — keep the name, document the meaning.** No API change. The collision stays.

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
2. If renamed: the Rust field is `rooted`. Serialized form is unchanged (`"absolute"`), proven by a
   test that deserializes a stored legacy plan (`plan.rs` ≈4952 already has such JSON). Equality,
   hashing and `encode` are unchanged.
3. If renamed: `liquers-py` exposes `rooted`, and keeps `absolute` returning the same value.
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
- Keeping the wire name avoids any stored-query migration, independent of the decision.
