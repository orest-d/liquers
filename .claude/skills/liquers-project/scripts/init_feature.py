#!/usr/bin/env python3
"""
Initialize a design folder in specs/design/ for the liquers-project workflow.

Usage:
    init_feature.py <feature-name>

Creates:
    specs/design/<feature-name>/
    ├── DESIGN.md              # Phase tracking
    ├── phase1-high-level-design.md
    ├── phase2-architecture.md
    ├── phase3-examples.md
    ├── phase4-implementation.md
    └── phase5-documentation.md

Example:
    python3 init_feature.py parquet-support
    # Creates specs/design/parquet-support/ with all phase documents
"""

import sys
from pathlib import Path
from datetime import datetime

# Template for DESIGN.md (phase tracking)
DESIGN_MD_TEMPLATE = """---
id: {feature_upper}
kind: design
title: {feature_name}
workflow: liquers-project
status: draft
phase: high-level
area: []
gh_pr: []
issues: []
affects_docs: []
created: {date}
superseded_by:
---
# {feature_name} Design Tracking

**Created:** {date}

## Phase Status

- [ ] Phase 1: High-Level Design
- [ ] Phase 2: Solution & Architecture
- [ ] Phase 3: Examples & Testing
- [ ] Phase 4: Implementation Plan
- [ ] Phase 5: Documentation
- [ ] Implementation Complete

## Notes

(Add notes as you progress through phases)

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
"""

# Template for phase1-high-level-design.md
PHASE1_TEMPLATE = """# Phase 1: High-Level Design - {feature_name}

## Feature Name

{feature_name}

## Purpose

[1-3 sentences explaining why this feature exists and what problem it solves]

## Core Interactions

### Query System
[How does this feature integrate with Query parsing, execution, or Key encoding?]

### Store System
[Does this feature read from or write to stores? Which store types?]

### Command System
[What new commands does this feature introduce? What namespace?]

### Asset System
[Does this feature create, consume, or transform assets?]

### Value Types
[Does this feature introduce new ExtValue variants or ValueExtension types?]

### Web/API (if applicable)
[Does this feature add HTTP endpoints or modify the Axum server?]

### UI (if applicable)
[Does this feature add UI elements, widgets, or modify the UI framework?]

## Crate Placement

[Which crate(s) will contain this feature? liquers-core, liquers-lib, liquers-store, liquers-axum?]
[Rationale for placement based on dependency flow]

## Documentation Intent

**Reference:** [Create a new reference, extend an existing reference at an exact path, or neither — explain why]

**Guide:** [Create a new guide, extend an existing guide at an exact path, or neither — explain why]

**Other documents to create:** [List kinds and candidate paths, or `None` with rationale]

**Specific documents to update:** [List exact paths and intended changes, or `None` with rationale]

[State the intended audience and what future developers or coding agents should understand without
reading this design. Reconsider `neither` later if substantial learning accumulates.]

## Open Questions

1. [Question 1]
2. [Question 2]

## References

- [Link to related specs, issues, or external documentation]
"""

