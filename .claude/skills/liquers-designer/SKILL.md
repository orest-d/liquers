---
name: liquers-designer
description: Structured 4-phase design workflow for new Liquers features. Use when designing substantial features requiring careful architecture (new value types, command libraries, storage backends, UI components, API endpoints). Triggers on "design a new...", "plan implementation of...", "architect the...", or explicit phase requests ("start Phase 1 design", "review Phase 2"). NOT for trivial additions (single command, bug fix, doc update).
---

# Liquers Feature Designer

A rigorous 4-phase workflow for designing and implementing substantial features in the Liquers framework.

## When to Use This Skill

**Use liquers-designer when:**
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

**Do NOT use liquers-designer for:**
- Single command additions (use register_command! directly)
- Bug fixes or small refactors
- Documentation updates
- Configuration changes
- Simple utility functions

## Overview

The liquers-designer workflow follows a **mandatory 4-phase process** with explicit user approval gates:

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
    ↓ [Offer Execution]
```

**Key principles:**
- **MANDATORY APPROVAL GATE:** NEVER start the next phase until the user explicitly says "proceed" or "Proceed to next phase". No other response (including "looks good", "approved", "ok", "yes", "LGTM", or silence) counts as approval. If the user provides feedback or asks questions, address them and WAIT for the explicit "proceed" keyword before moving on. This is the MOST IMPORTANT rule of this workflow.
- **Auto-invoke related skills** (rust-best-practices, liquers-unittest) as appropriate
- **Validate completeness** using phase-specific checklists before approval
- **Create feature folder** in `specs/<feature-name>/` to organize all phase documents

## Workflow Decision Tree

### Phase Transitions

```
START
  ↓
[User requests feature design] → Initialize feature folder → Phase 1
  ↓
Phase 1: High-Level Design
  → Run critical review (references/review-checklist.md)
  → Present to user with approval gate
  → STOP AND WAIT for user to say "proceed" or "Proceed to next phase"
  → If user provides feedback: address it, then WAIT again
  → If user says "proceed": Phase 2
  ↓
Phase 2: Solution & Architecture
  → Auto-invoke rust-best-practices skill
  → Identify relevant commands (new + existing namespaces) → Ask user
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
      Agent 1: Example scenario 1 (primary use case)
      Agent 2: Example scenario 2 (secondary/advanced)
      Agent 3: Example scenario 3 (edge case) — optional
      Agent 4: Unit tests (happy/error/edge paths)
      Agent 5: Integration tests + corner cases
  → Sonnet synthesizer integrates all outputs + creates overview table
  → Auto-invoke liquers-unittest skill
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
  → Execute now (implement the plan)
  → Create task list (for later execution)
  → Revise plan (return to Phase 4)
  → Exit (user will implement manually)
