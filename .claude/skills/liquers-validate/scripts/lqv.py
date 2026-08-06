#!/usr/bin/env python3
"""Compact front-end for the `liquers-validate` CLI.

The validator's own output is a complete JSON envelope: every query element, plan step and
error carries a `position` object, so a two-step plan prints as ~40 lines. That is the right
shape for a machine and the wrong shape for reading. This script runs the validator with
`--detail full` (so nothing is lost) and renders the part you actually have to look at —
the resolved plan steps and their parameters — as one line each.

Reading the plan is the point. A query that parses and plans cleanly can still mean something
other than what its author intended, and the status field cannot tell you that; only the steps
can. See SKILL.md.

Usage:
    lqv.py [--raw] [validator options] -- QUERY [QUERY ...]
    lqv.py --recipes recipes.yaml --cwd reports
    lqv.py --query-file queries.txt

All options except `--raw` are passed straight through to `liquers-validate`; `--format`,
`--detail` and `--quiet` are managed by this script and are stripped if you pass them.
Exit code is the validator's own: 0 ok/warning, 1 a query failed, 2 bad invocation.
"""

import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

BIN_NAME = "liquers-validate"
# Options this script owns; passing them through would either duplicate a clap argument or
# discard the detail the digest is built from.
MANAGED_WITH_VALUE = {"--format", "--detail"}
MANAGED_FLAGS = {"--quiet"}


def find_repo_root(start: Path) -> Path:
    """Walk up for the workspace root. The validator finds specs/command_registry.yaml the
    same way, so anchoring here keeps the script's view of the registry identical to the
    tool's."""
    for directory in [start, *start.parents]:
        if (directory / "Cargo.toml").is_file() and (directory / "liquers-core").is_dir():
            return directory
    return start


def find_binary(root: Path) -> list[str]:
    """Prefer an already-built binary; fall back to `cargo run`, which builds on demand.

    A cold build of liquers-core with the `cli` feature takes a couple of minutes, so it is
    worth saying that is what is happening rather than appearing to hang.
    """
    for profile in ("debug", "release"):
        candidate = root / "target" / profile / BIN_NAME
        if candidate.is_file() and os.access(candidate, os.X_OK):
            return [str(candidate)]
    on_path = shutil.which(BIN_NAME)
    if on_path:
        return [on_path]
    print(
        f"note: no built {BIN_NAME} found; building it with cargo (first run takes a few minutes)",
        file=sys.stderr,
    )
    return [
        "cargo", "run", "--quiet", "-p", "liquers-core",
        "--features", "cli", "--bin", BIN_NAME, "--",
    ]


def split_args(argv: list[str]) -> tuple[list[str], list[str]]:
    """Separate options from the positional queries after `--`.

    Queries begin with `-` (`-R/data/x.csv`), which is why the validator has no short flags and
    why `--` matters. Everything after the first standalone `--` is left untouched — stripping
    a managed option there would corrupt a query.
    """
    if "--" in argv:
        cut = argv.index("--")
        return argv[:cut], argv[cut:]
    return argv, []


def strip_managed(options: list[str]) -> list[str]:
    kept, skip_next = [], False
    for arg in options:
        if skip_next:
            skip_next = False
            continue
        if arg in MANAGED_WITH_VALUE:
            skip_next = True
            continue
        if any(arg.startswith(f"{name}=") for name in MANAGED_WITH_VALUE):
            continue
        if arg in MANAGED_FLAGS:
            continue
        kept.append(arg)
    return kept


def render_key(key) -> str:
    """A Key serializes as a list of {name, position} elements."""
    if isinstance(key, list):
        return "/".join(str(element.get("name", "?")) for element in key)
    if isinstance(key, dict):
        return str(key.get("name", key))
    return str(key)


def render_parameter(parameter) -> str:
    """ParameterValue is an externally tagged enum; each variant answers a different question
    about where the value came from, which matters when a recipe overrides an argument."""
    if not isinstance(parameter, dict) or len(parameter) != 1:
        return str(parameter)
    (variant, body), = parameter.items()
    if variant in ("ParameterValue", "OverrideValue", "DefaultValue"):
        name, value = body[0], body[1]
        mark = {"ParameterValue": "", "OverrideValue": "override ", "DefaultValue": "default "}[variant]
        return f"{mark}{name}={json.dumps(value)}"
    if variant in ("ParameterLink", "OverrideLink", "DefaultLink", "EnumLink"):
        return f"{body[0]}=<link>"
    if variant == "MultipleParameters":
        return ", ".join(render_parameter(item) for item in body)
    if variant in ("Injected", "Placeholder"):
        return f"{body}:{variant.lower()}"
    if variant == "None":
        return "-"
    return f"{variant}({body})"


