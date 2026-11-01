# Phase 5: Documentation - liquers-py-store

## Completion Preconditions

- [x] Implementation is finished and validated: Phase 4 steps 1-5 (7e31f8e..2c98d84); liquers-py
  suite 346 passed, 8 skipped, 1 xpassed on CPython 3.13; Rust unit tests 25 passed; PR #229 CI
- [ ] All user comments are answered or incorporated: decision log (a) (a store without `listdir`
  takes no `metadata:` section) is still open with Orest; implemented as recommended
- [x] All review comments are answered or incorporated: two reviewer passes per phase, findings fixed
- [x] Documentation is consistent with the implemented and tested behavior: the guide's examples
  run (`test_GUIDE_examples.py`, 20 passed)
- [x] Documentation is included in the implementation PR when practical: PR #229

## Implementation Summary

liquers-py now has the `STORE` feature, directions 1-3. A Python factory registered with
`Environment.register_store_factory(type, factory)` is a store type: a configuration entry
`{type, prefix, config}` calls it with the `config:` keys as keywords (and `prefix=`), the
declaration being read from the factory's signature by `liquers/_factories.py` or given as
`lq.StoreArgumentInfo`s. The returned object is resolved once into a method table
(`src/store/adapter.rs`): only `get` is mandatory, `get_metadata`/`contains`/`is_dir` are derived
when absent, any other absent method raises `KeyNotSupported` naming it, and a provided method is
always used. Data-only access is the object's five data methods or derived from
`get`/`set`/`remove`/`listdir`, so `read_only` and `metadata:` run in Rust around a Python mount
(Decision 5, options A+B). Calls run Python off the runtime's workers through the shared
`src/pycall.rs`, which commands now use too. `env.store` and `lq.Store.from_object` give Python
the store's methods with `_async` twins. A thread-local build guard turns a factory's use of its own
environment into a `ConfigurationError` instead of a deadlock.

Conformance to the request and the approved design: as approved, with the decision log (a)
narrowing (a `metadata:` section needs `listdir`) taken as recommended while it awaits Orest.
Added during implementation: `listdir` falls back to `listdir_data` for a store that has only the
data methods (found by `STORE16`); the shared `bind_arguments` / `call_factory` helpers in
`src/store/mod.rs`, reused by recipe provider factories. Omitted: the liquers-web follow-up for
`WEB-JS-STORE-WITH-ONLY-GET-CANNOT-BE-QUERIED`, never a step of this design (it stays that issue).

## Documentation Delivered

### New Reference Documents
None: the contract is a new §9 Stores of `reference/PYTHON_INTEGRATION.md`, as planned.

### New Guide Documents
None: `guides/PYTHON_GUIDE.md` §4 gained "A store written in Python". Phase 2 planned a chapter
"Stores"; it became a subsection of §4 Configuring an environment, because a Python store is
reached through the store configuration and a new chapter would have renumbered §5-§10, which other
documents cite.

### Existing Documents Reviewed or Updated
`affects_docs`: `reference/PYTHON_INTEGRATION.md` (§1-§3, §8, new §9), `reference/STORE_CONFIG_FSD.md`
(the `liquers-py` row of the factory table), `guides/PYTHON_GUIDE.md` (§2, §4, §10),
`guides/LANGUAGE-INTEGRATION_GUIDE.md` (§STORE: a language store type through a bridging factory;
§6 Python), `liquers-py/README.md` (`STORE` row), `specs/README.md`. Each changed `reference/` and
`guides/` document has a 2026-11-01 History row and `reviewed:`.

### Links and Capability Map
`specs/README.md`: the "Python stores and recipe providers" line is "built" and links §9/§10 and
the guide sections.

## Issues Filed

- `STORE-OPENDAL-FS-CONCURRENT-WRITES-TO-ONE-KEY-FAIL` (P2): ten concurrent writes to one key of an
  OpenDAL `fs` store sometimes fail (about 1 in 30); `STORE06[fs]` is a non-strict `xfail`.
- `WEB-JS-STORE-BYTES-WITHOUT-METADATA-READ-AS-BINARY` (filed in Phase 1): a page store returning
  data without metadata may read a `.txt` key as bytes; deferred work for liquers-web, its own PR.
- `VALIDATE-PHASE-PHASE1-SIZE-COUNTS-EXCLUDED-SECTIONS` (filed in Phase 1): the phase validator
  counts the problem example and readiness section against Phase 1's size limit.

Closed: `PY-NO-LANGUAGE-DEFINED-STORE`. Progress noted on `LANGUAGE-STORE-TYPE-NOT-DEFINABLE`
(done for Python, open for JavaScript).

## Important Learning

- The configured pipeline is the cheap way to keep configuration effective for a language store:
  deriving data-only access from the ordinary methods made every `metadata:` handler work with no
  Python handler at all.
- A factory runs inside the build, under the environment's lock; without the build guard a factory
  that evaluates through its environment hangs forever. Any integration calling user code during a
  build needs the same guard.
- Conformance over five store kinds found a real core defect (the OpenDAL `fs` race) that the Rust
  suite had no concurrent same-key case for.

## Conformance and Remaining Work

Requested: Python stores, configured routers, configuration effective for Python stores (Orest,
2026-11-01). Approved: Phases 1-4 with Decisions 1-5. Implemented: all of it; `STORE18` is `NA`
(`LANGUAGE-STORE-CONFIG-FUNCTION-NOT-DEFINABLE`). Remaining: decision log (a), whose other answer
would derive a listing-free `metadata:` mode (an issue if chosen); the OpenDAL issue and the
liquers-web follow-ups (`WEB-JS-STORE-BYTES-WITHOUT-METADATA-READ-AS-BINARY`,
`WEB-JS-STORE-WITH-ONLY-GET-CANNOT-BE-QUERIED`).

## Validation

`python3 scripts/docs_index.py` regenerated the index; `validate_phase.py liquers-py-store 5`;
`test_GUIDE_examples.py` and `test_DOCS_docstrings.py` pass.
