# Phase 1: High-Level Design — liquers-py-package

## Purpose

Let a Python user install liquers with `pip install liquers-py` on the common platforms, without a
Rust toolchain, and run a first query from the README: the `PACKAGE` feature of
`LANGUAGE-INTEGRATION_GUIDE.md` (release half; the build half was M0), for `VISION.md`'s Python
data science use case. Milestone M5 of the liquers-py plan, with
[`liquers-py-records`](../liquers-py-records/DESIGN.md) and [`liquers-py-stubs`](../liquers-py-stubs/DESIGN.md).

## Design Readiness

- **Readiness:** `ready` (Phase 1 approved; decision log below)
- **Automatic fixing:** `not-eligible`: a release pipeline that publishes outside the repository.
- **Leading issue:** none open at Phase 1. The first PyPI upload stays behind Orest's per-release
  approval (question 3), which pre-approval does not cover.
- **Explanation:** on 2026-11-01 Orest decided questions 1-3, 5, 7, 8 and 9 (Decisions 1-7), took
  reading (a) of question 4 (Decision 8) and approved Phase 1 with every later phase pre-approved
  ("proceed all"). Questions 6, 10 and 11 stand as proposed. Plan items that changed: nothing to
  vendor for OpenSSL (question 7); wheels start at 3.14, so no `cp310` or `abi3-py311` build; one
  version, kept in core and reported by every environment.
