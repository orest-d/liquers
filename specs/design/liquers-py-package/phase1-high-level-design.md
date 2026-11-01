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
- **Leading issue:** question 3 (publishing cannot be undone: a PyPI version, once uploaded, can
  never be uploaded again) and question 8 (the license line only the owner can state).
- **Explanation:** platforms, ABI and publishing set what the project promises its users and what
  CI spends; the license and the first version are the owner's. The plan's "vendored OpenSSL for
  the wheels" no longer applies (question 7).
- **Open questions:**
  1. **Open design - platforms.** Recommended: Linux x86_64 and aarch64 (`manylinux_2_28`, each
     built on a native runner), macOS arm64 and x86_64 (two wheels, not `universal2`, which would
     double a large binary), Windows x86_64. Source-only (the sdist): musllinux, Windows arm64,
     PyPy (`PY-PYPY-SUPPORT-UNVERIFIED`, never tested), 32-bit.
  2. **Open design - ABI.** Recommended: one `abi3` wheel per platform for CPython 3.11 and newer,
     plus a `cp310` wheel and a free-threaded `cp314t` wheel (free-threaded builds have no stable
     ABI before 3.15): three builds per platform, fifteen in all, instead of six per platform.
     The floor is 3.11 rather than 3.10 because the read-only column buffers of
     `liquers-py-records` need the buffer protocol, which the limited API has only from 3.11.
     Phase 2 confirms liquers-py and `pyo3-async-runtimes` build under `abi3` and measures the
     cost. Alternative: per-version wheels (`cp310`-`cp314`, `cp314t`), simpler and twice the CI.
  3. **Open design, irreversible - publishing.** Recommended: a `liquers-py-release` workflow on a
     `liquers-py-v*` tag (and by hand) builds every wheel and the sdist, tests them (AC-2 to AC-6),
     attaches them to a GitHub release, and uploads to TestPyPI; the PyPI upload is a final job
     behind a protected GitHub environment that Orest approves per release, using PyPI trusted
     publishing (no token in the repository). Orest registers the publisher for `liquers-py` on
     PyPI and TestPyPI (free on both, checked 2026-11-01). Alternatives: GitHub releases only in
     M5 (no PyPI), or automatic PyPI upload on tag.
  4. **Proposed resolution - version.** liquers-py's version is locked to `liquers-core`'s (both
     `0.1.0` today): a test fails when they differ, maturin takes the version from
     `liquers-py/Cargo.toml`, and `lq.__version__` and `lq.__core_version__` report both
     (PACKAGE04). A Cargo pre-release (`0.1.0-alpha.1`) becomes PEP 440 `0.1.0a1`.
  5. **Open design - the first release number.** Recommended `0.1.0a1`, marking the API unstable;
     alternative `0.1.0`.
  6. **Proposed resolution - what a wheel contains.** The same Cargo features as the test build
     (`image-support`, `polars`, `records`, `records-ipc`, `records-parquet`, `templates`), never
     `debug-handles`, no `egui` (M6 decides UI). Python extras add only Python packages:
     `[polars]`, `[images]` (Pillow), `[arrow]` (pyarrow), `[all]`; `lq.require(extra)` raises an
     error naming the missing package (PACKAGE06). A release profile for size (strip, thin LTO,
     one codegen unit) is measured in Phase 2 against PyPI's 100 MB file limit.
  7. **Proposed resolution - native libraries.** Nothing to vendor: no workspace build links
     OpenSSL (`cargo tree -i openssl-sys --workspace --target all` is empty; TLS is `rustls` with
     `aws-lc-rs` and `ring`), so wheels need a C compiler at build time and no system library at
     run time; `auditwheel` / `delocate` verify the last. The stale OpenSSL requirement in
     `CLAUDE.md` and CI is `OPENSSL-BUILD-REQUIREMENT-IS-STALE`, fixed in its own PR.
  8. **Blocking for release, open design - license.** The repository's `LICENSE` is the GNU AGPL
     version 3, but no crate declares a `license` and nothing says "only" or "or later". The wheel
     needs one SPDX expression (`AGPL-3.0-only` or `AGPL-3.0-or-later`) and, since it links many
     third-party crates statically, a generated third-party notices file (`cargo-about`) inside it.
     Only Orest can choose the expression; the notices are recommended.
  9. **Proposed resolution - CI cost.** The release workflow runs on tags and by hand only; a
     `liquers-py-package` job on pull requests that touch packaging files builds one Linux x86_64
     wheel and runs AC-2 to AC-6 against it, so the release path is exercised before a tag.
  10. **Proposed resolution - the quick-start.** `liquers-py/README.md` gains an "Install" block
      (`pip install liquers-py`) and the quick-start it already has; PACKAGE03 extracts the marked
      block, runs it in a clean venv from the built wheel, and fails when it fails.

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
  WHEN the release workflow runs
  THEN it produces a wheel per claimed platform and ABI (question 1, 2) and an sdist (PACKAGE01)
