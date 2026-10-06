# Phase 2: Solution and Architecture

## Step A (both answers): documentation

- `liquers-core/src/query.rs` module doc (≈67): replace "It currently has no semantic meaning" with
  "A leading `/` roots the query's resource segments at the logical root, independent of the
  current working directory; links inside the query keep resolving against the live CWD
  (see `CwdState::resolve_query_scoped`)."
- Field doc (≈2220): the same meaning, in one sentence.

## Step B (rename answer)

```rust
pub struct Query {
    pub segments: Vec<QuerySegment>,
    /// The text had a leading `/`: resource segments are rooted at `/`, independent of the CWD.
    #[serde(rename = "absolute")]
    pub rooted: bool,
    pub source: QuerySource,
}
```

Then update every use (`rg "\.absolute\b|absolute:" liquers-*/src --type rust` lists them:
`query.rs` constructors ≈1692/1702/2383/2458/2619–2643/2746/2799 and logic ≈2295/2769–2854,
`plan.rs` ≈1632/2215, and tests in `parse.rs`/`context.rs`). Rename the local
`absolute_resource_cursor` to `rooted_resource_cursor` for consistency.

`liquers-py/src/query.rs`: add `#[getter] fn rooted(&self) -> bool`, and keep `absolute` with a
doc comment "deprecated alias of `rooted`".

## Rejected alternatives

- Rename the wire name too. It would break stored queries in metadata, for a cosmetic gain.
- `had_leading_slash`. It describes syntax, not the meaning the flag now has.

## Risk Review

| Risk | Validation and recovery |
|---|---|
| Wire compatibility | Test: deserialize the legacy plan JSON (`plan.rs` ≈4952) and a serialized query; assert the key is still `absolute`. |
| Python API | Both getters exist; `cargo check -p liquers-py` |
| Missed use site | The compiler (public field) |
| Recovery | Revert the rename; Step A stands alone |
