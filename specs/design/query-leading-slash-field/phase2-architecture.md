# Phase 2: Solution and Architecture

## Step A: documentation

- `liquers-core/src/query.rs` module doc (≈67): replace "It currently has no semantic meaning" with
  "A leading `/` makes the query rooted: relative (`.`/`..`) keys in its resource segments resolve
  against the logical root instead of the current working directory; links inside the query keep
  resolving against the live CWD (see `CwdCursor::resolve_query_scoped`)."
- Field doc (≈2220): the same meaning, in one sentence.

## Step B: rename, wire name included (maintainer decision, 2026-10-10)

```rust
pub struct Query {
    pub segments: Vec<QuerySegment>,
    /// The text had a leading `/`: relative keys in resource segments resolve against `/`, not the CWD.
    pub rooted: bool,
    pub source: QuerySource,
}
```

No `#[serde(rename)]`. Update every use (`rg "\.absolute\b|absolute:" liquers-*/src --type rust`
lists them: `query.rs` constructors and logic, `plan.rs` including
`absolute_query_resource_step_index` → `rooted_query_resource_step_index` and the legacy plan JSON
in its tests, and tests in `parse.rs`/`context.rs`). Rename the local `absolute_resource_cursor` to
`rooted_resource_cursor`.

`liquers-py/src/query.rs`: replace the `absolute` getter with `rooted`.

`Key::to_absolute`, `Key::as_absolute`, `Query::to_absolute` keep their names: they are about
resolution and assertion, and the rename removes the collision.

## Rejected alternatives

- Keep the wire name with `serde(rename = "absolute")`: rejected by the maintainer; no compatibility
  is required yet.
- `had_leading_slash`: describes syntax, not the meaning.

## Risk Review

| Risk | Validation and recovery |
|---|---|
| Stored plan JSON | Struct-serialized plans written before the change fail to deserialize. Accepted. Metadata stores queries as encoded text and is unaffected (T3). |
| Python API | `absolute` getter removed; `cargo check -p liquers-py` |
| Missed use site | The compiler (public field) |
| Recovery | Revert the rename; Step A stands alone |