# Template for phase2-architecture.md
PHASE2_TEMPLATE = """# Phase 2: Solution & Architecture - {feature_name}

## Overview

[2-3 sentences summarizing the architectural approach]

## Known-Issue Preflight

[Search issues linked to the design, overlapping affected areas, and touching integration points,
dependencies, public APIs, or architecture assumptions. Include relevant locally open issues,
including `accepted` and `in_progress` items, from `specs/index.csv`.]

| Issue | Status | Current priority | Relevance and solution impact | Must be addressed first? | Blocking? | Required action | Priority action |
|---|---|---|---|---|---|---|---|
| [Issue ID or `None found`] | [open status] | [P0-P3] | [Impact on solution] | [yes/no] | [yes/no] | [Resolve, redesign, or monitor] | [Keep or recommend change] |

### Blocking and Priority Decision

[Resolve blockers first or redesign to remove the dependency; do not approve Phase 2 with an
unresolved blocker. Every blocker must be at least P1. Use P0 only when the issue also meets
`DOCS_STRUCTURE_GUIDE.md` §4.4 impact criteria. Record and confirm priority changes.]

## Data Structures

### New Structs

[Define structs with fields, types, ownership rationale]

### New Enums

[Define enums with variants and their semantics]

### ExtValue Extensions (if applicable)

[If adding new ExtValue variants, document them here]

## Trait Implementations

[List traits to implement, for which types, with signatures]

## Generic Parameters & Bounds

[Document generic parameters and justify bounds]

## Sync vs Async Decisions

[Table or list of functions with async/sync choice and rationale]

## Function Signatures

[Provide function signatures for all public functions]

## Integration Points

[Which crates, which files, which modules to modify or create]

## Documentation Architecture

### Reference Plan
[New, extend existing, or none; exact path, kind, audience, area, purpose, sections/claims, links]

### Guide Plan
[New, extend existing, or none; exact path, kind, audience, area, workflow, examples/snippets, links]

### Other Documents to Create
[Exact paths, kinds, audiences, purposes, and link destinations; or `None` with rationale]

### Existing Documents to Review or Update
[Every specific Phase 1 update plus area candidates, exact changes, discarded candidates, and the
proposed authoritative `affects_docs` set]

### Design and Capability Links
[Where links to design artifacts must be added, updated, or replaced, including `specs/README.md`]

## Relevant Commands

### New Commands
[List all new commands with full signatures]

### Relevant Existing Namespaces
[Which existing command namespaces interact with this feature?]

## Web Endpoints (if applicable)

[Document new or modified HTTP endpoints]

## Error Handling

[Error scenarios, which ErrorType to use, error propagation strategy]

## Serialization Strategy

[Serde annotations, round-trip compatibility]

## Concurrency Considerations

[Thread safety, locks, shared state]

## Compilation Validation

[Mental check: would this compile with cargo check?]

## References to liquers-patterns.md

[Verify alignment with established patterns]
"""

# Template for phase3-examples.md
PHASE3_TEMPLATE = """# Phase 3: Examples & Use-cases - {feature_name}

## High-Level Introduction

[Explain how these scenarios demonstrate the Phase 1 purpose and interactions. Introduce the
progression from the representative primary workflow, through additional detail, to optional
pitfalls and edge cases.]

## Example Type

**User choice:** [Runnable prototypes / Conceptual code]

## Overview Table

| # | Type | Name | Purpose | Drafted By |
|---|------|------|---------|------------|
| 1 | Example | [Primary scenario] | [Demonstrates the Phase 1 design through the primary workflow] | [Agent] |
| 2 | Example | [Detailed scenario] | [Explains additional details that build on Scenario 1] | [Agent] |
| 3 | Example | [Pitfalls and edge cases] | [Explains common pitfalls and edge cases; optional] | [Agent] |

## Example 1: [Primary Scenario Name]

### Connection to the High-Level Design
[How does this scenario solve or demonstrate the Phase 1 purpose and interactions?]

### Scenario
[Verbally explain the user context and intended outcome. Keep this representative use case
medium-complexity: non-trivial, but free of unnecessary details. Change defaults only when relevant.]

### Sequence of Steps
1. [First component interaction or method call]
2. [Next component and its responsibility]
3. [Execution, caching, persistence, or other relevant coordination]
4. [How the caller polls, awaits, retrieves, or uses the result]

### Core Example Code
```rust
// Show the core workflow; omit incidental setup and irrelevant non-default configuration.
```

### Guide and Executable Example
[If Phase 2 requires a guide, normally implement this primary scenario as a complete executable
example and give the path the guide will reference. Otherwise explain what canonical code or test
should be referenced.]

**Expected output:**
```
[What the user should see]
```

## Example 2: [Detailed Scenario]

[Build on Scenario 1 and explain an additional mechanism, meaningful configuration choice, or
component interaction. Reuse the primary setup and show only the relevant code delta. Include
expected output and validation.]

## Example 3 (Optional): [Pitfalls and Edge Cases]

[For each common pitfall or important edge case, state the symptom, cause, correct usage or
recovery, and protective test. Omit this scenario if it would only repeat later sections.]

## Corner Cases

### 1. Memory
[Large inputs, allocation failures, memory leaks]

### 2. Concurrency
[Race conditions, deadlocks, thread safety]

### 3. Errors
[Invalid input, network failures, serialization errors]

### 4. Serialization
[Round-trip, schema evolution, compression]

### 5. Integration
[Store, Command, Asset, Web/API interactions]

## Documentation and Learning Log

### Guide Candidate Workflows and Examples
[Answer: How do I use X? How do I achieve X? What is the typical workflow? Select potential guide
snippets and link a complete executable example or unit/integration test when available.]

### Usage and Meaning
[What helps users, developers, or coding agents understand why the feature matters and how it
connects to existing functionality]

### Repeatable Development Guidance
[What would help someone implement or extend similar functionality]

### Corrections and Unexpected Learning
[Design assumptions corrected, implementation surprises, useful dead ends, and facts to verify in
Phase 5]

## Test Plan

### Unit Tests
[File paths, test names, coverage]

### Integration Tests
[File paths, test names, end-to-end flows]

### Manual Validation
[Commands to run, expected outputs]

## Auto-Invoke: liquers-unittest Skill Output

[Test templates generated by skill]
"""

