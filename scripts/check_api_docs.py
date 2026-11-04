#!/usr/bin/env python3
"""Check module documentation against specs/guides/API_DOCUMENTATION_GUIDE.md §3.

A public module that declares more than one public item carries a purpose and the headings
`# Main components`, `# Where it is used`, `# Related concepts` and `# Examples`, in that order,
with at most five links under *Where it is used*, one executed example of 3-10 visible lines and at
most five example links. Other modules need only the purpose.

Sources:
    Rust        every workspace crate, from src/lib.rs through its `mod` declarations; a module
                is public when every declaration on its path is `pub mod`; the inner `//!` lines
    Python      the module docstrings of liquers-py/python/liquers/*.py (`_x.py` is private)
    JavaScript  the `@packageDocumentation` block of liquers-web/src/typescript.rs

Usage:
    check_api_docs.py [--crate NAME]... [--strict] [--format text|json] [PATH]...

    PATH limits the report to files under it (a file, a crate directory, `liquers-web`).

Findings are warnings, except a `https://github.com/orest-d/liquers/blob/main/<path>` link whose
<path> does not exist in the repository, which is an error. Exit 0 when there is no error (warnings
allowed), 1 on an error or, under --strict, on a warning; argparse exits 2 on a usage error.
Text output is `path:line: warning|error: message` on stdout; `--format json` prints a list of
{"path", "line", "level", "message"}; the counts go to stderr.

No third-party dependencies: this runs in a sandbox with no network and no pip.
"""

from __future__ import annotations

import argparse
import ast
import json
import re
import sys
from dataclasses import dataclass
from pathlib import Path

HEADINGS = ["Main components", "Where it is used", "Related concepts", "Examples"]
GITHUB_MAIN = "https://github.com/orest-d/liquers/blob/main/"
MAX_LINKS = 5
EXAMPLE_LINES = (3, 10)

MOD_DECL = re.compile(r"^\s*(pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;")
PATH_ATTR = re.compile(r'^\s*#\[path\s*=\s*"([^"]+)"\]')
PUB_ITEM = re.compile(
    r"^pub\s+(?:(?:unsafe|async|const|extern\s+\"[^\"]*\")\s+)*"
    r"(?:struct|enum|trait|fn|type|const|static|use|mod|union)\b"
)
# A link: `[text](target)`, `[text][ref]` or a bare intra-doc `[`Item`]` / `[Item]`.
LINK = re.compile(r"\[[^\[\]\n]+\](?:\([^)\n]*\)|\[[^\]\n]*\])?")
HEADING_LINE = re.compile(r"^#\s+(.+?)\s*$")
FENCE = re.compile(r"^\s*(`{3,}|~{3,})(.*)$")


@dataclass
class Finding:
    path: str
    line: int
    level: str  # "warning" | "error"
    message: str

    def as_dict(self) -> dict:
        return {"path": self.path, "line": self.line, "level": self.level, "message": self.message}


@dataclass
class ModuleDoc:
    path: Path
    line: int  # 1-based line where the doc starts (or 1)
    text: str  # the doc as Markdown, without comment markers
    full: bool  # whether the five parts are required
    language: str  # "rust" | "python" | "javascript"


# --------------------------------------------------------------------------- Rust

def rust_crates(root: Path) -> list[Path]:
    """The workspace crates that have a library root, by directory name."""
    return sorted(
        d for d in root.glob("liquers-*") if (d / "Cargo.toml").is_file() and (d / "src" / "lib.rs").is_file()
    )


def module_file(declaring: Path, name: str, path_attr: str | None) -> Path | None:
    """The file of `mod name;` declared in `declaring`, or None when it is not where Rust looks."""
    if path_attr is not None:
        candidate = declaring.parent / path_attr
        return candidate if candidate.is_file() else None
    if declaring.name in ("lib.rs", "mod.rs", "main.rs"):
        base = declaring.parent
    else:
        base = declaring.parent / declaring.stem
    for candidate in (base / f"{name}.rs", base / name / "mod.rs"):
        if candidate.is_file():
            return candidate
    return None


