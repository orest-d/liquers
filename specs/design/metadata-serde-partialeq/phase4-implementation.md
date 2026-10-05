# Phase 4: Implementation Plan - Serde and Equality for `Metadata`

1. **Derive `PartialEq`.** `liquers-core/src/metadata.rs`, `enum Metadata`: change
   `#[derive(Debug, Clone)]` to `#[derive(Debug, Clone, PartialEq)]`.
   Proof: `cargo check -p liquers-core`. Containment: one-word revert.
2. **Add `Serialize` / `Deserialize`.** Same file, directly after `impl Default for Metadata`:
   the two impls from Phase 2, each with a doc comment naming `to_json` / `from_json_value` as
   the form it mirrors. Depends on nothing. Proof: step 3 tests.
3. **Tests.** Add the five Phase 3 tests to `mod tests` in `metadata.rs`.
   Proof: `cargo test -p liquers-core --lib metadata::tests`.
4. **Refresh the consumer comment.** `liquers-records/src/batch.rs`, doc comment of
   `ChunkDescriptor`: replace the sentence saying `Metadata` lacks the traits with one saying
   `Metadata` now has them and the remaining reason `ChunkDescriptor` is `Debug + Clone` only is
   that it is not on a wire format yet and `Query`'s serialized form would need choosing. Do not
   change its derives. Proof: `cargo test -p liquers-records --all-features --lib --tests`.
5. **Records.** Set `specs/issues/METADATA-LACKS-SERIALIZE-DESERIALIZE-AND-PARTIALEQ.md`
   `status: closed` with a `## Resolution` naming the tests; update this design's lifecycle;
   `python3 scripts/docs_index.py` then `python3 scripts/docs_index.py --check`.
6. **Checks and review.** `cargo fmt`, `cargo test -p liquers-core --lib`,
   `cargo test -p liquers-lib --lib --tests` (consumes `Metadata` widely), and
   `cargo clippy -p liquers-core`. Review the diff for: an externally tagged form slipping in,
   any `unwrap` outside tests, `println!`, and edits beyond the files above.

## Final Review

Phases agree: the wire form is fixed by existing code, so no decision is open. The only
cross-crate edit is a doc comment. Rollback is the revert of steps 1-2 and their tests.

## Post-Phase-4 Review (2026-10-05)

- **Problem still valid:** yes. `enum Metadata` derives only `Debug, Clone` (`metadata.rs`
  ≈1819).
- **Solution correct:** yes. The hand-written untagged impls reproduce `to_json` /
  `from_json_value` exactly.
- **Unnecessary:** the stated motivating consumer (`ChunkDescriptor`) still cannot derive the
  traits afterwards because of `Query`'s serialized form, so `Serialize`/`Deserialize` have no
  in-tree user yet. They are cheap, so this is not a blocker. `PartialEq` is useful on its own
  (tests, snapshot comparisons).
- **Detail / tests:** sufficient. The variant-choice test against `from_json` is the right guard.
- **Interactions:** none.
- **Verdict:** ready (low value, low risk).
