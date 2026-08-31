# Phase 2: Solution and architecture

Refactor check eight in `scripts/docs_index.py` into a small local scan over tracked Markdown paths: `specs/README.md`, `issues/*.md`, `design/*/*.md`, `reference/**/*.md`, and `guides/**/*.md`; never traverse `archive/`. For every current regex match, split a fragment at `#`, skip empty, `http://`, `https://`, and absolute-path targets, then resolve the remaining target against `path.parent`. Emit `relative-file: dead link target` errors.

Fix the existing non-archive corpus in the same implementation: correct relative depth in API references, replace machine-local links with repository-relative links, and repair/remove targets that moved. Do not resolve all links from `SPECS`: that is precisely why nested document links are currently false. Do not broaden into URL checking or archive rewriting.

| Risk | Affected files/workflow | Validation and containment | Certainty |
|---|---|---|---|
| False positives on anchors | all docs checks | strip fragment before lookup; test anchor link | High |
| Historical archive rewrite | archive integrity | explicit traversal exclusion test | High |
| Source-link depth errors | API references | test source target from nested document | High |
| Massive blocking rollout | docs CI | repair known findings in same change, then error | High |

The script already uses `Path.resolve()` and returns errors from `--check`; no new dependency or parser is needed. Keep the restricted regex as a documented scope boundary.
