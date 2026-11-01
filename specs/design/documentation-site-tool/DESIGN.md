---
id: DOCUMENTATION-SITE-TOOL
kind: design
title: A documentation tool builds the user documentation site with rustdoc, Liquers queries and Mustache
workflow: liquers-project
status: superseded
superseded_by: user-documentation-guides
area: [docs, build, lib/commands]
issues: [USER-DOCUMENTATION-CANNOT-BE-GENERATED]
affects_docs: [guides/TEMPLATE_GUIDE.md]
created: 2026-11-01
---
# A documentation tool builds the user documentation site with rustdoc, Liquers queries and Mustache

GitHub #233, requested 2026-11-01. Phase 1 approved (`proceed`, 2026-11-01); Phase 2 written and awaiting review; nothing implemented. Requires [`api-documentation-rules`](../api-documentation-rules/DESIGN.md), which defines the `docs/` sources this tool builds.

**Superseded 2026-11-01** by [`user-documentation-guides`](../user-documentation-guides/DESIGN.md) (owner's `proceed` at its Phase 2 gate, recommendation 1). Moved there: the build script, the homepage, API-reference placement, CI and the `liquers-docs` crate, reshaped as a preprocessing step for mdBook and Pandoc. Dropped: this design's own Markdown-to-HTML, page layout and `ns-docs/site` / `md_to_html` / `commands` commands. Its Phases 1-2 stay as the record of the reasoning.

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
