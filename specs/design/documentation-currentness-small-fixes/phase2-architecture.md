# Phase 2: Solution and Architecture

## Chosen Solution

Use the current identifiers in prose and remove links that cannot resolve in public rustdoc. Keep
the feature-matrix command stable by replacing its duplicated numeric claim with a pointer to the
script's computed total.

## Exact Changes

| Issue | Files and change |
|---|---|
| `DOCS-STORE-CONFIG-DESCRIBES-ASYNC-FILESTORE-AS-FUTURE-WORK` | `specs/reference/STORE_CONFIG_FSD.md`: name `AsyncFileStore` and remove the future-work sentence. |
| `RUSTDOC-PUBLIC-DOCS-LINK-PRIVATE-ITEMS` | `liquers-core/src/escape.rs`, `plan.rs`, and `store.rs`: render `match_entity`, `CwdCursor::resolve_key`, and `AsyncOpenDALStore` as code prose, retaining the surrounding explanation. |
| `DOCS-BUILD-MATRIX-CONFIGURATION-COUNT-STALE` | `CLAUDE.md`: remove the hard-coded matrix count and direct readers to the script's emitted total. |

## Rejected Alternatives

Do not make private helpers public only to satisfy rustdoc, and do not add a dependency from
`liquers-core` to `liquers-store` merely to create an upstream link. Do not replace the matrix
number with a new manually maintained number.

## Risk Analysis

| Assessment | Result |
|---|---|
| Files | Six authored files: one reference, one guide-level repository instruction, three source documentation comments, and three issue records; generated indexes follow. |
| Impact area | Documentation only: store configuration, public core rustdoc, and build guidance. |
| Module/crate reach | `liquers-core` documentation spans `escape`, `plan`, and `store`; no compiled code changes. |
| Existing-test breakage | None expected; no tests or APIs change. |
| New validation | Re-run `cargo doc -p liquers-core --no-deps` with the two rustdoc warning lints denied; regenerate and check the documentation index. |
| Behavioural risk | Compatibility, persistence, concurrency, performance, security, and error paths are not applicable because only prose/link markup changes. |
| Recovery | Revert the isolated documentation and issue-record files together. |
| Certainty | High: all three warnings and all stale claims were reproduced at HEAD; no open questions or blockers remain. |

## Review

The plan conforms to Phase 1: every edit is a minimal current-documentation correction. Codebase
review confirms `AsyncFileStore` is public and implements `AsyncStore`; the three rustdoc targets
are respectively private, private, and unavailable by the dependency direction. No reusable public
link exists for those exact targets.

