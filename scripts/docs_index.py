#!/usr/bin/env python3
"""Index and validate the documents under specs/.

Contract: specs/DOCS_STRUCTURE_GUIDE.md. This script owns specs/index.csv, specs/index.md,
the untracked specs/index.html (§6), and generated blocks in specs/README.md (§8.2). It never
edits a document's prose.

Modes (§7):
    docs_index.py                       regenerate the indexes and README blocks
    docs_index.py --sort                temporarily queue-sort index.csv
    docs_index.py new "<title>" ...     scaffold an issue file (§7.1)
    docs_index.py --check               validate; non-zero exit on failure, never writes
    docs_index.py --sync                refresh GitHub metadata (not implemented yet)

No third-party dependencies: this has to run in a sandbox with no network and no pip.
"""

from __future__ import annotations

import argparse
import csv
import datetime as _dt
import html
import io
import re
import subprocess
import sys
from pathlib import Path

# --------------------------------------------------------------------------- vocabularies
# Mirrors DOCS_STRUCTURE_GUIDE.md §3, §4.3, §4.4, §4.5, §5.1, §5.1.1, §5.2. Kept in sync by CHECK 1,
# which fails when a document uses a value absent here.

AREAS = {
    "core/query",
    "core/plan",
    "core/commands",
    "core/assets",
    "core/store",
    "core/value",
    "core/context",
    "core/error",
    "core/validate",
    "macro",
    "store/backends",
    "store/config",
    "records",
    "lib/commands",
    "lib/value",
    "lib/polars",
    "lib/image",
    "lib/egui",
    "lib/ui",
    "axum",
    "web",
    "py",
    "docs",
    "build",
}
ISSUE_STATUS = {
    "draft",
    "accepted",
    "rejected",
    "duplicate",
    "in_progress",
    "closed",
    "closed_not_planned",
}
DESIGN_STATUS = {
    "draft",
    "in_review",
    "approved",
    "in_implementation",
    "implemented",
    "complete",
    "superseded",
    "abandoned",
}
DESIGN_STATUS_NEEDING_PHASE = {
    "draft",
    "in_review",
    "approved",
    "in_implementation",
    "implemented",
}
DESIGN_READINESS = {"ready", "needs-decision", "blocked", "phase2-blocked", "covered"}
# §5.1.1: whether an agent may fix the design without a human approving it first. Part of the
# readiness assessment, so it is only meaningful (and only allowed) beside a `readiness` value.
DESIGN_AUTOFIX = {"eligible", "not-eligible"}
# §5: a design's document form. `compact` keeps every phase as a `## Phase N` section of DESIGN.md
# (S/M work); the default full form has one file per phase.
DESIGN_FORMS = {"full", "compact"}
# §5.1.1 rule: an eligible design is fully designed and small.
AUTOFIX_READINESS = {"ready"}
AUTOFIX_COMPLEXITIES = {"S", "M"}
# §5.5: the statuses a human may write on a design that has a gh_pr. "" means "derived — ask
# GitHub"; the rest are terminal conclusions GitHub cannot draw.
TERMINAL_STATUS_WITH_PR = {"", "complete", "superseded", "abandoned"}
# §6: the statuses that mean "no longer work in front of anyone". They sink to the bottom of
# index.csv and drop out of the README's issues table. An empty status is never finished —
# for an issue or design owned by GitHub it means "not yet synced", not "done".
FINISHED_ISSUE_STATUS = {"closed", "closed_not_planned", "rejected", "duplicate"}
FINISHED_DESIGN_STATUS = {"complete", "superseded", "abandoned"}
PHASES = {
    "high-level": 1,
    "architecture": 2,
    "examples": 3,
    "implementation": 4,
    "documentation": 5,
}
RETIRED_PHASES: set[str] = set()  # §5.3 rule 2: never delete, only mark retired
# Ordered, because §6 sorts on them: most urgent and smallest first. The vocabularies §4.4 and
# §4.5 validate against are the keys, so a value can never be rankable but unlisted.
PRIORITY_ORDER = {"P0": 0, "P1": 1, "P2": 2, "P3": 3}
COMPLEXITY_ORDER = {"S": 0, "M": 1, "L": 2, "XL": 3}
# The active-work board puts work that is safe to implement before work that needs more
# design attention. An omitted readiness is last: it is an unknown, not a claim of readiness.
READINESS_ORDER = {
    "ready": 0,
    "needs-decision": 1,
    "blocked": 2,
    "phase2-blocked": 3,
    "covered": 4,
}
# Within one readiness, work an agent can pick up unattended comes first; unassessed is last.
AUTOFIX_ORDER = {"eligible": 0, "not-eligible": 1}
PRIORITIES = set(PRIORITY_ORDER)
COMPLEXITIES = set(COMPLEXITY_ORDER)
NEEDS_DESIGN = {"L", "XL"}
# §6: issues before designs before guides before reference documents — open questions above the
# answers. `feature` shares the issues block; it is an issue whose problem is an absence.
KIND_ORDER = {"issue": 0, "feature": 0, "design": 1, "guide": 2, "reference": 3}
AUDIENCES = {"internal", "user", "both"}
REVIEW_DAYS = 92  # §9.4

COLUMNS = [
    "id",
    "kind",
    "title",
    "status",
    "status_source",
    "phase",
    "readiness",
    "autofix",
    "priority",
    "complexity",
    "area",
    "gh_issue",
    "gh_pr",
    "branch",
    "design",
    "reviewed",
    "created",
    "file",
]

# No "# GENERATED" banner line: CSV has no comment syntax, so GitHub's renderer reads line 1
# as the header, finds one column, and refuses to render the table. The file is marked generated
# via .gitattributes (linguist-generated), and --check enforces that it matches regeneration —
# which is the protection that actually holds. A comment was only ever advisory.

