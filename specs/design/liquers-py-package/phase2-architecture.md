# Phase 2: Solution & Architecture — liquers-py-package

## Overview

The version and the license are stated once each, in `[workspace.package]`, and every root
workspace member inherits both. `liquers_core::VERSION` keeps reading the version at compile
time. The `Environment` trait reports it, and Python and JavaScript expose it.

- **Release workflow.** `liquers-py-release.yml` runs on a `liquers-py-v*` tag and by hand. It
  resolves one `Cargo.lock`, generates the third-party notices, and builds the sdist and fifteen
  wheels with `PyO3/maturin-action`. Each wheel is tested in a clean venv on its own runner and
  checked for system libraries. On a tag the wheels go to the GitHub release and TestPyPI, then
  to PyPI after the `pypi` environment's approval, through trusted publishing.
- **Pull-request check.** A light `liquers-py-package.yml` job builds and tests one Linux wheel
  when a packaging file changes.

Rejected:
- **A literal version in core.** Decision 8 took reading (a).
- **`cibuildwheel`.** It wraps the same maturin build and adds a second configuration layer;
  maturin-action is PyO3's own route and already knows manylinux and free-threaded builds.
- **`abi3t-py315` in place of `cp315t`.** See Interfaces: it compiles, but an installer must know
  the new tag.
- **Committing the notices file.** It would go stale with every lockfile resolution, and the
  release generates it from the same pinned lock it builds with.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `PY-PYPY-SUPPORT-UNVERIFIED` | draft | P3 | PyPy stays source-only | No | No | README states it (Decision 1) |
| `PY-EXIT-WITH-UNCANCELLABLE-COMMAND-CRASHES-BEFORE-3-14` | draft | P3 | None: every wheel is 3.14 or newer | No | No | None |
| `LIB-BUILD-EMITS-UNUSED-AND-UNHANDLED-RESULT-WARNINGS` | draft | P3 | Noise in release logs | No | No | None |
| `WORKSPACE-SERDE-DERIVE-UNDECLARED` | accepted | P2 | None: the wheel's feature set compiles today | No | No | None |
| `OPENSSL-BUILD-REQUIREMENT-IS-STALE` | closed | — | Fixed by PR #241 | — | — | None |

## Interfaces

**Cargo.** The workspace `Cargo.toml` gains `[workspace.package] version = "0.1.0"` and `license
= "AGPL-3.0-or-later"`. Every member has `version.workspace = true` and `license.workspace = true`.
`liquers-template` and `liquers-markdown` drop their `MIT`. `liquers-template/tests/spec/` keeps
its own `LICENSE` and is excluded from the crate package (`exclude = ["tests/spec"]`). A new
profile:

```toml
[profile.wheel]          # maturin build --profile wheel
inherits = "release"
strip = true
lto = "thin"
codegen-units = 1
```

`liquers-py/Cargo.toml` gains `abi3 = ["pyo3/abi3-py314"]`, passed only by the abi3 build.

**liquers-core.** This is a provided trait method, so no implementor changes:

```rust
pub trait Environment { /* … */
    /// The Liquers version this environment was built with: `crate::VERSION`. Provided, not to be overridden.
    fn liquers_version(&self) -> &'static str { crate::VERSION }
}
```

It is sync: a constant.

**Python.** The new names:
- `lq.__version__`: the PEP 440 form of `liquers_core::VERSION`, set by `_native` at import.
  `0.1.0` is the same string. A Cargo pre-release `0.2.0-rc.1` becomes `0.2.0rc1`, as maturin
  writes the wheel's version.
- `Environment.liquers_version`: a property holding the Cargo form, through the trait.
- `lq.require(extra: str) -> None`, in a new `liquers/_extras.py`. It imports the extra's
  modules: `polars` for `"polars"`, `PIL` for `"images"`, `pyarrow` for `"arrow"`, all of them
  for `"all"`. It raises `ModuleNotFoundError` naming the module and the install command (`pip
  install 'liquers-py[arrow]'`). An unknown extra raises `ValueError`.

