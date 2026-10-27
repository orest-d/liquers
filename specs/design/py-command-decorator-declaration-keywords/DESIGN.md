---
id: PY-COMMAND-DECORATOR-DECLARATION-KEYWORDS
kind: design
title: The Python command decorators declare per-argument metadata and a realm
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [py]
issues: [PY-COMMAND-DECORATOR-CANNOT-DECLARE-ARGUMENT-METADATA, PY-COMMAND-DECORATOR-CANNOT-SET-REALM]
merged: 2026-10-27
created: 2026-10-27
---
# The Python command decorators declare per-argument metadata and a realm

Bulk design, 2026-10-27: Phases 1-4 written, not approved, nothing implemented.

## Phase 1: High-Level Design

### Purpose

`@lq.command` / `@lq.first_command` build their declaration from a fixed keyword list, so a
Python author cannot declare argument labels, descriptions or GUI hints, nor a realm, although the
pipeline accepts both (`COMMAND_DECLARATION.md` §4.1, §2.1). This adds `arguments=` and `realm=`
to the decorators, and `realm=` to the two lookups that take a command key.

### Problem Example

Verified at HEAD (`04667a80`): `_decorator.decorate` and `_register`
(`liquers-py/python/liquers/_commands.py`) know neither keyword; `Environment.unregister` /
`command_metadata` (`liquers-py/src/environment.rs`) use `CommandKey::new("", namespace, name)`.

```python
@env.command(arguments={"count": {"label": "Repetitions",
                                  "gui_info": {"IntegerSlider": {"min": 1, "max": 9, "step": 1}}}})
def repeat(text: str, count: int = 2) -> str:
    return text * count
# today: TypeError (unexpected keyword 'arguments')
# should: command_metadata("repeat").arguments[0].label == "Repetitions"; "hello/repeat-3" runs

@env.first_command(realm="server", namespace="tools")
def greet() -> str:
    return "hi"
# today: TypeError; should: env.evaluate("-server/ns-tools/greet") == "hi"
```

Both queries validate. `IntegerSlider` requires `step`, which `COMMAND_DECLARATION.md` §9 and
the leading issue omit.

### Scope and Acceptance Criteria

- **AC-1** Declared argument metadata reaches the command
  WHEN a decorator gets `arguments={"count": {"label": …, "description": …, "gui_info": …}}`
  THEN `command_metadata` shows them on `count`, with the introspected type and default unchanged
- **AC-2** A list of entries is accepted too
  WHEN `arguments` is a list of dicts each carrying `name`
  THEN the result equals the dict form
- **AC-3** A wrong entry is refused at decoration
  WHEN an entry names no parameter, or `arguments` is neither a dict nor a list
  THEN the decorator raises (`lq.ParameterError` / `TypeError`) and nothing is registered
- **AC-4** A realm places the command
  WHEN a decorator gets `realm="server"`
  THEN its metadata has realm `server`, `-server/…` runs it, and the default realm does not find it
- **AC-5** Lookups take the realm
  WHEN `command_metadata` or `unregister` gets `realm="server"`
  THEN it addresses that key; without `realm`, the default realm as today
- **AC-6** Nothing changes for existing code
  WHEN neither keyword is given
  THEN the metadata equals today's (`test_COMMAND05`, `test_COMMAND10` unchanged)
- **AC-7** The store-config realm is refused
  WHEN a decorator gets `realm="store_config"`
  THEN it raises `ValueError` naming store-config handlers, and nothing is registered

Non-goals: new `lq.ArgumentInfo` getters (description and `gui_info` are read via
`CommandMetadata.to_json()`); command-level `hints`, `presets`, `expires`, `payload_required`.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — new keyword arguments on the decorators and two
  `Environment` methods change the Python binding's interface (rule 4); a feature (rule 2).
- **Leading issue:** question 1.
- **Explanation:** both keywords map onto declaration keys the pipeline already handles; only
  API choices remain.
