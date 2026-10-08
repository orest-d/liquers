#!/usr/bin/env python3
"""
Initialize a design folder in specs/design/ for the liquers-project workflow.

Usage:
    init_feature.py <slug> [--compact] [--no-workflow]

Creates specs/design/<slug>/ with DESIGN.md and, in the full form, one file per phase. The
skeletons are read from the ```markdown block of each reference template, so the templates are the
only copy.

    full (default)  DESIGN.md + phase1-high-level-design.md … phase5-documentation.md
    --compact       DESIGN.md only, with one `## Phase N` section per phase (S/M designs)
    --no-workflow   omit `workflow: liquers-project` (bulk design, triage and compaction designs,
                    which have four phases and no Phase 5)

Example:
    python3 init_feature.py parquet-support
    python3 init_feature.py empty-frame-head-panics --compact --no-workflow
"""

import argparse
import re
import sys
from datetime import datetime
from pathlib import Path

REFERENCES = Path(__file__).resolve().parent.parent / "references"

PHASE_FILES = [
    ("phase1-high-level-design.md", "phase1-template.md"),
    ("phase2-architecture.md", "phase2-template.md"),
    ("phase3-examples.md", "phase3-template.md"),
    ("phase4-implementation.md", "phase4-template.md"),
    ("phase5-documentation.md", "phase5-documentation.md"),
]

DESIGN_MD_FULL = """---
id: {upper}
kind: design
title: {slug}
{workflow}status: draft
phase: high-level
area: []
issues: []
affects_docs: []
created: {date}
---
# {slug}

## Phases

{links}
"""


def skeleton(template_name: str) -> str:
    """Return the first ```markdown block of a reference template."""
    text = (REFERENCES / template_name).read_text(encoding="utf-8")
    match = re.search(r"```markdown\n(.*?)\n```", text, re.S)
    if match is None:
        raise SystemExit(f"[ERROR] no ```markdown skeleton in {template_name}")
    return match.group(1) + "\n"


def find_specs_dir() -> Path | None:
    current = Path.cwd()
    for parent in [current, *current.parents]:
        if (parent / "specs").is_dir():
            return parent / "specs"
    return None


def init_feature(slug: str, compact: bool, workflow: bool) -> Path | None:
    specs_dir = find_specs_dir()
    if specs_dir is None:
        print("[ERROR] Could not find a specs/ directory above the working directory")
        return None
    feature_dir = specs_dir / "design" / slug
    if feature_dir.exists():
        print(f"[ERROR] Design folder already exists: {feature_dir}")
        return None
    feature_dir.mkdir(parents=True)

    upper = slug.upper()
    date = datetime.now().strftime("%Y-%m-%d")
    workflow_line = "workflow: liquers-project\n" if workflow else ""

    if compact:
        text = skeleton("compact-design-template.md")
        text = text.replace("<SLUG-UPPER>", upper).replace("<title>", slug)
        text = text.replace("<SOURCE-ID>", "").replace("<YYYY-MM-DD>", date)
        # readiness/autofix are added when assessed (§5.1.1); an empty key would fail --check.
        text = re.sub(r"(?m)^(readiness|autofix):.*\n", "", text)
        if workflow:
            text = re.sub(r"(?m)^workflow:.*$", "workflow: liquers-project", text)
        else:
            text = re.sub(r"(?m)^workflow:.*\n", "", text)
        (feature_dir / "DESIGN.md").write_text(text, encoding="utf-8")
        created = ["DESIGN.md"]
    else:
        phase_files = PHASE_FILES if workflow else PHASE_FILES[:4]
        links = "\n".join(
            f"- [Phase {i}](./{name})" for i, (name, _) in enumerate(phase_files, start=1)
        )
        (feature_dir / "DESIGN.md").write_text(
            DESIGN_MD_FULL.format(
                upper=upper, slug=slug, workflow=workflow_line, date=date, links=links
            ),
            encoding="utf-8",
        )
        created = ["DESIGN.md"]
        for name, template in phase_files:
            (feature_dir / name).write_text(
                skeleton(template).replace("<name>", slug).replace("<Feature Name>", slug),
                encoding="utf-8",
            )
            created.append(name)

    print(f"[OK] Design initialized: {feature_dir}")
    for name in created:
        print(f"  - {name}")
    print("Next: write Phase 1, review it (references/review-checklist.md), then")
    print(f"      python3 validate_phase.py {slug} 1")
    return feature_dir


# Lowercase-kebab, as defined by specs/DOCS_STRUCTURE_GUIDE.md §2 "Naming".
SLUG_RE = re.compile(r"^[a-z0-9]+(-[a-z0-9]+)*$")


def parse_args(argv=None):
    parser = argparse.ArgumentParser(
        description="Create specs/design/<slug>/ from the liquers-project templates.",
        epilog="Example: python3 init_feature.py parquet-support --compact",
    )
    parser.add_argument(
        "feature_name",
        help="lowercase-kebab design slug (DOCS_STRUCTURE_GUIDE.md §2), e.g. parquet-support",
    )
    parser.add_argument(
        "--compact", action="store_true", help="single-file design for S/M complexity"
    )
    parser.add_argument(
        "--no-workflow",
        action="store_true",
        help="omit `workflow: liquers-project` (four-phase bulk/triage designs)",
    )
    args = parser.parse_args(argv)
    if not SLUG_RE.match(args.feature_name):
        parser.error(
            f"'{args.feature_name}' is not a lowercase-kebab slug "
            "(lowercase letters and digits separated by single hyphens)"
        )
    if len(args.feature_name) > 60:
        parser.error(f"'{args.feature_name}' is longer than 60 characters")
    return args


def main(argv=None):
    args = parse_args(argv)
    if init_feature(args.feature_name, args.compact, not args.no_workflow) is None:
        sys.exit(1)


if __name__ == "__main__":
    main()