**liquers-web.** A `liquersVersion` getter on the JavaScript `Environment`
(`liquers-web/src/environment.rs`), the same value as the free `version()`.

**`liquers-py/pyproject.toml`** gains:
- `license = "AGPL-3.0-or-later"` and `license-files = ["LICENSE", "THIRD-PARTY-NOTICES.html"]`
  (PEP 639, which maturin 1.9 and newer write). `liquers-py/LICENSE` is a copy of the root one,
  and a test keeps the two equal, because PEP 639 paths cannot leave the project directory. The
  notices file is generated and git-ignored.
- `readme = "README.md"`, `authors`, `[project.urls]` and keywords.
- `[project.optional-dependencies]`: `polars`, `images = ["pillow"]`, `arrow = ["pyarrow"]` and
  `all`.
- `maturin>=1.9` in `[build-system]`.

`requires-python` stays `>=3.10,<3.16`.

**The wheel matrix.** Five platforms, three builds each:

| Platform | Runner | Build environment |
|---|---|---|
| Linux x86_64 | `ubuntu-24.04` | maturin-action, `manylinux: 2_28` |
| Linux aarch64 | `ubuntu-24.04-arm` | maturin-action, `manylinux: 2_28` |
| macOS arm64 | `macos-15` | host |
| Windows x86_64 | `windows-2025` | host |
| Windows arm64 | `windows-11-arm` | host: installs LLVM with `choco install llvm` when `clang-cl` is missing, and sets `CC=clang-cl` and `CXX=clang-cl` for `aws-lc-sys` and `ring` |

The three builds are:
- `--features abi3 -i python3.14`, which gives `cp314-abi3`;
- `-i python3.14t`, which gives `cp314t`;
- `-i python3.15t`, which gives `cp315t`.

Every build sets `AWS_LC_SYS_USE_SYSTEM=0` and uses `--profile wheel`.

The 3.15t build is published only when the interpreter is a final release. Before that, a job
that finds a pre-release prints a notice and skips the build, so a beta ABI is never published.
`abi3t-py315` compiles on 3.15.0b4 (measured below). It is not used, because an installer has to
know the `abi3t` tag (PEP 803), and a `cp315t` wheel installs with any current pip. Issue
`PY-ABI3T-WHEEL-NOT-USED` records when to switch.

**The workflow** (`.github/workflows/liquers-py-release.yml`). Jobs, each with the job it needs
first:

