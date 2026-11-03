#!/usr/bin/env python3
"""Check that the workspace states one version and one license, and that a release tag matches.

Contract: specs/design/liquers-py-package/phase2-architecture.md (the `lock` job). Checks:

- `[workspace.package]` declares `version` and `license = "AGPL-3.0-or-later"`;
- every root workspace member inherits both (`version.workspace = true`,
  `license.workspace = true`);
- `liquers-py/LICENSE` is a byte-for-byte copy of the root `LICENSE` (PEP 639 paths cannot leave
  the Python project, so the wheel ships the copy);
- with `--tag liquers-py-vX`, that X is the workspace version.

Usage:
    check-workspace-metadata.py [--root DIR] [--tag TAG] [--print-version]

Exits 0 when everything holds, 1 with one line per finding on stderr otherwise. No third-party
dependencies (tomllib is in the standard library from Python 3.11).
"""

from __future__ import annotations

import argparse
import sys
import tomllib
from pathlib import Path

LICENSE = "AGPL-3.0-or-later"
TAG_PREFIX = "liquers-py-v"


def _inherits(package: dict, field: str) -> bool:
    value = package.get(field)
    return isinstance(value, dict) and value.get("workspace") is True


def check(root: Path, tag: str | None) -> tuple[list[str], str | None]:
    """Return the findings and the workspace version (None when it is missing)."""
    findings: list[str] = []
    manifest = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))
    workspace = manifest.get("workspace", {})
    package = workspace.get("package", {})
    version = package.get("version")
    if not isinstance(version, str):
        findings.append("Cargo.toml: [workspace.package] has no version")
        version = None
    if package.get("license") != LICENSE:
        findings.append(
            f"Cargo.toml: [workspace.package] license is {package.get('license')!r}, "
            f"expected {LICENSE!r}"
        )
    for member in workspace.get("members", []):
        path = root / member / "Cargo.toml"
        if not path.is_file():
            findings.append(f"{member}: no Cargo.toml")
            continue
        member_package = tomllib.loads(path.read_text(encoding="utf-8")).get("package", {})
        for field in ("version", "license"):
            if not _inherits(member_package, field):
                findings.append(f"{member}/Cargo.toml: {field} is not `{field}.workspace = true`")
    root_license = root / "LICENSE"
    copy = root / "liquers-py" / "LICENSE"
    if not copy.is_file():
        findings.append("liquers-py/LICENSE is missing (a copy of the root LICENSE)")
    elif not root_license.is_file() or root_license.read_bytes() != copy.read_bytes():
        findings.append("liquers-py/LICENSE differs from the root LICENSE")
    if tag is not None:
        if not tag.startswith(TAG_PREFIX):
            findings.append(f"tag {tag!r} does not start with {TAG_PREFIX!r}")
        elif version is not None and tag[len(TAG_PREFIX):] != version:
            findings.append(
                f"tag {tag!r} names version {tag[len(TAG_PREFIX):]}, "
                f"but the workspace version is {version}"
            )
    return findings, version


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    parser.add_argument("--tag", default=None, help="a liquers-py-vX tag; omitted on a manual run")
    parser.add_argument("--print-version", action="store_true",
                        help="print the workspace version on stdout when the checks pass")
    args = parser.parse_args(argv)
    findings, version = check(args.root, args.tag or None)
    for finding in findings:
        print(finding, file=sys.stderr)
    if findings:
        return 1
    if args.print_version:
        print(version)
    return 0


if __name__ == "__main__":
    sys.exit(main())
