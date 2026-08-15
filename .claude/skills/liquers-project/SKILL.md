---
name: liquers-project
description: Structured five-phase workflow for substantial Liquers projects, from high-level intent through architecture, examples and tests, implementation planning and execution, to mandatory current-state documentation. Use for new value types, command libraries, storage backends, UI components, API endpoints, cross-crate changes, or explicit project-phase requests. Not for isolated commands, bug fixes, small refactors, documentation-only edits, or configuration changes.
---

# Liquers Project

A rigorous five-phase workflow for designing, implementing, and documenting substantial projects in the Liquers framework.

## When to Use This Skill

**Use liquers-project when:**
- Adding new value types (e.g., DataFrame support, image handling)
- Designing command libraries (e.g., Polars operations, data transformations)
- Implementing storage backends (e.g., S3, database integrations)
- Creating UI components (e.g., new widget types, visualization elements)
- Adding API endpoints or major integrations
- Any feature requiring architectural decisions across multiple crates

**Trigger phrases:**
- "design a new..."
- "plan implementation of..."
- "architect the..."
- "start Phase 1 design for..."
- "review Phase 2 for..."

**Do NOT use liquers-project for:**
- Single command additions (use register_command! directly)
- Bug fixes or small refactors
- Documentation updates
- Configuration changes
- Simple utility functions

## Overview

The liquers-project workflow follows a **mandatory 5-phase process** with explicit user approval gates:

```
Phase 1: High-Level Design (max 30 lines)
    ↓ [Critical Review → User Approval]
Phase 2: Solution & Architecture (data structures, interfaces, signatures)
    ↓ [Auto-invoke: rust-best-practices → Identify Relevant Commands → Ask User]
    ↓ [Multi-Agent Review: 2 haiku reviewers ∥ → sonnet fixer → User Approval]
Phase 3: Examples & Use-cases (2-3 examples, corner cases, test plan)
    ↓ [Multi-Agent Drafting: up to 5 haiku drafters ∥ → sonnet synthesizer]
    ↓ [Auto-invoke: liquers-unittest]
    ↓ [Multi-Agent Review: 3 haiku reviewers ∥ → sonnet fixer → User Approval]
Phase 4: Implementation Plan (step-by-step execution plan)
    ↓ [Auto-invoke: rust-best-practices → Specify Agent Assignments]
    ↓ [Multi-Agent Review: 4 haiku reviewers ∥ → opus final reviewer → User Approval]
    ↓ [Implement → Validate → Resolve all user/review comments]
Phase 5: Documentation (after implementation and review feedback are complete; normally before merge)
         Create the summary; create/update reference and guide documents; update links
    ↓ [Critical Review → User Approval → Complete]
```

**Key principles:**
- **MANDATORY APPROVAL GATE:** NEVER start the next phase until the user explicitly says "proceed" or "Proceed to next phase". No other response (including "looks good", "approved", "ok", "yes", "LGTM", or silence) counts as approval. If the user provides feedback or asks questions, address them and WAIT for the explicit "proceed" keyword before moving on. This is the MOST IMPORTANT rule of this workflow.
- **Auto-invoke related skills** (rust-best-practices, liquers-unittest) as appropriate
- **Validate completeness** using phase-specific checklists before approval
- **Create design folder** in `specs/design/<feature-name>/` to organize all phase documents
- **Mark the workflow unambiguously:** every project created by this skill carries
  `workflow: liquers-project` in `DESIGN.md`; that marker makes Phase 5 mandatory
- **Every phase transition updates `phase:` in `DESIGN.md`**. Phase 5 uses `documentation`; remove
  `phase` only when the design reaches `complete`
- **Status and phase vocabularies are `specs/DOCS_STRUCTURE_GUIDE.md` §5.1–5.2**, not freeform
  text. Do not write a `**Status:**` line into `DESIGN.md`; the front-matter owns it
- **A design with a `gh_pr` carries no *derived* `status`** (§5.5) — `in_implementation` and
  `implemented` follow from whether those PRs merged, so they are never written down. The terminal
  three (`complete`, `abandoned`, `superseded`) are conclusions GitHub cannot reach and *are*
  written, `gh_pr` or not

## Host Compatibility and Artifact Contract

Treat this directory as the canonical skill implementation for both Claude and Codex. A host-specific
adapter may point here, but it must not copy or redefine the workflow, templates, scripts, output
paths, headings, front-matter fields, or phase vocabulary.

- Read `CLAUDE.md` as the repository development guide on every host. Also follow `AGENTS.md` when
  the host supplies one; if the two conflict, follow the host's normal instruction precedence.
