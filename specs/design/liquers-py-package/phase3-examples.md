# Phase 3: Examples & Use-cases — liquers-py-package

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | Install and run the quick-start | A user's first five minutes | AC-2, AC-3, AC-4 |
| 2 | Example | Cut a release | The maintainer's path, approval included | AC-1, AC-7, AC-8 |
| 3 | Test | `liquers-py/tests/package/test_PACKAGE_wheel.py` | The installed wheel | AC-2, AC-3, AC-4, AC-5, AC-6, AC-9 |
| 4 | Test | `scripts/check-workspace-metadata.py` and its test | Manifests: version and license | AC-10, AC-11 |
| 5 | Test | `liquers_version` unit tests in core, py, web | Every environment reports it | AC-10 |
| 6 | Test | `liquers-py-release.yml` jobs | Builds, library checks, publishing gates | AC-1, AC-7, AC-8 |

## Example 1: install and run the quick-start

```bash
python -m venv v && v/bin/pip install liquers-py
v/bin/python -c 'import liquers as lq; print(lq.__version__)'     # 0.1.0
```

The README carries the quick-start between markers, and its printed output between a second pair:

````markdown
<!-- quickstart -->
```python
import liquers as lq

@lq.first_command
def hello() -> str:
    return "world"

@lq.command
def greet(text: str, greeting: str = "Hello") -> str:
    return f"{greeting}, {text}!"

print(lq.evaluate("hello/greet-Hi"))
```
<!-- /quickstart -->
<!-- quickstart-output -->
```text
Hi, world!
```
<!-- /quickstart-output -->
````

The query `hello/greet-Hi` validates (`liquers-validate --command hello --command greet`): two
actions, `hello` and then `greet` with the argument `Hi`. A first draft wrote `world/greet-Hi`,
which validates as an action `world`, not as data; see the Learning Log.

## Example 2: cut a release

1. Raise `[workspace.package] version`, and add a History row to the README's release notes.
2. Run `liquers-py-release` by hand. It builds and tests everything and publishes nothing.
3. Push the tag `liquers-py-v0.1.0`. The workflow checks the tag against the manifest, builds,
   tests and attaches everything to a GitHub release, and uploads to TestPyPI.
4. Orest approves the `pypi` environment's deployment, and the same files go to PyPI.

This cannot be run as a test: it publishes. Its parts are tested by the manual run in step 2 and by
the checks below.

## Edge and Error Cases

| Symptom | Expected | Test |
|---|---|---|
| Tag `liquers-py-v0.1.1` on a `0.1.0` workspace | `lock` fails naming both versions | `test_check_workspace_metadata.py::test_tag_mismatch_is_refused` |
| A member without `version.workspace` / `license.workspace` | The script names the crate | `test_member_without_inheritance_is_refused` |
| `liquers-py/LICENSE` differs from the root | The script fails | `test_license_copies_differ` |
| A wheel links `libssl` | The `wheels` job fails: the step greps `auditwheel show` for any library outside the manylinux policy | `wheels` job |
| An extra without a wheel on this platform (pyarrow on Windows arm64) | The marker skips it; `lq.require("arrow")` names the package | `test_require_names_a_missing_package` |
| `require("nope")` | `ValueError` | `test_require_refuses_an_unknown_extra` |
| `liquers_version` after `shutdown()` | Still returns the version and builds nothing | `test_liquers_version_needs_no_build` |
| A Cargo pre-release `0.2.0-rc.1` | `__version__ == "0.2.0rc1"` | `liquers-py` unit test `pep440_rewrites_prereleases` |

## Test Plan