def render_step(step) -> str:
    if not isinstance(step, dict) or len(step) != 1:
        return str(step)
    (variant, body), = step.items()
    if variant.startswith("Get"):
        return f"{variant}[{render_key(body)}]"
    if variant == "Action":
        namespace = body.get("ns") or ""
        realm = body.get("realm") or ""
        qualified = "/".join(part for part in (realm, namespace, body.get("action_name", "?")) if part)
        parameters = ", ".join(render_parameter(p) for p in body.get("parameters", []))
        return f"action {qualified}({parameters})"
    if variant == "Filename":
        return f"Filename[{body.get('name')}]"
    if variant in ("Info", "Warning", "Error"):
        return f"{variant}: {body}"
    if variant in ("SetCwd", "UseKeyValue"):
        return f"{variant}[{render_key(body)}]"
    if variant in ("Evaluate", "UseQueryValue"):
        return f"{variant}(<query>)"
    if variant == "Plan":
        return "Plan(<nested>)"
    return f"{variant}({json.dumps(body)[:80]})"


def render_report(report: dict) -> None:
    registry = report.get("registry", {})
    sources = [Path(f).name for f in registry.get("merged_files", [])]
    cli_commands = registry.get("cli_commands", [])
    origin = ", ".join(sources) if sources else "none"
    if cli_commands:
        names = ", ".join(
            "/".join(p for p in (c.get("namespace") or "", c.get("name", "?")) if p)
            for c in cli_commands
        )
        origin += f" + --command {names}"
    counts = report.get("counts", {})
    print(
        f"level={report.get('level')}  status={report.get('status')}  "
        f"registry={registry.get('command_count', 0)} commands ({origin})  "
        f"namespaces={registry.get('default_namespaces')}"
    )
    print(
        f"results: {counts.get('total', 0)} total, {counts.get('ok', 0)} ok, "
        f"{counts.get('warning', 0)} warning, {counts.get('error', 0)} error"
    )

    for result in report.get("results", []):
        print()
        label = f"[{result.get('index')}]"
        if result.get("line") is not None:
            label += f" line {result['line']}"
        print(f"{label} {result.get('status', '?').upper():7} {result.get('source', '')}")

        if result.get("title"):
            print(f"    title    {result['title']}")

        encoded = result.get("encoded")
        if encoded is not None and encoded != result.get("source"):
            # A difference means the parser normalized something; worth seeing explicitly.
            print(f"    encoded  {encoded}   <- differs from source")

        if result.get("recipe_check"):
            key = result.get("key")
            suffix = f", key {render_key(key)}" if key else ", no storage key"
            print(f"    recipe   {result['recipe_check']}{suffix}")

        plan = result.get("plan")
        if plan:
            steps = plan.get("init_steps") or []
            for step in steps:
                print(f"    init     {render_step(step)}")
            for step in plan.get("steps", []):
                print(f"    plan     {render_step(step)}")
            if plan.get("error"):
                print(f"    plan err {plan['error'].get('message', plan['error'])}")
            if plan.get("payload_required") not in (None, "None"):
                print(f"    payload  {plan['payload_required']}")
            if plan.get("is_volatile"):
                print("    volatile true")
        elif report.get("level") == "Parse":
            # No plan is expected at parse level; say so rather than leaving a silent gap.
            print("    plan     (parse level — no plan built; add a registry for level 2)")

        for warning in result.get("warnings", []):
            print(f"    warning  {warning.get('source')}: {warning.get('message')}")

        error = result.get("error")
        if error:
            position = error.get("position") or {}
            where = ""
            if position.get("line"):
                where = f" (line {position['line']}, column {position.get('column')})"
            print(f"    ERROR    {error.get('message', error)}{where}")


def main() -> int:
    argv = sys.argv[1:]
    raw = "--raw" in argv
    argv = [a for a in argv if a != "--raw"]

    options, positionals = split_args(argv)
    options = strip_managed(options)

    root = find_repo_root(Path.cwd())
    # The managed options go first, not last. The validator declares its query positionals with
    # `allow_hyphen_values`, so anything appended after a query — even a real flag — is taken as
    # another query when the caller omitted `--`.
    managed = ["--format", "json", "--detail", "full", "--quiet"]
    command = find_binary(root) + managed + options + positionals

    completed = subprocess.run(command, capture_output=True, text=True)

    if not completed.stdout.strip():
        # Exit 2 keeps stdout empty on purpose: the tool was invoked wrongly.
        sys.stderr.write(completed.stderr)
        return completed.returncode if completed.returncode else 2

    if raw:
        print(completed.stdout, end="")
        return completed.returncode

    try:
        report = json.loads(completed.stdout)
    except json.JSONDecodeError:
        sys.stderr.write(completed.stderr)
        print(completed.stdout, end="")
        return completed.returncode

    render_report(report)
    return completed.returncode


if __name__ == "__main__":
    sys.exit(main())