- **AC-2** A wheel installs into a clean environment
  WHEN each wheel is installed into a fresh venv on its platform with no Rust and no build tools
  THEN `import liquers` succeeds and `lq.__version__` is the release version (PACKAGE02)
- **AC-3** The quick-start runs
  WHEN the README's marked quick-start block runs in that venv
  THEN it exits 0 and prints its documented result (PACKAGE03)
- **AC-4** Versions agree
  WHEN `lq.__version__`, `lq.__core_version__`, the wheel's metadata and the crates' versions are compared
  THEN they are equal (PACKAGE04)
- **AC-5** The documented value type and extras
  WHEN the default wheel is installed, then each extra
  THEN the built-in commands of every documented Cargo feature are registered, and `lq.require(extra)` succeeds only with that extra installed (PACKAGE05, PACKAGE06)
- **AC-6** The wheel carries what it must
  WHEN a wheel's file list and metadata are read
  THEN it holds `py.typed`, the `.pyi` files, the license and the third-party notices, and declares `Requires-Python`, the license expression and the extras (PACKAGE07, STUBS07)
- **AC-7** No system library is required
  WHEN `auditwheel show` / `delocate-listdeps` / `dumpbin` inspect a wheel
  THEN it links only the platform's allowed system libraries, no `libssl`
- **AC-8** Publishing is approved per release
  WHEN a `liquers-py-v*` tag is pushed
  THEN wheels reach the GitHub release and TestPyPI, and PyPI only after the environment's approval (question 3)

Non-goals: conda packages; PyPy, musllinux and Windows arm64 wheels; a UI or server extra (M6);
publishing the API reference; signing beyond PyPI's attestations.

## Core Interactions

Bindings only: `lq.__version__`, `lq.__core_version__`, `lq.require`. Build: `pyproject.toml`
metadata and extras, release profile, two workflows. No library change.

## Crate Placement

`liquers-py/pyproject.toml`, `liquers-py/Cargo.toml`, `liquers-py/python/liquers/__init__.py`;
`.github/workflows/liquers-py-release.yml` and a pull-request job; `liquers-py/tests/package/`
(the clean-venv tests, run against a wheel, never `maturin develop`); a third-party notices step.

## Documentation Intent

- Reference: a §Packaging in `PYTHON_INTEGRATION.md` (platforms and ABI, features and extras,
  version lock, native requirements, the release pipeline and its approval).
- Guide: `PYTHON_GUIDE.md` §1 Installing rewritten (pip and extras first, from source second).
- Other: `liquers-py/README.md` (install, a `PACKAGE` row, how to cut a release);
  `LANGUAGE-INTEGRATION_GUIDE.md` §PACKAGE (its OpenSSL sentence, through the spin-off) and §6 Python.
- Open problems documented: source-only platforms; wheel size; the hand-made release approval.

## Design Dependencies

- `PY-NO-PUBLISHED-WHEELS` — **implemented** by this design.
- `liquers-py-stubs` — **requires**: the files AC-6 checks. `liquers-py-records` — **requires**:
  the `[arrow]` extra and the 3.11 buffer floor (question 2).
- `OPENSSL-BUILD-REQUIREMENT-IS-STALE` — **overlaps**; its own PR.
- `py-pypy-ci-job` — **overlaps**: PyPy stays source-only here whatever it decides.
- `language-binding-tests-in-ci` (complete) — precedent for the test workflow this reuses.

## References

- `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §PACKAGE, Appendix A PACKAGE
- `liquers-py/pyproject.toml`, `.github/workflows/liquers-py-tests.yml`, `scripts/test-liquers-py.sh`