- `liquers-py/tests/package/test_PACKAGE_wheel.py` runs against an installed wheel, never
  `maturin develop`. `conftest.py` skips the module unless `LIQUERS_PY_WHEEL_TEST=1`, which only
  the `test` job sets.
  - `test_import_and_version` (AC-2): `import liquers` works, and `lq.__version__` equals the
    `RELEASE_VERSION` environment variable, which the workflow sets from the workspace manifest.
  - `test_quickstart_runs` (AC-3): extracts the marked block from the README in the source
    checkout, runs it in a subprocess of the venv's Python, and compares stdout with the marked
    output.
  - `test_versions_agree` (AC-4): `packaging.version.Version` of
    `importlib.metadata.version("liquers-py")`, of `lq.__version__` and of
    `lq.Environment().liquers_version` are equal.
  - `test_documented_value_types` (AC-5): one query per documented type, each showing a type and
    a feature:
    - `-R/df.csv` through a memory-store config gives a polars `DataFrame` (`polars`);
    - an image command gives a PIL image (`image-support`);
    - `hello/ns-rec/to_record-csv` gives an `lq.RecordView` (`records`);
    - an opaque object gives `py.Object`.

    `command_metadata` is checked for one built-in of each Cargo feature (`polars`,
    `image-support`, `records`, `templates`).
  - `test_wheel_contents` (AC-6). From `importlib.metadata.files("liquers-py")` and its
    `METADATA`, it checks:
    - `py.typed`, `__init__.pyi`, `_native.pyi`, `LICENSE` and `THIRD-PARTY-NOTICES.html` are
      present;
    - `Requires-Python`, `License-Expression: AGPL-3.0-or-later` and `Provides-Extra` cover
      every extra.
  - `test_extras` (AC-9): parametrized over the extra × platform table in Phase 2. For each cell
    with a wheel, a fresh venv installs `wheel[extra]`, and `lq.require(extra)` succeeds. In the
    bare venv it raises `ModuleNotFoundError` naming the package. Cells without a wheel are
    skipped with the table's reason. Windows arm64 runs first in the matrix.
  - `test_require_names_a_missing_package`, `test_require_refuses_an_unknown_extra`,
    `test_liquers_version_needs_no_build` (AC-9, AC-10).
- `scripts/test_check_workspace_metadata.py` (pytest, run in `lock` and in
  `liquers-py-package.yml`), testing the real workspace and the three edge cases above on
  temporary copies:
  - `test_workspace_is_consistent` (AC-10, AC-11): every root member inherits the version and
    the license, and the workspace declares `AGPL-3.0-or-later`.
  - `test_tag_mismatch_is_refused`, `test_member_without_inheritance_is_refused`,
    `test_license_copies_differ`.
- Unit tests:
  - `liquers-core/src/context.rs`: `default_environment_reports_liquers_version` (AC-10).
  - `liquers-py/src/environment.rs`: `liquers_version_is_core_version`.
  - `liquers-py/src/lib.rs`: `pep440_rewrites_prereleases`.
  - `liquers-web/tests/` (Node): `environment_reports_liquers_version` (AC-10).
  - `liquers-web/tests/e2e/package.spec.ts`: its PACKAGE04 and PACKAGE07 tests read
    `[workspace.package]`.
- Workflow checks (not pytest):
  - `wheels` produces exactly the expected tag set: 5 × `{cp314-abi3, cp314t, cp315t}` and one
    sdist (AC-1).
  - Each wheel's library check passes (AC-7).
  - `pypi` is in a protected environment (AC-8). The first manual run proves the first two;
    Orest's environment setting proves the third.
- Commands to run:
  - `python3 -m pytest scripts/test_check_workspace_metadata.py`;
  - `cargo test -p liquers-core --lib liquers_version`;
  - the PR workflow, for the wheel tests.

## Learning Log

- A query's first segment is an action, not a value: `world/greet` calls a command named `world`.
  The quick-start therefore starts from a source command, and `liquers-validate` caught the draft.

- A clean-venv test must import the *installed* package. Running pytest from the repository root
  would put `liquers-py/python` on `sys.path` first, so the job runs pytest with `--rootdir`
  under `tests/package` and `-p no:cacheprovider`, from a working directory outside the
  checkout's `python/`.
- PyPI availability changes. The extra × platform table is dated, and `test_extras` reads it
  from one Python dict, so updating the table is a one-line change.
