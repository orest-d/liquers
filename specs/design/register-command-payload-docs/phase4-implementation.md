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
