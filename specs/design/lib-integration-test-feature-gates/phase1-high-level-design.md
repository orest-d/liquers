# Phase 1: Feature-Gate liquers-lib Integration Tests

## Design Readiness

- **Readiness:** ready
- **Leading issue:** None
- **Explanation:** The landed gates follow crate features, retain applicable tests in reduced
  builds, and the source issue records a six-configuration green matrix.
- **Open questions:** None

## Problem, Behaviour, and Acceptance

Integration targets imported optional Polars/egui dependencies unconditionally, so reduced-feature
test builds failed before running. Every target/test must compile only when its dependency and
semantic fixture exist, while feature-independent tests continue to run. The matrix must include
test targets so future leaks are detected.

## Scope and Compatibility

Scope is `liquers-lib/tests`, `scripts/check-build-matrix.sh`, and contributor build guidance. No
library API or runtime data changes. Over-gating that hides valid reduced-build coverage is the
principal risk.

## Design Dependencies

- `overlaps` `value-type-system`: its validation exposed the defect, but the feature-gate repair
  landed independently after that design.

## Documentation Assessment

`CLAUDE.md` and the matrix script document contributor commands; no current-state runtime reference
changes were required.

## Consolidated Findings

Use file-level gates only for wholly optional suites and item-level gates for mixed suites. Replace
default-build count assumptions with per-feature anchor assertions. Add `--tests` and
`image-support` to native matrix rows while retaining wasm as library-only due native dev
dependencies.