- Resolve every `scripts/` and `references/` path relative to this canonical `SKILL.md`, not relative
  to the repository working directory. Run Python with the launcher available on the host (`python`,
  `python3`, or `py -3`) without changing generated files.
- Keep generated artifacts identical in form across hosts: `DESIGN.md` plus the five named phase
  documents under `specs/design/<lowercase-kebab-slug>/`, using the templates and scripts bundled
  here. Do not add host names, model IDs, or host-specific metadata to design artifacts.
- Treat **Haiku**, **Sonnet**, and **Opus** in this skill as stable capability-tier labels, not a
  requirement that the host provide Anthropic models. Map them at execution time to the host's
  focused/fast, general-purpose, and deepest-review agents respectively. Preserve the original tier
  labels in generated artifacts so Claude and Codex produce the same document schema.
- When an auto-invoked skill is not registered on the current host, read and follow its canonical
  repository skill directly: `.claude/skills/rust-best-practices/SKILL.md`,
  `.claude/skills/liquers-unittest/SKILL.md`, or `.claude/skills/liquers-validate/SKILL.md`.
- Use the host's supported parallel-agent mechanism for the review roles. If the host cannot launch
  the requested agents, perform the same independent review passes sequentially and record the same
  review outcomes; never change the phase artifact format to expose host limitations.

## Workflow Decision Tree

### Phase Transitions

```
START
  ↓
[User requests feature design] → Initialize feature folder → Phase 1
  ↓
Phase 1: High-Level Design
  → Briefly decide new-vs-extended reference, new-vs-extended guide, other new documents,
    and specific documents to update using DOCS_STRUCTURE_GUIDE.md §2, §8, and §9
  → Run critical review (references/review-checklist.md)
  → Present to user with approval gate
  → STOP AND WAIT for user to say "proceed" or "Proceed to next phase"
  → If user provides feedback: address it, then WAIT again
  → If user says "proceed": Phase 2
  ↓
Phase 2: Solution & Architecture
  → Auto-invoke rust-best-practices skill
  → Check open known issues for prerequisites, solution impact, and blockers
  → Identify relevant commands (new + existing namespaces) → Ask user
  → Fully specify reference, guide, other-document, update, link, and affected-document plans
  → Run critical review (references/review-checklist.md)
  → Multi-Agent Review: 2 haiku reviewers in parallel
      Reviewer A: Phase 1 conformity check
      Reviewer B: Codebase alignment check
  → If issues: sonnet fixer agent resolves fixable issues, asks user for decisions
  → Present to user with approval gate
  → STOP AND WAIT for user to say "proceed" or "Proceed to next phase"
  → If user provides feedback: address it, then WAIT again
  → If user says "proceed": Phase 3
  ↓
Phase 3: Examples & Use-cases
  → Ask user: runnable prototypes or conceptual examples?
  → Multi-Agent Drafting: up to 5 haiku agents draft in parallel
      Agent 1: Primary scenario, from high-level purpose through verbal steps to core code
      Agent 2: Detailed scenario explaining additional mechanisms or relevant configuration
      Agent 3: Common pitfalls and edge cases — optional
      Agent 4: Unit tests (happy/error/edge paths)
      Agent 5: Integration tests + corner cases
  → Sonnet synthesizer integrates all outputs + creates overview table
  → Auto-invoke liquers-unittest skill
  → Select guide-worthy workflows/snippets and link complete executable examples or tests
  → Run critical review (references/review-checklist.md)
  → Multi-Agent Review: 3 haiku reviewers in parallel
      Reviewer 1: Phase 1 conformity
      Reviewer 2: Phase 2 conformity (signatures, data structures, traits)
      Reviewer 3: Codebase + query validation
  → If issues: sonnet fixer agent resolves fixable issues, asks user for decisions
  → Present to user with approval gate
  → STOP AND WAIT for user to say "proceed" or "Proceed to next phase"
  → If user provides feedback: address it, then WAIT again
  → If user says "proceed": Phase 4
  ↓
Phase 4: Implementation Plan
  → Generate step-by-step plan with agent specifications per step
  → Auto-invoke rust-best-practices skill
  → Run critical review (references/review-checklist.md)
  → Multi-Agent Review: 4 haiku reviewers in parallel
      Reviewer 1: Phase 1 conformity
      Reviewer 2: Phase 2 conformity
      Reviewer 3: Phase 3 conformity
      Reviewer 4: Codebase compatibility
  → Opus final reviewer: critical review of ALL phase documents
  → Present to user with approval gate
  → STOP AND WAIT for user to say "proceed" or "Proceed to next phase"
  → If user provides feedback: address it, then WAIT again
  → If user says "proceed": Offer execution options
  ↓
[Execution Options]
  → Execute now (implement the plan, then enter Phase 5)
  → Create task list (for later execution)
  → Revise plan (return to Phase 4)
  → Exit (user implements manually; Phase 5 remains outstanding)
  ↓
[Implementation complete and validated; all user/review comments resolved]
  → Phase 5: Documentation
  → Compare the request and approved design with implemented and tested behavior
  → Create the summary and planned reference/guide documents
  → Review affected documentation and update capability-map links
  → File issues for omitted work and newly discovered problems
  → Present to user with approval gate
  → If user says "proceed": mark design complete and remove `phase`
```

