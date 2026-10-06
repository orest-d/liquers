# Phase 4: Implementation Plan

Preconditions: questions 1–2 accepted. If merged with `command-metadata-command-hints`, run this
plan's steps inside that design's plan.

1. **Field + builder** in `command_metadata.rs`. Fix struct literals across the workspace
   (`cargo check --workspace --exclude liquers-web` lists them). Proof: T1, T2. Agent: haiku tier;
   rust-best-practices.
2. **Macro option** in `liquers-macro/src/registration.rs` (parse + codegen + grammar doc). Proof:
   T3 (`cargo test -p liquers-lib --lib --tests register_command_sets_argument_description`).
   Agent: sonnet tier; knowledge: REGISTER_COMMAND_FSD.
3. **Python getter** if applicable. Proof: `cargo check -p liquers-py`.
4. **Registry check:** `cargo test -p liquers-lib --test registry_export` (no regeneration
   expected). Containment: if it fails, the serde attribute is wrong. Fix it rather than
   regenerate.
5. **Docs** per Phase 2 table. Issue resolution, index. Diff review: no registry churn.
