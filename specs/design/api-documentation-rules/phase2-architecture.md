# Phase 2: Solution & Architecture — API documentation rules and the user documentation folder

## Overview

One internal guide, `specs/guides/API_DOCUMENTATION_GUIDE.md`, defines the module doc with fixed
headings in every language. A standard-library Python script, `scripts/check_api_docs.py`, reports
modules that miss them. A rustdoc lint job fails on broken intra-doc links. The `liquers-project`
templates, checklist and `validate_phase.py` ask for API docs in Phases 1, 2, 4 and 5. `docs/` is
created with its sources, homepage and API landing pages. Three modules, one per language, are
rewritten as the worked examples.

Rejected: putting the rules in `DOCS_STRUCTURE_GUIDE.md` (it governs `specs/`, not code); one
guide per language (three copies of the same rules drift); `#![warn(missing_docs)]` alone (it
checks that a doc exists, not what it says, and it warns on every item, not only on entry points).
It may still be added to crate roots later.

**Assumptions taken at the Phase 1 gate** (`proceed` on 2026-11-01, questions not answered). These
are the recommended answers, open to change at this gate:
Q1 fixed headings; Q2 GitHub links pinned to `main`; Q3 relative links into the combined rustdoc
output; Q4 sources in `docs/`, built site in `target/docs-site/`; Q5 a missing section warns and a
broken link fails. Q6 is **revised**, see Risks: the guides stay in `specs/` for now. Each language
gets a short getting-started page instead.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `RUSTDOC-STORE-SUBMODULE-LINKS-UNRESOLVED` | draft | P3 | 26 broken intra-doc links in `liquers-core`; the lint job would fail on them | yes | blocks Step 6 only | Fix in its own PR (`S`, looks eligible for automatic fixing) before Step 6 |
| `PY-API-REFERENCE-NOT-GENERATED` | draft, needs-decision | P2 | Nothing to place under `api/python/` yet | no | no | `api/python/index.md` says the reference is not built yet |
| `WEB-API-REFERENCE-NOT-GENERATED` | draft | P3 | The same for `api/javascript/` | no | no | Same |
| `MODULE-DOCS-DO-NOT-FOLLOW-THE-API-DOC-RULES` | draft | P3 | The check's warning list starts long (85 files have no `//!`) | no | no | The follow-up; the warning becomes a failure when it closes |

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

