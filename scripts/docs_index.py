#!/usr/bin/env python3
"""Index and validate the documents under specs/.

Contract: specs/DOCS_STRUCTURE_GUIDE.md. This script owns specs/index.csv (§6) and the
generated blocks in specs/README.md (§8.2). It never edits a document's prose.

Modes (§7):
    docs_index.py                       regenerate specs/index.csv and the README blocks
    docs_index.py new "<title>" ...     scaffold an issue file (§7.1)
    docs_index.py --check               validate; non-zero exit on failure, never writes
    docs_index.py --sync                refresh GitHub-owned columns (not implemented yet)

No third-party dependencies: this has to run in a sandbox with no network and no pip.
"""

from __future__ import annotations

import argparse
import csv
import datetime as _dt
import io
import re
import subprocess
import sys
from pathlib import Path

# --------------------------------------------------------------------------- vocabularies
# Mirrors DOCS_STRUCTURE_GUIDE.md §3, §4.3, §4.4, §4.5, §5.1, §5.2. Kept in sync by CHECK 1,
# which fails when a document uses a value absent here.

AREAS = {
    "core/query", "core/plan", "core/commands", "core/assets", "core/store", "core/value",
    "core/context", "core/error", "core/validate", "macro", "store/backends", "store/config",
    "lib/commands", "lib/value", "lib/polars", "lib/image", "lib/egui", "lib/ui",
    "axum", "web", "py", "docs", "build",
}
ISSUE_STATUS = {"draft", "accepted", "rejected", "duplicate",
                "tracked", "in_progress", "closed", "closed_not_planned"}
DESIGN_STATUS = {"draft", "in_review", "approved", "in_implementation",
                 "implemented", "complete", "superseded", "abandoned"}
DESIGN_STATUS_NEEDING_PHASE = {"draft", "in_review", "approved", "in_implementation", "implemented"}
# §5.5: the statuses a human may write on a design that has a gh_pr. "" means "derived — ask
# GitHub"; the rest are terminal conclusions GitHub cannot draw.
TERMINAL_STATUS_WITH_PR = {"", "complete", "superseded", "abandoned"}
PHASES = {"high-level": 1, "architecture": 2, "examples": 3, "implementation": 4,
          "documentation": 5}
RETIRED_PHASES: set[str] = set()          # §5.3 rule 2: never delete, only mark retired
PRIORITIES = {"P0", "P1", "P2", "P3"}
COMPLEXITIES = {"S", "M", "L", "XL"}
NEEDS_DESIGN = {"L", "XL"}
AUDIENCES = {"internal", "user", "both"}
REVIEW_DAYS = 92                          # §9.4

COLUMNS = ["id", "kind", "title", "status", "status_source", "phase", "priority", "complexity",
           "area", "gh_issue", "gh_pr", "branch", "design", "reviewed", "created", "file"]

# No "# GENERATED" banner line: CSV has no comment syntax, so GitHub's renderer reads line 1
# as the header, finds one column, and refuses to render the table. The file is marked generated
# via .gitattributes (linguist-generated), and --check enforces that it matches regeneration —
# which is the protection that actually holds. A comment was only ever advisory.

REPO = Path(__file__).resolve().parent.parent
SPECS = REPO / "specs"


# --------------------------------------------------------------------------- front-matter
def parse_front_matter(text: str) -> tuple[dict, str]:
    """Return (fields, body). A missing or malformed block yields ({}, text).

    Deliberately not YAML: the schema is flat scalars and `[a, b]` lists, and depending on
    PyYAML would make the check unrunnable wherever it is not installed.
    """
    if not text.startswith("---\n"):
        return {}, text
    end = text.find("\n---\n", 4)
    if end == -1:
        return {}, text
    fields: dict[str, object] = {}
    for line in text[4:end].split("\n"):
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        key, _, value = line.partition(":")
        if not _:
            continue
        key, value = key.strip(), value.strip()
        if value.startswith("[") and value.endswith("]"):
            inner = value[1:-1].strip()
            fields[key] = [v.strip() for v in inner.split(",") if v.strip()] if inner else []
        else:
            fields[key] = value
    return fields, text[end + 5:]