## Phase Workflows

> **CRITICAL RULE — READ THIS FIRST:** Each phase MUST end with an explicit approval gate. After presenting a phase's output, you MUST STOP and WAIT for the user to say "proceed" or "Proceed to next phase" before starting ANY work on the next phase. User feedback (corrections, questions, design changes, "looks good", "ok", "yes") is NOT approval — address the feedback and WAIT again. Only the exact word "proceed" (case-insensitive) advances to the next phase. Violating this rule invalidates the entire workflow.

### Phase 1: High-Level Design

**Purpose:** Establish WHAT and WHY in maximum 30 lines.

**Process:**
1. Run `<skill-root>/scripts/init_feature.py <feature-name>` to create folder structure
2. Use `references/phase1-template.md` to guide the design
3. Answer:
   - What is the feature name?
   - What is its purpose (1-3 sentences)?
   - How does it interact with existing systems (Query, Store, Commands, Assets)?
   - Does it require a new reference, an extension to an existing reference, or neither? Why?
   - Does it require a new guide, an extension to an existing guide, or neither? Why?
   - Are any other documents required? Which kinds, or why none?
   - Which specific existing documents need updates, or why none?
   - What open questions remain?
4. Perform critical review using Phase 1 checklist
5. Present to user with clear approval gate

**Output:** `specs/design/<feature-name>/phase1-high-level-design.md`

**Approval gate:** Present the Phase 1 document to the user. Then STOP and WAIT. Do NOT start Phase 2 until the user explicitly says "proceed" or "Proceed to next phase". If the user gives feedback, incorporate it and WAIT again. Any response other than "proceed" means "not yet approved".

**Detailed guidance:** See `references/phase1-template.md`

### Phase 2: Solution & Architecture

**Purpose:** Define HOW - data structures, interfaces, function signatures. NO implementations.

**Process:**
1. Auto-invoke **rust-best-practices** skill for Rust idiom validation
2. Use `references/phase2-template.md` to guide architecture
3. **Run the known-issue preflight:**
   - Inspect open repository issues linked to the design, overlapping its areas, or affecting its
     integration points, dependencies, and architectural assumptions
   - For every relevant issue, record its status and priority, whether it must be addressed first,
     how it affects the proposed solution, and whether it blocks this project
   - A blocker must be resolved first or the architecture must explicitly remove the dependency;
     do not approve Phase 2 while an unresolved blocker remains
   - Review blocker priority: it must be at least P1. Recommend P0 only when it also meets the P0
     criteria in `DOCS_STRUCTURE_GUIDE.md` §4.4; record and confirm any priority change
4. Define:
   - Data structures (fields, ownership, serialization)
   - Trait implementations (which traits, bounds)
   - Sync vs Async decisions (with rationale)
   - Generic parameters and bounds
   - Integration points (which crates, which modules)
   - Web endpoints (if applicable - routes, handlers, responses)
   - Documentation architecture: fully specify the Phase 1 reference decision, guide decision,
     every other document to create, and every specific document to update
   - For each document, give the exact path, kind, audience, area, purpose or exact change, and
     required links; identify the proposed authoritative `affects_docs` set
5. **Identify relevant commands:**
   - Newly defined commands (with full signatures)
   - Relevant existing command namespaces from liquers-lib (e.g., `lui` and `egui` for UI, `pl` for Polars)
   - **Ask user** for feedback on which command namespaces are relevant before finalizing
6. Check against `references/liquers-patterns.md` for consistency
7. Perform critical review using Phase 2 checklist
8. **Multi-Agent Review (2 haiku + 1 sonnet):**
   - Launch **2 haiku reviewer agents in parallel:**
     - **Reviewer A (Phase 1 conformity):** Check that Phase 2 architecture aligns with Phase 1 high-level design — scope hasn't drifted, all interactions from Phase 1 are addressed, no new unscoped features crept in
     - **Reviewer B (Codebase alignment):** Check Phase 2 against existing code at integration points — find inconsistencies, non-matching function signatures, detect functionality that already exists (perhaps under different names or with slightly different behavior that could be reused)
   - **If issues found:** Launch **1 sonnet agent** to fix all fixable issues in the Phase 2 document, ask user only for genuine design decisions that can't be resolved from context. Produce summary with list of fixes made + remaining questions.
