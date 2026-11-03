# Phase 3: Examples & Use-cases — api-documentation-rules

## Overview Table
| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | `liquers_core::commands` module doc | the five parts in Rust; the doctest registers a command and evaluates it | AC-1, AC-3 |
| 2 | Example | `liquers` package docstring | the five parts in Python; a `>>>` example run by pytest | AC-2, AC-3 |
| 3 | Example | `liquers_web` module doc and `@packageDocumentation` | the five parts in JavaScript; the `@example` is the guide's first query | AC-2, AC-3 |
| 4 | Test | `scripts/test_check_api_docs.py` | the checker's rules on fixtures and on the repository | AC-1, AC-2, AC-3, AC-5 |
| 5 | Test | `test_validate_phase.py` `ApiDocsLineTests` | Phase 1 needs the `API docs:` line, date-gated | AC-4 |
| 6 | Test | the rustdoc job command | a broken intra-doc link fails | AC-5 |
| 7 | Existing | `build::tests::workspace_members_reads_one_line_and_many`, `test_links_into_specs_are_errors` | `docs/` exists, links every crate's API and never links into `specs/` | AC-6 |

## Example 1: the `liquers_core::commands` module doc

`liquers-core/src/commands.rs` has no `//!`. It gets the Phase 2 shape: purpose (one sentence,
one paragraph), `# Main components` (`CommandRegistry`, `CommandExecutor`, `CommandArguments`,
`register_command!` as the usual entry), `# Where it is used` (the interpreter, `liquers-lib`'s
`commands.rs`, `image/`, `records/`, and "every test that builds an environment"), `# Related
concepts` (`command_metadata`: describes without running; `recipes`: a query per key, of which a
command is one step; `plan`: the resolved steps), `# Examples`. The doctest, eight visible lines:

```rust
use liquers_core::{context::SimpleEnvironment, error::Error, interpreter::evaluate};
use liquers_core::{state::State, value::Value};
use liquers_macro::register_command;
fn hello(_state: &State<Value>) -> Result<Value, Error> { Ok(Value::from("Hello, world!")) }
let mut env = SimpleEnvironment::<Value>::new();
let cr = &mut env.command_registry;
register_command!(cr, fn hello(state) -> result)?;
let state = tokio::runtime::Runtime::new()?.block_on(evaluate(env.to_ref(), "hello", None))?;
assert_eq!(state.try_into_string()?, "Hello, world!");
```

`liquers-macro` and `tokio` are dev-dependencies of `liquers-core`, which doctests may use. The
hidden lines (`# fn main() -> Result<(), Box<dyn std::error::Error>> {` … `# Ok(()) }`) are not
counted. The linked longer examples: `liquers-core/tests/async_hellow_world.rs` (registration
through evaluation), `liquers-lib/src/commands.rs` (a command library), and
`specs/guides/COMMAND_REGISTRATION_GUIDE.md`, as GitHub links pinned to `main`.

## Example 2: the `liquers` package docstring

`liquers-py/python/liquers/__init__.py` keeps its purpose paragraph and adds the four headings
as reStructuredText-free Markdown headings (`# Main components` …), which pdoc renders. The
executed example repeats `PYTHON_GUIDE.md` §3:

```python
>>> import liquers as lq
>>> lq.evaluate("commands_doc").startswith("# Commands")
True
```

`scripts/test-liquers-py.sh` runs `python3 -m pytest --doctest-modules liquers-py/python/liquers/__init__.py`
after the test suite. Main components: `evaluate`, `default_environment`, `command`, `Environment`,
`Query`. Where used: `tests/test_GUIDE_examples.py`, the user guide chapters, `_env.py`.

## Example 3: the `liquers_web` module doc

`liquers-web/src/lib.rs` keeps its text and gains the five headings before `# Target` and
`# Architecture`; its example is a `text` block (the crate is wasm32-only, so no doctest runs) and
says so. `typescript.rs` gains a second `typescript_custom_section` holding a JSDoc block with
`@packageDocumentation`, the same five parts, and an `@example` repeating `JAVASCRIPT_GUIDE.md`
§3 *A first query*, which `scripts/test-guide-examples.sh` runs. The block says it repeats that
example. TypeDoc takes the first `@packageDocumentation` comment of the entry file as the module
page; wasm-bindgen writes custom sections at the top of `liquers_web.d.ts`, which
`check_api_docs.py`'s JavaScript reader relies on and `test_javascript_package_documentation_is_read`
checks on the source side.

## Edge and Error Cases

