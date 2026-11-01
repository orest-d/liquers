---
id: UI-RUNNER-NONBLOCKING-EVALUATION
kind: design
title: The UI runner does not wait for the evaluations it starts
workflow: liquers-project
status: draft
phase: high-level
area: [lib/ui, web]
issues: [WEB-UI-FREEZES-WHILE-A-QUERY-AWAITS]
affects_docs: [specs/reference/JAVASCRIPT_INTEGRATION.md, specs/guides/JAVASCRIPT_GUIDE.md]
created: 2026-11-01
---
# The UI runner does not wait for the evaluations it starts

On wasm, a Liquers UI freezes while one of its queries awaits a Promise, because `AppRunner::run`
awaits every evaluation and the browser's asset manager evaluates inline. The runner will start
each evaluation as a task on the page's thread and collect its result on a later tick.

## Phase Status

- [ ] Phase 1: High-Level Design — written 2026-11-01, awaiting `proceed`
- [ ] Phase 2: Architecture
- [ ] Phase 3: Examples and Testing
- [ ] Phase 4: Implementation Plan
- [ ] Phase 5: Documentation

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
