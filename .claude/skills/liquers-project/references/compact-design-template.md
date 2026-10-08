# Compact design template (`S` and `M`)

One file, `specs/design/<slug>/DESIGN.md`. Three pages for `S`, five for `M`. Each `## Phase N`
section has the same obligations as the full-form phase document; its subsections are the short
versions. `scripts/init_feature.py <slug> --compact` creates it.

```markdown
---
id: <SLUG-UPPER>
kind: design
title: <title>
form: compact
workflow: liquers-project      # omit for bulk design, triage and compaction designs
status: draft
phase: high-level
readiness:                     # when assessed (§5.1.1)
autofix:                       # when assessed (§5.1.1)
area: []
issues: [<SOURCE-ID>]
created: <YYYY-MM-DD>
---
# <title>

## Phase 1: High-Level Design
### Purpose
### Problem Example
### Scope and Acceptance Criteria
<Scenarios: `- **AC-1** <name>` then a WHEN line and a THEN line (DOCS_STRUCTURE_GUIDE.md §5.2.1).>
### Design Readiness
<Or `### Open Questions` when no readiness is assessed. Add `### Design Dependencies` and
`### Scope Changes` when they apply.>

## Phase 2: Architecture
### Solution
<Chosen approach; rejected alternatives in one line each; known issues that block or must go first.>
### Changes
<Files and symbols; new or changed signatures only; errors; sync/async; commands; documents.>
### Risks
<Files, tests likely to change, compatibility, recovery, certainty, in a few lines.>

## Phase 3: Examples and Tests
### Examples
### Tests
<Exact test names, each citing the scenarios it proves (AC-1, …); the command to run them.>

## Phase 4: Implementation Plan
### Steps
<A checklist, one item per step: `- [ ] 1. <file and symbol> — <change> — <proof command>`.
Tick each step when its proof passes and append the commit.>
### Validation
<Final checks; documents and generated files to update.>

## Phase 5: Documentation
<Added after implementation, only for `workflow: liquers-project`: what was built versus approved,
documents reviewed, issues closed and filed.>
```

Convert to the full form when the design grows to `L`: move each section into its phase file and
remove `form: compact`.
