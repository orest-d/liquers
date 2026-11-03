# Phase 2: Solution & Architecture — API documentation rules

## Overview

One internal guide, `specs/guides/API_DOCUMENTATION_GUIDE.md`, defines the module doc with fixed
headings in every language; `scripts/check_api_docs.py` reports modules that miss them; a rustdoc
job fails on broken intra-doc links; the `liquers-project` skill asks for API docs in Phases 1, 2,
4 and 5; three modules, one per language, are the worked examples.

Rejected: the rules in `DOCS_STRUCTURE_GUIDE.md` (it governs `specs/`, not code); one guide per
language (three copies drift); `#![warn(missing_docs)]` alone (it checks that a doc exists, not
what it says, on every item rather than the entry points).

**Amended 2026-11-03.** The user documentation folder (request item 4, AC-6) was delivered by
[`user-documentation-guides`](../user-documentation-guides/DESIGN.md): `docs/` holds the home
page, three guides and a theme; the site build (`scripts/build-docs.sh`, `docs-site.yml`) renders
the API reference of every crate, the Python package and the JavaScript module into the deployed
site only, each linked from `docs/index.md` (`liquers-docs check` fails on a missing crate). This
design adds **nothing under `docs/`**: its original `docs/` tree is dropped, and Phase 1's
questions 4 and 6 are closed by that design. The other Phase 1 gate assumptions stand:
Q1 fixed headings; Q2 GitHub links pinned to `main` for examples and tests; Q3 relative links into
the combined rustdoc tree, which the site's `api/rust/` now is; Q5 a missing section warns, a
broken link fails.

## Known-Issue Preflight

At `d8bb375`, `RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links" cargo doc --no-deps` reports 26
unresolved links in `liquers-core` and 3 in `liquers-records`; `-template`, `-store`, `-mcp`,
`-markdown` and `-docs` are clean.

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `RUSTDOC-STORE-SUBMODULE-LINKS-UNRESOLVED` | draft | P3 | 29 broken links (core, records); the lint job would fail on them | yes | blocks the rustdoc job only | Fix in its own PR (`S`, eligible for automatic fixing) before that step; the job starts with the clean crates either way |
| `MODULE-DOCS-DO-NOT-FOLLOW-THE-API-DOC-RULES` | draft | P3 | The check's warning list starts long (85 files have no `//!`) | no | no | The follow-up; the warning becomes a failure when it closes |
| `WEB-API-REFERENCE-NOT-GENERATED` | draft | P3 | None: the TypeDoc reference now builds; only its per-export check is missing | no | no | Narrow the issue to that check when it is next touched |
| `RUSTDOC-WASM-CRATE-NOT-MERGED-INTO-API-TREE` | draft | P3 | `liquers-web`'s rustdoc is copied into the tree, so Q3's relative links from it work | no | no | None |

## Interfaces

No Rust API changes.

**The module doc**, in every language, as Markdown:

```text
<Purpose: one summary sentence, then at most a short paragraph.>

# Main components      entry points: 2-6 items, one line each, linked
# Where it is used     3, at most 5, linked places + one sentence on the rest
# Related concepts     each: link, and how it differs from this module
# Examples             one 3-10 line executed example; then at most 5 links, one line each,
                       and where more are found
```

All five parts when the module is public and declares more than one public item; otherwise the
purpose alone. Extra sections (`# Errors`, `# Feature flags`, `# Target`) go after
`# Related concepts`. `liquers-docs/src/lib.rs` already has this shape; the guide formalizes it.

**`scripts/check_api_docs.py`** (Python 3.10+, standard library only, like `docs_index.py`):

```text
check_api_docs.py [--crate NAME]… [--strict] [--format text|json] [PATH]…
exit 0: no errors (warnings allowed) · 1: an error, or a warning under --strict · 2: usage
```

- **Sources:** Rust: each crate from `src/lib.rs` through its `mod` declarations, a module being
  public when every declaration on its path is `pub mod`; the inner `//!` lines. Python: the
  module docstrings of `liquers-py/python/liquers/*.py`, read with `ast`. JavaScript: the `//!`
  of `liquers-web/src/lib.rs` and the `@packageDocumentation` block in `typescript.rs`.
- **Warnings:** no doc; a heading missing or out of order; more than 5 links under `# Where it
  is used` or more than 5 example links; no example, or one outside 3-10 lines (Rust `# ` hidden
  lines not counted). **Errors:** a `github.com/orest-d/liquers/blob/main/<path>` link whose
  `<path>` does not exist locally.

## Integration Points