8. Present to user with clear approval gate

**Output:** `specs/design/<feature-name>/phase2-architecture.md`

**Approval gate:** Present the Phase 2 document to the user. Then STOP and WAIT. Do NOT start Phase 3 until the user explicitly says "proceed" or "Proceed to next phase". If the user gives feedback (corrections, questions, design changes), incorporate them and WAIT again. Any response other than "proceed" means "not yet approved".

**Detailed guidance:** See `references/phase2-template.md`

### Phase 3: Examples & Use-cases

**Purpose:** Demonstrate usage, explore corner cases, plan comprehensive tests.

**Process:**
1. **Ask user:** Should examples be runnable prototypes or conceptual code?
2. **Multi-Agent Drafting (up to 5 haiku + 1 sonnet synthesizer):**
   - Split work by example/test type across up to 5 haiku agents (with rust-best-practices + liquers-unittest skills):
     - Agent 1: Primary use case — explain how it demonstrates Phase 1, narrate the component
       sequence, then show only the core code; keep it medium-complexity and use relevant defaults
     - Agent 2: Secondary/detailed use case — build on Scenario 1 and explain additional mechanisms,
       interactions, or configuration without repeating basic setup
     - Agent 3: Common pitfalls and edge cases — optional; show symptoms, causes, and corrections
     - Agent 4: Unit tests (happy path, error path, edge cases)
     - Agent 5: Integration tests + corner cases (memory, concurrency, serialization, cross-crate)
   - **1 sonnet synthesizer agent** (with rust-best-practices + liquers-unittest skills) reviews and integrates all outputs into the Phase 3 document
   - After a brief high-level introduction, add an **overview table** of all examples and tests
     proposed, explaining what each example demonstrates and what each test checks
   - The introduction connects the examples to the
     Phase 1 purpose and explains the progression from primary workflow to details and pitfalls
3. Auto-invoke **liquers-unittest** skill to generate test templates
4. Use `references/phase3-template.md` to organize findings
5. Identify guide candidates that answer "How do I use X?", "How do I achieve X?", and "What is
   the typical workflow for X?"; select useful snippets and link a complete executable example or
   unit test when one exists
6. Perform critical review using Phase 3 checklist
7. **Multi-Agent Review (3 haiku + 1 sonnet):**
   - Launch **3 haiku reviewer agents in parallel** (with rust-best-practices + liquers-unittest skills):
     - **Reviewer 1 (Phase 1 conformity):** Check examples/tests align with Phase 1 high-level design
     - **Reviewer 2 (Phase 2 conformity):** Check examples/tests match Phase 2 architecture — correct function signatures, data structures, trait usage
     - **Reviewer 3 (Codebase + query validation):** Check alignment with existing code. Validate all queries:
       - No spaces, newlines, or special characters in queries
       - If queries use resource part (`-R/`), verify the environment has a store defined
       - Check if commands used in queries are known (registered) — using the relevant command list from Phase 2
   - **1 sonnet agent** (with rust-best-practices, liquers-unittest, knowledge of PROJECT_OVERVIEW.md + Phase 1, 2, 3 documents) processes review output, fixes all fixable issues, provides list of potential problems, asks user only for genuine design decisions.
9. Present to user with clear approval gate

**Output:** `specs/design/<feature-name>/phase3-examples.md`

**Approval gate:** Present the Phase 3 document to the user. Then STOP and WAIT. Do NOT start Phase 4 until the user explicitly says "proceed" or "Proceed to next phase". If the user gives feedback, incorporate it and WAIT again. Any response other than "proceed" means "not yet approved".

**Detailed guidance:** See `references/phase3-template.md`

### Phase 4: Implementation Plan

**Purpose:** Create step-by-step, actionable execution plan with validation commands and explicit agent assignments.

**Process:**
1. Break down implementation into numbered, file-specific steps
2. For each step, specify:
   - Identify exact file paths
   - Specify function signatures or structure changes
   - Provide validation commands (cargo check, cargo test)
   - **Agent specification:** which model to use (haiku/sonnet/opus), which skills needed (rust-best-practices, liquers-unittest, etc.), what knowledge/context the agent needs (which files, specs, patterns)
