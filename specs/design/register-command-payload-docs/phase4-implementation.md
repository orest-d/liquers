# Phase 4: Implementation Plan - Documenting `payload: required`

1. **Re-verify the grammar.** Read `liquers-macro/src/registration.rs` `CommandSignatureStatement`
   parse arm for `"payload"` and the emitter block for `payload_required_code`; stop and revise
   Phase 2 if either changed. Run
   `cargo test -p liquers-core --test volatility_integration test_payload_required`.
2. **FSD.** `specs/reference/REGISTER_COMMAND_FSD.md`: example block line, table row, the
   §Injected Parameters sentence, History row, `reviewed:` bump.
3. **Guide.** `specs/guides/COMMAND_REGISTRATION_GUIDE.md`: DSL bullet, "Commands that need the
   payload" subsection with the Phase 3 snippet, History row, `reviewed:` bump.
4. **CLAUDE.md.** Append `payload:` to the DSL metadata list with the bare-identifier note.
5. **Records.** Close `specs/issues/REGISTER-COMMAND-PAYLOAD-STATEMENT-UNDOCUMENTED.md` with a
   resolution noting `PAYLOAD_GUIDE.md` already covered it and the three documents now do. Leave
   `REGISTER-COMMAND-EXPIRES-AND-VERSION-STATEMENTS-UNDOCUMENTED` open. Run
   `python3 scripts/docs_index.py` and `python3 scripts/docs_index.py --check`.
6. **Review.** Diff contains only the three documents plus generated index files and the issue;
   no restated inheritance rules; links resolve.

## Final Review

Documentation-only, consistent with the macro and the existing test. Rollback is a text revert.

## Post-Phase-4 Review (2026-10-05)

- **Problem still valid:** yes for the FSD table (rows stop at `volatile`, ≈364), `CLAUDE.md`
  (≈366) and the registration guide. `PAYLOAD_GUIDE.md` already covers it, as the design notes.
- **Solution correct:** yes. Link to `PAYLOAD_GUIDE.md` rather than duplicating it.
- **Unnecessary:** none.
- **Detail / tests:** sufficient for a docs change.
- **Interactions:** **merge with `REGISTER-COMMAND-EXPIRES-AND-VERSION-STATEMENTS-UNDOCUMENTED`.**
  It is the same FSD table, the same `CLAUDE.md` line and the same guide bullet. Doing them
  separately costs two History rows per document for one coherent edit, and the second issue is
  P3/S with no design of its own. `context-title-description` also edits
  `COMMAND_REGISTRATION_GUIDE.md` (History-row conflict only).
- **Verdict:** ready. Widen the scope to `expires:` / `version:`.