ISO_DATE_RE = re.compile(r"\d{4}-\d{2}-\d{2}")
REPO = Path(__file__).resolve().parent.parent
SPECS = REPO / "specs"

RELATIVE_LINK_RE = re.compile(r"\]\((?!https?://)([^)]+)\)")
FENCE_RE = re.compile(r"^ {0,3}(`{3,}|~{3,})")
BACKTICK_RUN_RE = re.compile(r"`+")


def _blank(text: str) -> str:
    """Replace every character except newlines with a space."""
    return re.sub(r"[^\n]", " ", text)


def _escaped(text: str, index: int) -> bool:
    """True when the character at index is preceded by an odd number of backslashes."""
    count = 0
    while index - count - 1 >= 0 and text[index - count - 1] == "\\":
        count += 1
    return count % 2 == 1


def _blank_inline_code(block: str) -> str:
    """Blank inline code spans in one block (a paragraph, list item, heading or table row).

    A run of n backticks closes at the next run of exactly n, which may be on a later line of the
    same block (CommonMark §6.1). A backslash-escaped backtick cannot open a span, but a backslash
    inside a span is literal, so it does not stop a closer. An unclosed run is literal text, so an
    unbalanced backtick cannot swallow a real link.
    """
    out = []
    pos = 0
    runs = [(m.start(), m.end()) for m in BACKTICK_RUN_RE.finditer(block)]
    i = 0
    while i < len(runs):
        start, end = runs[i]
        if _escaped(block, start):
            start += 1
        if start == end:
            i += 1
            continue
        length = end - start
        closer_index = next(
            (j for j in range(i + 1, len(runs)) if runs[j][1] - runs[j][0] == length),
            None,
        )
        if closer_index is None:
            i += 1
            continue
        closer_end = runs[closer_index][1]
        out.append(block[pos:start])
        out.append(_blank(block[start:closer_end]))
        pos = closer_end
        i = closer_index + 1
    out.append(block[pos:])
    return "".join(out)


# A line that starts a new block, so a code span cannot continue into it from the line above.
BLOCK_START_RE = re.compile(r"^\s*(#{1,6}(\s|$)|\||>|[-*+]\s|\d{1,9}[.)]\s)")


def blank_code(text: str) -> str:
    """Return text with fenced blocks and inline code spans replaced by spaces (newlines kept),
    so link-shaped text inside code is not mistaken for a link. Offsets and line numbers are
    unchanged.

    Inline spans are paired within a block: consecutive lines up to a blank line, a fence, or a
    line that starts a heading, table row, block quote or list item. Indented code blocks are not
    recognized: without a full parser they are indistinguishable from list continuations.
    """
    lines = text.split("\n")
    out = []
    block: list[str] = []
    fence = None  # (character, length) of the open fence

    def flush():
        if block:
            out.extend(_blank_inline_code("\n".join(block)).split("\n"))
            block.clear()

    for line in lines:
        if fence is not None:
            match = FENCE_RE.match(line)
            if (
                match
                and match.group(1)[0] == fence[0]
                and len(match.group(1)) >= fence[1]
                and not line[match.end() :].strip()
            ):
                fence = None
            out.append(_blank(line))
            continue
        match = FENCE_RE.match(line)
        # A backtick fence's info string may not contain a backtick (CommonMark §4.5).
        if match and not (match.group(1)[0] == "`" and "`" in line[match.end() :]):
            flush()
            fence = (match.group(1)[0], len(match.group(1)))
            out.append(_blank(line))
            continue
        if not line.strip():
            flush()
            out.append(line)
            continue
        if BLOCK_START_RE.match(line):
            flush()
        block.append(line)
    flush()
    return "\n".join(out)


def stable_paths(paths) -> list[Path]:
    """Sort paths identically on case-sensitive and case-insensitive hosts."""
    return sorted(paths, key=lambda path: (path.as_posix().casefold(), path.as_posix()))


def tracked_markdown_paths(specs: Path) -> list[Path]:
    """Return current-state Markdown documents whose local links are validated."""
    paths = [specs / "README.md", specs / "index.md"]
    for directory in ("issues", "design", "reference", "guides"):
        paths.extend((specs / directory).rglob("*.md"))
    return stable_paths(path for path in paths if path.is_file())


def relative_link_errors(specs: Path) -> list[str]:
    """Report missing filesystem targets for the restricted Markdown link grammar."""
    errors = []
    for path in tracked_markdown_paths(specs):
        text = path.read_text(encoding="utf-8")
        for match in RELATIVE_LINK_RE.finditer(blank_code(text)):
            raw_target = match.group(1)
            target_text = raw_target.split("#", 1)[0]
            if not target_text or target_text.startswith("/"):
                continue
            if not (path.parent / target_text).resolve().exists():
                source = path.relative_to(specs.parent).as_posix()
                errors.append(f"{source}: dead link {raw_target} (§8.4)")
    return errors


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
            fields[key] = (
                [v.strip() for v in inner.split(",") if v.strip()] if inner else []
            )
        else:
            fields[key] = value
    return fields, text[end + 5 :]


def _list(fields: dict, key: str) -> list[str]:
    v = fields.get(key)
    if isinstance(v, list):
        return v
    return [v] if v else []


# --------------------------------------------------------------------------- collection
def finished(row: dict) -> bool:
    """Is this row's work over? Reference documents and guides never are (§6).

    A reference document has no lifecycle to finish: `current` and `overdue` describe how
    recently someone checked it, and the moment it stops being maintained it stops being true.
    """
    if row["kind"] in ("issue", "feature"):
        return row["status"] in FINISHED_ISSUE_STATUS
    if row["kind"] == "design":
        return row["status"] in FINISHED_DESIGN_STATUS
    return False


