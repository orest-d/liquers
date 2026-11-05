# Phase 5: Documentation - liquers-py-templates

## Completion Preconditions

- [x] Implementation is finished and validated (commit `781667a7`; `bash scripts/test-liquers-py.sh`:
  476 passed, Rust unit tests pass)
- [x] All user comments are answered or incorporated (Orest 2026-11-04: templates work outside
  commands too, through a core closure-asset primitive; Q8 `lq.link` as recommended)
- [x] All review comments are answered or incorporated (two reviewers per phase, Phases 1-4)
- [x] Documentation is consistent with the implemented and tested behavior (commit `a01fcf09`)
- [x] Documentation is included in the implementation PR when practical

## Implementation Summary

Python renders Liquers templates, at the top level and inside a command, and declares template
commands, as approved.

- **Calls.** `render(template, state=None, data=None, *, errors="strict", cwd=None, html=False,
  full=False)` and `expand(template, data=None, …)` on `Environment` and `Context`, each with an
  `_async` form, plus `validate_template(text)`. One argument mapping (`template.rs`
  `build_request`) builds a `RenderRequest` for both receivers.
- **Top level.** `render_top_level` (`template-render-in-env`) runs the render in a closure asset
  (`ad-hoc-closure-asset`): its query tags are dependencies of an asset and its recovered errors are
  warnings. The sync wait spawns the run and, on Ctrl-C or shutdown, cancels the asset and lets the
  run settle (`closure.rs` `wait_for_run`).
- **Inside a command.** `render_in_context` with the command's context: the tags are the
  command's dependencies.
- **Template commands.** `env.template_command(name, template, …)` declares an alias of
  `template/render` (`declared.rs`), kept in the environment's declarations and applied at every
  rebuild; the last registration under a key wins, Python or declared. `CommandMetadata.definition`
  reads it back (`kind`, `alias_of`, `head_parameters`).
- **Default links.** `lq.link(query)` as a parameter default declares a default link.

Nothing was omitted or added.

## Documentation Delivered

### New Reference Documents
None: the contract is §15 of the existing Python reference.

### New Guide Documents
None: the usage is a section of the existing Python guide.

### Existing Documents Reviewed or Updated
`affects_docs`, each reviewed against the code and tests, with a History row and `reviewed:`
2026-11-04:
- `specs/reference/PYTHON_INTEGRATION.md` §15 Templates; §2, §3, §4 names.
- `specs/guides/PYTHON_GUIDE.md` §6 Templates (examples run by `test_GUIDE_examples.py`).
- `specs/guides/TEMPLATE_GUIDE.md` §3: the Python form of template commands.
- `specs/guides/COMMAND_REGISTRATION_GUIDE.md`: default links from Python (`lq.link`).
- `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §6 Python *Templates*; §TEMPLATE needed no change.
- `liquers-py/README.md`: `TEMPLATE` row, `COMPLETE`, tests `TEMPLATE01`-`11` mapped.
- `specs/README.md`: see Links.

### Module Docs
New: `liquers-py/src/template.rs`, `declared.rs`. Changed: `environment.rs`, `context.rs`,
`command.rs`, `command_metadata.rs`, `query.rs`, `aio.rs`, and `lib.rs` (a crate doc added in
Phase 5). `check_api_docs.py` on each: 0 errors, 0 warnings.

### User-Guide Chapters
`docs_index.py chapters-for` lists the `10-templates.md`, `05-commands.md` and other chapters of the
three guides; all are `status: outline`, so no chapter review was due.

### Links and Capability Map
`specs/README.md`'s M6 entry is updated when the three M6 designs are complete (the pipelines PR).

## Issues Filed

None. Known and linked: `TEMPLATE-DIAGNOSTICS-CHECK-COMMANDS-A-Q-QUOTES`,
`TEMPLATE-NON-SCALAR-HAS-NO-TEXT-FORM`, `PY-ADD-PYTHON-COMMAND-STILL-DECLARES-PYCALL-ALIASES`,
`PY-COMMAND-DECORATOR-CANNOT-SET-REALM` (documented in `PYTHON_INTEGRATION.md` §15).

## Important Learning

- A command registered only in the built registry vanishes at the next rebuild (any late Python
  registration rebuilds), so a declared command lives in the declarations, beside the Python
  specs. The same slot carries pipeline commands (`liquers-py-pipelines`).
- A Python render and the `ns-template/render` command share `render_in_context`, so one test
  (AC-2) covers the argument mapping, not two renderers.
- `declare` costs about 19 ms in a debug build (a trial build and validation).

## Conformance and Remaining Work

Requested (Orest, 2026-11-04): templates from Python, also outside commands. Approved: Phases 1-4.
Implemented: AC-1 to AC-13. Nothing remains outside the linked issues;
`PY-CANNOT-RENDER-TEMPLATES` is closed.

## Validation

`bash scripts/test-liquers-py.sh` (476 passed); `check_api_docs.py` on the touched modules;
`python3 scripts/docs_index.py --check` (0 errors).
