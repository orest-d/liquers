# Phase 4: Implementation Plan - `Context::set_title` and `Context::set_description`

Precedence was decided on 2026-10-04 (recipe wins, per field); no decision gate remains.

1. **Flags.** `liquers-core/src/assets.rs`: add `recipe_sets_title` / `recipe_sets_description`
   to `AssetData` with doc comments; initialise `false` in `AssetData::new` (and any other
   struct literal the compiler reports); clear both in `AssetData::reset`. Add
   `reset_clears_recipe_description_flags`. Proof: `cargo test -p liquers-core --lib reset_clears`.
2. **Set the flags on adoption.** Same file, the recipe-adoption block in
   `AssetRef::evaluate_recipe_outcome` (≈3036-3050): set both flags from `!recipe.title.is_empty()`
   / `!recipe.description.is_empty()` under the lock already held there. Depends on 1.
3. **Command-side write.** Same file: add `AssetRef::set_description_fields_from_command` after
   `set_description_fields`; extend `set_description_fields`' doc to point to it. Add the two
   `command_description_fields_*` unit tests. Depends on 1.
   Proof: `cargo test -p liquers-core --lib command_description_fields`.
4. **Context methods.** `liquers-core/src/context.rs`: `set_title` and `set_description` after
   `set_filename`, with the Phase 2 doc comments. Depends on 3.
5. **Integration tests.** Add `liquers-core/tests/context_title_description.rs` (five tests).
   Validate its queries first:
   `cargo run -p liquers-core --features cli --bin liquers-validate -- --command titled --command plain -- titled plain`.
   Proof: `cargo test -p liquers-core --test context_title_description`.
6. **Docs and records.** `specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md`: add both
   methods where metadata-writing context methods are listed (≈423, ≈469), with the
   recipe-wins-per-field rule; History row + `reviewed:`. `specs/guides/COMMAND_REGISTRATION_GUIDE.md`:
   short example in the context section stating that a recipe's title takes precedence; History
   row + `reviewed:`. Close `specs/issues/CONTEXT-CANNOT-SET-TITLE-OR-DESCRIPTION.md` with a
   resolution recording the decision. Regenerate and check the docs index.
7. **Checks.** `cargo fmt`, `cargo test -p liquers-core --lib --tests`,
   `cargo test -p liquers-lib --lib --tests`, `cargo check -p liquers-core --target
   wasm32-unknown-unknown`. Diff review: no `liquers-py`/`liquers-web` edits, no status checks or
   locks added to the setters beyond the data lock, no `println!`.

## Final Review

Phases 1-4 agree with the recorded decision; no open question remains. Rollback: remove the
`Context` methods and the command-side write; the flags are inert without them.

## Post-Phase-4 Review (2026-10-05)

- **Problem still valid:** yes. `Context` has `set_filename` (`context.rs` ≈877) but no title or
  description setter; `set_metadata_description` and the recipe adoption block (`assets.rs` ≈3044)
  are as described.
- **Solution correct:** yes. Two private flags is the smallest way to implement "recipe wins", and
  the check and the write happen under one lock.
- **Unnecessary abstractions:** none. Reusing `set_metadata_description` keeps the
  legacy-metadata refusal in one place.
- **Detail:** sufficient.
- **Tests:** sufficient for the criteria. Two cases are missing: (a) run the recipe tests on
  `ImmediateAssetManager` as well as the default manager (the flags live on `AssetData`, but
  adoption runs on both managers' paths); (b) a nested `apply` chain where two steps call
  `set_title` and the later one wins (Phase 1 states this, nothing tests it).
- **Interactions:** shares `assets.rs` persistence code with `save-to-store-skip-outcome`, with no
  semantic overlap. Both this design and `register-command-payload-docs` edit
  `COMMAND_REGISTRATION_GUIDE.md`, so expect History-row conflicts only. Independent.
- **Verdict:** ready.
