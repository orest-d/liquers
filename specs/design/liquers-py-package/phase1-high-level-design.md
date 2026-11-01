# Phase 1: High-Level Design — liquers-py-package

## Purpose

Let a Python user install liquers with `pip install liquers-py` on the common platforms, without a
Rust toolchain, and run a first query from the README: the `PACKAGE` feature of
`LANGUAGE-INTEGRATION_GUIDE.md` (release half; the build half was M0), for `VISION.md`'s Python
data science use case. Milestone M5 of the liquers-py plan, with
[`liquers-py-records`](../liquers-py-records/DESIGN.md) and [`liquers-py-stubs`](../liquers-py-stubs/DESIGN.md).

## Design Readiness

- **Readiness:** `needs-decision`
- **Automatic fixing:** `not-eligible`: a release pipeline that publishes outside the repository.
- **Leading issue:** question 3 (publishing cannot be undone: a file name once uploaded to PyPI
  can never be uploaded again), then questions 5 and 8.
- **Explanation:** platforms, ABI and publishing set what the project promises its users and what
  CI spends; the license and the first version are the owner's. Two plan items change: the
  "vendored OpenSSL" has nothing to vendor (question 7), and "locked to the workspace version"
  becomes locked to `liquers-core`, since the workspace declares no version (question 4).