def sort_key(row: dict) -> tuple:
    """Optional `--sort` order for index.csv (§6): live work first, finished work last.

    Within each half, issues come before designs before guides before reference documents.
    Then the rank that the kind actually has:

    * issues — `priority` ascending, then `complexity` smallest first. The top of the file is
      what to pick up next: the most urgent thing that is also the least work.
    * designs — nothing to rank by; designs carry no priority or complexity (§5.1).
    * guides and reference — `overdue` above `current`, so a document owed a review is the one
      you see.

    `id` breaks every remaining tie, which is what keeps the file stable: two runs over the same
    tree produce the same bytes, and a row moves only when a field it sorts on changed.
    """
    kind = row["kind"]
    if kind in ("issue", "feature"):
        rank = (
            PRIORITY_ORDER.get(row["priority"], len(PRIORITY_ORDER)),
            COMPLEXITY_ORDER.get(row["complexity"], len(COMPLEXITY_ORDER)),
        )
    elif kind == "design":
        rank = (
            PRIORITY_ORDER.get(row["priority"], len(PRIORITY_ORDER)),
            COMPLEXITY_ORDER.get(row["complexity"], len(COMPLEXITY_ORDER)),
        )
    else:
        rank = (0 if row["status"] == "overdue" else 1, 0)
    return (finished(row), KIND_ORDER.get(kind, len(KIND_ORDER)), rank, row["id"])


def collect() -> list[dict]:
    """One row per tracked document in stable source-path grouping order.

    This deliberately is not a work-queue sort. Changing a priority, lifecycle state, or
    review date must not relocate a CSV row and make an unrelated merge harder. `--sort`
    requests the queue order at write time; the Markdown board has its own reader-oriented sort.
    """
    rows: list[dict] = []

    for path in stable_paths((SPECS / "issues").glob("*.md")):
        f, _ = parse_front_matter(path.read_text(encoding="utf-8"))
        rows.append(
            {
                "id": f.get("id", path.stem),
                "kind": f.get("kind", "issue"),
                "title": f.get("title", ""),
                "status": f.get("status", ""),
                # Issue and feature documents always own their status (§4.3). `github` is a link,
                # not a transfer of authority.
                "status_source": "local",
                "phase": "",
                "readiness": "",
                "autofix": "",
                "priority": f.get("priority", ""),
                "complexity": f.get("complexity", ""),
                "area": ";".join(_list(f, "area")),
                "gh_issue": f.get("github", ""),
                "gh_pr": ";".join(_list(f, "gh_pr")),
                "branch": "",
                "design": f.get("design", ""),
                "reviewed": "",
                "created": f.get("created", ""),
                "file": path.relative_to(REPO).as_posix(),
                "_fm": f,
                "_path": path,
            }
        )

    for path in stable_paths((SPECS / "design").glob("*/DESIGN.md")):
        f, _ = parse_front_matter(path.read_text(encoding="utf-8"))
        slug = path.parent.name
        gh_pr = _list(f, "gh_pr")
        rows.append(
            {
                "id": f.get("id", slug.upper()),
                "kind": "design",
                "title": f.get("title", ""),
                "status": f.get("status", ""),
                # Who owns the value in the status column, not whether a PR exists (§5.5). A design
                # that has reached a hand-written terminal status owns it locally, gh_pr or not.
                "status_source": "github" if gh_pr and not f.get("status") else "local",
                "phase": f.get("phase", ""),
                "readiness": "",
                "autofix": "",
                "priority": "",
                "complexity": "",
                "area": ";".join(_list(f, "area")),
                "gh_issue": "",
                "gh_pr": ";".join(gh_pr),
                "branch": "",
                "design": slug,
                "reviewed": "",
                "created": f.get("created", ""),
                "file": path.relative_to(REPO).as_posix(),
                "_fm": f,
                "_path": path,
            }
        )

    today = _dt.date.today()
    for sub, kind in (("reference", "reference"), ("guides", "guide")):
        for path in stable_paths((SPECS / sub).rglob("*.md")):
            f, _ = parse_front_matter(path.read_text(encoding="utf-8"))
            reviewed = f.get("reviewed", "")
            status = ""
            if reviewed:
                try:
                    age = (today - _dt.date.fromisoformat(str(reviewed))).days
                    status = "current" if age <= REVIEW_DAYS else "overdue"
                except ValueError:
                    status = ""
            rows.append(
                {
                    "id": path.stem,
                    "kind": f.get("kind", kind),
                    "title": f.get("title", ""),
                    "status": status,
                    "status_source": "local",
                    "phase": "",
                    "readiness": "",
                    "autofix": "",
                    "priority": "",
                    "complexity": "",
                    "area": ";".join(_list(f, "area")),
                    "gh_issue": "",
                    "gh_pr": "",
                    "branch": "",
                    "design": "",
                    "reviewed": str(reviewed),
                    "created": "",
                    "file": path.relative_to(REPO).as_posix(),
                    "_fm": f,
                    "_path": path,
                }
            )

    join_design_sources(rows)
    return rows


def join_design_sources(rows: list[dict]) -> None:
    """Project source-owned queue fields onto designs, and readiness and autofix back onto sources.

    A design with one known source inherits its priority, complexity and GitHub issue. A
    readiness-labeled design formed by merging designs (§5.1.1) lists several sources: the first
    is its **leading** source, whose queue fields the design row inherits, and its readiness is
    projected onto every source. Invalid or ambiguous links remain unjoined and are reported by
    check().
    """
    issue_rows = {row["id"]: row for row in rows if row["kind"] in ("issue", "feature")}
    for row in rows:
        if row["kind"] != "design":
            continue
        readiness = row["_fm"].get("readiness", "")
        autofix = row["_fm"].get("autofix", "") if readiness else ""
        sources = _list(row["_fm"], "issues")
        if not sources or any(source not in issue_rows for source in sources):
            continue
        if len(sources) > 1 and not readiness:
            continue
        leading = issue_rows[sources[0]]
        row["priority"] = leading["priority"]
        row["complexity"] = leading["complexity"]
        row["gh_issue"] = leading["gh_issue"]
        row["readiness"] = readiness
        row["autofix"] = autofix
        if readiness:
            for source in sources:
                issue_rows[source]["readiness"] = readiness
                issue_rows[source]["autofix"] = autofix