| Symptom | Expected | Test |
|---|---|---|
| A `pub mod` under a private `mod` | not public: no warning | `test_module_under_private_parent_is_not_public` |
| A public module with one public item and only a purpose | no warning | `test_single_item_module_needs_only_purpose` |
| Headings present but `# Examples` before `# Related concepts` | warning "out of order" | `test_headings_out_of_order_warn` |
| Six links under `# Where it is used` | warning | `test_more_than_five_use_links_warn` |
| A doctest of 12 visible lines, or none | warning; `# ` lines not counted | `test_example_length_counts_visible_lines_only` |
| `https://github.com/orest-d/liquers/blob/main/<path>` with no such file | error, exit 1 | `test_github_main_link_to_missing_path_is_error` |
| `--strict` with warnings only | exit 1 | `test_strict_turns_warnings_into_exit_1` |
| An old design without the `API docs:` line | passes (date gate) | `test_phase1_without_api_docs_line_passes_before_gate_date` |
| `[COLUMNAR]` citation in `liquers-records` | rustdoc job fails until the link issue is fixed | the job runs on the clean crates only until then |

## Test Plan

- Unit: `scripts/test_check_api_docs.py` (standard library; `python3 -m unittest -v scripts/test_check_api_docs.py`)
  - `test_public_module_without_doc_warns` (AC-5): a `pub mod` with no `//!` is listed.
  - `test_module_under_private_parent_is_not_public` (AC-5): no warning.
  - `test_single_item_module_needs_only_purpose` (AC-1): one public item, purpose only, clean.
  - `test_missing_section_warns` (AC-1, AC-5): two public items, no `# Where it is used`.
  - `test_headings_out_of_order_warn` (AC-1).
  - `test_more_than_five_use_links_warn` (AC-1); `test_more_than_five_example_links_warn` (AC-1).
  - `test_example_length_counts_visible_lines_only` (AC-1).
  - `test_github_main_link_to_missing_path_is_error` (AC-5); `test_github_main_link_to_existing_path_is_fine` (AC-5).
  - `test_python_module_docstring_is_read` (AC-2): `ast` reads a fixture package's docstring and finds the headings.
  - `test_javascript_package_documentation_is_read` (AC-2): the `@packageDocumentation` block in a fixture `typescript.rs`.
  - `test_strict_turns_warnings_into_exit_1` (AC-5); `test_json_format_lists_findings` (AC-5).
  - `test_repository_worked_examples_are_clean` (AC-3): the real `liquers-core/src/commands.rs`, `liquers-py/python/liquers/__init__.py`, `liquers-web/src/lib.rs` + `typescript.rs` and `liquers-docs/src/lib.rs` produce no warning.
- Unit: `.claude/skills/liquers-project/scripts/test_validate_phase.py` `ApiDocsLineTests`
  - `test_phase1_without_api_docs_line_fails_after_gate_date` (AC-4).
  - `test_phase1_without_api_docs_line_passes_before_gate_date` (AC-4).
  - `test_phase1_with_api_docs_line_passes` (AC-4).
- Doctests (AC-3): `cargo test -p liquers-core --doc commands`; `bash scripts/test-liquers-py.sh`
  (the new pytest step); `liquers-web/scripts/test-guide-examples.sh` (the example the `@example` repeats).
- Rustdoc job (AC-5): `RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links" cargo doc --no-deps -p liquers-template -p liquers-store -p liquers-mcp -p liquers-markdown`; exit 0 at HEAD, 101 when a link breaks.
- Existing (AC-6): `cargo test -p liquers-docs workspace_members_reads_one_line_and_many` and `python3 -m unittest scripts.test_docs_index.ChapterTests.test_links_into_specs_are_errors`.
- Review, not tests (AC-1, AC-2, AC-4): the guide's five parts, limits and language terms; the skill templates and checklist, read against Phase 2's list.
- Command to run everything: `python3 -m unittest discover -s .claude/skills/liquers-project/scripts -p 'test_*.py'`, `python3 -m unittest scripts.test_check_api_docs`, `python3 scripts/check_api_docs.py`, `cargo test -p liquers-core --doc commands`.

## Learning Log

- `CommandExecutor::execute` needs a hand-built `CommandArguments` and `Context`; a doctest that
  shows the real entry point registers and calls `evaluate` under a `tokio` runtime instead (eight
  visible lines). Phase 2's "registration and one call through the executor" is revised to this.
- The JavaScript worked example cannot be a doctest: `liquers-web` is wasm32-only and the `.d.ts`
  is generated. The executed example is the guide's, run by `test-guide-examples.sh`.
