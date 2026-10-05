# Phase 4: Implementation Plan - Documenting `payload:`, `expires:` and `version:`

1. **Re-verify the grammar.** Read `CommandSignatureStatement` parse arms for `payload`, `expires`,
   `version` and the emitter (`payload_required_code`, `expires_code`, `impl_version_code`); stop and
   revise Phase 2 if any changed. Run the three Phase 3 test commands.
2. **FSD.** `specs/reference/REGISTER_COMMAND_FSD.md`: example block, three table rows, the
   §Injected Parameters sentence, the "Implementation versions" subsection, History row,
   `reviewed:` bump.
3. **Guide.** `specs/guides/COMMAND_REGISTRATION_GUIDE.md`: DSL bullet, the two subsections with the
   Phase 3 snippets, History row, `reviewed:` bump.
4. **CLAUDE.md.** Extend the DSL metadata list.
5. **Phase 5.** Execute `phase5-documentation.md` (closes both issues).
6. **Review.** Diff contains only the three documents, the generated index files and the issues; no
   restated inheritance rules or expiration grammar; links resolve.

## Final Review

Documentation-only, consistent with the macro and existing tests. Rollback is a text revert.

## Post-Phase-4 Review Resolution (2026-10-05)

The review's recommendation is applied: `REGISTER-COMMAND-EXPIRES-AND-VERSION-STATEMENTS-UNDOCUMENTED`
is merged into this design (Phases 1-4 rewritten for the wider scope, including the
`#[command_version]` requirement and the `version: now` caution), and Phase 5 holds the
documentation plan.