- **Open questions:**
  1. **Open design - platforms.** Recommended: Linux x86_64 and aarch64 (`manylinux_2_28`, each
     on a native runner, `ubuntu-24.04-arm` for aarch64), macOS arm64 and x86_64 (two wheels, not
     `universal2`, which would double a large binary; x86_64 on the last Intel runner image,
     `macos-15-intel`, cross-compiled from arm64 once it retires), Windows x86_64. Source-only
     (the sdist): musllinux, Windows arm64, PyPy (`PY-PYPY-SUPPORT-UNVERIFIED`, never tested),
     32-bit. `liquers-py/README.md`'s "Supported Python" says which of its claims are wheels.
  2. **Open design - ABI.** Recommended: per platform, one `abi3` wheel for CPython 3.11 and
     newer, a `cp310` wheel, and free-threaded `cp314t` and `cp315t` wheels (or one `abi3t`
     wheel if 3.15 has the free-threaded stable ABI, which Phase 2 checks): four builds per
     platform instead of seven. The floor is 3.11 because the read-only column buffers of
     `liquers-py-records` need the buffer protocol, which the limited API has only from 3.11.
     liquers-py and `pyo3-async-runtimes` already compile with `pyo3/abi3-py311` (checked
     2026-11-01). The abi3 build is a crate feature (`abi3 = ["pyo3/abi3-py311"]`) passed only to
     that job, so a 3.10 source build is unaffected. `requires-python = "<3.16"` stays as the
     tested ceiling and is raised per release, although it stops an `abi3` wheel from reaching 3.16
     unasked. Alternative: per-version wheels only, simpler and about twice the CI.
  3. **Open design, irreversible - publishing.** Recommended: a `liquers-py-release` workflow on a
     `liquers-py-v*` tag builds every wheel and the sdist, tests them (AC-2 to AC-7), attaches them
     to a GitHub release and uploads to TestPyPI (`skip-existing`, so a re-run is safe); the PyPI
     upload is a final job behind a protected GitHub environment that Orest approves per release,
     using PyPI trusted publishing (no token in the repository). Run by hand, the workflow builds
     and tests the whole matrix without publishing. Orest registers the publisher for
     `liquers-py` on PyPI and TestPyPI (the name is free on both, checked 2026-11-01).
     Alternatives: GitHub releases only in M5, or automatic PyPI upload on tag.
  4. **Proposed resolution - version and build tool.** Built with maturin (`maturin-action` in CI).
     liquers-py's version is locked to `liquers-core`'s (both `0.1.0` today) by a test, not by a
     workspace version (the plan's wording; there is none). maturin takes it from
     `liquers-py/Cargo.toml`; `lq.__version__` reads the installed distribution's metadata
     (`importlib.metadata`), so it is the PEP 440 form, and `lq.__core_version__` reports the
     linked core's Cargo version (PACKAGE04).
  5. **Open design, irreversible - the first release number.** Recommended `0.1.0a1` (Cargo
     `0.1.0-alpha.1`), marking the API unstable. Locked to core, this moves `liquers-core` to
     `0.1.0-alpha.1`, which semver orders *before* `0.1.0`; harmless while no crate is published,
     but the alternative is to lock on the base version only. Or release `0.1.0` outright.
  6. **Proposed resolution - what a wheel contains.** The test build's Cargo features
     (`image-support`, `polars`, `records`, `records-ipc`, `records-parquet`, `templates`), never
     `debug-handles`, no `egui` (M6 decides UI): one value type in every wheel (PACKAGE05). Python
     extras add only Python packages: `[polars]`, `[images]` (Pillow), `[arrow]` (pyarrow),
     `[all]`; `lq.require(extra)` raises an error naming the missing package (PACKAGE06). Size: a
     named `[profile.wheel]` (inheriting `release`; strip, thin LTO, one codegen unit) is measured
     in Phase 2 against PyPI's 100 MB file limit. If a wheel is still too big, the plan's fallback
     is wheels split by feature, which changes PACKAGE05's contract and returns to Orest.
  7. **Proposed resolution - native libraries.** Nothing to vendor: liquers-py links no OpenSSL
     (`cargo tree -p liquers-py -i openssl-sys -e normal --target all` finds nothing; TLS is
     `rustls` with `aws-lc-rs` and `ring`). `aws-lc-sys` 0.47 builds with a C compiler alone
     (pregenerated bindings, prebuilt NASM objects on Windows); release jobs set
     `AWS_LC_SYS_USE_SYSTEM=0` so a runner's library is never picked up. `auditwheel`,
     `delocate` and `dumpbin` verify that a wheel needs no extra system library. The stale
     OpenSSL requirement in `CLAUDE.md`, the guide's §PACKAGE and CI is
     `OPENSSL-BUILD-REQUIREMENT-IS-STALE`, fixed in its own PR, which owns the guide's sentence.
  8. **Open design - license.** The repository's `LICENSE` is the GNU AGPL version 3, but no
     document says "only" or "or later", liquers-py declares no `license`, and `liquers-template`
     declares `license = "MIT"`. The wheel needs one SPDX expression for its own code
     (`AGPL-3.0-only` or `AGPL-3.0-or-later`), and Orest decides whether every crate states it
     (and what `liquers-template`'s MIT means beside it). Recommended in any case: a third-party
     notices file generated by `cargo-about` inside the wheel, since it links many crates
     statically. Gates AC-6 and AC-8.
  9. **Proposed resolution - reproducible inputs.** `Cargo.lock` is not committed (`.gitignore`),
     so two release runs may resolve different dependencies. The release workflow resolves the
     lockfile once and hands it to every job, and the sdist ships it. Alternative: commit
     `Cargo.lock` (a repository-wide change, Orest's call).
  10. **Proposed resolution - CI cost.** The release workflow runs on tags and by hand only; a
      `liquers-py-package` job on pull requests that touch packaging files builds one Linux x86_64
      wheel and runs AC-2 to AC-7 against it.
  11. **Proposed resolution - the quick-start.** `liquers-py/README.md` gains an "Install" block
      (`pip install liquers-py`), and its quick-start is changed to print its result; PACKAGE03
      extracts the marked block, runs it in a clean venv from the built wheel, and compares the
      output.

## Problem Example

```bash
python -m venv /tmp/v && /tmp/v/bin/pip install liquers-py
/tmp/v/bin/python -c 'import liquers as lq; print(lq.__version__)'
```

**Today** pip answers "No matching distribution found for liquers-py": the package exists only as
source, built with `maturin` on Rust 1.95 or newer, which compiles polars and takes minutes and
gigabytes. There is no `__version__`. **After** pip installs a wheel in seconds on each claimed
platform, the second line prints the version, and the README's quick-start runs as written.

## Scope and Acceptance Criteria

- **AC-1** Every claimed wheel builds
  WHEN the release workflow runs, on a tag or by hand
  THEN it produces a wheel per claimed platform and ABI (questions 1, 2) and an sdist (PACKAGE01)
- **AC-2** A wheel installs into a clean environment
  WHEN each wheel is installed into a fresh venv on its platform with no Rust and no build tools
  THEN `import liquers` succeeds and `lq.__version__` is the release version (PACKAGE02)
- **AC-3** The quick-start runs
  WHEN the README's marked quick-start block runs in that venv
  THEN it exits 0 and prints its documented output (PACKAGE03)
- **AC-4** Versions agree
  WHEN `lq.__version__`, `lq.__core_version__`, the wheel's metadata and the crates' versions are compared in normalized form
  THEN they are equal (PACKAGE04)
- **AC-5** The documented value type
  WHEN the default wheel is installed
  THEN the type registry holds the documented identifiers (the DataFrame, image and record types, `py.Object`) and the built-in commands of every documented Cargo feature are registered (PACKAGE05)
- **AC-6** The wheel carries what it must
  WHEN a wheel's file list and metadata are read
  THEN it holds `py.typed`, the `.pyi` files, the license and the third-party notices, and declares `Requires-Python`, the license expression and the extras (PACKAGE07, STUBS07)
- **AC-7** No system library is required
  WHEN `auditwheel show` / `delocate-listdeps` / `dumpbin` inspect a wheel
  THEN it links only the platform's allowed system libraries, no `libssl`
- **AC-8** Publishing is approved per release
  WHEN a `liquers-py-v*` tag is pushed
  THEN wheels reach the GitHub release and TestPyPI, and PyPI only after the environment's approval (question 3)
- **AC-9** Each extra activates its feature
  WHEN the wheel is installed with each extra, and without it
  THEN `lq.require(extra)` succeeds only with that extra installed, naming the missing package otherwise (PACKAGE06)

Non-goals: conda packages; PyPy, musllinux and Windows arm64 wheels; a UI or server extra (M6);
publishing the generated API reference with the artifact, which the guide's §DOCS suggests, until
`py-api-reference` builds one; signing beyond PyPI's attestations.

## Core Interactions

Bindings only: `lq.__version__`, `lq.__core_version__`, `lq.require`. Build: `pyproject.toml`
metadata and extras, an `abi3` feature, a wheel profile, two workflows. No library change.

## Crate Placement

`liquers-py/pyproject.toml`, `liquers-py/Cargo.toml`, `liquers-py/python/liquers/__init__.py`;
`.github/workflows/liquers-py-release.yml` and a pull-request job; `liquers-py/tests/package/`
(the clean-venv tests, run against a wheel, never `maturin develop`); a third-party notices step;
`[profile.wheel]` in the workspace `Cargo.toml`.

## Documentation Intent

- Reference: a §Packaging in `PYTHON_INTEGRATION.md` (platforms and ABI, features and extras,
  version lock, native requirements, the release pipeline and its approval).
- Guide: `PYTHON_GUIDE.md` §1 Installing rewritten (pip and extras first, from source second).
- Other: `liquers-py/README.md` (install, a `PACKAGE` row, how to cut a release);
  `LANGUAGE-INTEGRATION_GUIDE.md` §6 Python.
- Documents to update: the "no published wheel" limitation in `PYTHON_INTEGRATION.md` §8; the
  README's "Supported Python" (wheels versus source-only, PyPy); `CLAUDE.md`'s liquers-py line if
  the supported set changes; `specs/README.md`.
- Open problems documented: source-only platforms; wheel size; the hand-made release approval;
  the `requires-python` ceiling on `abi3` wheels.

## Design Dependencies

- `PY-NO-PUBLISHED-WHEELS` — **implemented** by this design.
- `liquers-py-stubs` — **requires**: the files AC-6 checks. `liquers-py-records` — **requires**:
  the `[arrow]` extra and the 3.11 buffer floor (question 2).
- `OPENSSL-BUILD-REQUIREMENT-IS-STALE` — **overlaps** (T3, kept apart by E1); its own PR.
- `py-pypy-ci-job` — **overlaps**: PyPy stays source-only here whatever it decides.
- `py-api-reference` — **overlaps**: the generated reference is not shipped yet (non-goals).
- `language-binding-tests-in-ci` (complete) — precedent for the test workflow this reuses.

## References

- `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §PACKAGE, Appendix A PACKAGE
- `liquers-py/pyproject.toml`, `.github/workflows/liquers-py-tests.yml`, `scripts/test-liquers-py.sh`
