#!/usr/bin/env python3
"""
Initialize a feature folder in specs/ for liquers-designer workflow.

Usage:
    init_feature.py <feature-name>

Creates:
    specs/<feature-name>/
    ├── DESIGN.md              # Phase tracking
    ├── phase1-high-level-design.md
    ├── phase2-architecture.md
    ├── phase3-examples.md
    └── phase4-implementation.md

Example:
    python3 init_feature.py parquet-support
    # Creates specs/parquet-support/ with all phase documents
"""

import sys
from pathlib import Path
from datetime import datetime

# Template for DESIGN.md (phase tracking)
DESIGN_MD_TEMPLATE = """# {feature_name} Design Tracking

**Created:** {date}

**Status:** In Progress

## Phase Status

- [ ] Phase 1: High-Level Design
- [ ] Phase 2: Solution & Architecture
- [ ] Phase 3: Examples & Testing
- [ ] Phase 4: Implementation Plan
- [ ] Implementation Complete

## Notes

(Add notes as you progress through phases)

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
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

## Example Type

**User choice:** [Runnable prototypes / Conceptual code]

## Overview Table

| # | Type | Name | Purpose | Drafted By |
|---|------|------|---------|------------|
| 1 | Example | [Scenario 1] | [Purpose] | [Agent] |

## Example 1: [Scenario Name]

**Scenario:** [Brief description]

**Context:** [When would a user encounter this?]

**Code:**
```rust
// Example code here
```

**Expected output:**
```
[What the user should see]
```

## Example 2: [Another Scenario]

[Repeat for 2-3 examples]

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

[CLAUDE.md, PROJECT_OVERVIEW.md, README.md updates]

## Execution Options

[Execute now, create tasks, revise, exit]
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
        print(f"⚠️  No existing specs/ directory found. Creating: {specs_dir}")

    feature_dir = specs_dir / feature_name

    if feature_dir.exists():
        print(f"❌ Feature directory already exists: {feature_dir}")
        print(f"   Please choose a different name or remove the existing directory.")
        return None

    # Create feature directory
    feature_dir.mkdir(parents=True)

    # Create DESIGN.md
    design_md = feature_dir / "DESIGN.md"
    design_content = DESIGN_MD_TEMPLATE.format(
        feature_name=feature_name,
        date=datetime.now().strftime("%Y-%m-%d")
    )
    design_md.write_text(design_content)

    # Create phase documents
    phase1_md = feature_dir / "phase1-high-level-design.md"
    phase1_content = PHASE1_TEMPLATE.format(feature_name=feature_name)
    phase1_md.write_text(phase1_content)

    phase2_md = feature_dir / "phase2-architecture.md"
    phase2_content = PHASE2_TEMPLATE.format(feature_name=feature_name)
    phase2_md.write_text(phase2_content)

    phase3_md = feature_dir / "phase3-examples.md"
    phase3_content = PHASE3_TEMPLATE.format(feature_name=feature_name)
    phase3_md.write_text(phase3_content)

    phase4_md = feature_dir / "phase4-implementation.md"
    phase4_content = PHASE4_TEMPLATE.format(feature_name=feature_name)
    phase4_md.write_text(phase4_content)

    print(f"✅ Feature initialized: {feature_dir}")
    print(f"\nCreated files:")
    print(f"  - DESIGN.md")
    print(f"  - phase1-high-level-design.md")
    print(f"  - phase2-architecture.md")
    print(f"  - phase3-examples.md")
    print(f"  - phase4-implementation.md")
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
    if not feature_name.replace("-", "").replace("_", "").isalnum():
        print(f"❌ Invalid feature name: {feature_name}")
        print(f"   Feature names should contain only letters, numbers, hyphens, and underscores.")
        sys.exit(1)

    result = init_feature(feature_name)
    if result is None:
        sys.exit(1)


if __name__ == "__main__":
    main()