# Template for phase4-implementation.md
PHASE4_TEMPLATE = """# Phase 4: Implementation Plan - {feature_name}

## Overview

**Feature:** {feature_name}

**Architecture:** [1-2 sentence summary]

**Estimated complexity:** [Low / Medium / High]

**Estimated time:** [X hours]

**Prerequisites:**
- Phase 1, 2, 3 approved
- All open questions resolved
- Dependencies identified

## Implementation Steps

### Step 1: [Action Description]

**File:** `[exact-file-path]`

**Action:**
- [Specific change 1]
- [Specific change 2]

**Code changes:**
```rust
// NEW: Add this code
// MODIFY: Change existing code
// DELETE: Remove this code
```

**Validation:**
```bash
cargo check -p [crate-name]
```

**Rollback:**
```bash
git checkout [file-path]
```

**Agent Specification:**
- **Model:** [haiku / sonnet / opus]
- **Skills:** [rust-best-practices, liquers-unittest, etc.]
- **Knowledge:** [Which files, specs, patterns the agent needs]
- **Rationale:** [Why this model]

---

[Repeat for each step]

## Testing Plan

### Unit Tests
[When to run, file paths, commands]

### Integration Tests
[When to run, file paths, commands]

### Manual Validation
[Commands to run, expected outputs]

## Agent Assignment Summary

| Step | Model | Skills | Rationale |
|------|-------|--------|-----------|
| 1 | [model] | [skills] | [rationale] |

## Rollback Plan

[Per-step and full feature rollback procedures]

## Documentation Updates

[New reference/guide files, affected existing documents, History/reviewed updates, capability-map
links, and a step for collecting implementation/review learning for Phase 5]

## Phase 5 Entry Criteria

- [ ] Implementation is finished and validated
- [ ] All user comments are answered
- [ ] All review comments are answered
- [ ] Documentation can be verified against implemented and tested behavior
- [ ] Phase 5 documentation is included in the implementation PR when practical

## Execution Options

[Execute now, create tasks, revise, exit]
"""

PHASE5_TEMPLATE = """# Phase 5: Documentation - {feature_name}

## Completion Preconditions

- [ ] Implementation is finished and validated
- [ ] All user comments are answered or incorporated
- [ ] All review comments are answered or incorporated
- [ ] Documentation is consistent with implemented and tested behavior
- [ ] Documentation is included in the implementation PR when practical

## Implementation Summary

[About one page; never more than three pages for this whole document. State what was implemented
and whether it conforms to the request and approved design. Identify anything omitted or added and
explain why. Link to reference/guide documents for detail.]

## Documentation Delivered

### New Reference Documents
[Paths and purposes, or `None` with rationale]

### New Guide Documents
[Paths and purposes, or `None` with rationale]

### Existing Documents Reviewed or Updated
[Authoritative `affects_docs` set, review result, and History/reviewed updates]

### Links and Capability Map
[Links added, updated, or replaced in `specs/README.md` and other documentation]

## Issues Filed

[New issue IDs and one-line explanations, including intentionally omitted design scope; or `None`]

## Important Learning

[Meaning and importance of the work, connections to existing functionality, repeatable guidance,
corrections, and unexpected learning. Keep detail in reference/guide documents and link it here.]

## Conformance and Remaining Work

[Explicitly compare requested, approved, and implemented scope. State whether anything remains. Any
deferred remainder must be represented by an issue rather than a partial design status.]

## Validation

[Documentation checks run and their outcomes. If rebase, merge conflict, or integration changed
relevant content, record the post-merge consistency review here.]
"""


