#!/usr/bin/env python3
"""
Validate one phase of a liquers-project design.

Usage:
    validate_phase.py <slug> <phase-number>

Works on both forms:
    full     specs/design/<slug>/phase<N>-*.md, with `##` sections
    compact  the `## Phase <N>` section of specs/design/<slug>/DESIGN.md (`form: compact`),
             with `###` subsections

Checks: the phase exists and is non-empty; required sections are present; no template
placeholders (`<…>` hints) remain; the length is within the skill's size limits (warning).

Example:
    python3 validate_phase.py parquet-support 1
"""

import argparse
import re
import sys
from pathlib import Path

# Section names are matched case-insensitively as a heading prefix. A tuple means "any of".
REQUIRED_FULL = {
    1: ["Purpose", "Problem Example", ("Scope and Acceptance Criteria", "Acceptance Criteria"),
        "Core Interactions", "Crate Placement", "Documentation Intent",
        ("Open Questions", "Design Readiness")],
    2: ["Overview", "Known-Issue Preflight", ("Interfaces", "Data Structures"), "Integration Points",
        "Error Handling", "Relevant Commands", "Documentation Architecture", "Risks"],
    3: ["Overview Table", "Example", ("Edge and Error Cases", "Corner Cases"), "Test Plan"],
    4: ["Overview", "Implementation Steps", "Testing Plan", "Rollback Plan",
        "Documentation Updates", "Phase 5 Entry Criteria"],
    5: ["Completion Preconditions", "Implementation Summary", "Documentation Delivered",
        "Issues Filed", "Important Learning", "Conformance and Remaining Work", "Validation"],
}
REQUIRED_COMPACT = {
    1: ["Purpose", "Problem Example", ("Scope and Acceptance Criteria", "Acceptance Criteria"),
        ("Design Readiness", "Open Questions")],
    2: ["Solution", "Changes", "Risks"],
    3: ["Examples", "Tests"],
    4: ["Steps", "Validation"],
    5: [],
}
# Size limits from SKILL.md §Design depth, as characters (≈ 3,000 per page). Warnings only.
MAX_CHARS_FULL = {1: 6000, 2: 9000, 3: 9000, 4: 9000, 5: 9000}
MAX_CHARS_COMPACT_TOTAL = {"S": 9000, "M": 15000}

# A template hint: `<` + capital letter + a phrase with a space, or a bare name hint. Inline code
# is removed before matching, so `Result<Value, Error>` in backticks is never a placeholder.
PLACEHOLDER = re.compile(r"<[A-Z][^<>\n]* [^<>\n]*>|<(name|title|slug)>")
INLINE_CODE = re.compile(r"`[^`\n]*`")


def find_specs_dir() -> Path | None:
    current = Path.cwd()
    for parent in [current, *current.parents]:
        if (parent / "specs").is_dir():
            return parent / "specs"
    return None


def front_matter(text: str) -> dict:
    match = re.match(r"---\n(.*?)\n---\n", text, re.S)
    fields = {}
    if match:
        for line in match.group(1).splitlines():
            if ":" in line and not line.startswith(" "):
                key, value = line.split(":", 1)
                fields[key.strip()] = value.split("#", 1)[0].strip()
    return fields


def compact_section(text: str, phase: int) -> str | None:
    match = re.search(rf"(?m)^## Phase {phase}\b.*?(?=^## Phase \d|\Z)", text, re.S)
    return match.group(0) if match else None


def missing_sections(content: str, required: list, level: str) -> list[str]:
    missing = []
    for item in required:
        names = item if isinstance(item, tuple) else (item,)
        if not any(
            re.search(rf"(?mi)^{level} +(\d+\.? +)?{re.escape(n)}", content) for n in names
        ):
            missing.append(" / ".join(names))
    return missing