# --------------------------------------------------------------------------- writing
def render_csv(rows: list[dict]) -> str:
    out = io.StringIO()
    w = csv.DictWriter(out, fieldnames=COLUMNS, lineterminator="\n")
    w.writeheader()
    for r in rows:
        w.writerow({c: r.get(c, "") for c in COLUMNS})
    return out.getvalue()


def active_work_rows(rows: list[dict]) -> list[dict]:
    """Return active issue/feature rows in the order used by the Markdown work board."""
    active = [r for r in rows if r["kind"] in ("issue", "feature") and not finished(r)]
    return sorted(
        active,
        key=lambda r: (
            PRIORITY_ORDER.get(r["priority"], len(PRIORITY_ORDER)),
            COMPLEXITY_ORDER.get(r["complexity"], len(COMPLEXITY_ORDER)),
            READINESS_ORDER.get(r["readiness"], len(READINESS_ORDER)),
            r["readiness"],
            AUTOFIX_ORDER.get(r["autofix"], len(AUTOFIX_ORDER)),
            r["id"],
        ),
    )


def relative_specs_path(row: dict) -> str:
    return Path(row["file"]).relative_to("specs").as_posix()


def markdown_cell(value: object) -> str:
    """Make one scalar safe inside the deliberately simple generated Markdown table."""
    return str(value).replace("|", r"\|").replace("\n", " ")


def render_index_markdown(rows: list[dict]) -> str:
    """Render the committed, active-work view of issues and features."""
    designs = {r["design"]: r for r in rows if r["kind"] == "design"}

    total_rows = len(active_work_rows(rows))
    statistics = f"Total rows: {total_rows}"
    for priority in sorted(set(r["priority"] for r in active_work_rows(rows))):
        statistics += f"\n- {priority}: {sum(1 for r in active_work_rows(rows) if r['priority'] == priority)}"
    lines = [
        "# Active issues and features",
        "",
        "<!-- GENERATED by scripts/docs_index.py; do not edit by hand. -->",
        "",
        "Only active issues and features appear here. Rows are ordered by priority, complexity,",
        "then implementation readiness and automatic-fix eligibility. `--check` verifies this file.",
        "",
        "> Merge-conflict recovery: keep either generated version while resolving the merge, then",
        "> run `python scripts/docs_index.py` after the merge and commit the regenerated `index.md`.",
        "",
        statistics,
        "",
        "| Issue | Kind | Title | Status | Readiness | Auto-fix | Priority | Complexity | Area | Design | Created |",
        "|---|---|---|---|---|---|---|---|---|---|---|",
    ]
    for r in active_work_rows(rows):
        issue = f"[`{markdown_cell(r['id'])}`]({relative_specs_path(r)})"
        design = r["design"]
        if design and design in designs:
            design_phases = ' '.join(
                f"[{x.name.split('-')[0]}]({x.relative_to(SPECS).as_posix()}) "
                for x in stable_paths(Path(designs[design]["_path"]).parent.iterdir())
                if x.name != "DESIGN.md" and x.name.endswith(".md")
            )
            # A compact design (§5) keeps its phases inside DESIGN.md, so link that file.
            design = design_phases or (
                f"[design]({Path(designs[design]['_path']).relative_to(SPECS).as_posix()})"
            )
        elif design:
            design = f"`{markdown_cell(design)}`"
        lines.append(
            "| "
            + " | ".join(
                (
                    issue,
                    markdown_cell(r["kind"]),
                    markdown_cell(r["title"]),
                    markdown_cell(r["status"]),
                    markdown_cell(r["readiness"]),
                    markdown_cell(r["autofix"]),
                    markdown_cell(r["priority"]),
                    markdown_cell(r["complexity"]),
                    markdown_cell(r["area"]),
                    design,
                    markdown_cell(r["created"]),
                )
            )
            + " |"
        )

    return "\n".join(lines) + "\n"


def html_link(label: str, target: str) -> str:
    return f'<a href="{html.escape(target, quote=True)}">{html.escape(label)}</a>'


def render_index_html(rows: list[dict]) -> str:
    """Render an untracked HTML counterpart of the generated Markdown board."""
    designs = {r["design"]: r for r in rows if r["kind"] == "design"}
    headings = (
        "Issue",
        "Kind",
        "Title",
        "Status",
        "Readiness",
        "Auto-fix",
        "Priority",
        "Complexity",
        "Area",
        "Design",
        "Created",
    )
    body = []
    for r in active_work_rows(rows):
        design = r["design"]
        design_cell = (
            html_link(design, relative_specs_path(designs[design]))
            if design and design in designs
            else html.escape(design)
        )
        cells = (
            html_link(r["id"], relative_specs_path(r)),
            r["kind"],
            r["title"],
            r["status"],
            r["readiness"],
            r["autofix"],
            r["priority"],
            r["complexity"],
            r["area"],
            design_cell,
            r["created"],
        )
        rendered = [
            cell if i in (0, 9) else html.escape(str(cell))
            for i, cell in enumerate(cells)
        ]
        body.append("<tr>" + "".join(f"<td>{cell}</td>" for cell in rendered) + "</tr>")
    header = "".join(f"<th>{html.escape(h)}</th>" for h in headings)
    return (
        """<!doctype html>
<html lang="en">
<meta charset="utf-8">
<title>Active issues and features</title>
<style>body { font-family: system-ui, sans-serif; margin: 2rem; } table { border-collapse: collapse; } th, td { border: 1px solid #bbb; padding: .4rem; text-align: left; vertical-align: top; } th { background: #eee; }</style>
<h1>Active issues and features</h1>
<p>Generated by <code>scripts/docs_index.py</code>. The committed source view is <a href="index.md">index.md</a>.</p>
<table><thead><tr>"""
        + header
        + "</tr></thead><tbody>\n"
        + "\n".join(body)
        + "\n</tbody></table>\n</html>\n"
    )