3. Define testing plan (when to run unit tests, integration tests)
4. Auto-invoke **rust-best-practices** skill for implementation validation
5. Create rollback plan for each major change
6. Use `references/phase4-template.md` to structure the plan
7. Perform critical review using Phase 4 checklist (VERY HIGH certainty required)
8. **Multi-Agent Review (4 haiku + 1 opus):**
   - Launch **4 haiku reviewer agents in parallel** (with rust-best-practices, liquers-unittest, knowledge of PROJECT_OVERVIEW.md):
     - **Reviewer 1:** Check conformity with Phase 1
     - **Reviewer 2:** Check conformity with Phase 2
     - **Reviewer 3:** Check conformity with Phase 3
     - **Reviewer 4:** Check conformity/compatibility with existing codebase
   - **1 opus agent** (with rust-best-practices, liquers-unittest, knowledge of PROJECT_OVERVIEW.md + all Phase 1-4 documents) critically reviews ALL documents, fixes problems or raises issues and asks questions.
9. Present to user with clear approval gate

**Output:** `specs/design/<feature-name>/phase4-implementation.md`

**Approval gate:** Present the Phase 4 document to the user. Then STOP and WAIT. Do NOT offer execution until the user explicitly says "proceed" or "Proceed to next phase". If the user gives feedback, incorporate it and WAIT again. Any response other than "proceed" means "not yet approved".

**After approval:** Offer execution options:
- Execute now (start implementing)
- Create task list (defer to later)
- Revise plan (return to Phase 4)
- Exit (user implements manually)

Phase 4 approval does not complete the design. After implementation and review feedback are finished,
continue to Phase 5.

**Detailed guidance:** See `references/phase4-template.md`

### Phase 5: Documentation

**Purpose:** Record what was actually implemented, preserve important learning, and anchor the completed
capability in current reference and guide documentation without requiring future readers to inspect
the design history.

**Start only when:**
- All planned implementation work for this effort is finished and validated
- All user comments and review comments are answered or incorporated
- The documentation can be made consistent with the implementation; normally complete it in the
  same PR before merge rather than in a follow-up PR

**Process:**
1. Set `phase: documentation` in `DESIGN.md`; when `gh_pr` is present, do not add a derived status
2. Read the implementation and tests, all prior phase documents, review feedback, and filed issues
3. Create `phase5-documentation.md` in the design folder using
   `references/phase5-documentation.md`; target one page and never exceed three pages
4. Summarize what was implemented, deviations from the request and their reasons, newly filed issues, and
   important learning points
5. Create the reference and/or guide committed in Phases 1-2; reconsider a previous `neither`
   decision when the accumulated information is too substantial for the summary
6. Generate candidate affected documents by `area`, decide the authoritative `affects_docs` set,
   and review each kept document against implemented and tested behavior as required by
   `DOCS_STRUCTURE_GUIDE.md` §9
7. Add or update links in `specs/README.md` and other documentation so readers enter through the
   current reference/guide rather than stale design artifacts where appropriate
8. Update every issue or feature completed by this work to `status: closed` (or
   `closed_not_planned` for that outcome), with a concise resolution or decision note, following
   `DOCS_STRUCTURE_GUIDE.md` §4.3. This is required even when `github:` is present.
9. Run documentation validation, perform the Phase 5 critical review, and present the results at
   the approval gate
10. If a later rebase or merge conflict changes code or documentation, review the affected material
   again after merge and fix any inconsistency

**Output:** `specs/design/<feature-name>/phase5-documentation.md`, plus any new or updated files in
`specs/reference/`, `specs/guides/`, and `specs/README.md` required by the approved documentation plan.

**Approval gate:** Present the summary and documentation changes. STOP and WAIT for the user to say
`proceed`. After approval, set `status: complete`, remove `phase`, and ensure the capability map and
documentation history are current.

**Detailed guidance:** See `references/phase5-documentation.md`

## Reporting Test Results

Every phase, every progress report and every summary that mentions tests must make **one thing
unambiguous: did all tests pass?** Everything else about test reporting is optional.

**Required.** State the outcome in words, not only numbers:

> `cargo test -p liquers-core --lib --tests` — **all tests passed**

If anything failed, say so first, name the failing tests, and give the reason for each. A phase is
not complete and work is not "green" while a test fails. "Mostly passing" is a failure report.

**Optional, and omit it when it is not trivially true.** A total count is only worth reporting when
one number is obviously correct. Do not compute, estimate or reconcile counts:

- **Never write a ratio.** `548/549` reads as "one test failed" and will be understood that way.
  Write `548 passed, 0 failed`, or write nothing.
- **Two configurations usually have two different totals**, because `#[cfg]`-gated tests change the
  test *set*. Reporting both bare numbers invites the same misreading. Either say
  "both configurations: all tests passed", or state each as `N passed, 0 failed` and say in one
  clause why the totals differ.
- **Numbers go stale within the session.** A count quoted from an earlier run after later commits
  is simply wrong. Re-run or drop the number.

The failure mode this rule exists to prevent is a reader spending time investigating a test failure
that never happened.