1. `lock`: `cargo generate-lockfile`, then checks that the tag's version equals the workspace
   version (`scripts/check-workspace-metadata.py`, which also checks AC-10's manifests and
   AC-11's licenses), and uploads `Cargo.lock`.
2. `notices` (needs 1): `cargo about generate` with a checked-in `liquers-py/about.toml` (the
   accepted license list) and `about.hbs`, writing `THIRD-PARTY-NOTICES.html`.
3. `sdist` (needs 1 and 2): `maturin sdist`, carrying the lock and the notices.
4. `wheels` (needs 1 and 2): the fifteen-build matrix above. Each build is followed by
   `auditwheel show`, `delocate-listdeps` or `dumpbin /dependents` (AC-7).
5. `test` (needs 4): on the same runner as the build, a fresh venv with no Rust installs the
   wheel and runs `pytest liquers-py/tests/package` (AC-2 to AC-6 and AC-9). The sdist is tested
   once, on Linux, by building it in a clean venv.
6. On a tag only:
   - `github-release` (needs 3 and 5): `softprops/action-gh-release`.
   - `testpypi` (needs 3 and 5): `pypa/gh-action-pypi-publish` with `repository-url` set to
     TestPyPI and `skip-existing: true`, under `permissions: id-token: write` and the `testpypi`
     environment.
   - `pypi` (needs `testpypi`): the same action, under the `pypi` environment, which Orest
     protects with a required reviewer.

`liquers-py-package.yml`, on pull requests that touch `liquers-py/pyproject.toml`, the release
workflow, `liquers-py/tests/package/`, `liquers-py/README.md`, `liquers-py/about.toml` or the
workspace `Cargo.toml`, runs jobs 1, 4 and 5 for Linux x86_64 abi3 only.

## Integration Points

- `Cargo.toml` (workspace), and every member's `Cargo.toml`.
- `liquers-core/src/context.rs` (`Environment::liquers_version`). `liquers-py/src/lib.rs` (sets
  `__version__`) and `environment.rs` (the property).
- `liquers-py/python/liquers/{__init__,_extras}.py`; `liquers-py/{pyproject.toml,LICENSE,README.md,about.toml,about.hbs}`.
- `liquers-web/src/environment.rs`.
- `.github/workflows/{liquers-py-release,liquers-py-package}.yml`;
  `scripts/check-workspace-metadata.py`; `liquers-py/tests/package/`; `.gitignore` (the notices file).

## Error Handling

`liquers_version` cannot fail. `lq.require` raises `ModuleNotFoundError` or `ValueError`, which
are Python's own errors for an absent module and a bad argument; there is no Liquers error here. A
release whose tag disagrees with the workspace version fails `lock` before anything is built. A
wheel that links an unexpected library fails its `wheels` job, and nothing is published.

## Relevant Commands

None: the design adds no command and touches no namespace.

## Documentation Architecture

| Path | Kind | Change |
|---|---|---|
| `specs/reference/PYTHON_INTEGRATION.md` | reference, py | New §Packaging: platforms and ABI, the Cargo features in a wheel, extras and `require`, the version (`__version__`, `liquers_version`), native requirements, the release pipeline and its approval, cutting a release |
| `specs/guides/PYTHON_GUIDE.md` | guide, py | §1 Installing: pip and extras first, from source second |
| `specs/reference/PROJECT_OVERVIEW.md` | reference | One workspace version, and `Environment::liquers_version` |
| `specs/reference/JAVASCRIPT_INTEGRATION.md` | reference, web | `liquersVersion` |
| `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` | guide | Appendix A `PACKAGE04` uses the environment's version; §6 Python (release) |
| `liquers-py/README.md` | readme | Install, wheels versus source, a `PACKAGE` row, the quick-start markers, how to cut a release |
| `CLAUDE.md` | rules | Unchanged: the supported Python range is not changed |
| `specs/README.md` | map | The design's row |

`affects_docs` adds `specs/reference/PROJECT_OVERVIEW.md` and `specs/reference/JAVASCRIPT_INTEGRATION.md`.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | Every root `Cargo.toml`; `liquers-core/src/context.rs`; `liquers-py/{src/lib.rs,src/environment.rs,pyproject.toml,python/liquers/*}`; `liquers-web/src/environment.rs`; two workflows; one script |
| Crates and workflows affected | All crates (manifests only); new release and package workflows |
| Existing tests likely to change | None. A `liquers_version` test is added in liquers-core, liquers-py and liquers-web |
| New validation | `liquers-py/tests/package/test_PACKAGE_*.py`; `scripts/check-workspace-metadata.py`; the auditwheel-family steps |
| Compatibility / data / concurrency / performance / security | Relicensing `liquers-template` and `liquers-markdown` is Orest's decision (Decision 5). Every commit in them is this project's own. Publishing is irreversible, so it sits behind the `pypi` environment. Trusted publishing keeps tokens out of the repository |
| Recovery | A bad PyPI release cannot be replaced, only yanked; TestPyPI goes first for that reason. The workflow runs by hand with no upload |
| Certainty and open questions | Measured on 2026-11-01: the abi3 build (`pyo3/abi3-py314`) and `abi3t-py315` on 3.15.0b4 both compile. Wheel size: see the measurement note below. Untested until CI runs: the Windows arm64 `clang-cl` path and the 3.15t manylinux interpreter. Both are reported by the first manual run, before any tag |