def replace_block(text: str, name: str, body: str) -> str:
    """Rewrite only what lies between the markers; never touch a line outside them (§8.2)."""
    begin, end = f"<!-- BEGIN generated: {name} -->", f"<!-- END generated: {name} -->"
    i, j = text.find(begin), text.find(end)
    if i == -1 or j == -1 or j < i:
        return text
    return text[: i + len(begin)] + "\n" + body.rstrip("\n") + "\n" + text[j:]


def render_readme_blocks(rows: list[dict], readme: str) -> str:
    # This README table is a queue, independent from the merge-friendly CSV source order.
    issues = [
        r
        for r in active_work_rows(rows)
        if r["design"] and r["priority"] in ("P0", "P1")
    ]
    if issues:
        body = ["| Issue | Pri | Cx | Design |", "|---|---|---|---|"] + [
            f"| [`{r['id']}`]({Path(r['file']).relative_to('specs').as_posix()}) "
            f"| {r['priority']} | {r['complexity']} | `{r['design']}` |"
            for r in issues
        ]
    else:
        body = ["*None.*"]
    readme = replace_block(readme, "issues", "\n".join(body))

    # Only the hand-written prose counts as "placed". Generated blocks must be stripped first:
    # the issues table names every design slug, so leaving them in makes every design look
    # referenced — and on a second run the unplaced list would mark itself placed.
    referenced = re.sub(
        r"<!-- BEGIN generated: .*?<!-- END generated: [a-z]+ -->",
        "",
        readme,
        flags=re.S,
    )
    unplaced = []
    for r in rows:
        if r["kind"] == "design" and r["status"] != "superseded":
            if r["design"] not in referenced:
                unplaced.append(f"- design `{r['design']}`")
        elif r["kind"] in ("reference", "guide"):
            rel = Path(r["file"]).relative_to("specs").as_posix()
            if rel not in referenced:
                unplaced.append(f"- `{rel}`")
        elif r["kind"] == "feature" and not finished(r):
            if r["id"] not in referenced:
                unplaced.append(f"- feature `{r['id']}`")
    body = (
        "\n".join(sorted(unplaced))
        if unplaced
        else "*Everything is placed in the capability map.*"
    )
    readme = replace_block(readme, "unplaced", body)

    # By name, not in §6 order: this block is how a reader finds a guide, and someone looking
    # for the testing guide scans for its name rather than for how overdue it is (§8.2).
    guides = []
    for r in sorted(rows, key=lambda r: r["id"]):
        if r["kind"] != "guide":
            continue
        _, text = parse_front_matter((REPO / r["file"]).read_text(encoding="utf-8"))
        first = next(
            (
                l.strip()
                for l in text.split("\n")
                if l.strip() and not l.startswith("#")
            ),
            "",
        )
        rel = Path(r["file"]).relative_to("specs").as_posix()
        guides.append(f"- [`{Path(rel).name}`]({rel}) — {first[:160]}")
    readme = replace_block(readme, "guides", "\n".join(guides) if guides else "*None.*")
    return readme


# --------------------------------------------------------------------------- checks
def readiness_errors(rows: list[dict]) -> list[str]:
    """§5.1.1: a readiness-labeled design owns one or more sources, each linking back to it, and
    no source is owned by two readiness-labeled designs. Several sources arise from a maintainer
    merge of designs or from triage of strongly overlapping issues; the first listed leads."""
    errors: list[str] = []
    issue_rows = {r["id"]: r for r in rows if r["kind"] in ("issue", "feature")}
    readiness_design_by_issue: dict[str, str] = {}
    for r in rows:
        if r["kind"] != "design" or not r["_fm"]:
            continue
        f, where = r["_fm"], r["file"]
        if f.get("readiness"):
            sources = _list(f, "issues")
            if not sources:
                errors.append(
                    f"{where}: readiness-labeled design must name at least one "
                    f"source issue or feature (§5.1.1)"
                )
            elif len(set(sources)) != len(sources):
                errors.append(f"{where}: a source is listed twice in issues (§5.1.1)")
            elif len(sources) > 1 and not ISO_DATE_RE.fullmatch(str(f.get("merged", ""))):
                errors.append(
                    f"{where}: a readiness-labeled design with several sources must record "
                    f"the merge as `merged: YYYY-MM-DD` (§5.1.1)"
                )
            for source in dict.fromkeys(sources):
                if source not in issue_rows:
                    errors.append(
                        f"{where}: source issue or feature '{source}' does not exist"
                    )
                else:
                    linked_design = issue_rows[source]["_fm"].get("design")
                    if linked_design != r["design"]:
                        errors.append(
                            f"{where}: source '{source}' links to design "
                            f"'{linked_design}', expected '{r['design']}' (§5.1.1)"
                        )
                    owner = readiness_design_by_issue.get(source)
                    if owner:
                        errors.append(
                            f"{where}: source '{source}' is already owned by "
                            f"readiness-labeled design '{owner}' (§5.1.1)"
                        )
                    else:
                        readiness_design_by_issue[source] = r["design"]
    # A design merged into another is `superseded`, names it, and no longer claims readiness.
    by_slug = {r["design"]: r for r in rows if r["kind"] == "design" and r["_fm"]}
    for r in rows:
        if r["kind"] != "design" or not r["_fm"]:
            continue
        f, where = r["_fm"], r["file"]
        successor = f.get("superseded_by")
        if not successor or successor not in by_slug:
            continue
        if f.get("status") != "superseded":
            errors.append(
                f"{where}: superseded_by '{successor}' requires status 'superseded' (§5.1.1)"
            )
        if f.get("readiness") and by_slug[successor]["_fm"].get("merged"):
            errors.append(
                f"{where}: merged into '{successor}', so it must drop readiness (§5.1.1)"
            )
    return errors


