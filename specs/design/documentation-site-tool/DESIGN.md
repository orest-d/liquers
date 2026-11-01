---
id: DOCUMENTATION-SITE-TOOL
kind: design
title: A documentation tool builds the user documentation site with rustdoc, Liquers queries and Mustache
workflow: liquers-project
status: in_review
phase: architecture
area: [docs, build, lib/commands]
issues: [USER-DOCUMENTATION-CANNOT-BE-GENERATED]
affects_docs: [guides/TEMPLATE_GUIDE.md]
created: 2026-11-01
---
# A documentation tool builds the user documentation site with rustdoc, Liquers queries and Mustache

GitHub #233, requested 2026-11-01. Phase 1 approved (`proceed`, 2026-11-01); Phase 2 written and awaiting review; nothing implemented. Requires [`api-documentation-rules`](../api-documentation-rules/DESIGN.md), which defines the `docs/` sources this tool builds.

**2026-11-01 amendment pending:** [`user-documentation-books`](../user-documentation-books/DESIGN.md) adds books (manifests, table of contents, chapter navigation, example extraction) to this tool and takes over Phase 1 question 5 (CI and GitHub Pages). Applied here at Phase 3.

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
