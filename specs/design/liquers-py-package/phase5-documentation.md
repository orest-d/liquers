# Phase 5: Documentation - liquers-py-package

## Completion Preconditions

- [x] Implementation is finished and validated, except the release workflow's full five-platform
  run, which happens on its first manual run or tag (see Conformance)
- [x] All user comments are answered or incorporated
- [x] All review comments are answered or incorporated
- [x] Documentation is consistent with the implemented and tested behavior
- [x] Documentation is included in the implementation PR when practical (PR #240)

## Implementation Summary

Implemented as approved: Phase 4 Steps 1-7, commit `ce719aa`.

**Version and license.**
- One workspace version (0.1.0) and one license (`AGPL-3.0-or-later`) are inherited by every
  crate.
- `Environment::liquers_version()` is a provided trait method.
- The version is exposed as `Environment.liquers_version`, `lq.__version__` (PEP 440) and the
  JavaScript `liquersVersion`.

**The package.**
- The `abi3` feature and the `wheel` profile.
- PyPI metadata, with `LICENSE` and the generated third-party notices.
- The extras `polars`, `images`, `arrow`, `pandas` and `all`, and `lq.require`.

**The workflows.**
- `liquers-py-release.yml` builds the sdist and fifteen wheels, checks the release set, and
  tests every wheel in a fresh venv on its own runner. On a tag it publishes to GitHub, TestPyPI
  and then PyPI, and the PyPI step waits for the protected `pypi` environment.
- `liquers-py-package.yml` builds and tests the Linux abi3 wheel on pull requests.

**Supporting files.**
- `check-workspace-metadata.py` and `check-wheel.py`, with their tests.
- `allowed-libraries.toml`.
- The installed-wheel suite `tests/package/`.

## Documentation Delivered

### New Reference Documents
None. The contract is §12 of the Python reference.

### New Guide Documents
None.

### Existing Documents Reviewed or Updated
Each with a History row and `reviewed: 2026-11-01`:
- `specs/reference/PYTHON_INTEGRATION.md`: new §12 Packaging, with its open problems; §1 the
  `abi3` feature and `_extras.py`; §8 drops "no published wheel".
- `specs/guides/PYTHON_GUIDE.md` §1 Installing: pip and the extras first, from source second.
- `specs/reference/PROJECT_OVERVIEW.md`: one version, and `Environment::liquers_version`.
- `specs/reference/JAVASCRIPT_INTEGRATION.md`: `liquersVersion`.
- `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md`: Appendix A `PACKAGE04`; §6 Python covers the
  release route and what cost time (abi3t, extras without wheels).
- `liquers-py/README.md`: installing, platforms, the license, the quick-start markers, a new
  §Releasing, and the `PACKAGE` row.
- `CLAUDE.md`: the wheels line, written with the implementation.
- `specs/README.md`: the M5 row.

### Links and Capability Map
`specs/README.md`'s M5 row links PYTHON_INTEGRATION §12, PYTHON_GUIDE §1 and the release
workflow.

## Issues Filed

None new.
- `PY-NO-PUBLISHED-WHEELS` moves to `in_progress`. The pipeline exists, and the issue closes
  when 0.1.0 is on PyPI, which needs Orest: the trusted publishers, the tag, and the approval.
- `PY-ABI3T-WHEEL-NOT-USED` and `PY-PYPY-SUPPORT-UNVERIFIED` stay open.

## Important Learning

- An abi3 library does not load on a free-threaded interpreter, and maturin's abi3t wheel does
  not import yet. The free-threaded builds therefore each get their own wheel.
- An extra whose package has no wheel on a platform must be excluded by a marker, or pip builds
  pyarrow from source. No marker exists for free-threading.
- Publishing is the one irreversible step. The workflow keeps everything before it repeatable:
  manual runs publish nothing, TestPyPI is checked against the built files, and PyPI waits for a
  person.

## Conformance and Remaining Work

The implemented scope matches the approved one. Two things are still to be seen.
- **The full wheel matrix** (Windows arm64's `clang-cl`, the manylinux 3.15t interpreter) has not
  run yet. It runs on the first manual run of `liquers-py-release.yml`, which publishes nothing.
  The `PACKAGE` row is `PARTIAL` until it passes.
- **The first publication** is Orest's, and is tracked by `PY-NO-PUBLISHED-WHEELS`.

## Validation

- CI's `liquers-py-package` job builds and tests the Linux abi3 wheel.
- The `lock` job runs the workspace-metadata and workflow tests.
- liquers-web's Node suite covers `liquersVersion`.
- `check-build-matrix.sh` includes the `liquers-py --features abi3` row; every configuration
  passes.
- `docs_index.py --check`: clean after regeneration.