def _list(fields: dict, key: str) -> list[str]:
    v = fields.get(key)
    if isinstance(v, list):
        return v
    return [v] if v else []


# --------------------------------------------------------------------------- collection
def collect() -> list[dict]:
    """One row per tracked document. Sorted by id, always (§6)."""
    rows: list[dict] = []

    for path in sorted((SPECS / "issues").glob("*.md")):
        f, _ = parse_front_matter(path.read_text(encoding="utf-8"))
        rows.append({
            "id": f.get("id", path.stem), "kind": f.get("kind", "issue"),
            "title": f.get("title", ""), "status": f.get("status", ""),
            "status_source": "github" if f.get("github") else "local",
            "phase": "", "priority": f.get("priority", ""),
            "complexity": f.get("complexity", ""), "area": ";".join(_list(f, "area")),
            "gh_issue": f.get("github", ""), "gh_pr": ";".join(_list(f, "gh_pr")),
            "branch": "", "design": f.get("design", ""), "reviewed": "",
            "created": f.get("created", ""), "file": path.relative_to(REPO).as_posix(),
            "_fm": f, "_path": path,
        })

    for path in sorted((SPECS / "design").glob("*/DESIGN.md")):
        f, _ = parse_front_matter(path.read_text(encoding="utf-8"))
        slug = path.parent.name
        gh_pr = _list(f, "gh_pr")
        rows.append({
            "id": f.get("id", slug.upper()), "kind": "design", "title": f.get("title", ""),
            "status": f.get("status", ""),
            # Who owns the value in the status column, not whether a PR exists (§5.5). A design
            # that has reached a hand-written terminal status owns it locally, gh_pr or not.
            "status_source": "github" if gh_pr and not f.get("status") else "local",
            "phase": f.get("phase", ""), "priority": "", "complexity": "",
            "area": ";".join(_list(f, "area")), "gh_issue": "", "gh_pr": ";".join(gh_pr),
            "branch": "", "design": slug, "reviewed": "", "created": f.get("created", ""),
            "file": path.relative_to(REPO).as_posix(), "_fm": f, "_path": path,
        })

    today = _dt.date.today()
    for sub, kind in (("reference", "reference"), ("guides", "guide")):
        for path in sorted((SPECS / sub).rglob("*.md")):
            f, _ = parse_front_matter(path.read_text(encoding="utf-8"))
            reviewed = f.get("reviewed", "")
            status = ""
            if reviewed:
                try:
                    age = (today - _dt.date.fromisoformat(str(reviewed))).days
                    status = "current" if age <= REVIEW_DAYS else "overdue"
                except ValueError:
                    status = ""
            rows.append({
                "id": path.stem, "kind": f.get("kind", kind), "title": f.get("title", ""),
                "status": status, "status_source": "local", "phase": "", "priority": "",
                "complexity": "", "area": ";".join(_list(f, "area")), "gh_issue": "",
                "gh_pr": "", "branch": "", "design": "", "reviewed": str(reviewed),
                "created": "", "file": path.relative_to(REPO).as_posix(),
                "_fm": f, "_path": path,
            })

    rows.sort(key=lambda r: r["id"])
    return rows


# --------------------------------------------------------------------------- writing
def render_csv(rows: list[dict]) -> str:
    out = io.StringIO()
    w = csv.DictWriter(out, fieldnames=COLUMNS, lineterminator="\n")
    w.writeheader()
    for r in rows:
        w.writerow({c: r.get(c, "") for c in COLUMNS})
    return out.getvalue()


def replace_block(text: str, name: str, body: str) -> str:
    """Rewrite only what lies between the markers; never touch a line outside them (§8.2)."""
    begin, end = f"<!-- BEGIN generated: {name} -->", f"<!-- END generated: {name} -->"
    i, j = text.find(begin), text.find(end)
    if i == -1 or j == -1 or j < i:
        return text
    return text[:i + len(begin)] + "\n" + body.rstrip("\n") + "\n" + text[j:]