def validate_phase(slug: str, phase: int) -> bool:
    specs_dir = find_specs_dir()
    if specs_dir is None:
        print("[ERROR] Could not find a specs/ directory above the working directory")
        return False
    feature_dir = specs_dir / "design" / slug
    design_file = feature_dir / "DESIGN.md"
    if not design_file.is_file():
        print(f"[ERROR] {design_file} not found (run init_feature.py first)")
        return False
    design_text = design_file.read_text(encoding="utf-8")
    fields = front_matter(design_text)
    compact = fields.get("form") == "compact"
    workflow = fields.get("workflow") == "liquers-project"

    if phase == 5 and not workflow:
        print("[ERROR] Phase 5 exists only for `workflow: liquers-project` designs")
        return False

    if compact:
        content = compact_section(design_text, phase)
        where = f"{design_file} § Phase {phase}"
        if content is None:
            print(f"[ERROR] No `## Phase {phase}` section in {design_file}")
            return False
        required, level = REQUIRED_COMPACT[phase], "###"
    else:
        files = sorted(feature_dir.glob(f"phase{phase}-*.md"))
        if not files:
            print(f"[ERROR] No phase{phase}-*.md in {feature_dir}")
            return False
        content = files[0].read_text(encoding="utf-8")
        where = str(files[0])
        required, level = REQUIRED_FULL[phase], "##"

    print(f"[VALIDATE] {where} ({'compact' if compact else 'full'} form)")
    ok = True
    if not content.strip():
        print("[ERROR] Phase is empty")
        return False

    missing = missing_sections(content, required, level)
    if missing:
        ok = False
        print("[ERROR] Missing sections:")
        for name in missing:
            print(f"   - {name}")
    else:
        print(f"[OK] Required sections present ({len(required)})")

    placeholders = [
        (n, line.strip()[:80])
        for n, line in enumerate(content.splitlines(), start=1)
        if PLACEHOLDER.search(INLINE_CODE.sub("", line))
    ]
    if placeholders:
        ok = False
        print(f"[ERROR] {len(placeholders)} template placeholder line(s) remain, e.g.:")
        for n, line in placeholders[:5]:
            print(f"   line {n}: {line}")
    else:
        print("[OK] No template placeholders")

    if compact:
        size = fields.get("complexity") or "M"
        limit = MAX_CHARS_COMPACT_TOTAL.get(size, MAX_CHARS_COMPACT_TOTAL["M"])
        length = len(design_text)
    else:
        limit, length = MAX_CHARS_FULL[phase], len(content)
    if length > limit:
        print(f"[WARN] {length} characters, over the ~{limit} size limit: split or trim")
    else:
        print(f"[OK] Length {length} characters (limit ~{limit})")

    if not workflow:
        print("[INFO] No `workflow: liquers-project`: a four-phase design (bulk/triage/compaction)")

    print(f"[{'OK' if ok else 'FAIL'}] Phase {phase} validation {'passed' if ok else 'failed'}")
    return ok


# Lowercase-kebab, as defined by specs/DOCS_STRUCTURE_GUIDE.md §2 "Naming".
SLUG_RE = re.compile(r"^[a-z0-9]+(-[a-z0-9]+)*$")


def parse_args(argv=None):
    parser = argparse.ArgumentParser(
        description="Validate one phase of specs/design/<slug>/ (full or compact form).",
        epilog="Example: python3 validate_phase.py parquet-support 1",
    )
    parser.add_argument("feature_name", help="lowercase-kebab design slug, e.g. parquet-support")
    parser.add_argument("phase_number", type=int, choices=[1, 2, 3, 4, 5], help="phase to validate")
    args = parser.parse_args(argv)
    if not SLUG_RE.match(args.feature_name):
        parser.error(f"'{args.feature_name}' is not a lowercase-kebab slug")
    return args


def main(argv=None):
    args = parse_args(argv)
    if not validate_phase(args.feature_name, args.phase_number):
        sys.exit(1)


if __name__ == "__main__":
    main()
