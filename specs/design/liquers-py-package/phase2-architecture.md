# Phase 2: Solution & Architecture — liquers-py-package

## Overview

The version and the license are stated once, in `[workspace.package]`, and every root workspace
member inherits them. `liquers_core::VERSION` keeps reading the version at compile time; the
`Environment` trait reports it, and Python and JavaScript expose it.

- **`liquers-py-release.yml`** runs on a `liquers-py-v*` tag, by hand, and on a pull request that
  changes the workflow itself (which publishes nothing, as a manual run does).
  1. It resolves one `Cargo.lock` and generates the third-party notices.
  2. It builds the sdist and fifteen wheels with `PyO3/maturin-action`.
  3. It tests every wheel in a clean venv on its own runner, and checks it for system libraries.
  4. On a tag it publishes to the GitHub release and to TestPyPI, and to PyPI after the `pypi`
     environment approves, through trusted publishing.
- **`liquers-py-package.yml`** builds and tests one Linux wheel on pull requests that touch
  packaging.

Rejected:
- **`cibuildwheel`.** It wraps the same maturin build and adds a second configuration layer.
  maturin-action is PyO3's own route.
- **`abi3t-py315` in place of `cp315t`.** The wheel builds but does not import (Interfaces).
- **Committing the notices.** They would go stale with every lock resolution.
- **A literal version in core.** Decision 8 rejected it.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `PY-NO-TYPE-DECLARATIONS` (`liquers-py-stubs`) | draft | P2 | AC-6's stub half needs its files | No | No | Lands last within M5; it adds AC-6's `.pyi` / `py.typed` assertions |
| `PY-RECORD-VALUES-CANNOT-CROSS-TO-PYTHON`, `PY-RECORD-SOURCES-…` (`liquers-py-records`) | draft | P2 | The `[arrow]` / `[pandas]` extras serve them | No | No | Same release |
| `PY-PYPY-SUPPORT-UNVERIFIED` | draft | P3 | PyPy stays source-only | No | No | README states it |
| `PY-API-REFERENCE-NOT-GENERATED` | draft | P3 | No reference ships with the wheel | No | No | Non-goal |
| `OPENSSL-BUILD-REQUIREMENT-IS-STALE` | closed | — | Fixed by PR #241 | — | — | — |

## Interfaces

**Cargo.**
- The workspace `Cargo.toml` gains `[workspace.package] version = "0.1.0"` and `license =
  "AGPL-3.0-or-later"`.
- Every member declares `version.workspace = true` and `license.workspace = true`.
- `liquers-template` and `liquers-markdown` drop `MIT`. `liquers-markdown` has no third-party
  files. `liquers-template/tests/spec/` keeps its own MIT `LICENSE` and is left out of the crate
  package (`exclude = ["tests/spec", "tests/spec.rs", "tests/node.rs"]`: the two test files need the
  MIT spec files, so the packaged crate carries neither).
- `[profile.wheel]` inherits `release` and adds `strip = true`, `lto = "thin"` and
  `codegen-units = 1`. The release jobs pass `--profile wheel`. `pyproject.toml` does not set
  `[tool.maturin] profile`, because `maturin develop` would read it too and the test loop would
  become an 18-minute build. An install from the sdist therefore uses Cargo's `release`.
- `liquers-py/Cargo.toml` gains `abi3 = ["pyo3/abi3-py314"]`, which only the abi3 job passes.

**liquers-core.** `Environment` gets a provided method, so no implementor changes:

```rust
/// The Liquers version this environment was built with: `crate::VERSION`. Provided; not to be overridden.
fn liquers_version(&self) -> &'static str { crate::VERSION }
```

**Python.** The new names are:
- `lq.__version__`: the PEP 440 form of `liquers_core::VERSION`, computed in `_native` at import
  by `pep440(cargo: &str) -> String`, which rewrites the Cargo pre-release suffixes the way
  maturin writes a wheel version: `-alpha.N` → `aN`, `-beta.N` → `bN`, `-rc.N` → `rcN`.
  `0.1.0` is unchanged.
- `Environment.liquers_version`: a property returning `liquers_core::VERSION` directly. Reading it
  must neither build the environment nor fail after `shutdown()`. A test asserts that it equals
  the built environment's trait value.
