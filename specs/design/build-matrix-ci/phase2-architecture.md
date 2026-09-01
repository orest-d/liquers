# Phase 2: Solution and Architecture

`build-matrix.yml` owns `push` to `main` and `pull_request` triggers with paths covering crate
sources, `Cargo.toml`, `Cargo.lock`, `.cargo/**`, the matrix script, and itself. One Ubuntu job checks
out code, installs the wasm target and `libssl-dev`, applies `Swatinem/rust-cache`, then invokes
`bash scripts/check-build-matrix.sh`.

Rejected alternatives were duplicating configurations in a YAML strategy matrix and running only a
scheduled subset: both create a second row list or delay feedback. The single job is easier to keep
consistent; it can be split later if measured runner time warrants it.

| Risk | Control |
|---|---|
| Files/workflows | One workflow; script remains canonical. |
| Existing checks | Docs workflow remains independent. |
| Validation | Parse workflow, inspect paths, run matrix locally, observe CI conclusion. |
| Compatibility/data/security | No product data; third-party actions remain version-pinned and reviewable. |
| Performance | Cache dependencies; split only from measured CI duration. |
| Recovery | Revert workflow commits without touching crate code. |
| Certainty | High; implementation and resolution are present at HEAD. |