A module needs all five parts when it is public and declares more than one public item.
Otherwise the purpose alone is enough. Extra sections (`# Errors`, `# Feature flags`, or a topic
like `pipeline.rs`'s `# Who runs the handlers`) go after `# Related concepts`.

**`scripts/check_api_docs.py`** (Python 3.10+, standard library only, like `docs_index.py`):

```text
check_api_docs.py [--crate NAME]… [--strict] [--format text|json] [PATH]…
exit 0: no errors (warnings allowed) · 1: an error, or a warning under --strict · 2: usage
```

- **Rust:** it walks each crate from `src/lib.rs` through `mod` declarations. A module is public
  when every declaration on its path is `pub mod`. It reads the inner `//!` lines.
- **Python:** it reads the module docstrings of `liquers-py/python/liquers/*.py` with `ast`.
- **JavaScript:** it reads the `//!` of `liquers-web/src/lib.rs` and the `@packageDocumentation`
  block in `typescript.rs`.
- **Warnings:** no doc; a missing heading or headings out of order; more than 5 links under
  `# Where it is used` or more than 5 example links; no example, or an example outside 3-10
  lines (Rust `# ` hidden lines are not counted).
- **Errors:** a link `https://github.com/orest-d/liquers/blob/main/<path>` whose `<path>` does
  not exist locally.

## Integration Points

- `specs/guides/API_DOCUMENTATION_GUIDE.md` (new). Its sections:
  1. Audience.
  2. Terms: crate, module or package; struct, enum or trait versus class, `Enum`, `Protocol`,
     class and interface; function; method.
  3. The module doc, its five parts and their limits.
  4. Entry-point item docs.
  5. Links: in-crate, cross-crate, examples, `specs/`.
  6. Examples: executed, and which are not.
  7. Rust (`//!`, intra-doc links, doctest attributes `no_run`, `ignore` with a reason,
     `compile_fail`).
  8. Python: PyO3 `///` becomes `__doc__`, the package docstring, `>>>` examples run by pytest.
  9. JavaScript: `///` becomes JSDoc in the `.d.ts`, `@packageDocumentation`, `@example`, and
     what wasm-bindgen drops.
  10. Checking.
  11. When to write it: the design phases.
- **Worked examples:**
  - `liquers-core/src/commands.rs` gets a `//!`, with a doctest that registers and runs one
    command.
  - `liquers-py/python/liquers/__init__.py` gets the five parts in its docstring, and
    `scripts/test-liquers-py.sh` adds `pytest --doctest-modules python/liquers/__init__.py`.
  - `liquers-web/src/lib.rs` gets a `//!`, and `typescript.rs` gets a `typescript_custom_section`
    with `@packageDocumentation`. Its `@example` repeats a `JAVASCRIPT_GUIDE.md` example that
    `test-guide-examples.sh` already runs, and names that example.
- **`liquers-project` skill:**
  - `phase1-template.md`: an `- API docs:` line under Documentation Intent, naming the modules whose module doc
    is created or changed, or `none` with the reason.
  - `phase2-template.md`: Documentation Architecture lists each module and the parts that change.
  - `phase4-template.md`: Documentation Updates names the module docs and the check run.
  - `phase5-documentation.md`: records each module doc reviewed against the guide, and the
    check's output for those files.
  - `compact-design-template.md`: the same four.
  - `review-checklist.md`: one item each for Phases 1, 2 and 5.
  - `auto-fix.md` rule 2: module docs count as documentation.
  - `SKILL.md`: the phases table.
  - `validate_phase.py`: in Phase 1, Documentation Intent needs an `API docs:` line when
    `DESIGN.md` `created:` is on or after the date this lands, so existing designs still pass. Its
    tests are in `test_validate_phase.py`.
- **`docs/`** (new):
  - `README.md`: layout and rules.
  - `site.yaml`: title, guides folder, packages per language with their built paths.
  - `index.md`: homepage.
  - `getting-started/{rust,python,javascript}.md`: each one page, its example copied from a named
    test that runs it.
  - `guides/index.md`.
  - `concepts/index.md` and `reference/index.md`: stubs that say what will go there.
  - `api/index.md` and `api/{rust,python,javascript}/index.md`.
  - `templates/page.html`.
  - `assets/site.css`.
  - The Mustache in these files uses only `ns-template` and the `ns-docs` commands from
    `documentation-site-tool`. Until that design lands, the files are plain Markdown and no build
    exists.
- **`scripts/docs_index.py`:** check 9 (dead links) also scans `docs/**/*.md`. The new check 18
  refuses a relative link from `docs/` into `specs/`. Tests are in `test_docs_index.py`.
- **CI:**
  - `docs-check.yml` gains `check_api_docs.py`, warnings only, and the `docs/**` and `**/*.rs`
    paths.
  - A new `rustdoc` job runs `RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links" cargo doc
    --no-deps` for `liquers-core`, `-template`, `-records`, `-store`, `-mcp`. Those build without
    optional native libraries. `liquers-lib` follows once its links are clean.

## Error Handling

No library code changes, so no `Error` constructors are involved. The scripts report problems as
`path:line: warning|error: message` on stdout in text mode (`docs_index.py`'s form), or as a JSON
list, and exit as above.

## Relevant Commands

None. `ns-docs` belongs to `documentation-site-tool`.

## Documentation Architecture

| Path | Kind | Audience | Area | Change |
|---|---|---|---|---|
| `guides/API_DOCUMENTATION_GUIDE.md` | guide | internal | docs | new |
| `DOCS_STRUCTURE_GUIDE.md` | contract | internal | docs | §2 `docs/` exists; §7.2 check 18; §9.6 rewritten (what `docs/` holds, built site location); §10 rows; History |
| `guides/LANGUAGE-INTEGRATION_GUIDE.md` | guide | internal | docs | §DOCS: module docs follow the guide; `DOCS04` output goes under `api/<language>/`; the user guide moves to `docs/guides/` when it moves; History |
| `CLAUDE.md` | — | — | docs | Structure lists `docs/`; Documentation points to the guide for any module doc |
| `liquers-py/README.md`, `liquers-web/README.md` | — | user | py, web | one line naming the guide |
| `docs/README.md` | — | internal | docs | new |
| `specs/README.md` | map | internal | docs | the guide under "When to use what"; generated blocks |

Proposed `affects_docs`: `guides/LANGUAGE-INTEGRATION_GUIDE.md`, `DOCS_STRUCTURE_GUIDE.md`.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | About 25: the guide, 9 skill files, 2 scripts and their tests, 2 workflows, ~14 new `docs/` files, 3 module docs, 4 specs documents |
| Crates and workflows affected | Doc comments in `liquers-core`, `liquers-web`; `liquers-py` package docstring and test script; `docs-check.yml`, a new rustdoc job |
| Existing tests likely to change | `test_validate_phase.py`, `test_docs_index.py` (new cases); none in Rust |
| New validation | The doctest in `commands.rs`, the Python doctest, the check script and its tests, check 18, the rustdoc job |
| Compatibility / data / concurrency / performance / security | None at runtime. `validate_phase.py`'s new rule is date-gated, so existing designs keep passing |
| Recovery | Revert per step. The warnings never fail CI, and the rustdoc job can be removed alone |
| Certainty and open questions | High for the rules and the process. **Q6 revised:** moving `PYTHON_GUIDE.md` and `JAVASCRIPT_GUIDE.md` means rewriting 17 links into `specs/reference/` (check 18 forbids them) and paths in `liquers-py/tests/test_GUIDE_examples.py`, `liquers-web/scripts/test-guide-examples.sh`, `run-guide-examples.mjs` and 7 `liquers-py/src` files. The guides also lean on their contract references, which are internal. Moving them belongs with moving those references, so it is a separate issue, `USER-GUIDES-LIVE-IN-SPECS`. |