- `lq.require(extra: str) -> None`, in a new `_extras.py`. It imports the modules the extra
  provides on this interpreter:
  - `polars` for `"polars"`;
  - `PIL` for `"images"`;
  - `pyarrow` for `"arrow"`;
  - `pyarrow` and `pandas` for `"pandas"`;
  - for `"all"`, every one of those that the extras table gives this platform a wheel for.

  A missing module raises `ModuleNotFoundError: No module named 'PIL'; install it with pip
  install 'liquers-py[images]'`. An extra this platform cannot have (the table's last column:
  `arrow` and `pandas` on Windows arm64, `polars` on a free-threaded interpreter, detected by
  `sysconfig.get_config_var("Py_GIL_DISABLED")`) raises `ModuleNotFoundError: liquers-py[arrow]
  is not available on this platform: pyarrow has no wheel for Windows arm64`, with no install
  advice. An unknown extra raises `ValueError`. The table lives in `_extras.py` as one dict,
  shared with `liquers-py-records`' error messages and with `test_extras`.
- `__init__.py` imports `__version__` and lists `require` in `__all__`. `__version__` stays out of
  `__all__`, since `test_OBJECT08` refuses underscore names there. `require` and the
  `liquers_version` getter carry docstrings (`test_DOCS_docstrings.py`). `liquers-py-stubs`
  declares all three.

**liquers-web.** A `liquersVersion` getter on the JavaScript `Environment` returns
`liquers_core::VERSION`. The free `version()` returns the longer `"liquers-web X (liquers-core
Y)"` and stays as it is.

**`liquers-py/pyproject.toml`.**
- `license = "AGPL-3.0-or-later"` and `license-files = ["LICENSE"]` (PEP 639).
- `liquers-py/LICENSE` is a copy of the root one, and a test keeps them equal. PEP 639 paths
  cannot leave the project.
- The notices are generated into `python/liquers/THIRD-PARTY-NOTICES.html` and git-ignored. Being
  under `python-source`, they ship when present, and a build without them still works.
- It also gains `readme`, `authors`, `[project.urls]`, keywords, and `maturin>=1.9` in
  `[build-system]`.
- `requires-python` stays `>=3.10,<3.16`.

**The extras.** `[project.optional-dependencies]` adds `pandas`, beside question 6's `polars`,
`images`, `arrow` and `all`; this is a scope change, because records' `to_pandas` needs it. A
package with no wheel for a platform is excluded by an environment marker, so the install never
falls back to a source build of a third-party package. Checked on PyPI on 2026-11-01:

| Extra | Package (version) | Wheels exist for | No wheel → marker excludes it |
|---|---|---|---|
| `polars` | polars 2.1 (runtime `cp311-abi3`) | every platform, GIL builds | free-threaded interpreters (an abi3 library does not load there): documented; `require` says it is not available |
| `images` | pillow 13.0 | every platform and ABI | — |
| `arrow` | pyarrow 26.0 | Linux, macOS arm64, Windows x86_64; `cp314` and `cp314t` | Windows arm64: `pyarrow; not (sys_platform == "win32" and platform_machine == "ARM64")` |
| `pandas` | pandas 3.1, plus `arrow` | every platform and ABI | as `arrow` on Windows arm64 |

There is no standard marker for a free-threaded interpreter yet, so `polars` on 3.14t cannot be
excluded by a marker. The README says so. AC-9 is tested only where the table says a wheel exists.
Windows arm64 was checked first: only pyarrow is missing there.

**The wheel matrix.** Five platforms, three builds each, all with `--profile wheel --locked`, and
with `AWS_LC_SYS_USE_SYSTEM=0` (passed into the manylinux container through `docker-options:
-e`).

| Platform | Runner | Build environment |
|---|---|---|
| Linux x86_64 | `ubuntu-24.04` | maturin-action, `manylinux: 2_28` (its own interpreters) |
| Linux aarch64 | `ubuntu-24.04-arm` | maturin-action, `manylinux: 2_28` |
| macOS arm64 | `macos-15` | `actions/setup-python` (`freethreaded: true` for the `t` builds), `-i` that action's `python-path` output |
| Windows x86_64 | `windows-2025` | the same |
| Windows arm64 | `windows-11-arm` | the same, plus LLVM from `choco install llvm` when `clang-cl` is missing, and `CC`/`CXX=clang-cl` for `aws-lc-sys` and `ring` |

The three builds per platform:
- `--features abi3` on 3.14, which gives `cp314-abi3`;
- 3.14t, which gives `cp314t`;
- 3.15t, which gives `cp315t`. 3.15.2 is current, according to `actions/python-versions`.

**abi3t was measured** on 2026-11-01 with 3.15.0b4t and maturin 1.16. maturin tags the wheel
`cp315-abi3.abi3t`. The wheel does not import, because its library is named
`_native.abi3t-x86_64-linux-gnu.so` and the interpreter accepts only `.abi3t.so`
(`importlib.machinery.EXTENSION_SUFFIXES`). The new tag also needs an installer that knows PEP
803. `PY-ABI3T-WHEEL-NOT-USED` records when to switch.