def render_readme_blocks(rows: list[dict], readme: str) -> str:
    live = {"closed", "closed_not_planned", "rejected", "duplicate"}

    issues = [r for r in rows if r["kind"] in ("issue", "feature")
              and r["status"] not in live and r["design"] and r["priority"] in ("P0", "P1")]
    if issues:
        body = ["| Issue | Pri | Cx | Design |", "|---|---|---|---|"] + [
            f"| [`{r['id']}`]({Path(r['file']).relative_to('specs').as_posix()}) "
            f"| {r['priority']} | {r['complexity']} | `{r['design']}` |" for r in issues]
    else:
        body = ["*None.*"]
    readme = replace_block(readme, "issues", "\n".join(body))

    # Only the hand-written prose counts as "placed". Generated blocks must be stripped first:
    # the issues table names every design slug, so leaving them in makes every design look
    # referenced — and on a second run the unplaced list would mark itself placed.
    referenced = re.sub(r"<!-- BEGIN generated: .*?<!-- END generated: [a-z]+ -->", "",
                        readme, flags=re.S)
    unplaced = []
    for r in rows:
        if r["kind"] == "design" and r["status"] != "superseded":
            if r["design"] not in referenced:
                unplaced.append(f"- design `{r['design']}`")
        elif r["kind"] in ("reference", "guide"):
            rel = Path(r["file"]).relative_to("specs").as_posix()
            if rel not in referenced:
                unplaced.append(f"- `{rel}`")
        elif r["kind"] == "feature" and r["status"] not in live:
            if r["id"] not in referenced:
                unplaced.append(f"- feature `{r['id']}`")
    body = ("\n".join(sorted(unplaced)) if unplaced else
            "*Everything is placed in the capability map.*")
    readme = replace_block(readme, "unplaced", body)

    guides = []
    for r in rows:
        if r["kind"] != "guide":
            continue
        _, text = parse_front_matter((REPO / r["file"]).read_text(encoding="utf-8"))
        first = next((l.strip() for l in text.split("\n")
                      if l.strip() and not l.startswith("#")), "")
        rel = Path(r["file"]).relative_to("specs").as_posix()
        guides.append(f"- [`{Path(rel).name}`]({rel}) — {first[:160]}")
    readme = replace_block(readme, "guides",
                           "\n".join(guides) if guides else "*None.*")
    return readme


