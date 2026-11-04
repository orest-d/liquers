---
title: API Documentation Guide
kind: guide
audience: internal
area: [docs]
reviewed: 2026-11-03
---
# API Documentation Guide

What a module's API documentation must say, in Rust (`//!`), Python (module and class docstrings)
and JavaScript (JSDoc carried into the generated `.d.ts`); where each part is written in each
language; and when the design process asks for it. The rules are the owner's request in GitHub
#232 (design [`design/api-documentation-rules/`](../design/api-documentation-rules/DESIGN.md)).

The generated references are published on the documentation site
(<https://orest-d.github.io/liquers/>: `api/rust/<crate>/`, `api/python/`, `api/js/`), built by
`scripts/build-docs.sh`. This guide is about their *content*; the site build is
[`GITHUB_ACTIONS_GUIDE.md`](GITHUB_ACTIONS_GUIDE.md) §2.

## 1. Audience

A reader landing on a module from the site, an IDE or a search result, who may be a person or a
coding agent: they need to know what the module is for, which item to start from, where the module
is used, which neighbouring concept it is not, and where a working example is. The module doc is
written for that reader. Contributors changing the module read `specs/` as well; the module doc
does not restate `specs/`, it links it.

## 2. Terms

| This guide says | Rust | Python | JavaScript (`liquers-web`) |
|---|---|---|---|
| **package** | crate | package (`liquers`) | the module `liquers_web.js` with its `.d.ts` |
| **module** | module (`mod`) | module (`.py` file) | the package itself; there is one |
| **type** | struct, enum, trait, type alias | class, `Enum`, `Protocol` | class, interface, type alias |
| **function** | `fn` at module level | function | exported function |
| **method** | associated function or method | method, property | method, getter |
| **entry point** | the item a first use reaches for | the same | the same |
| **module doc** | the inner `//!` block | the module docstring | the `@packageDocumentation` comment |
| **item doc** | `///` | the docstring (PyO3 `///` becomes `__doc__`) | JSDoc (`///` on an exported item) |

## 3. The module doc

Five parts, in this order, under these headings (the purpose has no heading). Fixed headings let a
reader scan and let `scripts/check_api_docs.py` find them.

```text
<Purpose: one summary sentence, then at most one short paragraph.>

# Main components      the entry points: 2-6 items, one line each, linked
# Where it is used     3, at most 5, linked places, plus one sentence on the rest
# Related concepts     each: a link and how it differs from this module
# Examples             one executed example of 3-10 lines; then at most 5 links to longer
                       examples or tests, one line each, and where more are found
```

- **Purpose.** The first sentence is the summary line (rustdoc and pdoc show it in listings):
  what the module is for, in terms a user has. One more paragraph at most.
- **Main components.** The items a first use reaches for, not an inventory: the generated
  reference already lists every item. Name the usual entry even when it lives elsewhere
  (`register_command!` for `liquers_core::commands`).
- **Where it is used.** Typically three, at most five, linked places, chosen for diversity when use
  is widespread (a command library, a test, a binding, a server), then one sentence on the rest
  ("and every test that builds an environment").
- **Related concepts.** Each neighbour a reader could confuse with this module, with the difference
  in one sentence: the store and the asset manager both reach keyed data, the store gives raw data
  access, the asset manager builds assets and sends notifications.
- **Examples.** One high-level example, 3-10 lines, executed by the language's test loop (§6).
  Then up to five links to longer examples or tests, each with a one-line description, and a note
  where more are (`liquers-core/tests/`, a guide chapter).

**Which modules.** A module that is public and declares more than one public item carries all five
parts. A module with one public item, or a private module, needs only the purpose; it may carry the
rest. Extra sections (`# Errors`, `# Feature flags`, `# Target`, a topic such as `pipeline.rs`'s
*Who runs the handlers*) come after `# Related concepts`, before `# Examples`, or after them.

`liquers-docs/src/lib.rs` has this shape and is the shortest worked example; the three written for
this guide are `liquers-core/src/commands.rs`, `liquers-py/python/liquers/__init__.py` and
`liquers-web/src/lib.rs` with `typescript.rs`.

## 4. Entry-point item docs

An item named under `# Main components` carries an item doc with a summary line, the parameters or
fields a caller must understand, its errors, and a link back to the module doc's example when that
example uses it. Other items follow the language's ordinary convention; every public name has at
least a summary line (`DOCS04` in [`LANGUAGE-INTEGRATION_GUIDE.md`](LANGUAGE-INTEGRATION_GUIDE.md)
§DOCS for the bindings).

## 5. Links

| Target | Rust | Python | JavaScript |
|---|---|---|---|
| An item in the same package | intra-doc link: `` [`CommandRegistry`] ``, `` [`Self::new`] ``, `[text](crate::recipes)` | `` `liquers.Query` `` (pdoc links a qualified name in backticks) | `{@link Query}` |
| An item in another crate of the site | a relative link into the combined rustdoc tree, from the page's own depth: from `liquers_core/commands/index.html`, `../../liquers_lib/image/index.html`; or the path in a code span, `liquers_lib::image` | the Rust side is not linked; name it in a code span | the same |
| An example or test file | `https://github.com/orest-d/liquers/blob/main/<path>` (the checker verifies that `<path>` exists) | the same | the same |
| A `specs/` document | the repository path in a code span: `specs/reference/ASSETS.md` (the site does not publish `specs/`) | the same | the same |
| A user guide chapter | the site URL, `https://orest-d.github.io/liquers/guides/rust/05-commands.html` | the same, `guides/python/` | the same, `guides/web/` |

Rustdoc intra-doc links resolve in the scope of the module that carries the doc; a `pub mod x;`
line with its own outer `///` is documented in the *parent* scope, so write `crate::` paths there
or move the doc into the module (`RUSTDOC-STORE-SUBMODULE-LINKS-UNRESOLVED`). A broken intra-doc
link fails the rustdoc CI job (§10).

## 6. Examples

The executed example is the module's proof: it runs in the routine test loop, so it cannot rot.

| Language | Where | Runs under |
|---|---|---|
| Rust | a fenced block in the `//!` | `cargo test -p <crate> --doc` |
| Python | a `>>>` session in the docstring | `python3 -m pytest --doctest-modules <file>` (`scripts/test-liquers-py.sh`) |
| JavaScript | an `@example` block repeating a `JAVASCRIPT_GUIDE.md` example, named | `liquers-web/scripts/test-guide-examples.sh` runs the guide's copy |

An example that cannot run says why, in the block itself: a rustdoc block tagged `no_run` (compiles,
not run), `ignore` with a comment giving the reason, `compile_fail` for a deliberate error, or a
`text` block when the crate does not build natively (`liquers-web`). The 3-10 line limit counts
visible lines; rustdoc's hidden `# ` lines (the `fn main` wrapper, imports the point does not need)
are free.

## 7. Rust

- The module doc is the inner `//!` block at the top of the file, after inner attributes
  (`#![allow(…)]`) and before the first item. A crate root (`lib.rs`) documents the crate the same
  way; its `# Main components` are modules.
- Headings in `//!` are `#` (rustdoc renders them one level below the item). The five headings are
  spelled exactly as in §3.
- Doctests are compiled with the crate's dev-dependencies: `liquers-core`'s can use
  `liquers_macro::register_command` and a `tokio` runtime. Return `Result` from the hidden `main`
  so `?` works: `# fn main() -> Result<(), Box<dyn std::error::Error>> {` … `# Ok(()) }`.
- A `#[cfg(feature = …)]` module says so in a `# Feature flags` section. `cargo test --doc`
  builds with the default features, so a doctest of a default-on feature runs as usual; one that
  needs a non-default feature is tagged `ignore` with the reason, and the module links a
  `#[cfg(feature = …)]` test instead.

## 8. Python

- `liquers-py/python/liquers/__init__.py` is the package's module doc; each `_*.py` module has its
  own docstring, purpose-only when it exports nothing a user imports directly. Classes defined in
  Rust take their docstring from the PyO3 `///` comment (`#[pyclass]`, `#[pymethods]`), with
  `#[pyo3(text_signature = "…")]` for the signature; the stub `_native.pyi` repeats them.
- The docstring is Markdown, which pdoc renders. The headings are `# Main components` and so on;
  the leading text of the docstring is the purpose.
- The example is a `>>>` session, run by `pytest --doctest-modules` in
  `scripts/test-liquers-py.sh`; its output lines must match exactly, so prefer `True` from a
  predicate to printing a long value.
- `liquers-py/tests/test_DOCS_docstrings.py` already fails when a public name has no docstring.

## 9. JavaScript

- The module doc is a JSDoc comment tagged `@packageDocumentation`, in a
  `#[wasm_bindgen(typescript_custom_section)]` constant in `liquers-web/src/typescript.rs`;
  wasm-bindgen writes custom sections at the top of `liquers_web.d.ts`, and TypeDoc takes the first
  such comment of the entry file as the module page. The crate's `//!` in `lib.rs` carries the same
  five parts for the *Rust* reader (that page is on the site too) and never reaches the `.d.ts`.
- `///` on a `#[wasm_bindgen]` item becomes its JSDoc. wasm-bindgen copies the text verbatim: a Rust
  intra-doc link renders as literal brackets, so item docs in `liquers-web` link with `{@link Name}`
  and plain code spans. Docs on items the crate does not export never appear.
- The `@example` repeats, verbatim, a `JAVASCRIPT_GUIDE.md` example that
  `scripts/test-guide-examples.sh` runs, and says which (the example block is the one executed;
  the copy is kept identical by review).

## 10. Checking

| Check | Command | Fails when |
|---|---|---|
| Sections and limits | `python3 scripts/check_api_docs.py [PATH…]` | a GitHub `blob/main` link points at a path that does not exist; with `--strict`, on any warning |
| Rust links | `RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links" cargo doc --no-deps -p <crate>` | an intra-doc link does not resolve (the `rustdoc` job of `build-matrix.yml`, on the crates that are clean) |
| Rust examples | `cargo test -p <crate> --doc` | a doctest fails |
| Python examples, docstrings | `bash scripts/test-liquers-py.sh` | the `>>>` session or `test_DOCS_docstrings.py` fails |
| JavaScript examples | `liquers-web/scripts/test-guide-examples.sh` | a guide example fails |

`check_api_docs.py` reports a missing module doc, a missing or misordered heading, more than five
links under `# Where it is used` or `# Examples`, and an example missing or outside 3-10 lines as
**warnings**; `docs-check.yml` runs it on every pull request that touches a source file, and the
list is long until `MODULE-DOCS-DO-NOT-FOLLOW-THE-API-DOC-RULES` closes, when the warnings become
errors. Run it on the files you touched before pushing.

## 11. When to write it: the design phases

The `liquers-project` skill asks for API docs in four places, and `validate_phase.py` refuses a
Phase 1 that skips the first:

| Phase | Where | What |
|---|---|---|
| 1 | `## Documentation Intent`, line `- API docs:` | the modules whose module doc is created or changed, or `none` with the reason |
| 2 | `## Documentation Architecture` | one row per module: which of the five parts change |
| 4 | `## Documentation Updates` | the module docs, and the check commands of §10 to run |
| 5 | `### Existing Documents Reviewed or Updated` | each module doc reviewed against §3, and the checker's output for those files |

A design that touches a module brings that module's doc up to §3 even when the design's own change
is small; the rest of the backlog is `MODULE-DOCS-DO-NOT-FOLLOW-THE-API-DOC-RULES`. Outside a
design, a documentation fix to a module doc is eligible for automatic fixing
(`.claude/skills/liquers-project/references/auto-fix.md`, rule 2).

## History

| Date | Change | Source |
|---|---|---|
| 2026-11-03 | Created. | `design/api-documentation-rules/` |
