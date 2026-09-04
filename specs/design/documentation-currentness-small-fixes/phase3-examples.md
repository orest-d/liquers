# Phase 3: Examples and Tests

| Case | Current evidence | Corrected expectation |
|---|---|---|
| Filesystem configuration | The reference names `FileStore` and says `AsyncFileStore` is future work. | It names the existing native async implementation and preserves the same YAML. |
| Public rustdoc | `cargo doc -p liquers-core --no-deps` reports three broken/private intra-doc links. | The same command with those warning lints denied succeeds. |
| Build-matrix guidance | `CLAUDE.md` says the script has 11 configurations while the script computes 20. | The guide does not duplicate a count; readers use the script's final total. |

## Regression Checks

1. Search `STORE_CONFIG_FSD.md` for `AsyncFileStore` and confirm the obsolete future-work sentence
   is absent.
2. Run `cargo doc -p liquers-core --no-deps` with `broken_intra_doc_links` and
   `private_intra_doc_links` denied.
3. Search `CLAUDE.md` for the obsolete `11 configurations` text and confirm the command points to
   the script's output.
4. Regenerate and check the documentation index after closing the three issues.

No unit or integration test changes are needed: the repair changes rendered documentation only.

