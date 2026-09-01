# Phase 2: Solution and Architecture

Gate the two all-Polars targets at crate level. Gate only the egui conversion case in the mixed
shortcut suite. In `registry_export.rs`, gate the committed default registry comparison on the full
feature set, gate the variadic Polars assertion on `polars`, and replace a global command count with
always-on plus feature-specific anchors. Extend native matrix rows to `--tests` and add the missing
image-only configuration.

| Risk | Control |
|---|---|
| Files/crates | Integration tests, matrix script, contributor guidance; `liquers-lib` only. |
| Existing tests | Default suite remains complete; reduced suites retain applicable tests. |
| Validation | Six native feature sets plus 11-row matrix and registry freshness. |
| Compatibility/data/security | Test/build only; no product contract or data change. |
| Performance | Additional compile work in matrix; bounded to existing configurations. |
| Recovery | Revert test gates and matrix rows together if a feature map is wrong. |
| Certainty | High; implementation and per-configuration counts are recorded. |