## Critical Review Process

Before each approval gate, conduct a thorough review using the appropriate checklist. Phases 2-4 use **multi-agent reviews** for deeper, parallelized analysis.

**Phase 1 Review (inline):**
- Scope clarity: Purpose fits in 1-3 sentences, interactions identified
- No duplication: Feature doesn't overlap with existing functionality
- Aligns with Liquers philosophy: Fits the query-based, layered architecture
- Questions identified: Open problems documented, no blocking unknowns
- Documentation needs briefly address all four questions: new or extended reference, new or
  extended guide, other documents to create, and specific documents to update, each with rationale

**Phase 2 Review (2 haiku + 1 sonnet):**
- Known-issue preflight: relevant open issues, solution impact, prerequisites, blocking status, and
  priority actions are explicit; no unresolved blocker remains, and no blocker is below P1
- Type design: Ownership clear (Arc/Box/owned), serialization strategy defined
- No default match arms: Explicit handling of all enum variants
- Generics justified: Generic parameters have clear purpose
- Integration verified: Compatibility with existing crates checked
- Async/Sync decisions: Made with rationale, AsyncStore pattern followed
- Error handling: Uses `Error::typed_constructor()` (not `Error::new`)
- **Relevant commands identified:** New commands with signatures + existing namespaces
- **Reviewer A (haiku):** Phase 1 conformity — scope hasn't drifted
- **Reviewer B (haiku):** Codebase alignment — no signature mismatches, no missed reusable code
- **Sonnet fixer:** Resolves all fixable issues, surfaces genuine design decisions to user
- Documentation architecture: all four Phase 1 decisions are fully specified with exact paths,
  document kinds, audiences, intended changes, `affects_docs`, and link changes

**Phase 3 Review (3 haiku + 1 sonnet):**
- Overview table of all examples and tests present
- Examples cover 2-3 realistic scenarios
- User feedback incorporated on prototype type
- Corner cases addressed: Memory, concurrency, errors, serialization
- Test coverage: Unit + integration tests planned, error paths included
- Guide candidates answer how to use or achieve the capability and its typical workflow; useful
  snippets and complete executable example or test links are identified
- **Reviewer 1 (haiku):** Phase 1 conformity
- **Reviewer 2 (haiku):** Phase 2 conformity — signatures, data structures, traits
- **Reviewer 3 (haiku):** Codebase + query validation (no spaces/newlines, `-R/` store check, command registration check)
- **Sonnet fixer:** Resolves fixable issues, lists potential problems, asks user for decisions

**Phase 4 Review (4 haiku + 1 opus):**
- Steps actionable: Each step has file path, signature, validation command, agent specification
- Testing plan complete: Unit, integration, manual commands specified
- Documentation updates: CLAUDE.md, PROJECT_OVERVIEW.md if needed
- Very high certainty: Clear path forward, team can execute
- **Reviewer 1-4 (haiku):** Phase 1, 2, 3 conformity + codebase compatibility (one each)
- **Opus final reviewer:** Critical review of ALL phase documents, fixes or raises issues

**Phase 5 Review (inline):**
- Start criteria met: implementation and review feedback are complete
- Summary is 1-3 pages and distinguishes requested, implemented, omitted, and added scope
- New issues and important learning points are recorded
- Approved reference/guide documents explain present behavior or repeatable work without requiring
  the design folder
- `affects_docs`, `reviewed:`, `## History`, and capability-map links follow
  `DOCS_STRUCTURE_GUIDE.md`

**Full checklist:** See `references/review-checklist.md`

## Feature Folder Management

### Initializing a Feature

Use the provided script to create the folder structure:

```bash
python <skill-root>/scripts/init_feature.py <feature-name>
```

**Creates:**
```
specs/design/<feature-name>/
├── DESIGN.md                    # Phase status tracking
├── phase1-high-level-design.md  # Phase 1 document (from template)
├── phase2-architecture.md       # Phase 2 document (from template)
├── phase3-examples.md           # Phase 3 document (from template)
├── phase4-implementation.md     # Phase 4 document (from template)
└── phase5-documentation.md      # Phase 5 summary (from template)
```

### Validating Phase Completion

Before requesting user approval, validate the phase:

```bash
python <skill-root>/scripts/validate_phase.py <feature-name> <phase-number>
```

**Checks:**
- Phase file exists and is non-empty
- Required sections present (per phase)
- No template placeholders remaining (e.g., `[TODO: ...]`)

### Migration Note

This folder structure applies to **new features only**. Existing specs in the flat `specs/` directory remain as-is. Only use feature folders for designs created with liquers-project.

## Agent Orchestration