def rust_modules(crate: Path) -> list[tuple[Path, bool]]:
    """Every module file of the crate with whether it is public, root first, depth-first."""
    result: list[tuple[Path, bool]] = []
    seen: set[Path] = set()

    def walk(file: Path, public: bool) -> None:
        if file in seen:
            return
        seen.add(file)
        result.append((file, public))
        path_attr = None
        for raw in file.read_text(encoding="utf-8", errors="replace").splitlines():
            attr = PATH_ATTR.match(raw)
            if attr:
                path_attr = attr.group(1)
                continue
            decl = MOD_DECL.match(raw)
            if not decl:
                if raw.strip() and not raw.lstrip().startswith("#["):
                    path_attr = None
                continue
            child = module_file(file, decl.group(2), path_attr)
            path_attr = None
            if child is None:
                continue
            child_public = public and decl.group(1) is not None and decl.group(1).strip() == "pub"
            walk(child, child_public)

    walk(crate / "src" / "lib.rs", True)
    return result


def rust_inner_doc(source: str) -> tuple[int, str]:
    """The leading `//!` block: (1-based line of its first line, text). Inner attributes, blank
    lines and plain comments before it are skipped; the block ends at the first item."""
    lines = source.splitlines()
    doc: list[str] = []
    start = 0
    for number, raw in enumerate(lines, start=1):
        stripped = raw.strip()
        if stripped.startswith("//!"):
            if not doc:
                start = number
            doc.append(stripped[3:][1:] if stripped[3:].startswith(" ") else stripped[3:])
        elif doc:
            if stripped == "" or stripped.startswith("#!["):
                continue
            break
        elif stripped == "" or stripped.startswith("#![") or stripped.startswith("//"):
            continue
        else:
            break
    return start or 1, "\n".join(doc)


def rust_public_items(source: str) -> int:
    return sum(1 for raw in source.splitlines() if PUB_ITEM.match(raw))


def rust_docs(crate: Path) -> list[ModuleDoc]:
    docs = []
    for file, public in rust_modules(crate):
        source = file.read_text(encoding="utf-8", errors="replace")
        line, text = rust_inner_doc(source)
        docs.append(ModuleDoc(file, line, text, public and rust_public_items(source) > 1, "rust"))
    return docs


# --------------------------------------------------------------------------- Python

def python_docs(package: Path) -> list[ModuleDoc]:
    docs = []
    for file in sorted(package.glob("*.py")):
        source = file.read_text(encoding="utf-8", errors="replace")
        try:
            tree = ast.parse(source)
        except SyntaxError:
            docs.append(ModuleDoc(file, 1, "", False, "python"))
            continue
        text = ast.get_docstring(tree, clean=True) or ""
        line = tree.body[0].lineno if text and tree.body else 1
        public = file.name == "__init__.py" or not file.name.startswith("_")
        docs.append(ModuleDoc(file, line, text, public, "python"))
    return docs


# --------------------------------------------------------------------------- JavaScript

JSDOC_BLOCK = re.compile(r"/\*\*(.*?)\*/", re.S)


def javascript_doc(typescript_rs: Path) -> ModuleDoc:
    """The `@packageDocumentation` JSDoc block of liquers-web/src/typescript.rs, as Markdown."""
    source = typescript_rs.read_text(encoding="utf-8", errors="replace")
    for match in JSDOC_BLOCK.finditer(source):
        body = match.group(1)
        if "@packageDocumentation" not in body:
            continue
        lines = []
        for raw in body.splitlines():
            stripped = raw.strip()
            if stripped.startswith("*"):
                stripped = stripped[1:]
                if stripped.startswith(" "):
                    stripped = stripped[1:]
            if stripped.strip() == "@packageDocumentation":
                continue
            lines.append(stripped)
        line = source.count("\n", 0, match.start()) + 1
        return ModuleDoc(typescript_rs, line, "\n".join(lines).strip("\n"), True, "javascript")
    return ModuleDoc(typescript_rs, 1, "", True, "javascript")


