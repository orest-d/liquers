#!/usr/bin/env python3
"""
Validate phase completion for liquers-project workflow.

Usage:
    validate_phase.py <feature-name> <phase-number>

Checks:
- Phase file exists and is non-empty
- Required sections present
- Review checklist addressed
- No template placeholders remaining

Example:
    python3 validate_phase.py parquet-support 1
    # Validates specs/design/parquet-support/phase1-high-level-design.md
"""

import sys
import re
from pathlib import Path

# Required sections per phase
REQUIRED_SECTIONS = {
    1: [
        "Feature Name",
        "Purpose",
        "Core Interactions",
        "Crate Placement",
        "Documentation Intent",
        "Open Questions"
    ],
    2: [
        "Overview",
        "Known-Issue Preflight",
        "Data Structures",
        "Trait Implementations",
        "Sync vs Async",
        "Function Signatures",
        "Integration Points",
        "Documentation Architecture",
        "Relevant Commands",
        "Error Handling"
    ],
    3: [
        "Overview Table",
        "Example",
        "Corner Cases",
        "Documentation and Learning Log",
        "Test Plan"
    ],
    4: [
        "Overview",
        "Implementation Steps",
        "Testing Plan",
        "Agent Assignment",
        "Rollback Plan",
        "Phase 5 Entry Criteria"
    ],
    5: [
        "Completion Preconditions",
        "Implementation Summary",
        "Documentation Delivered",
        "Issues Filed",
        "Important Learning",
        "Conformance and Remaining Work",
        "Validation"
    ]
}

# Template placeholders that indicate incomplete sections
TEMPLATE_PLACEHOLDERS = [
    r"\[.*?\]",  # [Square brackets with content]
    r"\(.*?\)",  # (Parentheses with content like (Add notes))
]


def find_specs_dir():
    """Find the specs/ directory by walking up from current directory."""
    current_dir = Path.cwd()
    for parent in [current_dir, *current_dir.parents]:
        potential_specs = parent / "specs"
        if potential_specs.is_dir():
            return potential_specs
    return None


def validate_phase(feature_name, phase_num):
    """Validate phase document completeness."""
    # Find specs directory
    specs_dir = find_specs_dir()
    if specs_dir is None:
        print("[ERROR] Could not find specs/ directory")
        print("   Run this script from the project root or a subdirectory")
        return False

    # Construct phase file path
    feature_dir = specs_dir / "design" / feature_name
    if not feature_dir.is_dir():
        print(f"[ERROR] Feature directory not found: {feature_dir}")
        print(f"   Did you run init_feature.py first?")
        return False

    design_file = feature_dir / "DESIGN.md"
    if not design_file.is_file():
        print(f"[ERROR] DESIGN.md not found: {design_file}")
        return False
    try:
        design_content = design_file.read_text(encoding="utf-8")
    except Exception as e:
        print(f"[ERROR] Could not read DESIGN.md: {e}")
        return False
    if not re.search(r"(?m)^workflow:\s*liquers-project\s*$", design_content):
        print("[ERROR] DESIGN.md must contain 'workflow: liquers-project'")
        print("   This marker distinguishes the mandatory five-phase project workflow")
        return False
    print("[OK] DESIGN.md declares workflow: liquers-project")

    phase_file = feature_dir / f"phase{phase_num}-*.md"
    # Find matching file
    matching_files = list(feature_dir.glob(f"phase{phase_num}-*.md"))
    if not matching_files:
        print(f"[ERROR] Phase {phase_num} file not found in {feature_dir}")
        return False

    phase_file = matching_files[0]

    print(f"[VALIDATE] {phase_file}")
    print()

    # Read phase content
    try:
        content = phase_file.read_text(encoding="utf-8")
    except Exception as e:
        print(f"[ERROR] Could not read file: {e}")
        return False

    # Check 1: File is non-empty
    if len(content.strip()) == 0:
        print("[ERROR] File is empty")
        return False
    print("[OK] File is non-empty")

    # Check 2: Required sections present
    required_sections = REQUIRED_SECTIONS.get(phase_num, [])
    missing_sections = []
    for section in required_sections:
        # Look for section heading (case-insensitive, flexible formatting)
        pattern = re.compile(f"##.*{re.escape(section)}", re.IGNORECASE)
        if not pattern.search(content):
            missing_sections.append(section)

    if missing_sections:
        print("[ERROR] Missing required sections:")
        for section in missing_sections:
            print(f"   - {section}")
        return False
    print(f"[OK] All required sections present ({len(required_sections)} sections)")

    # Check 3: Template placeholders remaining
    # Count placeholder patterns
    placeholder_count = 0
    placeholder_lines = []

    for line_num, line in enumerate(content.splitlines(), start=1):
        for pattern in TEMPLATE_PLACEHOLDERS:
            if re.search(pattern, line):
                # Exclude headings (they use [brackets] for emphasis)
                if not line.strip().startswith("#"):
                    placeholder_count += 1
                    if len(placeholder_lines) < 5:  # Show max 5 examples
                        placeholder_lines.append((line_num, line.strip()[:80]))

    if placeholder_count > 0:
        print(f"[WARN] Found {placeholder_count} potential template placeholders")
        print(f"   (This may indicate incomplete sections)")
        if placeholder_lines:
            print(f"   Example lines:")
            for line_num, line in placeholder_lines:
                print(f"     Line {line_num}: {line}")
        print()
        print("   [INFO] These might be legitimate content; review manually")
        # Don't fail validation, just warn
    else:
        print("[OK] No obvious template placeholders found")

    # Check 4: Minimum content length (heuristic for completeness)
    min_length = {
        1: 500,   # Phase 1: ~30 lines, ~500 chars
        2: 2000,  # Phase 2: More detailed, ~2000 chars
        3: 1500,  # Phase 3: Examples + tests, ~1500 chars
        4: 2000,  # Phase 4: Implementation plan, ~2000 chars
        5: 1000,  # Phase 5: Concise one-to-three-page summary
    }

    min_chars = min_length.get(phase_num, 500)
    if len(content) < min_chars:
        print(f"[WARN] Phase {phase_num} content is shorter than expected")
        print(f"   Expected: ~{min_chars} chars, Found: {len(content)} chars")
        print(f"   (This may indicate incomplete content)")
        # Don't fail, just warn
    else:
        print(f"[OK] Content length is adequate ({len(content)} chars)")

    print()
    print(f"[OK] Phase {phase_num} validation passed")
    print()
    print(f"Next steps:")
    print(f"1. Review the phase document manually")
    print(f"2. Run critical review using references/review-checklist.md")
    if phase_num == 5:
        print("3. Request user approval before marking the design complete")
    else:
        print(f"3. Request user approval before proceeding to Phase {phase_num + 1}")

    return True


def main():
    if len(sys.argv) != 3:
        print("Usage: validate_phase.py <feature-name> <phase-number>")
        print("\nExample:")
        print("  python3 validate_phase.py parquet-support 1")
        print("\nValidates:")
        print("  specs/design/<feature-name>/phase<phase-number>-*.md")
        sys.exit(1)

    feature_name = sys.argv[1]
    try:
        phase_num = int(sys.argv[2])
    except ValueError:
        print("[ERROR] Phase number must be an integer (1-5)")
        sys.exit(1)

    if phase_num not in [1, 2, 3, 4, 5]:
        print("[ERROR] Phase number must be 1, 2, 3, 4, or 5")
        sys.exit(1)

    result = validate_phase(feature_name, phase_num)
    if not result:
        sys.exit(1)


if __name__ == "__main__":
    main()