The liquers-project workflow uses **multi-agent review** to distribute review work across specialized sub-agents, improving coverage and catching issues earlier.

### Model Selection Rationale

| Model | Role | When Used |
|-------|------|-----------|
| **Haiku** | Parallel reviewer / drafter | Phase 2-4 reviews, Phase 3 drafting. Fast, cheap, good for focused single-concern checks. Run many in parallel. |
| **Sonnet** | Synthesizer / fixer | After haiku reviews surface issues. Integrates multiple review outputs, fixes documents, asks user targeted questions. Also used for Phase 3 synthesis. |
| **Opus** | Final critical reviewer | Phase 4 holistic review and, when needed, Phase 5 cross-document consistency review. |

### Agent Skills and Knowledge

Each agent must be launched with explicit **skills** and **knowledge context**:

- **Skills:** Specify which skills the agent needs (e.g., `rust-best-practices`, `liquers-unittest`). Agents without the right skills will miss domain-specific issues.
- **Knowledge:** Specify which files/specs the agent must read (e.g., `PROJECT_OVERVIEW.md`, Phase 1-3 documents, relevant source files). Agents without context will produce shallow reviews.

**Example agent launch specification:**
```
Agent: Haiku Reviewer B (Phase 2 codebase alignment)
Skills: rust-best-practices
Knowledge: Phase 2 document, integration point files from codebase
Task: Check Phase 2 architecture against existing code at integration points.
       Find inconsistencies, non-matching function signatures, detect reusable
       existing functionality.
```

### Orchestration Pattern

All multi-agent reviews follow the same pattern:

1. **Launch reviewers in parallel** (haiku agents, each with a single focused concern)
2. **Collect review outputs** (wait for all parallel agents to complete)
3. **Launch fixer agent sequentially** (sonnet or opus) to process all review outputs:
   - Fix all fixable issues directly in the document
   - Produce a summary: list of fixes made + remaining questions
   - Ask user ONLY for genuine design decisions that can't be resolved from context
4. **Present fixed document + summary to user** for approval

### When Multi-Agent Review Finds No Issues

If all reviewers report no issues, skip the fixer agent and proceed directly to the user approval gate. Do not launch a fixer agent when there is nothing to fix.

## Integration with Other Skills

### Auto-invoke rust-best-practices

**When:** Phase 2 (architecture), Phase 4 (implementation validation)

**Purpose:**
- Validate Rust idioms (ownership, borrowing, trait bounds)
- Check for common anti-patterns
- Ensure compilation feasibility

**Example invocation:**
```
"Review the architecture in specs/<feature>/phase2-architecture.md for Rust best practices"
```

### Auto-invoke liquers-unittest

**When:** Phase 3 (test plan generation)

**Purpose:**
- Generate test templates (unit tests, integration tests)
- Ensure comprehensive coverage
- Validate test structure follows liquers conventions

**Example invocation:**
```
"Generate test templates for the feature described in specs/<feature>/phase3-examples.md"
```

**Note:** Do NOT invoke other skills manually. The liquers-project workflow automatically calls them at the appropriate phases.

## Examples

### Example 1: Designing Parquet File Support

**User request:** "Design a new feature for reading and writing Parquet files in liquers"

**Workflow:**

1. **Initialize:**
   ```bash
   python3 scripts/init_feature.py parquet-support
   ```

2. **Phase 1:** Write high-level design
   - Purpose: Add Parquet format support to liquers for efficient columnar data storage
   - Interactions: Integrates with Store (read/write), Commands (to_parquet, from_parquet), Polars DataFrames
   - Review → User approval

3. **Phase 2:** Architecture
   - Auto-invoke rust-best-practices
   - Check relevant open issues; resolve/design around blockers and review their priority
   - Define: ParquetStore (AsyncStore impl), commands (register_command!), ExtValue variant
   - Identify relevant commands: new (`to_parquet`, `from_parquet`) + existing `polars` namespace
   - Multi-agent review (2 haiku + sonnet fixer) → User approval

4. **Phase 3:** Examples
   - Multi-agent drafting: haiku agents draft examples + tests in parallel
   - Sonnet synthesizer integrates outputs + creates overview table
   - Auto-invoke liquers-unittest for test templates
   - Multi-agent review (3 haiku + sonnet fixer) → User approval

5. **Phase 4:** Implementation plan
   - Step 1: Add parquet dependency (haiku, rust-best-practices)
   - Step 2: Extend ExtValue with Parquet variant (sonnet, rust-best-practices)
   - Step 3: Implement to_parquet command (sonnet, rust-best-practices)
   - ... (each step with agent model + skills + knowledge)
   - Auto-invoke rust-best-practices for validation
   - Multi-agent review (4 haiku + opus final reviewer) → User approval → Offer execution

