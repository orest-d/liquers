# Phase 3: Examples & Use-cases — User guides, their formats, publishing and CI

**Gate assumptions (`proceed`, 2026-11-01).** Phase 2's two questions were left unanswered, so
the recommended answers apply:
- `documentation-site-tool` is merged into this design (see `DESIGN.md`).
- Appendix B is generated from the committed registry.

**Checked against the code for this phase:**
- **Relative partials.** `{{> ./x.md}}` resolves against the `cwd` link
  (`liquers-lib/tests/template_commands.rs` `relative_partial_uses_cwd_link`). The `..` form is
  asserted by a new test below; if it fails, the shared fragments are named by absolute key
  (`{{> /docs/guides/_shared/x.md}}`).
- **Section reads.** `ns-mem/read`'s section (`liquers-lib/src/search/md_split.rs`
  `find_section`) returns the heading line and stops at the next heading of *any* level. It
  drops subsections, and it needs the full heading text (`1. Installing`).
  - That is wrong for an include, so includes use a new `liquers-docs` command,
    `ns-docs/include`. It returns the section's body with its subsections, without its own
    heading. It matches the heading with or without its number, and fails on a missing section
    or a relative link.
  - The `ns-mem` behaviour itself is right for memory reads and stays.

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | Writing a chapter in a PR | skeleton → draft chapter with an include → PR build artifact | AC-1, AC-3, AC-5, AC-7 |
| 2 | Example | Releasing on `main` | release build, markers stripped, Pages deploy | AC-2, AC-6, AC-7 |
| 3 | Test | `liquers-docs/tests/skeleton.rs::guides_are_parallel` | the three `guide.yaml` list the same chapter files; every chapter is `outline` with `PURPOSE:` and `TBD:` | AC-1 |
| 4 | Test | `liquers-docs::markers::tests::release_fails_on_open_markers_in_ready` | `TBD:` / `CHECK:` in a `ready` chapter is an error in release, a warning in draft | AC-2 |
| 5 | Test | `liquers-docs::markers::tests::release_strips_writer_markers` | `PURPOSE:`, `SOURCE:`, `EDITOR:` gone in release, kept in draft | AC-2 |
| 6 | Test | `liquers-docs::markers::tests::release_replaces_unready_chapter` | a `draft` chapter becomes its title + "In preparation" | AC-2 |
| 7 | Test | `liquers-docs::include::tests::section_body_with_subsections` | heading dropped, subsections kept, number optional | AC-5 |
| 8 | Test | `liquers-docs::include::tests::missing_section_and_relative_link_fail` | both are errors naming the source | AC-5 |
| 9 | Test | `liquers-docs/tests/expand.rs::chapter_expands_partials_and_includes` | a fixture chapter with `{{> ../_shared/q.md}}` and `ns-docs/include` expands; the include is a recorded dependency | AC-5 |
| 10 | Test | `liquers-docs::neutral::tests::renderer_syntax_is_refused` | `{{#include}}`, `::: x`, `{.c}`, `@ref`, a MyST role are findings | AC-3 |
| 11 | Test | `liquers-docs/tests/build.rs::build_tree_feeds_both_renderers` | `SUMMARY.md` follows `guide.yaml`, appendices after a separator; `book.toml` has search on; `<guide>.md` has the chapters in order with a title block | AC-6 |
| 12 | Test | `scripts/test_docs_index.py::ChapterTests` | chapters indexed as `chapter`; `reviewed:` without a `HISTORY.md` row warns; a `docs/` link into `specs/` is an error; `chapters-for <spec>` lists chapters by `sources` | AC-4 |
| 13 | Test | `scripts/test_docs_index.py::ActionsGuideTests` | every `.github/workflows/*.yml` is named in `GITHUB_ACTIONS_GUIDE.md` (warning otherwise) | AC-8 |
| 14 | CI | `docs-site.yml` job `build`, step `verify outputs` | per guide: `index.html`, `searchindex.js`, offline zip without it, `.epub`, `.pdf`; `/api/rust/liquers_core/index.html` | AC-6, AC-7 |
| 15 | CI | `docs-site.yml` job `deploy` on `main` | `deploy-pages` reports the page URL; the homepage answers 200 | AC-7 |

## Example 1: writing a chapter in a PR