def autofix_errors(rows: list[dict]) -> list[str]:
    """§5.1.1: `autofix` is part of the readiness assessment. It needs a readiness value, and
    `eligible` claims a fully designed (`ready`) design whose leading source is `S` or `M`."""
    errors: list[str] = []
    issue_rows = {r["id"]: r for r in rows if r["kind"] in ("issue", "feature")}
    for r in rows:
        if r["kind"] != "design" or not r["_fm"]:
            continue
        f, where = r["_fm"], r["file"]
        autofix = f.get("autofix")
        if not autofix:
            continue
        if autofix not in DESIGN_AUTOFIX:
            errors.append(f"{where}: autofix '{autofix}' not in §5.1.1")
            continue
        if not f.get("readiness"):
            errors.append(f"{where}: autofix requires a readiness value (§5.1.1)")
            continue
        if autofix != "eligible":
            continue
        if f["readiness"] not in AUTOFIX_READINESS:
            errors.append(
                f"{where}: autofix 'eligible' requires readiness 'ready', "
                f"not '{f['readiness']}' (§5.1.1)"
            )
        sources = _list(f, "issues")
        leading = issue_rows.get(sources[0]) if sources else None
        if leading and leading["complexity"] not in AUTOFIX_COMPLEXITIES:
            errors.append(
                f"{where}: autofix 'eligible' requires complexity S or M, but leading "
                f"source '{leading['id']}' is '{leading['complexity']}' (§5.1.1)"
            )
    return errors


def form_errors(rows: list[dict]) -> list[str]:
    """§5: `form` is `full` (the default) or `compact`; a compact design carries its phases as
    `## Phase N` sections of DESIGN.md, starting with Phase 1."""
    errors: list[str] = []
    for r in rows:
        if r["kind"] != "design" or not r["_fm"]:
            continue
        f, where = r["_fm"], r["file"]
        form = f.get("form")
        if form and form not in DESIGN_FORMS:
            errors.append(f"{where}: form '{form}' not in §5")
        elif form == "compact" and not re.search(
            r"(?m)^## Phase 1\b", Path(r["_path"]).read_text(encoding="utf-8")
        ):
            errors.append(f"{where}: a compact design keeps its phases as `## Phase N` (§5)")
    return errors


# §5.2.1: acceptance scenarios and implementation progress. A scenario is a Phase 1 list item
# that starts with a bold `AC-<n>` id and carries a WHEN line and a THEN line; Phase 3 cites the id
# for each test that proves it. Phase 4 tracks progress as a checklist, one item per step.
SCENARIO_DEF_RE = re.compile(r"(?m)^[ \t]*[-*][ \t]+\*\*(AC-\d+)\*\*")
SCENARIO_REF_RE = re.compile(r"\bAC-\d+\b")
PROGRESS_ITEM_RE = re.compile(r"(?mi)^[ \t]*[-*][ \t]+\[([ x])\][ \t]+(?:Step[ \t]+)?(\d+)\b")
# The checklist lives in `## Progress` (full form) or `### Steps` (compact form); checkboxes
# elsewhere in Phase 4, such as entry criteria, are not step progress.
PROGRESS_SECTION_RE = re.compile(r"(?ms)^(##|###) (?:Progress|Steps)\b[^\n]*\n(.*?)(?=^#{2,3} |\Z)")
NO_SCENARIOS_NOTE = "**Acceptance scenarios not defined.**"


def design_phase_text(row: dict, phase: int) -> str | None:
    """The text of one phase of a design: its `## Phase N` section in a compact design (§5), or
    every `phase<N>-*.md` file of a full one (some designs split a phase over two files)."""
    path = Path(row["_path"])
    if row["_fm"].get("form") == "compact":
        text = path.read_text(encoding="utf-8")
        match = re.search(rf"(?ms)^## Phase {phase}\b.*?(?=^## Phase \d|\Z)", text)
        return match.group(0) if match else None
    files = stable_paths(path.parent.glob(f"phase{phase}-*.md"))
    if not files:
        return None
    return "\n".join(f.read_text(encoding="utf-8") for f in files)


def scenario_definitions(phase1: str) -> dict[str, str]:
    """`AC-<n>` id → the text of its list item, up to the next list item or heading."""
    found: dict[str, str] = {}
    matches = list(SCENARIO_DEF_RE.finditer(phase1))
    for i, m in enumerate(matches):
        end = matches[i + 1].start() if i + 1 < len(matches) else len(phase1)
        block = phase1[m.start() : end]
        heading = re.search(r"(?m)^#", block[1:])
        found[m.group(1)] = block[: heading.start() + 1] if heading else block
    return found


def duplicate_scenario_ids(phase1: str) -> list[str]:
    """Scenario ids defined more than once; ids are unique and never reused (§5.2.1)."""
    seen: set[str] = set()
    duplicates: list[str] = []
    for m in SCENARIO_DEF_RE.finditer(phase1):
        sid = m.group(1)
        if sid in seen and sid not in duplicates:
            duplicates.append(sid)
        seen.add(sid)
    return duplicates