6. **Phase 5:** Documentation after implementation
   - Record conformance and deviations in the one-to-three-page summary
   - Create or update the planned Parquet reference/guide documents
   - Review affected documents against implemented and tested behavior and update the capability map
   - User approval → mark design complete

### Example 2: Designing a New UI Container Widget

**User request:** "Architect the TabContainer widget for Phase 1b UI"

**Workflow:**

1. **Initialize:**
   ```bash
   python3 scripts/init_feature.py tab-container-widget
   ```

2. **Phase 1:** High-level design (30 lines)
   - Purpose: Multi-tab container for organizing UI elements
   - Interactions: Implements UIElement trait, integrates with AppState
   - Review → User approval

3. **Phase 2:** Architecture
   - Auto-invoke rust-best-practices
   - Check relevant open issues; resolve/design around blockers and review their priority
   - Define: TabContainerElement struct, UIElement impl, message handling
   - Identify relevant commands: new (`add_tab`, `remove_tab`) + existing `lui` namespace
   - Multi-agent review (2 haiku + sonnet fixer) → User approval

4. **Phase 3:** Examples
   - Multi-agent drafting: haiku agents draft examples + tests in parallel
   - Sonnet synthesizer creates overview table + integrated document
   - Auto-invoke liquers-unittest
   - Multi-agent review (3 haiku + sonnet fixer) → User approval

5. **Phase 4:** Implementation plan
   - Detailed steps with file paths, validation commands, agent specifications
   - Auto-invoke rust-best-practices
   - Multi-agent review (4 haiku + opus final reviewer) → User approval → Execute now

6. **Phase 5:** Documentation after implementation
   - Summarize what was implemented, deviations, filed issues, and UI framework learning
   - Create/update the planned UI reference or guide and affected links
   - User approval → mark design complete

## Tips for Effective Use

1. **Be thorough in Phase 1:** A clear high-level design prevents downstream rework
2. **Don't skip critical reviews:** They catch issues early when they're cheap to fix
3. **Use the provided templates:** They ensure consistency and completeness
4. **Leverage auto-invoke skills:** rust-best-practices and liquers-unittest add expertise
5. **Ask for user feedback early:** The approval gates are opportunities to align
6. **Validate before approval:** Run `validate_phase.py` to check completeness
7. **Document open questions:** Better to acknowledge unknowns than make assumptions
8. **Follow liquers patterns:** Use `references/liquers-patterns.md` as a guide
9. **Plan for testing:** Phase 3 is not optional; tests are first-class outputs
10. **Be realistic in Phase 4:** If a step feels uncertain, break it down further
11. **Collect documentation evidence continuously:** Record developer guidance, connections,
    corrections, and unexpected learning while it is fresh; synthesize it in Phase 5

## Troubleshooting

**Problem:** Feature folder already exists

**Solution:** Use a different feature name or manually delete the existing folder

---

**Problem:** Phase validation fails (missing sections)

**Solution:** Check `references/phaseN-template.md` for required sections, fill them in

---

**Problem:** Auto-invoke skill not found

**Solution:** Ensure rust-best-practices and liquers-unittest skills are installed

---

**Problem:** User rejects a phase

**Solution:** Iterate within that phase, don't skip to the next. Use feedback to revise.

---

**Problem:** Implementation plan too vague

**Solution:** Return to Phase 4, break down steps further, add more validation commands

## Version

This is **liquers-project v1.0**, derived from the legacy `liquers-designer` workflow and extended
with a mandatory documentation phase.

**Changelog:**
- v1.0 (2026-08-10): Introduced the distinct five-phase `liquers-project` workflow
  - Phase 1 briefly answers four documentation-needs questions
  - Phase 2 fully specifies new, extended, and updated documentation
  - Phase 3 selects guide-worthy examples, snippets, workflows, and executable evidence
  - Phase 5 normally completes in the implementation PR and verifies the implemented behavior
  - `workflow: liquers-project` makes the mandatory Phase 5 unambiguous
- Heritage: retains the architecture, review, and artifact conventions of the four-phase
  `liquers-designer` skill so both hosts and transitional projects remain compatible.


## After implementation

Run Phase 5 after implementation and review feedback are complete, normally in the same PR before
merge. A design is not `complete` until its Phase 5 summary, affected-document review, planned
reference/guide work, History rows, and capability-map updates are approved.

## Filing issues from a design

When a design ships in part, the remainder becomes an issue — there is no partial design status
(§5.6). When an issue is `complexity: L` or `XL`, it requires a design folder (§4.5). Both
directions use the procedure in `specs/DOCS_STRUCTURE_GUIDE.md` §4.8; do not restate it here.