- `specs/guides/API_DOCUMENTATION_GUIDE.md` (new): audience; terms (crate, module or package;
  struct, enum or trait versus class, `Enum`, `Protocol`, interface; function; method); the five
  parts and their limits; entry-point item docs; links (in-crate, cross-crate through the site's
  `api/rust/` tree, examples and tests on GitHub, `specs/`); executed examples; a Rust, a Python
  and a JavaScript section (`//!` and doctest attributes; PyO3 `///` as `__doc__` and `>>>`
  examples under pytest; `///` as JSDoc in the `.d.ts`, `@packageDocumentation`, `@example`);
  checking; when to write it: the design phases.
- **Worked examples (AC-3):**
  - `liquers-core/src/commands.rs` gets a `//!` with a doctest that registers and runs one command.
  - `liquers-py/python/liquers/__init__.py` gets the five parts in its docstring;
    `scripts/test-liquers-py.sh` adds `pytest --doctest-modules liquers-py/python/liquers/__init__.py`.
  - `liquers-web/src/lib.rs` gets the five parts, keeping `# Target` and `# Architecture` after
    them; `typescript.rs` gets a `typescript_custom_section` with `@packageDocumentation`, whose
    `@example` names and repeats a `JAVASCRIPT_GUIDE.md` example `test-guide-examples.sh` runs.
- **`liquers-project` skill:** `phase1-template.md` adds an `- API docs:` line under
  Documentation Intent (modules whose module doc is created or changed, or `none` with the
  reason); `phase2-template.md` lists each module and the parts that change; `phase4-template.md`
  names the module docs and the check run; `phase5-documentation.md` records each module doc
  reviewed against the guide and the check's output; `compact-design-template.md` the same four;
  `review-checklist.md` one item for Phases 1, 2 and 5 each; `auto-fix.md` rule 2; `SKILL.md`'s
  phases table. `validate_phase.py` requires the line when `DESIGN.md` `created:` is on or after
  the landing date (as `TEMPLATE_REWRITE`); tests in `test_validate_phase.py`.
- **CI:** `docs-check.yml` gains a `check_api_docs.py` step (warnings only) and the
  `liquers-*/src/**` and `liquers-py/python/**` paths. `build-matrix.yml`, already run on
  `liquers-*/**`, gains a `rustdoc` job: `RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links"
  cargo doc --no-deps` for `liquers-template`, `-store`, `-mcp` and `-markdown` at once (no
  optional native library, no `liquers-lib`); `-core` and `-records` after the link fix; `-lib`
  and `-docs` once `-lib`'s links are clean. `docs_index.py` is unchanged: checks 18 and 19
  already cover `docs/`.

## Error Handling

No library code changes, so no `Error` constructors. The script reports
`path:line: warning|error: message` on stdout (`docs_index.py`'s form) or a JSON list, and exits
as above.

## Relevant Commands

None.

## Documentation Architecture

| Path | Kind | Audience | Area | Change |
|---|---|---|---|---|
| `guides/API_DOCUMENTATION_GUIDE.md` | guide | internal | docs | new |
| `DOCS_STRUCTURE_GUIDE.md` | contract | internal | docs | §9.3: a landing design reviews the module docs it touches against the guide; §10 row; History |
| `guides/LANGUAGE-INTEGRATION_GUIDE.md` | guide | internal | docs | §DOCS: module docs follow the guide, `DOCS04` names it; History |
| `CLAUDE.md` | — | — | docs | Documentation section points to the guide for any module doc |
| `liquers-py/README.md`, `liquers-web/README.md` | — | user | py, web | the `DOCS` row names the guide |
| `specs/README.md` | map | internal | docs | "When to use what" row; capability line to built; generated blocks |

API docs: `liquers_core::commands`, the `liquers` package, `liquers_web` (above).
Proposed `affects_docs`: `guides/LANGUAGE-INTEGRATION_GUIDE.md`, `DOCS_STRUCTURE_GUIDE.md`.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | About 22: the guide, 9 skill files, the script and its test, 2 workflows, `test-liquers-py.sh`, 4 source files, 6 specs and README documents |
| Crates and workflows affected | Doc comments in `liquers-core`, `liquers-web`; the `liquers-py` package docstring; `docs-check.yml`, `build-matrix.yml`, `liquers-py-tests.yml` through the test script |
| Existing tests likely to change | `test_validate_phase.py` (new cases); none in Rust |
| New validation | The `commands.rs` doctest, the Python doctest, `check_api_docs.py` and its tests, the rustdoc job |
| Compatibility / data / concurrency / performance / security | None at runtime. The `validate_phase.py` rule is date-gated, so existing designs keep passing. `@packageDocumentation` must be the first comment of the `.d.ts`; wasm-bindgen emits custom sections first, confirmed at Phase 3 |
| Recovery | Revert per step. The warnings never fail CI; the rustdoc job can be removed alone |
| Certainty and open questions | High. One decision left: whether the `commands.rs` doctest builds a whole environment (8-10 lines) or shows registration and one call through `CommandExecutor`; recommended the latter, with `async_hellow_world.rs` linked for the whole flow |