def phase3_is_written(row: dict) -> bool:
    """Phase 3 counts as written once it is at review, or the design has moved past it."""
    phase = row["_fm"].get("phase", "")
    if PHASES.get(phase, 0) > PHASES["examples"]:
        return True
    if phase == "examples":
        return row["_fm"].get("status") in ("in_review", "approved")
    return row["status"] == "complete"


def scenario_errors(rows: list[dict]) -> list[str]:
    """§5.2.1: when Phase 1 defines scenarios, each has WHEN and THEN, and a written Phase 3 cites
    every one and no other. A design without scenarios is not checked."""
    errors: list[str] = []
    for r in rows:
        if r["kind"] != "design" or not r["_fm"]:
            continue
        phase1 = design_phase_text(r, 1)
        if not phase1:
            continue
        defined = scenario_definitions(phase1)
        if not defined:
            continue
        where = r["file"]
        for sid in duplicate_scenario_ids(phase1):
            errors.append(f"{where}: scenario {sid} is defined more than once (§5.2.1)")
        for sid, block in defined.items():
            missing = [k for k in ("WHEN", "THEN") if not re.search(rf"\b{k}\b", block)]
            if missing:
                errors.append(f"{where}: scenario {sid} has no {' or '.join(missing)} (§5.2.1)")
        phase3 = design_phase_text(r, 3)
        if phase3 is None or not phase3_is_written(r):
            continue
        cited = set(SCENARIO_REF_RE.findall(phase3))
        for sid in sorted(set(defined) - cited, key=lambda x: int(x[3:])):
            errors.append(f"{where}: scenario {sid} has no Phase 3 test citing it (§5.2.1)")
        for sid in sorted(cited - set(defined), key=lambda x: int(x[3:])):
            errors.append(f"{where}: Phase 3 cites {sid}, which Phase 1 does not define (§5.2.1)")
    return errors


def progress_items(phase4: str) -> list[tuple[bool, int]]:
    """Phase 4 progress checklist items as (ticked, step number)."""
    items: list[tuple[bool, int]] = []
    for section in PROGRESS_SECTION_RE.finditer(phase4):
        items += [
            (m.group(1).lower() == "x", int(m.group(2)))
            for m in PROGRESS_ITEM_RE.finditer(section.group(2))
        ]
    return items


def scenario_and_progress_warnings(rows: list[dict]) -> list[str]:
    """§5.2.1 warnings: open designs that define no scenarios (one summary line), a "not
    defined" note left behind after scenarios were added, and a design past implementation
    whose progress checklist still has unticked steps."""
    warnings: list[str] = []
    without: list[str] = []
    for r in rows:
        if r["kind"] != "design" or not r["_fm"]:
            continue
        where = r["file"]
        phase1 = design_phase_text(r, 1)
        has_scenarios = bool(phase1 and scenario_definitions(phase1))
        if not finished(r) and not has_scenarios:
            without.append(r["design"])
        if has_scenarios and NO_SCENARIOS_NOTE in Path(r["_path"]).read_text(encoding="utf-8"):
            warnings.append(f"{where}: scenarios are defined; remove the 'not defined' note (§5.2.1)")
        if r["_fm"].get("phase") == "documentation" or r["status"] == "complete":
            phase4 = design_phase_text(r, 4) or ""
            open_steps = [n for ticked, n in progress_items(phase4) if not ticked]
            if open_steps:
                steps = ", ".join(str(n) for n in open_steps)
                warnings.append(f"{where}: Phase 4 progress has unticked steps {steps} (§5.2.1)")
    if without:
        warnings.append(
            f"{len(without)} open designs define no acceptance scenarios (§5.2.1); "
            f"consider adding them, e.g. {', '.join(sorted(without)[:3])}"
        )
    return warnings


