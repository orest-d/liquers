# Phase 5: Documentation - configuration-document

## Completion Preconditions

- [x] Implementation is finished and validated
- [x] All user comments are answered or incorporated
- [x] All review comments are answered or incorporated (no PR review comments yet)
- [x] Documentation is consistent with the implemented and tested behavior
- [x] Documentation is included in the implementation PR when practical (PR #264)

## Implementation Summary

One file configures a whole host, as Orest asked on 2026-11-07. `liquers-core` gained
`ConfigDocument` (`liquers-core/src/environment_config.rs`): it reads YAML, JSON or TOML (feature
`toml`), from text or from a file by extension (`load`, native only); it splits the document into
the `EnvironmentConfig` keys, `command_registry` and the host's sections; `take_section` hands a
section to the part that owns it; `finish` refuses every section nobody took and lists the known
ones. YAML tags, non-string keys and non-finite numbers are a `ParseError`, `null` is `{}`, and
`command_registry_path` resolves against the file's folder.

liquers-py reads it in `PyConfig::from_python` (`liquers-py/src/config.rs`): `config=` takes an
`os.PathLike` (a file), a `str` (YAML text, with a hint to pass `pathlib.Path` when the text is a
bare file name) or a `dict`. It takes `server` (liquers-axum's `ServerConfig`) and its own `python`
section (`conversion`, `pickle`, `async_loops`), both refusing unknown keys. The command registry
file is read at construction and its commands join the imported metadata on every build. Keywords
override the file, as Decision 2 answered ("keyword wins everywhere", 2026-11-09): `job_capacity=`
now replaces `assets.job_capacity` instead of raising a conflict, so that case moved from
`test_ASYNC_loops.py` to `test_CONFIG_keywords_override_the_file`.

The implementation conforms to Phases 1–4. Nothing was omitted. One addition: `remaining_sections`
and `known_keys`, small accessors `finish` and liquers-py's messages share.

## Documentation Delivered

### New Reference Documents
None: the document is a section of `ENVIRONMENT_CONFIG.md`, as Phase 2 planned.

### New Guide Documents
None, as planned.

### Existing Documents Reviewed or Updated
`affects_docs`, each updated with a History row and `reviewed: 2026-11-09`:
`specs/reference/ENVIRONMENT_CONFIG.md` (§The configuration document; unknown top-level keys
refused), `specs/guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md` (§Configure from one file),
`specs/reference/PYTHON_INTEGRATION.md` (§3: `config=` forms, the `python` section, the keyword
rule, the command registry file), `specs/guides/PYTHON_GUIDE.md` (§4, its examples run by
`test_GUIDE_examples.py`). `specs/README.md` keeps its design row.

### Module Docs
`liquers-core/src/environment_config.rs` (rewritten), `liquers-py/src/config.rs` (new),
`liquers-py/src/environment.rs` (brought up to §3): `check_api_docs.py` reports 0 warnings, 0
errors for each.

### User-Guide Chapters
`docs_index.py chapters-for` lists `docs/guides/{rust,python,web}/08-environment.md` and most
Python chapters (`PYTHON_GUIDE.md` is a source of each). `python/08-environment.md` is now `draft`
with 8.1–8.4 written (making an environment, one file, keywords override, the command metadata
file); `rust/08-environment.md` is `draft` with 8.2 written. The others are outlines holding only
`TBD:` markers, so no claim in them is wrong; left as they are.

### Links and Capability Map
`specs/README.md` already lists the design; its row is unchanged until `status: complete`.

## Issues Filed

None new. `HOSTS-READ-CONFIGURATION-DOCUMENTS-DIFFERENTLY` stays open with a progress note:
liquers-agent's `AgentConfig`, liquers-web, and unknown keys inside the environment sections remain.

## Important Learning

A host section type should set `deny_unknown_fields`; `take_section` cannot check inside a
section. `EnvironmentConfig` does not, so the environment sections still ignore unknown keys,
recorded in the issue above rather than changed here (it would break existing documents).

## Conformance and Remaining Work

Requested, approved and implemented scope agree. Remaining: the open issue above only.

## Validation

`cargo test -p liquers-core --lib environment_config` (default and `--features toml`), the doctest,
`cargo check -p liquers-core --target wasm32-unknown-unknown`;
`cargo test -p liquers-axum --test server_config_document`; `bash scripts/test-liquers-py.sh` on
CPython 3.13 (450 passed, 23 skipped; the STUBS checks need 3.14 and run in CI);
`cargo run -p liquers-docs -- check` (0 errors, 0 warnings); `python3 scripts/docs_index.py --check`.