- **Open questions:**
  1. **Proposed resolution - realm, add or keep fixed:** add (recommended): §4.1 lists `realm` as
     declarable and liquers-web's `registerCommand` takes it. Keeping it fixed drops AC-4, AC-5,
     AC-7 and step 2, and the guide says why.
  2. **Proposed resolution - `realm="store_config"`:** refuse it (recommended), as
     `language-store-config-function` does for JavaScript under its option (a): a command there
     gets an executor that never runs. The alternative, documenting it, leaves that trap.
  3. **Proposed resolution - `arguments` shape:** a dict keyed by name (§9) or a list of entries
     (the declaration format, liquers-web's shape); a key/`name` mismatch is a `TypeError`.

### Design Dependencies

- `language-store-config-function` (needs-decision) — **overlaps**, T2 on the realm contract: its
  Q1(b) assumes the decorators take no `realm=`. Its recommended (a) refuses that realm on
  command entry points, which AC-7 does; under (b) AC-7 changes. Both edit `_commands.py`
  (different functions); no order.
- `language-global-enum-import` — **overlaps**: an entry naming a global enum needs it. No order.
- `py-api-reference` — **overlaps**: docstrings on the same functions; line-level only.

Overlap: the sources strongly overlap by **T3** (`_decorator.decorate`, `_register`); T1, T2, T4
do not hold. No other design changes those signatures.

### Consolidated Findings

- `CommandDeclaration::enhance` merges `arguments` by name and rejects an unknown one (`merge05`,
  `merge06`); `PyCommandSpec::new` takes the key from the metadata, so a realm needs no Rust change
  on the registration path. Only the two lookups hard-code `""`.
- `liquers_core::store::realm::STORE_CONFIG_REALM` is `"store_config"`; §9's `IntegerSlider`
  example lacks `step` (step 4 fixes it); the Python suite is not in CI (`PY-TESTS-NOT-RUN-IN-CI`).

## Phase 2: Architecture

### Solution

1. `_commands.py`: `decorate(fn=None, *, name=None, namespace="", realm="", label=None, doc=None,
   filename=None, volatile=None, cached=None, arguments=None, env=None)`. `_register` raises
   `ValueError` for `realm == "store_config"`, sets `declaration["realm"] = realm`, and, when
   `arguments` is not `None`, `declaration["arguments"] = _argument_entries(arguments, name)` — a
   private helper turning a dict into `[{"name": k, **v}, …]`, passing a list through, `TypeError`
   otherwise. The replacement warning names the realm when it is not empty. `_bound` forwards
   `**options` unchanged.
2. `environment.rs` `Environment::unregister` / `command_metadata`:
   `#[pyo3(signature = (name, namespace="", realm=""))]`, `CommandKey::new(realm, namespace, name)`.

Rejected: one `declaration=` dict keyword (unpythonic).

### Changes

- `_commands.py`: `_decorator`, `_register`, new `_argument_entries`, module docstring.
- `environment.rs`: two `#[pymethods]` signatures. No other crate, command or registry change.
- Documents: `specs/guides/PYTHON_GUIDE.md` §5 keyword table, §10; `specs/reference/PYTHON_INTEGRATION.md`
  §3 table rows, §4 keyword list, §8; `specs/reference/COMMAND_DECLARATION.md` §9 (sentence and
  `step`); `liquers-py/README.md` `COMMAND` row. History rows, `reviewed:` bumps.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| `gui_info` in the wrong serde shape | `CommandDeclaration::build` raises `ParameterError` at decoration | high |
| `realm="store_config"` registers a command that never runs | refused (AC-7) | high |
| Another unreachable realm (e.g. `R`) | as with `register_command!`; out of scope | high |


## Phase 3: Examples and Tests

### Examples

The Problem Example; the tests below give the secondary and error cases.

### Tests

In `liquers-py/tests/test_COMMAND_declaration_edges.py`, `env` fixture (`hello` is a fixture):

- `test_COMMAND_declared_argument_metadata_reaches_the_metadata` — label, and description and
  `gui_info` via `json.loads(meta.to_json())`; type and default kept; `hello/repeat-3` runs: AC-1
- `test_COMMAND_argument_entries_accept_a_list_or_a_dict` — equal `to_json()`: AC-2
- `test_COMMAND_a_wrong_argument_entry_is_refused` — unknown name, string, non-dict entry,
  key/name mismatch; `command_metadata` then raises: AC-3
- `test_COMMAND_realm_places_the_command_in_that_realm` — `-server/ns-tools/greet` is `"hi"`;
  `ns-tools/greet` raises `lq.ActionNotRegisteredError`: AC-4
- `test_COMMAND_lookups_and_unregister_take_a_realm` (AC-5);
  `test_COMMAND_omitted_keywords_keep_todays_metadata` (AC-6)
- `test_COMMAND_store_config_realm_is_refused` — `ValueError`; `command_metadata(…,
  realm="store_config")` raises: AC-7

Run: `bash scripts/test-liquers-py.sh -k COMMAND`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `_commands.py` `_decorator`, `_register`, `_argument_entries` (new), docstring — `arguments` — proof: `bash scripts/test-liquers-py.sh -k "argument_metadata or argument_entries or wrong_argument"` (AC-1, AC-2, AC-3)
- [ ] 2. Same file — `realm`, the `store_config` refusal, the warning — proof: `bash scripts/test-liquers-py.sh -k "realm_places or store_config_realm"` (AC-4, AC-7)
- [ ] 3. `liquers-py/src/environment.rs` `Environment::unregister`, `Environment::command_metadata` — `realm=""` — proof: `bash scripts/test-liquers-py.sh -k "take_a_realm or omitted_keywords"` (AC-5, AC-6)
- [ ] 4. Documents in Phase 2 §Changes — proof: `python3 scripts/docs_index.py --check`
- [ ] 5. Both source issues `status: closed` with a resolution note; `python3 scripts/docs_index.py` — proof: `python3 scripts/docs_index.py --check`

Rollback: each step is one file; revert it.

### Validation

`bash scripts/test-liquers-py.sh`, `python3 scripts/docs_index.py --check`. The diff touches
`liquers-py/` and the listed documents; `specs/command_registry.yaml` does not change.