def check(rows: list[dict]) -> tuple[list[str], list[str]]:
    errors: list[str] = []
    warnings: list[str] = []
    ids: dict[str, str] = {}

    designs = {r["design"] for r in rows if r["kind"] == "design"}
    issue_ids = {r["id"] for r in rows if r["kind"] in ("issue", "feature")}
    issue_rows = {r["id"]: r for r in rows if r["kind"] in ("issue", "feature")}
    errors.extend(readiness_errors(rows))
    errors.extend(autofix_errors(rows))
    errors.extend(form_errors(rows))
    errors.extend(scenario_errors(rows))
    warnings.extend(scenario_and_progress_warnings(rows))

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
            # CHECK 5 — issue and feature status is always local, including when github: is set.
            if f.get("status") not in ISSUE_STATUS:
                errors.append(f"{where}: status '{f.get('status')}' not in §4.3")
            if f.get("priority") not in PRIORITIES:
                errors.append(f"{where}: priority '{f.get('priority')}' not in §4.4")
            if f.get("complexity") not in COMPLEXITIES:
                errors.append(
                    f"{where}: complexity '{f.get('complexity')}' not in §4.5"
                )
            # CHECK 3 — L/XL implies a design that exists
            if f.get("complexity") in NEEDS_DESIGN:
                d = f.get("design")
                if not d:
                    # Work owed, not a malformed document: guide §4.8.3 explicitly tells a
                    # filer to record L/XL without a design rather than understate complexity
                    # to dodge the check. A hard error would make understating it the easy path.
                    warnings.append(
                        f"{where}: complexity {f['complexity']} has no design (§4.5)"
                    )
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
                    errors.append(
                        f"{where}: has gh_pr and status '{status}' — with a PR linked, "
                        f"write nothing (derived) or a terminal status (§5.5)"
                    )
            elif status not in DESIGN_STATUS:
                errors.append(f"{where}: status '{status}' not in §5.1")
            # CHECK 6 — phase present exactly when the status requires it. Checked for every
            # hand-written status; a derived one is empty here and carries its phase untouched.
            if status:
                if status in DESIGN_STATUS_NEEDING_PHASE and not f.get("phase"):
                    errors.append(f"{where}: status '{status}' requires a phase (§5.1)")
                if status not in DESIGN_STATUS_NEEDING_PHASE and f.get("phase"):
                    errors.append(
                        f"{where}: status '{status}' must not carry a phase (§5.1)"
                    )
            if (
                f.get("phase")
                and f["phase"] not in PHASES
                and f["phase"] not in RETIRED_PHASES
            ):
                errors.append(f"{where}: unknown phase '{f['phase']}' (see guide §5.2)")
            if f.get("readiness") and f["readiness"] not in DESIGN_READINESS:
                errors.append(f"{where}: readiness '{f['readiness']}' not in §5.1.1")
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
            elif (
                f.get("reviewed")
                and str(f["reviewed"]) not in body.split("## History", 1)[1]
            ):
                errors.append(
                    f"{where}: History has no row dated {f['reviewed']} (§9.5)"
                )
            # CHECK 13 — staleness is a warning, never an error
            if r["status"] == "overdue":
                warnings.append(
                    f"{where}: last reviewed {f['reviewed']} (>{REVIEW_DAYS} days, §9.4)"
                )

    # CHECK 7 — committed generated indexes match what regeneration would produce.
    idx = SPECS / "index.csv"
    if not idx.exists():
        errors.append("specs/index.csv is missing — run scripts/docs_index.py")
    elif idx.read_text(encoding="utf-8") != render_csv(rows):
        errors.append("specs/index.csv is stale — run scripts/docs_index.py")
    markdown_index = SPECS / "index.md"
    if not markdown_index.exists():
        errors.append("specs/index.md is missing — run scripts/docs_index.py")
    elif markdown_index.read_text(encoding="utf-8") != render_index_markdown(rows):
        errors.append("specs/index.md is stale — run scripts/docs_index.py")

    # CHECK 8 — relative links in every tracked current-state document must exist
    errors.extend(relative_link_errors(SPECS))

    # The capability map also names issue IDs that must exist.
    readme_path = SPECS / "README.md"
    if readme_path.exists():
        readme = readme_path.read_text(encoding="utf-8")
        for m in re.finditer(r"`([A-Z][A-Z0-9-]{4,})`", readme):
            if m.group(1) not in ids and m.group(1) not in issue_ids:
                warnings.append(f"specs/README.md: names unknown id {m.group(1)}")
    return errors, warnings


# --------------------------------------------------------------------------- new
STOP = {
    "the",
    "a",
    "an",
    "is",
    "are",
    "to",
    "of",
    "in",
    "on",
    "for",
    "and",
    "or",
    "not",
    "does",
    "do",
    "with",
    "when",
    "that",
    "this",
    "it",
    "its",
    "can",
    "cannot",
    "but",
}


def significant(text: str) -> set[str]:
    return {
        w
        for w in re.findall(r"[a-z0-9]+", text.lower())
        if len(w) > 2 and w not in STOP
    }


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

    ident = (
        args.id
        or re.sub(r"-+", "-", re.sub(r"[^A-Z0-9]+", "-", args.title.upper())).strip(
            "-"
        )[:60]
    )
    path = SPECS / "issues" / f"{ident}.md"
    if path.exists():
        print(f"error: {path.relative_to(REPO)} already exists", file=sys.stderr)
        return 1
    for a in args.area:
        if a not in AREAS:
            print(f"error: unknown area '{a}' (see guide §3)", file=sys.stderr)
            return 2

    path.write_text(
        f"""---
id: {ident}
kind: {args.kind}
title: {args.title}
status: draft
priority: {args.priority}
complexity: {args.complexity}
area: [{", ".join(args.area)}]
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
""",
        encoding="utf-8",
    )
    print(f"created {path.relative_to(REPO)}")
    write_all(collect())
    print("regenerated specs/index.csv")
    print(
        "\nNow fill in the body. Do not open a GitHub issue — that happens when work starts."
    )
    return 0


# --------------------------------------------------------------------------- main
def write_all(rows: list[dict], *, sort_csv: bool = False) -> None:
    csv_rows = sorted(rows, key=sort_key) if sort_csv else rows
    (SPECS / "index.csv").write_text(render_csv(csv_rows), encoding="utf-8")
    (SPECS / "index.md").write_text(render_index_markdown(rows), encoding="utf-8")
    (SPECS / "index.html").write_text(render_index_html(rows), encoding="utf-8")
    readme = SPECS / "README.md"
    if readme.exists():
        readme.write_text(
            render_readme_blocks(rows, readme.read_text(encoding="utf-8")),
            encoding="utf-8",
        )


def main() -> int:
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
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
    ap.add_argument(
        "--sort",
        action="store_true",
        help="write index.csv in work-queue order (priority, complexity, and status)",
    )
    ap.add_argument(
        "--sync",
        action="store_true",
        help="refresh GitHub metadata; never issue status",
    )
    args = ap.parse_args()

    if args.cmd == "new":
        return cmd_new(args)

    if args.sync:
        print(
            "--sync is not implemented yet: it needs the GitHub API and a token.\n"
            "When implemented, it may refresh GitHub metadata but never overwrites the\n"
            "authoritative local status of an issue or feature (guide §4.3).",
            file=sys.stderr,
        )
        return 2

    rows = collect()
    if args.check:
        errors, warnings = check(rows)
        for w in warnings:
            print(f"warning: {w}")
        for e in errors:
            print(f"error: {e}", file=sys.stderr)
        print(
            f"\n{len(rows)} documents · {len(errors)} errors · {len(warnings)} warnings"
        )
        return 1 if errors else 0

    write_all(rows, sort_csv=args.sort)
    print(
        f"wrote specs/index.csv, specs/index.md, and untracked specs/index.html "
        f"({len(rows)} documents), plus the specs/README.md blocks"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
