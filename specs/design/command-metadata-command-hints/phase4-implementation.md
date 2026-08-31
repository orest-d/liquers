# Phase 4: Implementation plan

1. In `liquers-core/src/command_metadata.rs`, add the serde-compatible map, constructors, and builder. Prove with Phase 3 metadata round-trip tests; rollback is removal of the additive field before any registry entry uses it.
2. In `liquers-macro/src/registration.rs`, add the proposed command statement grammar, duplicate-key rejection, and metadata application. Prove with macro compilation/registration tests; contain syntax risk by keeping it string-only.
3. Add focused core and macro tests from Phase 3, including empty-registry byte identity. Run focused crates and `registry_export`/its existing equivalent.
4. Update applicable command registration/declaration documentation, issue resolution notes, and `specs/index.csv` through `python3 scripts/docs_index.py`; run `--check`.
5. Format, run focused and crate tests, and review the final diff for registry churn, accidental parameter-hint work, and unapproved syntax expansion.