```

## Phase Workflows

> **CRITICAL RULE — READ THIS FIRST:** Each phase MUST end with an explicit approval gate. After presenting a phase's output, you MUST STOP and WAIT for the user to say "proceed" or "Proceed to next phase" before starting ANY work on the next phase. User feedback (corrections, questions, design changes, "looks good", "ok", "yes") is NOT approval — address the feedback and WAIT again. Only the exact word "proceed" (case-insensitive) advances to the next phase. Violating this rule invalidates the entire workflow.

### Phase 1: High-Level Design

**Purpose:** Establish WHAT and WHY in maximum 30 lines.

**Process:**
1. Run `scripts/init_feature.py <feature-name>` to create folder structure
2. Use `references/phase1-template.md` to guide the design
3. Answer:
   - What is the feature name?
   - What is its purpose (1-3 sentences)?
   - How does it interact with existing systems (Query, Store, Commands, Assets)?
   - What open questions remain?
4. Perform critical review using Phase 1 checklist
5. Present to user with clear approval gate

**Output:** `specs/<feature-name>/phase1-high-level-design.md`

**Approval gate:** Present the Phase 1 document to the user. Then STOP and WAIT. Do NOT start Phase 2 until the user explicitly says "proceed" or "Proceed to next phase". If the user gives feedback, incorporate it and WAIT again. Any response other than "proceed" means "not yet approved".

**Detailed guidance:** See `references/phase1-template.md`

### Phase 2: Solution & Architecture

**Purpose:** Define HOW - data structures, interfaces, function signatures. NO implementations.

**Process:**
1. Auto-invoke **rust-best-practices** skill for Rust idiom validation
2. Use `references/phase2-template.md` to guide architecture
3. Define:
   - Data structures (fields, ownership, serialization)
   - Trait implementations (which traits, bounds)
   - Sync vs Async decisions (with rationale)
   - Generic parameters and bounds
   - Integration points (which crates, which modules)
   - Web endpoints (if applicable - routes, handlers, responses)
4. **Identify relevant commands:**
   - Newly defined commands (with full signatures)
   - Relevant existing command namespaces from liquers-lib (e.g., `lui` and `egui` for UI, `pl` for Polars)
   - **Ask user** for feedback on which command namespaces are relevant before finalizing
5. Check against `references/liquers-patterns.md` for consistency
6. Perform critical review using Phase 2 checklist
7. **Multi-Agent Review (2 haiku + 1 sonnet):**
   - Launch **2 haiku reviewer agents in parallel:**
     - **Reviewer A (Phase 1 conformity):** Check that Phase 2 architecture aligns with Phase 1 high-level design — scope hasn't drifted, all interactions from Phase 1 are addressed, no new unscoped features crept in
     - **Reviewer B (Codebase alignment):** Check Phase 2 against existing code at integration points — find inconsistencies, non-matching function signatures, detect functionality that already exists (perhaps under different names or with slightly different behavior that could be reused)
   - **If issues found:** Launch **1 sonnet agent** to fix all fixable issues in the Phase 2 document, ask user only for genuine design decisions that can't be resolved from context. Produce summary with list of fixes made + remaining questions.
8. Present to user with clear approval gate

**Output:** `specs/<feature-name>/phase2-architecture.md`

**Approval gate:** Present the Phase 2 document to the user. Then STOP and WAIT. Do NOT start Phase 3 until the user explicitly says "proceed" or "Proceed to next phase". If the user gives feedback (corrections, questions, design changes), incorporate them and WAIT again. Any response other than "proceed" means "not yet approved".

**Detailed guidance:** See `references/phase2-template.md`

### Phase 3: Examples & Use-cases

**Purpose:** Demonstrate usage, explore corner cases, plan comprehensive tests.

**Process:**
1. **Ask user:** Should examples be runnable prototypes or conceptual code?
2. **Multi-Agent Drafting (up to 5 haiku + 1 sonnet synthesizer):**
   - Split work by example/test type across up to 5 haiku agents (with rust-best-practices + liquers-unittest skills):
     - Agent 1: Example scenario 1 (primary use case)
     - Agent 2: Example scenario 2 (secondary/advanced use case)
     - Agent 3: Example scenario 3 (edge case scenario) — optional
     - Agent 4: Unit tests (happy path, error path, edge cases)
     - Agent 5: Integration tests + corner cases (memory, concurrency, serialization, cross-crate)
   - **1 sonnet synthesizer agent** (with rust-best-practices + liquers-unittest skills) reviews and integrates all outputs into the Phase 3 document
   - Document must begin with an **overview table** of all examples and tests proposed, explaining what each example demonstrates and what each test checks
3. Auto-invoke **liquers-unittest** skill to generate test templates
4. Use `references/phase3-template.md` to organize findings
5. Perform critical review using Phase 3 checklist
6. **Multi-Agent Review (3 haiku + 1 sonnet):**
   - Launch **3 haiku reviewer agents in parallel** (with rust-best-practices + liquers-unittest skills):
     - **Reviewer 1 (Phase 1 conformity):** Check examples/tests align with Phase 1 high-level design
     - **Reviewer 2 (Phase 2 conformity):** Check examples/tests match Phase 2 architecture — correct function signatures, data structures, trait usage
     - **Reviewer 3 (Codebase + query validation):** Check alignment with existing code. Validate all queries:
       - No spaces, newlines, or special characters in queries
       - If queries use resource part (`-R/`), verify the environment has a store defined
       - Check if commands used in queries are known (registered) — using the relevant command list from Phase 2
   - **1 sonnet agent** (with rust-best-practices, liquers-unittest, knowledge of PROJECT_OVERVIEW.md + Phase 1, 2, 3 documents) processes review output, fixes all fixable issues, provides list of potential problems, asks user only for genuine design decisions.
7. Present to user with clear approval gate

**Output:** `specs/<feature-name>/phase3-examples.md`

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

**Output:** `specs/<feature-name>/phase4-implementation.md`

**Approval gate:** Present the Phase 4 document to the user. Then STOP and WAIT. Do NOT offer execution until the user explicitly says "proceed" or "Proceed to next phase". If the user gives feedback, incorporate it and WAIT again. Any response other than "proceed" means "not yet approved".

**After approval:** Offer execution options:
- Execute now (start implementing)
- Create task list (defer to later)
- Revise plan (return to Phase 4)
- Exit (user implements manually)

**Detailed guidance:** See `references/phase4-template.md`

## Critical Review Process

Before each approval gate, conduct a thorough review using the appropriate checklist. Phases 2-4 use **multi-agent reviews** for deeper, parallelized analysis.

**Phase 1 Review (inline):**
- Scope clarity: Purpose fits in 1-3 sentences, interactions identified
- No duplication: Feature doesn't overlap with existing functionality
- Aligns with Liquers philosophy: Fits the query-based, layered architecture
- Questions identified: Open problems documented, no blocking unknowns

**Phase 2 Review (2 haiku + 1 sonnet):**
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

**Phase 3 Review (3 haiku + 1 sonnet):**
- Overview table of all examples and tests present
- Examples cover 2-3 realistic scenarios
- User feedback incorporated on prototype type
- Corner cases addressed: Memory, concurrency, errors, serialization
- Test coverage: Unit + integration tests planned, error paths included
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

**Full checklist:** See `references/review-checklist.md`

## Feature Folder Management

### Initializing a Feature

Use the provided script to create the folder structure:

```bash
python3 scripts/init_feature.py <feature-name>
```

**Creates:**
```
specs/<feature-name>/
├── DESIGN.md                    # Phase status tracking
├── phase1-high-level-design.md  # Phase 1 document (from template)
├── phase2-architecture.md       # Phase 2 document (from template)
├── phase3-examples.md           # Phase 3 document (from template)
└── phase4-implementation.md     # Phase 4 document (from template)
```

### Validating Phase Completion

Before requesting user approval, validate the phase:

```bash
python3 scripts/validate_phase.py <feature-name> <phase-number>
```

**Checks:**
- Phase file exists and is non-empty
- Required sections present (per phase)
- No template placeholders remaining (e.g., `[TODO: ...]`)

### Migration Note

This folder structure applies to **new features only**. Existing specs in the flat `specs/` directory remain as-is. Only use feature folders for designs created with liquers-designer.

## Agent Orchestration

The liquers-designer workflow uses **multi-agent review** to distribute review work across specialized sub-agents, improving coverage and catching issues earlier.

### Model Selection Rationale

| Model | Role | When Used |
|-------|------|-----------|
| **Haiku** | Parallel reviewer / drafter | Phase 2-4 reviews, Phase 3 drafting. Fast, cheap, good for focused single-concern checks. Run many in parallel. |
| **Sonnet** | Synthesizer / fixer | After haiku reviews surface issues. Integrates multiple review outputs, fixes documents, asks user targeted questions. Also used for Phase 3 synthesis. |
| **Opus** | Final critical reviewer | Phase 4 only. Comprehensive review of ALL phase documents together. Catches cross-phase inconsistencies and architectural issues. |

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

**Note:** Do NOT invoke other skills manually. The liquers-designer workflow automatically calls them at the appropriate phases.

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

This is **liquers-designer v2.0** - multi-agent review architecture.

**Changelog:**
- v2.0 (2026-02-14): Multi-agent review architecture
  - Phase 2: Added relevant commands identification + 2 haiku / 1 sonnet review
  - Phase 3: Added multi-agent drafting (up to 5 haiku + 1 sonnet synthesizer) + 3 haiku / 1 sonnet review
  - Phase 4: Added agent specification per step + 4 haiku / 1 opus review
  - Added Agent Orchestration section (model selection, skills/knowledge, orchestration pattern)
  - Updated workflow diagram, critical review process, and all phase templates
- v1.0 (2026-02-13): Initial 4-phase workflow with auto-invoke support