**Size.** Measured on 2026-11-01 with the `wheel` profile's settings (strip, thin LTO, one codegen unit) and the default features: the Linux x86_64 abi3 wheel `liquers_py-0.1.0-cp314-abi3-manylinux_2_39_x86_64.whl` is 30.9 MB compressed, holding an 85.7 MB `_native.abi3.so`; the build took 18 minutes on 4 cores. That is well under PyPI's 100 MB default file limit, so no size exception is requested. The `manylinux_2_39` tag comes from the build machine's glibc; the release builds in the manylinux container to get the `manylinux_2_28` tag the `wheels` job checks.

**`liquers-py-release.yml`.** Each job lists the jobs it needs.

1. `lock`: runs `cargo generate-lockfile`, then `scripts/check-workspace-metadata.py [--root
   DIR] [--tag TAG]` (the tag comes from `GITHUB_REF_NAME` on a tag run and is left out on a
   manual run, which skips that check). It checks the tag against the workspace version, every
   member's `version.workspace` and `license.workspace` (AC-10, AC-11), and the two LICENSE
   copies. It runs `cargo package -p liquers-template --list` to confirm `tests/spec/` is left
   out, and `python3 -m unittest` over `scripts/test_check_*.py` and
   `scripts/test_release_workflow.py`. It uploads `Cargo.lock`.
2. `notices` (needs 1): runs `cargo about generate -m liquers-py/Cargo.toml -c
   liquers-py/about.toml --features <the wheel's> -o <triple>.html liquers-py/about.hbs` once per
   target triple, and merges the results into `python/liquers/THIRD-PARTY-NOTICES.html`.
   `liquers-py/about.toml` lists the accepted licenses, `AGPL-3.0-or-later` included, and
   `about.hbs` is the template.
3. `sdist` (needs 1 and 2): `maturin sdist`, with the lock and the notices.
4. `wheels` (needs 1 and 2): the matrix above, built with `--auditwheel check` so that nothing
   is grafted into the wheel. `scripts/check-wheel.py <wheel>` then unpacks it and fails when:
   - it holds a `*.libs/` directory or any `libssl*` / `libcrypto*` file;
   - the native library links anything outside the platform's allow-list in
     `liquers-py/packaging/allowed-libraries.toml`: the manylinux_2_28 policy (read from
     `auditwheel show`), macOS's `/usr/lib/` and `/System/Library/` (from
     `delocate-listdeps --all`), or Windows' system DLLs, the `api-ms-win-*` set, the VC
     runtime and `python3*.dll` (from `dumpbin /dependents` inside `ilammy/msvc-dev-cmd`);
   - it is over 90 MB, which leaves a margin under PyPI's 100 MB limit.

   This is AC-7.
5. `dist` (needs 3 and 4): `scripts/check-wheel.py --dist dist/` fails unless `dist/` holds
   exactly the fifteen expected tags and one sdist, and the sdist holds `Cargo.lock` (AC-1).
6. `test` (needs 4): on the build's own runner, a fresh venv installs the wheel file directly and
   runs `pytest --rootdir liquers-py/tests/package --confcutdir liquers-py/tests/package -p
   no:cacheprovider liquers-py/tests/package` (AC-2 to AC-6, AC-9). `test-sdist` (needs 3) installs
   the sdist on Linux with Rust and runs the same suite's `test_import_and_version`.
7. On a tag only:
   - `github-release` (needs 5 and 6; `contents: write`): `softprops/action-gh-release`.
   - `testpypi` (needs 5 and 6; environment `testpypi`; `id-token: write`):
     `pypa/gh-action-pypi-publish` with TestPyPI's `repository-url` and `skip-existing: true`,
     then downloads `liquers-py==X` from TestPyPI and compares its hashes with `dist/`.
   - `pypi` (needs `testpypi`; environment `pypi`, which Orest protects with a required reviewer;
     `id-token: write`).

**`liquers-py-package.yml`** runs jobs 1, 2, 4 and 6 for Linux x86_64 abi3 only. It is
triggered by changes to:
- `liquers-py/{pyproject.toml,Cargo.toml,README.md,LICENSE,about.toml,about.hbs}`, and
  `liquers-py/src/**`, so a change that breaks the abi3 build is seen before a release;
- `liquers-py/python/liquers/**`;
- `liquers-py/tests/package/**`;
- both workflows;
- the workspace `Cargo.toml`;
- `scripts/check-workspace-metadata.py`, `scripts/check-wheel.py`, `scripts/test_release_workflow.py`
  and their tests.