def init_feature(feature_name):
    """Initialize feature folder structure."""
    # Determine project root (assume script is in liquers/scripts/ or skill scripts/)
    # Look for specs/ directory by walking up the tree
    current_dir = Path.cwd()
    specs_dir = None

    # Try to find specs/ directory
    for parent in [current_dir, *current_dir.parents]:
        potential_specs = parent / "specs"
        if potential_specs.is_dir():
            specs_dir = potential_specs
            break

    # If not found, create in current directory
    if specs_dir is None:
        specs_dir = current_dir / "specs"
        specs_dir.mkdir(exist_ok=True)
        print(f"[WARN] No existing specs/ directory found. Creating: {specs_dir}")

    feature_dir = specs_dir / "design" / feature_name

    if feature_dir.exists():
        print(f"[ERROR] Feature directory already exists: {feature_dir}")
        print(f"   Please choose a different name or remove the existing directory.")
        return None

    # Create feature directory
    feature_dir.mkdir(parents=True)

    # Create DESIGN.md
    design_md = feature_dir / "DESIGN.md"
    design_content = DESIGN_MD_TEMPLATE.format(
        feature_name=feature_name,
        feature_upper=feature_name.upper().replace("_", "-"),
        date=datetime.now().strftime("%Y-%m-%d")
    )
    design_md.write_text(design_content, encoding="utf-8")

    # Create phase documents
    phase1_md = feature_dir / "phase1-high-level-design.md"
    phase1_content = PHASE1_TEMPLATE.format(feature_name=feature_name)
    phase1_md.write_text(phase1_content, encoding="utf-8")

    phase2_md = feature_dir / "phase2-architecture.md"
    phase2_content = PHASE2_TEMPLATE.format(feature_name=feature_name)
    phase2_md.write_text(phase2_content, encoding="utf-8")

    phase3_md = feature_dir / "phase3-examples.md"
    phase3_content = PHASE3_TEMPLATE.format(feature_name=feature_name)
    phase3_md.write_text(phase3_content, encoding="utf-8")

    phase4_md = feature_dir / "phase4-implementation.md"
    phase4_content = PHASE4_TEMPLATE.format(feature_name=feature_name)
    phase4_md.write_text(phase4_content, encoding="utf-8")

    phase5_md = feature_dir / "phase5-documentation.md"
    phase5_content = PHASE5_TEMPLATE.format(feature_name=feature_name)
    phase5_md.write_text(phase5_content, encoding="utf-8")

    print(f"[OK] Feature initialized: {feature_dir}")
    print(f"\nCreated files:")
    print(f"  - DESIGN.md")
    print(f"  - phase1-high-level-design.md")
    print(f"  - phase2-architecture.md")
    print(f"  - phase3-examples.md")
    print(f"  - phase4-implementation.md")
    print(f"  - phase5-documentation.md")
    print(f"\nNext steps:")
    print(f"1. Edit {feature_dir}/phase1-high-level-design.md")
    print(f"2. Run critical review using references/review-checklist.md")
    print(f"3. Get user approval before proceeding to Phase 2")

    return feature_dir


def main():
    if len(sys.argv) != 2:
        print("Usage: init_feature.py <feature-name>")
        print("\nExample:")
        print("  python3 init_feature.py parquet-support")
        sys.exit(1)

    feature_name = sys.argv[1]

    # Validate feature name (basic sanity check)
    if (
        not feature_name
        or len(feature_name) > 60
        or feature_name != feature_name.lower()
        or feature_name.startswith("-")
        or feature_name.endswith("-")
        or "--" in feature_name
        or not feature_name.replace("-", "").isalnum()
    ):
        print(f"[ERROR] Invalid feature name: {feature_name}")
        print("   Use a lowercase-kebab slug of at most 60 characters.")
        sys.exit(1)

    result = init_feature(feature_name)
    if result is None:
        sys.exit(1)


if __name__ == "__main__":
    main()
