#!/usr/bin/env python3
"""Check a liquers-py wheel, or a whole release `dist/` directory, before anything is published.

Contract: specs/design/liquers-py-package/phase2-architecture.md (the `wheels` and `dist` jobs).

    check-wheel.py WHEEL [--deps FILE] [--manylinux 2_28] [--max-mb 90]
        Fails when the wheel holds a grafted-library directory (`*.libs/`, `.dylibs/`) or any
        `libssl*` / `libcrypto*` file, when it is larger than --max-mb, or when the dependency
        listing in FILE (or on stdin with `--deps -`) names a library outside the platform's
        allow-list in liquers-py/packaging/allowed-libraries.toml. The listing is the output of
        `auditwheel show` (Linux), `delocate-listdeps --all` (macOS) or `dumpbin /dependents`
        (Windows); the platform is read from the wheel's tag.

    check-wheel.py --dist DIR
        Fails unless DIR holds exactly the expected wheel tags (five platforms x cp314-abi3,
        cp314t, cp315t) and one sdist whose archive contains Cargo.lock.

Exits 0 when everything holds, 1 with one line per finding on stderr. Standard library only.
"""

from __future__ import annotations

import argparse
import fnmatch
import re
import sys
import tarfile
import tomllib
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ALLOWED = ROOT / "liquers-py" / "packaging" / "allowed-libraries.toml"

PLATFORMS = {
    "linux_x86_64": re.compile(r"manylinux_2_\d+_x86_64"),
    "linux_aarch64": re.compile(r"manylinux_2_\d+_aarch64"),
    "macos_arm64": re.compile(r"macosx_\d+_\d+_arm64"),
    "win_amd64": re.compile(r"win_amd64"),
    "win_arm64": re.compile(r"win_arm64"),
}
BUILDS = {("cp314", "abi3"), ("cp314", "cp314t"), ("cp315", "cp315t")}


def wheel_tags(name: str) -> tuple[str, str, str]:
    """(python tag, abi tag, platform tag) of a wheel file name."""
    parts = name[: -len(".whl")].split("-")
    if len(parts) < 5:
        raise ValueError(f"{name}: not a wheel file name")
    return parts[-3], parts[-2], parts[-1]


def platform_family(platform_tag: str) -> str:
    if "manylinux" in platform_tag or "linux" in platform_tag:
        return "linux"
    if platform_tag.startswith("macosx"):
        return "macos"
    if platform_tag.startswith("win"):
        return "windows"
    raise ValueError(f"unknown platform tag {platform_tag!r}")


def dependencies(listing: str, family: str) -> list[str]:
    """Library names (Linux, Windows) or paths (macOS) from a platform tool's output."""
    found: list[str] = []
    for raw in listing.splitlines():
        line = raw.strip()
        if family == "linux":
            # `auditwheel show` lists external references as `libfoo.so.N`, one per token.
            found.extend(re.findall(r"\b(lib[\w+.-]+\.so(?:\.\d+)*|ld-linux[\w.-]*\.so\.\d+)", line))
        elif family == "macos":
            if line.startswith("/") or line.startswith("@"):
                found.append(line.split()[0])
        else:
            if re.fullmatch(r"[\w.-]+\.dll", line, flags=re.IGNORECASE):
                found.append(line)
    return sorted(set(found))


def check_wheel(path: Path, listing: str | None, max_mb: float, manylinux: str) -> list[str]:
    findings: list[str] = []
    _, _, platform_tag = wheel_tags(path.name)
    family = platform_family(platform_tag)
    size_mb = path.stat().st_size / 1_000_000
    if size_mb > max_mb:
        findings.append(f"{path.name}: {size_mb:.1f} MB, over the {max_mb:g} MB limit")
    if family == "linux" and f"manylinux_{manylinux}_" not in platform_tag:
        findings.append(f"{path.name}: platform tag {platform_tag} is not manylinux_{manylinux}")
    with zipfile.ZipFile(path) as archive:
        for name in archive.namelist():
            first = name.split("/", 1)[0]
            base = name.rsplit("/", 1)[-1].lower()
            if first.endswith(".libs") or "/.dylibs/" in f"/{name}":
                findings.append(f"{path.name}: grafted library {name}")
            elif base.startswith(("libssl", "libcrypto")):
                findings.append(f"{path.name}: OpenSSL library {name}")
    if listing is not None:
        allowed = tomllib.loads(ALLOWED.read_text(encoding="utf-8"))[family]
        for library in dependencies(listing, family):
            if not _allowed(library, family, allowed):
                findings.append(f"{path.name}: links {library}, which is not in the {family} allow-list")
    return findings


def _allowed(library: str, family: str, allowed: dict) -> bool:
    if family == "macos":
        if library.startswith("@"):  # @rpath/@loader_path: the extension itself
            return True
        return any(library.startswith(prefix) for prefix in allowed["prefixes"])
    name = library.lower()
    return any(fnmatch.fnmatch(name, pattern.lower()) for pattern in allowed["libraries"])


def check_dist(directory: Path) -> list[str]:
    findings: list[str] = []
    expected = {(platform, build) for platform in PLATFORMS for build in BUILDS}
    seen: set[tuple[str, tuple[str, str]]] = set()
    for wheel in sorted(directory.glob("*.whl")):
        python_tag, abi_tag, platform_tag = wheel_tags(wheel.name)
        platform = next((key for key, pattern in PLATFORMS.items() if pattern.fullmatch(platform_tag)), None)
        build = (python_tag, abi_tag)
        if platform is None or build not in BUILDS:
            findings.append(f"unexpected wheel {wheel.name}")
            continue
        if (platform, build) in seen:
            findings.append(f"duplicate wheel for {platform} {python_tag}-{abi_tag}")
        seen.add((platform, build))
    for platform, build in sorted(expected - seen):
        findings.append(f"missing wheel for {platform} {build[0]}-{build[1]}")
    sdists = sorted(directory.glob("*.tar.gz"))
    if len(sdists) != 1:
        findings.append(f"expected one sdist, found {len(sdists)}")
    else:
        with tarfile.open(sdists[0]) as archive:
            names = archive.getnames()
        if not any(name.endswith("/Cargo.lock") for name in names):
            findings.append(f"{sdists[0].name}: no Cargo.lock in the sdist")
    return findings


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("wheel", nargs="?", type=Path)
    parser.add_argument("--dist", type=Path)
    parser.add_argument("--deps", help="the platform tool's dependency listing, or - for stdin")
    parser.add_argument("--manylinux", default="2_28")
    parser.add_argument("--max-mb", type=float, default=90.0)
    args = parser.parse_args(argv)
    if (args.wheel is None) == (args.dist is None):
        parser.error("give either a wheel or --dist DIR")
    if args.dist is not None:
        findings = check_dist(args.dist)
    else:
        listing = None
        if args.deps == "-":
            listing = sys.stdin.read()
        elif args.deps is not None:
            listing = Path(args.deps).read_text(encoding="utf-8", errors="replace")
        findings = check_wheel(args.wheel, listing, args.max_mb, args.manylinux)
    for finding in findings:
        print(finding, file=sys.stderr)
    return 1 if findings else 0


if __name__ == "__main__":
    sys.exit(main())