`scripts/test_release_workflow.py` (unittest, with PyYAML installed in `lock`) checks the release
workflow statically: every publishing job has the tag-only `if:`, `pypi` needs `testpypi` and
declares `environment: pypi` and `id-token: write`. `check-wheel.py` takes the manylinux policy
as `--manylinux 2_28`, so a host-built wheel can be checked against its own tag.

**Out-of-repository setup (Orest).** Before the first tag: register the trusted publisher
(`orest-d/liquers`, `liquers-py-release.yml`, environments `testpypi` and `pypi`) on TestPyPI and on
PyPI, and give the `pypi` environment a required reviewer.

## Integration Points

- Every root `Cargo.toml`. `liquers-core/src/context.rs`. `liquers-py/src/{lib,environment}.rs`.
- `scripts/check-build-matrix.sh` gains a `liquers-py --features abi3 --lib` row.
- `liquers-web/tests/stubs/valid_usage.ts` uses `liquersVersion`.
- `liquers-py/python/liquers/{__init__,_extras}.py` and the `.pyi` files (`liquers-py-stubs`).
- `liquers-py/{pyproject.toml,LICENSE,README.md,about.toml,about.hbs}`.
- `liquers-web/src/environment.rs`, and `liquers-web/tests/e2e/package.spec.ts`: its PACKAGE04 and
  PACKAGE07 tests read `^version = "…"` from `liquers-web` and `liquers-core`, so they now read
  `[workspace.package]` instead.
- Two workflows; `scripts/check-workspace-metadata.py`, `scripts/check-wheel.py` and their tests;
  `liquers-py/packaging/allowed-libraries.toml`; `liquers-py/tests/package/`;
  `.gitignore`.

## Error Handling

`liquers_version` cannot fail. `require` raises Python's own `ModuleNotFoundError` or
`ValueError`. A tag that disagrees with the workspace fails `lock` before anything is built. An
unexpected linked library fails `wheels`, and nothing is published.

## Relevant Commands

None.

## Documentation Architecture

Each changed reference or guide document adds a History row and bumps `reviewed:`.
`affects_docs` adds `PROJECT_OVERVIEW.md` and `JAVASCRIPT_INTEGRATION.md`.

| Path | Change |
|---|---|
| `specs/reference/PYTHON_INTEGRATION.md` | New §Packaging covers platforms and ABI, the wheel's Cargo features, extras and their table, `require`, versions, native requirements, the release pipeline and its approval, and cutting a release. Open problems: source-only platforms, polars on free-threaded builds, the size margin, the manual PyPI approval, the `requires-python` ceiling on abi3, abi3t. §8 drops "no published wheel" |
| `specs/guides/PYTHON_GUIDE.md` | §1 Installing: pip and extras first, from source second |
| `specs/reference/PROJECT_OVERVIEW.md` | One workspace version; `Environment::liquers_version` |
| `specs/reference/JAVASCRIPT_INTEGRATION.md` | `liquersVersion` |
| `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` | Appendix A `PACKAGE04` uses the environment's version. §6 Python covers the release route and what cost time (abi3t, extras without wheels) |
| `liquers-py/README.md` | Install; which platforms are wheels and which source-only (PyPy, macOS x86_64, musllinux, 3.10-3.13), with `--only-binary :all:`; the license wording; a `PACKAGE` row; quick-start markers; how to cut a release |
| `CLAUDE.md` | The liquers-py line adds that published wheels cover CPython 3.14+ (abi3, 3.14t, 3.15t); source builds keep 3.10-3.15 and PyPy |
| `specs/README.md` | The design's row |

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | Those under Integration Points |
| Crates and workflows affected | Every crate's manifest; two new workflows; `liquers-web`'s e2e package tests |
| Existing tests likely to change | `liquers-web/tests/e2e/package.spec.ts`: its version reading |
| New validation | `liquers-py/tests/package/test_PACKAGE_*.py`; `check-workspace-metadata.py`; library checks; `liquers_version` tests in core, py and web |
| Compatibility / data / concurrency / performance / security | Relicensing is Orest's Decision 5; every commit is this project's own. Publishing is irreversible and gated by the `pypi` environment. Trusted publishing keeps tokens out of the repository. Release time: thin LTO with one codegen unit over polars on 15 jobs, about 25 minutes each, in parallel. PyPI's default 10 GB project quota allows roughly 100 releases at the measured size |
| Recovery | A PyPI release can be yanked but not replaced, so TestPyPI goes first. A manual run publishes nothing |
| Certainty and open questions | Measured: the abi3 wheel, abi3t, wheel size and wheel availability. Not testable before CI runs: Windows arm64's `clang-cl`, and the manylinux 3.15t interpreter. The first manual run, before any tag, reports both. No decision needs Orest |