# --------------------------------------------------------------------------- the rules

def sections(text: str) -> tuple[str, list[tuple[str, list[str]]]]:
    """(purpose text, [(heading, body lines)]) of a Markdown module doc; fences are opaque."""
    purpose: list[str] = []
    parts: list[tuple[str, list[str]]] = []
    fence: tuple[str, int] | None = None
    for raw in text.splitlines():
        match = FENCE.match(raw)
        if fence is None and match:
            fence = (match.group(1)[0], len(match.group(1)))
        elif fence is not None and match and match.group(1)[0] == fence[0] and len(match.group(1)) >= fence[1] and not match.group(2).strip():
            fence = None
            (parts[-1][1] if parts else purpose).append(raw)
            continue
        heading = HEADING_LINE.match(raw) if fence is None else None
        if heading:
            parts.append((heading.group(1), []))
        else:
            (parts[-1][1] if parts else purpose).append(raw)
    return "\n".join(purpose).strip(), parts


def example_blocks(body: list[str], language: str) -> list[tuple[int, int]]:
    """(visible line count, index after the block) of each example in an Examples section."""
    blocks: list[tuple[int, int]] = []
    if language == "python":
        i = 0
        while i < len(body):
            if body[i].lstrip().startswith(">>>"):
                count = 0
                while i < len(body) and body[i].strip():
                    count += 1
                    i += 1
                blocks.append((count, i))
            else:
                i += 1
        return blocks
    if language == "javascript":
        # An `@example` holds either a fenced block (the example ends at its closing fence, so the
        # links and prose after it are not code) or bare lines up to the next blank line or tag.
        i = 0
        while i < len(body):
            if not body[i].strip().startswith("@example"):
                i += 1
                continue
            i += 1
            while i < len(body) and not body[i].strip():
                i += 1
            count = 0
            opened = FENCE.match(body[i]) if i < len(body) else None
            if opened:
                fence = (opened.group(1)[0], len(opened.group(1)))
                i += 1
                while i < len(body):
                    match = FENCE.match(body[i])
                    i += 1
                    if match and match.group(1)[0] == fence[0] and len(match.group(1)) >= fence[1] and not match.group(2).strip():
                        break
                    if body[i - 1].strip():
                        count += 1
            else:
                while i < len(body) and body[i].strip() and not body[i].strip().startswith("@"):
                    count += 1
                    i += 1
            blocks.append((count, i))
        return blocks
    fence: tuple[str, int] | None = None
    count = 0
    for i, raw in enumerate(body):
        match = FENCE.match(raw)
        if fence is None:
            if match:
                fence = (match.group(1)[0], len(match.group(1)))
                count = 0
        elif match and match.group(1)[0] == fence[0] and len(match.group(1)) >= fence[1] and not match.group(2).strip():
            blocks.append((count, i + 1))
            fence = None
        else:
            stripped = raw.strip()
            if stripped and not (stripped == "#" or stripped.startswith("# ")):
                count += 1
    return blocks


