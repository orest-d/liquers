# Phase 5: Documentation - liquers-mustache

## Completion Preconditions

- [x] Implementation is finished and validated
- [ ] All user comments are answered or incorporated — the volatility decision (D6) is on a
  decision card awaiting Orest; AC-2's text is revised when it is answered
- [x] All review comments are answered or incorporated
- [x] Documentation is consistent with the implemented and tested behavior
- [x] Documentation is included in the implementation PR when practical

## Implementation Summary

All sixteen Phase 4 steps are done, in PR #139.

- **`liquers-core`**: `DataNode` / `AsyncDataNode` / `AnyNode` and `ValueInterface::data_node`;
  `Context::apply_ad_hoc`, which passes an ad-hoc evaluation's dependencies on (and, per D6, no
  volatility); `ArgumentInfo::with_default_link`; `simple_template` deprecated.
- **`liquers-records`**: `RecordViewNode` (sync, a row at a time) and `RecordSourceNode` (async).
- **`liquers-template`** (new crate, core only, builds and tests on wasm32 under Node): names with
  dotted and bracket lookup, the context stack, `QueryTemplate` with its sentinel parser (D7) and
  instantiation, the Mustache lexer, parser, encoder and renderer, `MapHost`, error modes,
  `validate_template`. The official Mustache spec suite (six modules) passes.
- **`liquers-lib`** (feature `templates`, on by default): `template/render`, `template/expand`,
  `template/validate`; `ContextHost`; data nodes for `SimpleValue`, polars frames and directory
  listings (`FolderNode`); `Key` command arguments for liquers-lib's `Value`. The registry is
  regenerated.

**Conforms** to the approved design, with the decisions logged in Phase 1 §Design Readiness:
D6 (volatility is not inherited, which revises AC-2 and is awaiting Orest), D7 (sentinel parser
instead of a shadow grammar), D8 (partial cycles are the same key at the same stack depth;
`MapHost`'s missing partial is empty; an unmatched close is an `Invalid` node), D9 (evaluation
errors take their tag's position; a folder is recognised from its listing; `TryFrom<Value> for
Key`). Added beyond the plan: the polars `DataFrameNode` reads rows natively instead of through the
records bridge, and `RecordViewNode::as_any` lets an apply input stay a view.

## Documentation Delivered

### New Reference Documents
- `specs/reference/TEMPLATE_LANGUAGE.md` — tags, slot positions P1–P6, names, data nodes, the
  `ns-template` commands, evaluation routes, error modes, the AST.

### New Guide Documents
- `specs/guides/TEMPLATE_GUIDE.md` — writing and validating templates, `render` versus `expand`,
  template commands (the context pack as the worked example), rendering from Rust, making a value
  readable, adding a syntax.

### Existing Documents Reviewed or Updated
The nine `affects_docs` paths, each with a History row and `reviewed: 2026-10-17`:
`COMMAND_REGISTRATION_GUIDE.md` (link heads, `with_default_link`, template commands),
`COMMAND_DESIGN_GUIDE.md` (§Aliases: a template), `COMMAND_ALIASES.md` (link heads, the example),
`VALUE_TYPE_SYSTEM.md` (§Reading a value by name), `DOC_04` (`apply_ad_hoc`), `DOC_02`
(`simple_template` deprecated), `PROJECT_OVERVIEW.md` (crate and dependency flow), plus the two
new documents. `CLAUDE.md` gains the crate, the dependency flow, where code goes, the template
command line, the `templates` feature and test loop, and "five groups" for registry-gated tests.

### Links and Capability Map
`specs/README.md`: the query-aware templates row is now "built", pointing at the reference, the
guide and this design.

## Issues Filed

- `CWD-DEFAULT-LINK-WARNS-WITHOUT-WORKING-DIRECTORY` — the guide's `-R-key/.` default logs a
  spurious warning whenever no working directory is set.
- `COMBINED-VALUE-DATA-NODE-COPIES-BASE-VALUE` — a base value is cloned once per render to become
  a data node.
- `BUILD-MATRIX-EXHAUSTS-SESSION-DISK` — the build-matrix script outgrows a cloud session's disk.
- Filed earlier in the design and still open: `REGISTRY-QUERY-DEFAULT-NOT-WRITABLE-AS-TEXT`,
  `SEARCH-COMMANDS-RECORD-NO-DEPENDENCY-ON-READ-FILES`, `RECIPE-OVERRIDE-ONLY-LAST-ACTION`,
  `TEMPLATE-HAS-NO-HIGHLIGHTER`, `NO-TEMPLATE-EDITOR-UI`.

## Important Learning

- Parity with core's parser came for free by parsing a sentinel-substituted text with core itself;
  a second grammar was never needed.
- A query's error positions are in the query's text; anything embedding queries must re-anchor them.
- Directory listings carry no `Directory` status or key in their state; recognise them by value.
- Volatility is fixed before evaluation, so nothing computed during a render can make it volatile.

## Conformance and Remaining Work

Requested, approved and implemented scope match, except AC-2's "volatility inherited", which D6
replaces pending Orest's answer. Nothing else remains in this design; the deferred items are the
issues above. `NO-QUERY-AWARE-TEMPLATE-LANGUAGE` and `SIMPLE-TEMPLATE-HAS-NO-COMMAND` close with
this design (the latter's "volatility carried into the result" is D6).

## Validation

- `cargo test -p liquers-core --lib --tests`, `cargo test -p liquers-template`,
  `cargo test -p liquers-records --all-features --lib --tests`, `cargo test -p liquers-lib --lib --tests`:
  all pass. `cargo test -p liquers-template --target wasm32-unknown-unknown`: passes under Node.
- The build matrix (`scripts/check-build-matrix.sh`'s 34 rows, run one at a time because a single
  run fills the session disk, `BUILD-MATRIX-EXHAUSTS-SESSION-DISK`): all pass.
- `python3 scripts/docs_index.py --check`: 0 errors. The queries in the new documents pass
  `liquers-validate` against the regenerated registry.