# --------------------------------------------------------------------------- checks
def check(rows: list[dict]) -> tuple[list[str], list[str]]:
    errors: list[str] = []
    warnings: list[str] = []
    ids: dict[str, str] = {}

    designs = {r["design"] for r in rows if r["kind"] == "design"}
    issue_ids = {r["id"] for r in rows if r["kind"] in ("issue", "feature")}

    for r in rows:
        f, where = r["_fm"], r["file"]
        if not f:
            errors.append(f"{where}: no front-matter")
            continue

        # CHECK 2 — ids unique, and matching the filename for issues
        if r["id"] in ids:
            errors.append(f"{where}: duplicate id {r['id']} (also {ids[r['id']]})")
        ids[r["id"]] = where
        if r["kind"] in ("issue", "feature") and r["id"] != r["_path"].stem:
            errors.append(f"{where}: id {r['id']} does not match filename")

        for a in _list(f, "area"):
            if a not in AREAS:
                errors.append(f"{where}: unknown area '{a}' (see guide §3)")

        if r["kind"] in ("issue", "feature"):
            # CHECK 5 — github ownership transfers the status field
            if f.get("github") and "status" in f and f.get("status"):
                errors.append(f"{where}: has github:{f['github']} and a status field (§4.3)")
            if not f.get("github"):
                if f.get("status") not in ISSUE_STATUS:
                    errors.append(f"{where}: status '{f.get('status')}' not in §4.3")
            if f.get("priority") not in PRIORITIES:
                errors.append(f"{where}: priority '{f.get('priority')}' not in §4.4")
            if f.get("complexity") not in COMPLEXITIES:
                errors.append(f"{where}: complexity '{f.get('complexity')}' not in §4.5")
            # CHECK 3 — L/XL implies a design that exists
            if f.get("complexity") in NEEDS_DESIGN:
                d = f.get("design")
                if not d:
                    # Work owed, not a malformed document: guide §4.8.3 explicitly tells a
                    # filer to record L/XL without a design rather than understate complexity
                    # to dodge the check. A hard error would make understating it the easy path.
                    warnings.append(f"{where}: complexity {f['complexity']} has no design (§4.5)")
                elif d not in designs:
                    errors.append(f"{where}: design '{d}' does not exist")
            elif f.get("design") and f["design"] not in designs:
                errors.append(f"{where}: design '{f['design']}' does not exist")
            dup = f.get("duplicate_of")
            if dup and dup not in issue_ids:
                errors.append(f"{where}: duplicate_of '{dup}' does not exist")

        elif r["kind"] == "design":
            gh_pr = _list(f, "gh_pr")
            status = f.get("status") or ""
            # §5.5: gh_pr transfers `in_implementation` and `implemented` to GitHub — nothing
            # else. The terminal statuses stay hand-written: `abandoned` and `superseded` are
            # conclusions a PR cannot reach, and `complete` turns on whether a phase is still
            # outstanding, which is a question about this folder rather than about GitHub.
            if gh_pr:
                if status not in TERMINAL_STATUS_WITH_PR:
                    errors.append(f"{where}: has gh_pr and status '{status}' — with a PR linked, "
                                  f"write nothing (derived) or a terminal status (§5.5)")
            elif status not in DESIGN_STATUS:
                errors.append(f"{where}: status '{status}' not in §5.1")
            # CHECK 6 — phase present exactly when the status requires it. Checked for every
            # hand-written status; a derived one is empty here and carries its phase untouched.
            if status:
                if status in DESIGN_STATUS_NEEDING_PHASE and not f.get("phase"):
                    errors.append(f"{where}: status '{status}' requires a phase (§5.1)")
                if status not in DESIGN_STATUS_NEEDING_PHASE and f.get("phase"):
                    errors.append(f"{where}: status '{status}' must not carry a phase (§5.1)")
            if f.get("phase") and f["phase"] not in PHASES and f["phase"] not in RETIRED_PHASES:
                errors.append(f"{where}: unknown phase '{f['phase']}' (see guide §5.2)")
            sb = f.get("superseded_by")
            if sb and sb not in designs:
                errors.append(f"{where}: superseded_by '{sb}' does not exist")

        else:  # reference | guide
            if f.get("audience") not in AUDIENCES:
                errors.append(f"{where}: audience '{f.get('audience')}' not in §9.6")
            # CHECK 10 — reviewed:, a History section, and a top row bearing that date
            if not f.get("reviewed"):
                errors.append(f"{where}: no reviewed: date (§9.2)")
            body = r["_path"].read_text(encoding="utf-8")
            if "## History" not in body:
                errors.append(f"{where}: no ## History section (§9.5)")
            elif f.get("reviewed") and str(f["reviewed"]) not in body.split("## History", 1)[1]:
                errors.append(f"{where}: History has no row dated {f['reviewed']} (§9.5)")
            # CHECK 13 — staleness is a warning, never an error
            if r["status"] == "overdue":
                warnings.append(f"{where}: last reviewed {f['reviewed']} (>{REVIEW_DAYS} days, §9.4)")

    # CHECK 7 — index.csv matches what regeneration would produce
    idx = SPECS / "index.csv"
    if not idx.exists():
        errors.append("specs/index.csv is missing — run scripts/docs_index.py")
    elif idx.read_text(encoding="utf-8") != render_csv(rows):
        errors.append("specs/index.csv is stale — run scripts/docs_index.py")

    # CHECK 8 — every path and issue id the capability map names must exist
    readme_path = SPECS / "README.md"
    if readme_path.exists():
        readme = readme_path.read_text(encoding="utf-8")
        for m in re.finditer(r"\]\((?!https?:)([A-Za-z0-9_./-]+)\)", readme):
            target = (SPECS / m.group(1)).resolve()
            if not target.exists():
                errors.append(f"specs/README.md: dead link {m.group(1)} (§8.4)")
        for m in re.finditer(r"`([A-Z][A-Z0-9-]{4,})`", readme):
            if m.group(1) not in ids and m.group(1) not in issue_ids:
                warnings.append(f"specs/README.md: names unknown id {m.group(1)}")
    return errors, warnings


# --------------------------------------------------------------------------- new
STOP = {"the", "a", "an", "is", "are", "to", "of", "in", "on", "for", "and", "or", "not",
        "does", "do", "with", "when", "that", "this", "it", "its", "can", "cannot", "but"}