def check_doc(doc: ModuleDoc, root: Path) -> list[Finding]:
    rel = doc.path.relative_to(root).as_posix() if doc.path.is_relative_to(root) else str(doc.path)
    findings: list[Finding] = []

    def warn(message: str) -> Finding:
        return Finding(rel, doc.line, "warning", message)

    if not doc.text.strip():
        return [warn("no module doc" if doc.language != "javascript" else "no @packageDocumentation block")]
    purpose, parts = sections(doc.text)
    if not purpose:
        findings.append(warn("no purpose text before the first heading"))
    # GitHub links pinned to main must point at files that exist.
    for offset, raw in enumerate(doc.text.splitlines()):
        for match in re.finditer(re.escape(GITHUB_MAIN) + r"([^)\s#>\"']+)", raw):
            if not (root / match.group(1)).exists():
                findings.append(Finding(rel, doc.line + offset, "error", f"GitHub link to a path that does not exist: {match.group(1)}"))
    if not doc.full:
        return findings
    names = [heading for heading, _ in parts]
    present = [h for h in HEADINGS if h in names]
    for heading in HEADINGS:
        if heading not in names:
            findings.append(warn(f"missing heading `# {heading}`"))
    if [h for h in names if h in HEADINGS] != present:
        findings.append(warn("headings out of order (Main components, Where it is used, Related concepts, Examples)"))
    bodies = dict(parts)
    used = bodies.get("Where it is used", [])
    if len(LINK.findall("\n".join(used))) > MAX_LINKS:
        findings.append(warn(f"more than {MAX_LINKS} links under `# Where it is used`"))
    examples = bodies.get("Examples")
    if examples is not None:
        blocks = example_blocks(examples, doc.language)
        if not blocks:
            findings.append(warn("no example under `# Examples`"))
        else:
            count, end = blocks[0]
            low, high = EXAMPLE_LINES
            if not low <= count <= high:
                findings.append(warn(f"the example has {count} visible lines; {low}-{high} expected"))
            if len(LINK.findall("\n".join(examples[end:]))) > MAX_LINKS:
                findings.append(warn(f"more than {MAX_LINKS} example links under `# Examples`"))
    return findings


# --------------------------------------------------------------------------- driver

def find_root(start: Path) -> Path:
    for parent in [start, *start.parents]:
        if (parent / "Cargo.toml").is_file() and (parent / "specs").is_dir():
            return parent
    return start


def collect(root: Path, crates: list[str] | None) -> list[ModuleDoc]:
    docs: list[ModuleDoc] = []
    for crate in rust_crates(root):
        if crates and crate.name not in crates:
            continue
        docs.extend(rust_docs(crate))
    package = root / "liquers-py" / "python" / "liquers"
    if package.is_dir() and (not crates or "liquers-py" in crates):
        docs.extend(python_docs(package))
    typescript = root / "liquers-web" / "src" / "typescript.rs"
    if typescript.is_file() and (not crates or "liquers-web" in crates):
        docs.append(javascript_doc(typescript))
    return docs


def selected(doc: ModuleDoc, paths: list[Path]) -> bool:
    if not paths:
        return True
    resolved = doc.path.resolve()
    return any(resolved == p or p in resolved.parents for p in paths)


def run(argv: list[str] | None = None, root: Path | None = None, out=None, err=None) -> int:
    out = out or sys.stdout
    err = err or sys.stderr
    parser = argparse.ArgumentParser(description="Check module docs against API_DOCUMENTATION_GUIDE.md §3.")
    parser.add_argument("paths", nargs="*", help="limit the report to files under these paths")
    parser.add_argument("--crate", action="append", help="only this crate directory (repeatable)")
    parser.add_argument("--strict", action="store_true", help="a warning also fails (exit 1)")
    parser.add_argument("--format", choices=["text", "json"], default="text")
    args = parser.parse_args(argv)
    root = (root or find_root(Path.cwd())).resolve()
    paths = [Path(p).resolve() for p in args.paths]
    findings: list[Finding] = []
    docs = [d for d in collect(root, args.crate) if selected(d, paths)]
    for doc in docs:
        findings.extend(check_doc(doc, root))
    findings.sort(key=lambda f: (f.path, f.line, f.level))
    if args.format == "json":
        json.dump([f.as_dict() for f in findings], out, indent=2)
        out.write("\n")
    else:
        for finding in findings:
            out.write(f"{finding.path}:{finding.line}: {finding.level}: {finding.message}\n")
    errors = sum(1 for f in findings if f.level == "error")
    warnings = len(findings) - errors
    err.write(f"check_api_docs: {len(docs)} modules, {warnings} warnings, {errors} errors\n")
    return 1 if errors or (args.strict and warnings) else 0


if __name__ == "__main__":
    sys.exit(run())
