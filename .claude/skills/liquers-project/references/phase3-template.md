# Phase 3 template: examples and tests

**Proof.** Three pages at most. Tests are runnable by default; use conceptual code only where
execution is impossible, and say why. Every Phase 1 scenario (`AC-<n>`) is cited by at least one test, and no test cites an undefined one
(`validate_phase.py` and `docs_index.py --check` enforce both when Phase 1 defines scenarios).
Validate every query with `liquers-validate`. File: `phase3-examples.md`.

```markdown
# Phase 3: Examples & Use-cases — <name>

## Overview Table
| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | <primary scenario> | | AC-1 |
| 2 | Test | `<crate>::<module>::tests::<test_name>` | | AC-1, AC-2 |

## Example 1: <primary scenario>
<Context in two sentences; the steps through the components; the core code only (no incidental
setup); the expected result. Medium complexity, defaults unless a non-default is the point.>

## Example 2: <secondary scenario>
<What it adds beyond Example 1; show only the delta.>

## Edge and Error Cases
<Only those Phase 2's risks make relevant: errors, empty or large input, concurrency, serialization
round-trip, persistence, bindings, feature flags. Symptom → expected behaviour → test.>

## Test Plan
- Unit: `<file>` — `<test_name>` (AC-1): <what it asserts>
- Integration: `<tests/file.rs>` — `<test_name>` (AC-2): <what it asserts>
- Command to run: `<cargo test ...>`

## Learning Log
<Guide-worthy workflows and snippets (with the test or example that proves them), corrected
assumptions, surprises. Phase 5 draws on this.>
```

Use `liquers-unittest` for test structure and `specs/guides/UNITTEST_GUIDE.md` for conventions.
