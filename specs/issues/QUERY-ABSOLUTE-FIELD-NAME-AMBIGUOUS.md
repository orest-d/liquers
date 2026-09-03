---
id: QUERY-ABSOLUTE-FIELD-NAME-AMBIGUOUS
kind: issue
title: Query::absolute names a leading slash, colliding with two other meanings of absolute
status: draft
priority: P3
complexity: S
area: [core/query]
design: query-leading-slash-field
created: 2026-08-17
github:
---
## Problem

`liquers-core` now uses "absolute" for three different things in one module:

| Name | Means | Location |
|---|---|---|
| `Query::absolute` | the textual form had a leading `/` | `query.rs:2151` |
| `Key::to_absolute(cwd)` | *resolve* `.` and `..` against a working directory | `query.rs:1525` |
| `Key::as_absolute()` | *assert* there is nothing to resolve | added by `store-key-guard` |

The first is the odd one out, and it is also the weakest: its own documentation says it is
"independent of relative `.` and `..` resolution" and that it "currently has no semantic meaning"
(`query.rs:67`, `:2148`). It is a syntactic fact about the input text, not a property of what the
query addresses — yet it carries the word that the other two use for exactly that property.

A reader who meets `query.absolute == false` next to `key.as_absolute()?` has no reason to guess
they are unrelated.

## Impact

Readability only; no behavioural defect. The field is part of `Query` equality, hashing and
`encode`, so it *is* load-bearing — just not for what its name suggests.

Filed because the `store-key-guard` design had to reason about the collision and decided not to
touch it: renaming a public field that appears in serialized queries is a wire-visible change, and
it did not belong inside a P0 security fix. That reasoning does not make the collision go away.

## Expected behaviour

Rename the field to name what it holds — `had_leading_slash`, or `rooted` if the leading `/` is
ever given meaning — and keep `absolute` for the address property that `Key::as_absolute` tests.

Complexity `S` reflects the mechanical change; the care is in the blast radius, not the diff:

- `Query::absolute` is `pub` and part of `PartialEq`/`Hash`, so anything comparing queries is
  affected by a rename only cosmetically, but anything *constructing* a `Query` literally must be
  updated.
- It is `Serialize`/`Deserialize`, so a stored or transmitted query written by one build would not
  deserialize on the other unless `#[serde(rename = "absolute")]` preserves the wire name. Decide
  deliberately whether the wire name changes.
- `liquers-py` and `liquers-web` expose query structure; check whether the field name is visible
  there.

Alternatively, decide the field should carry meaning after all and document that instead — but
"currently has no semantic meaning" and a name implying one is the worst of both.

## Discovery

Recorded 2026-08-17 while designing `specs/design/store-key-guard/`, which introduces
`Key::as_absolute` and had to state explicitly in its Phase 2 document that this is not the same
concept as `Query::absolute`. Filed so the observation survives that design folder.