An editor drafts *Liquers for Python*, chapter 5. The skeleton's `TBD:` lines name the sections
and sources. The editor writes 5.1 by hand. For 5.2 the defaulting rules must match the shared
reference word for word, so the editor includes it rather than copying:

```markdown
---
title: Commands
status: draft
sources: [guides/PYTHON_GUIDE.md, reference/COMMAND_DECLARATION.md]
reviewed:
---
# 5. Commands

A Python function becomes a command with `@command` …   (hand-written)

## 5.2 Defaults

{{$ -R/specs/reference/COMMAND_DECLARATION.md/-/ns-docs/include-Argument~.defaults}}

TBD: 5.3 Namespaces and realms.
```

`liquers-docs check` passes (draft status allows `TBD:`). The PR's `docs-site` run uploads
`docs-site.zip`. In it, `guides/python/05-commands.html` shows the included rules and the `TBD:`
box. The run's log lists the chapter in the marker report. Validation: the include query parses
(`liquers-validate --command docs/include`, as `ns-docs` commands are not in the committed
registry). `~.` is a space (corrected in Phase 4: `~_` decodes to `-`), and the heading matches `### 4.3 Argument defaults` without its
number.

## Example 2: releasing on `main`

The PR merges with chapter 5 still `draft`. On `main`, `build-docs.sh --release` runs:
- chapter 5 is published as "5. Commands — in preparation";
- chapters 1-2 are `ready`, with their `PURPOSE:` lines stripped;
- `deploy-pages` publishes the site to `https://orest-d.github.io/liquers/`.

`/downloads/` holds `liquers-for-python.epub`, `.pdf` (Typst) and `-offline.zip`. Then someone sets
chapter 5 to `ready` with one `TBD:` left. The PR's draft build passes, so the editor sees the box,
but the `main` release build fails, naming `python/05-commands.md` and the line. To avoid that
surprise, the PR job also runs `liquers-docs check --release`.

## Edge and Error Cases

| Case | Expected | Test |
|---|---|---|
| A `specs/` heading an include names is renamed | release and draft builds fail naming the chapter and the missing section; the specs PR that renamed it is the one that fails, since `docs-site.yml` also triggers on `specs/**` | 8, 14 |
| An included section gains a relative link | error naming the specs file and the link | 8 |
| `mdbook`, `pandoc` or `typst` absent locally | step skipped with a notice; `--release` fails instead | 14 (CI always has them) |
| Pages not enabled | the `deploy` job fails at `configure-pages` with GitHub's message; the guide's setup section names the setting | 15 |
| A chapter file not in `guide.yaml`, or listed but missing | warning / error | 3, 11 |
| Python and Web examples | outlines carry only chapter 1's hello-world, copied from tests that already run (`test_GUIDE_examples.py`, `run-guide-examples.mjs`); running chapter examples directly is a later step | — |

## Test Plan

- Unit (`liquers-docs/src/{markers,include,neutral,build}.rs`): tests 4-8, 10 (AC-2, AC-3, AC-5).
- Integration (`liquers-docs/tests/`): `skeleton.rs` over the real `docs/guides/` (AC-1);
  `expand.rs` over a memory-store fixture with an `AsyncMemoryStore`, `register_all_commands!`
  and `register_docs_commands` (AC-5); `build.rs` writing into a temp folder (AC-6).
- Python: `scripts/test_docs_index.py` `ChapterTests`, `ActionsGuideTests` (AC-4, AC-8).
- CI: `docs-site.yml` `build` (PR and `main`) and `deploy` (`main`) (AC-6, AC-7).
- Commands: `cargo test -p liquers-docs`; `python3 -m unittest -v scripts/test_docs_index.py`;
  `bash scripts/build-docs.sh` locally with the three tools installed.

## Learning Log

- `ns-mem/read`'s section is a *reading* unit (one heading's own text), not an *include* unit (a
  heading's subtree). The two differ, and documentation needs the second.
- `ns-docs` commands are not in `specs/command_registry.yaml` (it exports `liquers-lib` only), so a
  chapter's queries validate with `--command`, or with `liquers-docs check`, which uses the real
  registry.
- Phase 4: in a query argument a space is `~.`, not `~_` (which decodes to `-`). A clean
  `liquers-validate` result did not show this; the expansion test did. An ad-hoc query has no
  working directory, so `liquers-docs` passes the chapter's folder as `expand`'s `cwd` argument;
  `{{> ../_shared/x.md}}` then resolves, and the include is a recorded dependency.