- **Open questions:**
  1. **Decided (Decision 1) - platforms.** Wheels: Linux x86_64 and aarch64 (`manylinux_2_28`,
     each on a native runner, `ubuntu-24.04-arm` for aarch64), macOS arm64, Windows x86_64 and
     arm64 (`windows-11-arm`). Source-only (the sdist): macOS x86_64, musllinux, PyPy
     (`PY-PYPY-SUPPORT-UNVERIFIED`), 32-bit. `liquers-py/README.md`'s "Supported Python" says
     which of its claims are wheels.
  2. **Decided (Decision 2) - ABI.** Wheels for CPython 3.14 and newer only: per platform one
     `abi3` wheel (`abi3-py314`), and free-threaded `cp314t` and `cp315t` wheels: three builds per
     platform, fifteen in all. PyO3 0.30 already offers PEP 803's `abi3t` (from 3.15), so
     `abi3t-py315` may replace `cp315t` (still three builds; it cannot replace `cp314t`); Phase 2
     checks that liquers-py compiles under it and that maturin tags it. 3.10 to 3.13 stay supported from source. The 3.14 floor is above
     the 3.11 the buffer protocol needs in the limited API (`liquers-py-records`). liquers-py and
     `pyo3-async-runtimes` compile with `pyo3/abi3-py311` (checked 2026-11-01; `abi3-py314`,
     which PyO3 0.30.1 has, only relaxes that and is rechecked in Phase 2). The abi3
     build is a crate feature (`abi3 = ["pyo3/abi3-py314"]`) passed only to that job, so a source
     build on an older interpreter is unaffected. `requires-python = "<3.16"` stays as the tested
     ceiling and is raised per release, although it stops an `abi3` wheel from reaching 3.16
     unasked. On 3.10 to 3.13, pip falls back to the sdist and a long Rust build; the README says so
     and suggests `--only-binary :all:` to fail fast instead.
  3. **Decided (Decision 3) - publishing, irreversible.** As recommended: a `liquers-py-release` workflow on a
     `liquers-py-v*` tag builds every wheel and the sdist, tests them (AC-2 to AC-7), attaches them
     to a GitHub release and uploads to TestPyPI (`skip-existing`, so a re-run is safe); the PyPI
     upload is a final job behind a protected GitHub environment that Orest approves per release,
     using PyPI trusted publishing (no token in the repository). Run by hand, the workflow builds
     and tests the whole matrix without publishing. Orest registers the publisher for
     `liquers-py` on PyPI and TestPyPI (the name is free on both, checked 2026-11-01).
     Alternatives (not taken): GitHub releases only in M5, or automatic PyPI upload on tag.
  4. **Decided (Decision 8) - where the version lives** (Decision 4): reading (a). Built with maturin
     (`maturin-action` in CI). Either way all crates of the root workspace share one version, the
     `Environment` trait gains a provided `liquers_version()` returning core's constant (documented
     as not to be overridden), Python exposes `env.liquers_version` and `lq.__version__`,
     liquers-web's JavaScript environment gains the same accessor beside its free `version()`, and
     a test compares every crate, environment and the wheel in normalized form (PEP 440 against
     Cargo), so a later pre-release still compares equal (PACKAGE04). The two readings:
     - **(a) recommended - the manifest is the source, core holds the constant.**
       `[workspace.package] version = "0.1.0"`, every crate `version.workspace = true`, and
       `liquers_core::VERSION` stays `env!("CARGO_PKG_VERSION")` (`liquers-core/src/lib.rs`, as
       today). One place to edit, and Cargo, maturin and the constant can never disagree.
     - **(b) a literal in core's source** (`pub const VERSION: &str = "0.1.0";`), with every
       manifest kept equal by the test. Closest to the words, but two places to edit per release,
       held together only by the test.
  5. **Decided (Decision 4) - the first release is `0.1.0`.** No alpha or beta releases are
     defined.
  6. **Accepted at the gate - what a wheel contains.** The test build's Cargo features
     (`image-support`, `polars`, `records`, `records-ipc`, `records-parquet`, `templates`), never
     `debug-handles`, no `egui` (M6 decides UI): one value type in every wheel (PACKAGE05). Python
     extras add only Python packages: `[polars]`, `[images]` (Pillow), `[arrow]` (pyarrow),
     `[all]`; `lq.require(extra)` raises an error naming the missing package (PACKAGE06). Size: a
     named `[profile.wheel]` (inheriting `release`; strip, thin LTO, one codegen unit) is measured
     in Phase 2 against PyPI's 100 MB file limit. If a wheel is still too big, the plan's fallback
     is wheels split by feature, which changes PACKAGE05's contract and returns to Orest.
  7. **Decided (Decision 7) - native libraries.** Nothing to vendor: liquers-py links no OpenSSL
     (`cargo tree -p liquers-py -i openssl-sys -e normal --target all` finds nothing; TLS is
     `rustls` with `aws-lc-rs` and `ring`). `aws-lc-sys` 0.47 builds with a C compiler alone
     (pregenerated bindings, prebuilt NASM objects on Windows x86_64), except on Windows arm64,
     where it and `ring` need clang (`clang-cl`): that job installs LLVM if the runner lacks it and
     sets `CC`; release jobs set
     `AWS_LC_SYS_USE_SYSTEM=0` so a runner's library is never picked up. `auditwheel`,
     `delocate` and `dumpbin` verify that a wheel needs no extra system library. The stale
     OpenSSL requirement in `CLAUDE.md`, the guide's §PACKAGE and CI is
     `OPENSSL-BUILD-REQUIREMENT-IS-STALE`, fixed in its own PR, which owns the guide's sentence.
  8. **Decided (Decision 5) - license.** `AGPL-3.0-or-later`, declared on every crate for now,
     `liquers-template` included (it declares `MIT` today; all its commits are this project's),
     and in the wheel's metadata. Its `tests/spec/*.json`, copied from mustache/spec, stay MIT
     under their own `tests/spec/LICENSE`, and are not in any package. A
     third-party notices file generated by `cargo-about` ships inside the wheel, since it links
     many crates statically.
  9. **Decided (Decision 6) - reproducible inputs.** `Cargo.lock` is not committed (`.gitignore`),
     so the release workflow resolves the lockfile once and hands it to every job, and the sdist
     ships it.
  10. **Accepted at the gate - CI cost.** The release workflow runs on tags and by hand only; a
      `liquers-py-package` job on pull requests that touch packaging files builds one Linux x86_64
      wheel and runs AC-2 to AC-7 against it.
  11. **Accepted at the gate - the quick-start.** `liquers-py/README.md` gains an "Install" block
      (`pip install liquers-py`), and its quick-start is changed to print its result; PACKAGE03
      extracts the marked block, runs it in a clean venv from the built wheel, and compares the
      output.

- **Decision log (pre-approved phases; none needs Orest):**
  - Phase 2: a `[pandas]` extra (pyarrow and pandas) beside question 6's list, since
    `liquers-py-records`' `to_pandas` / `from_pandas` need both.
  - Phase 2: extras whose package has no wheel on a platform are excluded by markers (pyarrow on
    Windows arm64); polars cannot be excluded on free-threaded builds (no marker exists) and is
    documented; AC-9 runs where a wheel exists.
  - Phase 2: `cp315t`, not `abi3t-py315` (measured: it does not import), and
    `Environment.liquers_version` returns the constant without building the environment.
  - Phase 2: `liquers-markdown`, created after Phase 1, is relicensed too (Decision 5, "every crate").
  - Phase 3 review: AC-9's "succeeds only with that extra installed" applies where the extra is
    available. Where the platform cannot have it (`arrow`/`pandas` on Windows arm64, `polars` on a
    free-threaded interpreter), `require` says it is not available instead of advising an
    install, and `"all"` covers only what the platform can have. AC-8 is conceptual (it
    publishes); its parts are checked by named workflow steps. The wheel-size limit becomes a
    check (90 MB).

## Decisions

Orest, 2026-11-01, on gate items 8-13 and the side item, quoted:

1. Item 8, question 1: "I want wheels also for Windows arm64, the macOS x86_64 is enough source only."
2. Item 9, question 2: "It is enough to support wheels for python 3.14(t) and further."
3. Item 10, question 3: "ok".
4. Item 11, questions 4 and 5: "We'll not define alpha and beta releases, 0.1.0 is ok. Versioning
   needs to be consistent with the rest of the liquers - so the version should be maintained in
   core as a constant and it should be accessible via (every) environment."
5. Item 12, question 8: "use or-later, for now on every crate".
6. Item 13, question 9: "pin it with release workflow".
7. Side item, question 7: "OK, make an OpenSSL fix in a separate PR" (PR #241, merged).
8. Round 2, question 4: "Agreed with 1 and 2, proceed all" — reading (a), the workspace manifest
   as the source and `liquers_core::VERSION` reading it, and every later phase pre-approved.

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
  WHEN an installed wheel's metadata version, `lq.__version__` and `lq.Environment().liquers_version` are compared in normalized form
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
  WHEN the wheel is installed with each extra, and without it, on each platform where that extra's package publishes a wheel (Phase 2 lists them; Windows arm64 is checked first)
  THEN `lq.require(extra)` succeeds only with that extra installed, naming the missing package otherwise (PACKAGE06)
- **AC-10** One version, reported by every environment
  WHEN `liquers_core::VERSION`, a Rust environment's `liquers_version()`, Python's `env.liquers_version` and `lq.__version__`, liquers-web's environment accessor and every root workspace member's manifest are compared in normalized form
  THEN all are the one workspace version
- **AC-11** One license
  WHEN every root workspace member's manifest and the wheel's metadata are read
  THEN each declares `AGPL-3.0-or-later`

Non-goals: conda packages; PyPy, musllinux and macOS x86_64 wheels; wheels below 3.14; a UI or server extra (M6);
publishing the generated API reference with the artifact, which the guide's §DOCS suggests, until
`py-api-reference` builds one; signing beyond PyPI's attestations.

## Core Interactions

Core: the `Environment` trait gains a provided `liquers_version()` over `liquers_core::VERSION`.
Bindings: `lq.__version__`, `env.liquers_version`, `lq.require`; liquers-web's environment
accessor. Build: one workspace version and license
in every `Cargo.toml`, `pyproject.toml` metadata and extras, an `abi3` feature, a wheel profile,
two workflows.

## Crate Placement

`liquers-core/src/context.rs` (`Environment`, one provided method, additive);
`liquers-web/src/environment.rs` (the accessor); every root workspace member's
`Cargo.toml` (`version.workspace`, `license`); `liquers-py/pyproject.toml`,
`liquers-py/python/liquers/__init__.py`;
`.github/workflows/liquers-py-release.yml` and a pull-request job; `liquers-py/tests/package/`
(the clean-venv tests, run against a wheel, never `maturin develop`); a third-party notices step;
`[profile.wheel]` in the workspace `Cargo.toml`.

## Documentation Intent

- Reference: a §Packaging in `PYTHON_INTEGRATION.md` (platforms and ABI, features and extras,
  version lock, native requirements, the release pipeline and its approval).
- Guide: `PYTHON_GUIDE.md` §1 Installing rewritten (pip and extras first, from source second).
- Reference: `PROJECT_OVERVIEW.md` (the environment reports the Liquers version; one workspace
  version).
- Other: `liquers-py/README.md` (install, a `PACKAGE` row, how to cut a release);
  `LANGUAGE-INTEGRATION_GUIDE.md` §6 Python.
- Documents to update: the "no published wheel" limitation in `PYTHON_INTEGRATION.md` §8;
  `LANGUAGE-INTEGRATION_GUIDE.md` Appendix A `PACKAGE04` (its `__core_version__` example becomes
  the environment's version); `JAVASCRIPT_INTEGRATION.md` (the accessor); the license: README
  wording, the notices file, `liquers-template`'s manifest; the
  README's "Supported Python" (wheels versus source-only, PyPy); `CLAUDE.md`'s liquers-py line if
  the supported set changes; `specs/README.md`.
- Open problems documented: source-only platforms; wheel size; the hand-made release approval;
  the `requires-python` ceiling on `abi3` wheels.

## Design Dependencies

- `PY-NO-PUBLISHED-WHEELS` — **implemented** by this design.
- `liquers-py-stubs` — **requires**: the files AC-6 checks. `liquers-py-records` — **requires**:
  the `[arrow]` extra.
- `OPENSSL-BUILD-REQUIREMENT-IS-STALE` — **overlaps** (T3, kept apart by E1); its own PR.
- `py-pypy-ci-job` — **overlaps**: PyPy stays source-only here whatever it decides.
- `py-api-reference` — **overlaps**: the generated reference is not shipped yet (non-goals).
- `liquers-web` `version()` — precedent for reporting `liquers_core::VERSION`; kept, and its environment gains the accessor (Decision 4).
- `language-binding-tests-in-ci` (complete) — precedent for the test workflow this reuses.

## Scope Changes

- **2026-11-01** — Orest at the Phase 1 gate (Decisions 1-6): Windows arm64 added, macOS x86_64
  source-only, wheels from 3.14 only, `0.1.0`, one version kept in core and reported by every
  environment, `AGPL-3.0-or-later` on every crate, and the lockfile pinned. Added AC-10 and
  AC-11; questions 1-3, 5, 7-9 decided, question 4 to confirm. Approved: Orest at the gate. The
  version and license now touch every crate's manifest and one core trait method. Size: still
  `L`. Phases: 1.
- **2026-11-01** — Phase 2 (pre-approved): a `[pandas]` extra, platform markers on extras, and
  `liquers-markdown` relicensed with the others (decision log), so that every documented extra
  installs from wheels and the records exits name an extra that works. Approved: pre-approval of
  2026-11-01. Size: still `L`. Phases: 1, 2.

## References

- `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §PACKAGE, Appendix A PACKAGE
- `liquers-py/pyproject.toml`, `.github/workflows/liquers-py-tests.yml`, `scripts/test-liquers-py.sh`