def significant(text: str) -> set[str]:
    return {w for w in re.findall(r"[a-z0-9]+", text.lower()) if len(w) > 2 and w not in STOP}


def cmd_new(args) -> int:
    rows = collect()
    words = significant(args.title)

    # near-duplicate search across titles and areas — the step a grep does worst (§7.1)
    hits = []
    for r in rows:
        if r["kind"] not in ("issue", "feature"):
            continue
        overlap = words & significant(r["title"] + " " + r["area"])
        if overlap:
            hits.append((len(overlap), r, overlap))
    hits.sort(key=lambda h: -h[0])
    if hits:
        print("Possible duplicates — rule these out before filing:\n")
        for n, r, overlap in hits[:6]:
            print(f"  {r['id']}  [{r['status'] or '-'} {r['priority']}]  {r['title']}")
            print(f"      shared: {', '.join(sorted(overlap))}")
        print()
        if not args.force:
            print("Re-run with --force to file anyway.")
            return 1

    ident = args.id or re.sub(r"-+", "-", re.sub(r"[^A-Z0-9]+", "-", args.title.upper())).strip("-")[:60]
    path = SPECS / "issues" / f"{ident}.md"
    if path.exists():
        print(f"error: {path.relative_to(REPO)} already exists", file=sys.stderr)
        return 1
    for a in args.area:
        if a not in AREAS:
            print(f"error: unknown area '{a}' (see guide §3)", file=sys.stderr)
            return 2

    path.write_text(f"""---
id: {ident}
kind: {args.kind}
title: {args.title}
status: draft
priority: {args.priority}
complexity: {args.complexity}
area: [{', '.join(args.area)}]
design: {args.design}
created: {_dt.date.today().isoformat()}
github:
---
## Problem

What is wrong, with the code locations that show it.

## Impact

Who or what this affects, and how badly. If there is a workaround, say so — it is the
difference between P0 and P1.

## Expected behaviour

What should happen instead.

## Discovery

How this surfaced.
""", encoding="utf-8")
    print(f"created {path.relative_to(REPO)}")
    write_all(collect())
    print("regenerated specs/index.csv")
    print("\nNow fill in the body. Do not open a GitHub issue — that happens when work starts.")
    return 0


# --------------------------------------------------------------------------- main
def write_all(rows: list[dict]) -> None:
    (SPECS / "index.csv").write_text(render_csv(rows), encoding="utf-8")
    readme = SPECS / "README.md"
    if readme.exists():
        readme.write_text(render_readme_blocks(rows, readme.read_text(encoding="utf-8")),
                          encoding="utf-8")


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd")
    n = sub.add_parser("new", help="scaffold an issue file (§7.1)")
    n.add_argument("title")
    n.add_argument("--area", action="append", default=[], required=True)
    n.add_argument("--priority", default="P2", choices=sorted(PRIORITIES))
    n.add_argument("--complexity", default="M", choices=sorted(COMPLEXITIES))
    n.add_argument("--kind", default="issue", choices=["issue", "feature"])
    n.add_argument("--design", default="")
    n.add_argument("--id", default="")
    n.add_argument("--force", action="store_true", help="file despite near-duplicates")
    ap.add_argument("--check", action="store_true", help="validate only; never writes")
    ap.add_argument("--sync", action="store_true", help="refresh GitHub-owned columns")
    args = ap.parse_args()

    if args.cmd == "new":
        return cmd_new(args)

    if args.sync:
        print("--sync is not implemented yet: it needs the GitHub API and a token.\n"
              "Until it exists, gh_pr on a design is hand-written once (guide §5.5) and no\n"
              "issue carries a github: number.", file=sys.stderr)
        return 2

    rows = collect()
    if args.check:
        errors, warnings = check(rows)
        for w in warnings:
            print(f"warning: {w}")
        for e in errors:
            print(f"error: {e}", file=sys.stderr)
        print(f"\n{len(rows)} documents · {len(errors)} errors · {len(warnings)} warnings")
        return 1 if errors else 0

    write_all(rows)
    print(f"wrote specs/index.csv ({len(rows)} documents) and the specs/README.md blocks")
    return 0


if __name__ == "__main__":
    sys.exit(main())
