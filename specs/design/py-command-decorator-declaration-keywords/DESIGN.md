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

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-27: Phases 1-4 written, not
approved, nothing implemented.

## Phase 1: High-Level Design

### Purpose

`@lq.command` and `@lq.first_command` hand Rust a declaration built from a fixed keyword list, so
a Python author cannot declare argument labels, descriptions or GUI hints, nor place a command in
a realm. The declaration pipeline already accepts both (`COMMAND_DECLARATION.md` §4.1, §2.1). This
design adds `arguments=` and `realm=` to the decorators, and `realm=` to the two lookups that take
a command key.

### Problem Example

Verified at HEAD (`04667a80`): `_decorator.decorate` in `liquers-py/python/liquers/_commands.py`
takes `name, namespace, label, doc, filename, volatile, cached, env` only, and `_register` copies
exactly those into the declaration. `Environment.unregister` and `Environment.command_metadata`
(`liquers-py/src/environment.rs`) build `CommandKey::new("", namespace, name)`.

```python
@env.command(arguments={"count": {"label": "Repetitions",
                                  "gui_info": {"IntegerSlider": {"min": 1, "max": 9, "step": 1}}}})
def repeat(text: str, count: int = 2) -> str:
    return text * count
# today: TypeError: decorate() got an unexpected keyword argument 'arguments'
# should: env.command_metadata("repeat").arguments[0].label == "Repetitions", and
#         env.evaluate("hello/repeat-3") == "hellohellohello"

@env.first_command(realm="server", namespace="tools")
def greet() -> str:
    return "hi"
# today: TypeError (realm); should: env.evaluate("-server/ns-tools/greet") == "hi", and
#        env.command_metadata("greet", "tools", realm="server").realm == "server"
```

Both queries validate; `-server/ns-tools/greet` plans `Action{realm: server, ns: tools, greet}`,
and without a `server` registration it fails with keys tried `server-tools-greet`, `server--greet`.
`IntegerSlider` needs `step` (`liquers-core/src/command_metadata.rs` `ArgumentGUIInfo`); the
example in `COMMAND_DECLARATION.md` §9 and in the leading issue omits it and would be refused.

### Scope and Acceptance Criteria

- **AC-1** Declared argument metadata reaches the command
  WHEN a decorator gets `arguments={"count": {"label": …, "description": …, "gui_info": …}}`
  THEN `command_metadata` shows that label, description and `gui_info` on `count`, and the
  introspected type and default are unchanged
- **AC-2** A list of entries is accepted too
  WHEN `arguments` is a list of dicts each carrying `name`
  THEN the result equals the dict form
- **AC-3** A wrong entry is refused at decoration
  WHEN an entry names no parameter of the function, or `arguments` is neither a dict nor a list
  THEN the decorator raises (`lq.ParameterError` for the unknown name, `TypeError` for the shape)
  and nothing is registered
- **AC-4** A realm places the command
  WHEN a decorator gets `realm="server"`
  THEN the command's metadata has realm `server`, `-server/…` queries run it, and a query in the
  default realm does not find it
- **AC-5** Lookups take the realm
  WHEN `command_metadata` or `unregister` is called with `realm="server"`
  THEN it addresses the realm-`server` key; without `realm` it addresses the default realm, as today
- **AC-6** Nothing changes for existing code
  WHEN neither keyword is given
  THEN the metadata equals today's (`test_COMMAND05`, `test_COMMAND10` pass unchanged)

Non-goals: argument-level `hints`/`presets` beyond passing them through; new getters on
`lq.ArgumentInfo` (description and `gui_info` are read through `CommandMetadata.to_json()`);
command-level `hints`, `presets`, `expires`, `payload_required` keywords.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — adds keyword arguments to the decorators and to
  `Environment.unregister` / `command_metadata`, a change to the Python binding's interface (rule 4);
  it is a feature, not a fix of documented behaviour (rule 2).
- **Leading issue:** question 1.
- **Explanation:** both keywords map onto declaration keys the shared pipeline already merges and
  validates, so the change is a pass-through; the open points are API choices only.
- **Open questions:**
  1. **Proposed resolution - realm, add or keep fixed:** add `realm=` (recommended).
     `COMMAND_DECLARATION.md` §4.1 lists `realm` as declarable, liquers-web's `registerCommand`
     accepts it, and the cost is one keyword on three entry points. Keeping it fixed instead drops
     AC-4/AC-5 and steps 2-3's realm parts, and the guide states why.
  2. **Proposed resolution - `arguments` shape:** accept a dict keyed by parameter name (the §9
     shape) and also a list of entries with `name` (the declaration format, liquers-web's shape); a
     dict entry whose own `name` differs from its key is a `TypeError`.
  3. **Implementation detail - realm in the replacement warning:** the warning names the key with
     its realm when the realm is not empty.

### Design Dependencies

- `GLOBAL-ENUMS-NOT-IMPORTED-BY-LANGUAGE-INTEGRATIONS` (issue, no design at HEAD) and
  `global-enum-registration` — **overlaps**: an `arguments` entry can declare an inline enum type
  today; one naming a *global* enum needs those. No order.
- `py-api-reference` — **overlaps**: both edit `_commands.py` and the two `#[pymethods]` lookups
  (docstrings there). Line-level only; no order.

Overlap triage: the two sources strongly overlap by **T3** (both add a keyword to
`_decorator.decorate` and `_register`, and to the same declaration dict). T1, T2, T4 do not hold.
No open design touches `_commands.py`; `liquers-py` is complete (E4).

### Consolidated Findings

- Both keys are already declarable: `CommandDeclaration::enhance` merges `arguments` by name and
  rejects an unknown name (`merge05`, `merge06`), and `PyCommandSpec::new` builds the key from the
  finished metadata, so a realm needs no Rust change on the registration path.
- `register_py_command`, `EnvCell::register` / `unregister` and `builtin_keys` compare full
  `CommandKey`s already; only the two Python-facing lookups hard-code `""`.
- `COMMAND_DECLARATION.md` §9's `IntegerSlider` example lacks the required `step`; Phase 4 fixes it
  while editing that paragraph.
- Tests run only locally (`PY-TESTS-NOT-RUN-IN-CI`).

## Phase 2: Architecture

### Solution

1. `liquers-py/python/liquers/_commands.py`: `decorate(fn=None, *, name=None, namespace="",
   realm="", label=None, doc=None, filename=None, volatile=None, cached=None, arguments=None,
   env=None)`; `_register` takes `realm` and `arguments`, sets `declaration["realm"] = realm`
   (always, as `namespace`) and, when `arguments` is not `None`,
   `declaration["arguments"] = _argument_entries(arguments, name)`. The new private helper turns a
   dict into `[{"name": k, **v}, …]`, passes a list through, and raises `TypeError` for any other
   type, a non-dict entry, or a key/`name` mismatch. `_bound` forwards `**options`, so
   `@env.command(...)` gets both keywords with no change.
2. `liquers-py/src/environment.rs` `Environment::unregister` / `command_metadata`:
   `#[pyo3(signature = (name, namespace="", realm=""))]`, `CommandKey::new(realm, namespace, name)`.
   Positional `(name, namespace)` calls keep working.

Rejected: one `declaration=` dict keyword (unpythonic, and bypasses the keyword list the guide
documents); new `ArgumentInfo` getters (not needed to prove the feature; a separate API change).

### Changes

- `_commands.py`: `_decorator`, `_register`, new `_argument_entries`, module docstring.
- `environment.rs`: two `#[pymethods]` signatures.
- No `liquers-core` / `liquers-lib` change, no command or registry change.
- Documents: `specs/guides/PYTHON_GUIDE.md` (§Metadata keyword table: `realm`, `arguments`; §10
  limitation removed), `specs/reference/PYTHON_INTEGRATION.md` (§3 table rows for `unregister`,
  `command_metadata`; §4 decorator keyword list; §8 limitation removed),
  `specs/reference/COMMAND_DECLARATION.md` §9 (the "not `arguments`" sentence; `step` in the
  example), `liquers-py/README.md` `COMMAND` row. History rows and `reviewed:` bumps.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| A declared `gui_info` in the wrong serde shape | `CommandDeclaration::build` refuses it with `ParameterError` at decoration (AC-3 style); the guide shows the shape | high |
| An entry for `context` or the state parameter | merged, then removed by the conventions stage like the discovered entry; harmless | medium |
| A realm the query syntax cannot select (e.g. `R`) | registers but is unreachable, as with `register_command!`; out of scope | high |
| Existing positional calls to `unregister` / `command_metadata` | `realm` is the third, defaulted parameter | high |

Recovery: revert the two files. Tests likely to change: none existing; six added.

## Phase 3: Examples and Tests

### Examples

The Problem Example is the primary case. Secondary: `arguments=[{"name": "count", "label": "N"}]`
gives the same metadata as `arguments={"count": {"label": "N"}}`; `arguments={"cnt": {}}` raises
`lq.ParameterError` naming `cnt`; `env.unregister("greet", "tools", realm="server")` returns `True`
once, then `False`.

### Tests

In `liquers-py/tests/test_COMMAND_declaration_edges.py`, using the `env` fixture (`hello` is a
fixture command):

- `test_COMMAND_declared_argument_metadata_reaches_the_metadata` — label via `ArgumentInfo.label`,
  description and `gui_info` via `json.loads(meta.to_json())`, type `int` and default `2` kept,
  `hello/repeat-3` evaluates: AC-1
- `test_COMMAND_argument_entries_accept_a_list_or_a_dict` — two environments, equal `to_json()`: AC-2
- `test_COMMAND_a_wrong_argument_entry_is_refused` — unknown name → `lq.ParameterError`; a string,
  a non-dict entry and a key/name mismatch → `TypeError`; `command_metadata` then raises: AC-3
- `test_COMMAND_realm_places_the_command_in_that_realm` — `-server/ns-tools/greet` is `"hi"`,
  `ns-tools/greet` raises `lq.ActionNotRegisteredError`: AC-4
- `test_COMMAND_lookups_and_unregister_take_a_realm` — `command_metadata(..., realm="server")`,
  the default-realm lookup raises, `unregister` true then false: AC-5
- `test_COMMAND_omitted_keywords_keep_todays_metadata` — `to_json()` of a command declared without
  the keywords has `realm` absent or empty and the introspected arguments: AC-6

Run: `bash scripts/test-liquers-py.sh -k COMMAND` (all suites: no `-k`).

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-py/python/liquers/_commands.py` `_decorator`, `_register`, `_argument_entries` (new, private), module docstring — add `arguments` — proof: `bash scripts/test-liquers-py.sh -k "argument_metadata or argument_entries or wrong_argument"` (AC-1, AC-2, AC-3)
- [ ] 2. Same file: add `realm`, include it in the declaration and in the replacement warning — proof: `bash scripts/test-liquers-py.sh -k realm_places` (AC-4)
- [ ] 3. `liquers-py/src/environment.rs` `Environment::unregister`, `Environment::command_metadata` — `realm=""` parameter — proof: `bash scripts/test-liquers-py.sh -k "take_a_realm or omitted_keywords"` (AC-5, AC-6)
- [ ] 4. Documents listed in Phase 2 §Changes, with History rows and `reviewed:` — proof: `python3 scripts/docs_index.py --check`
- [ ] 5. Both source issues `status: closed` with a resolution note; `python3 scripts/docs_index.py` — proof: `python3 scripts/docs_index.py --check`

Rollback: each step is one file; revert it.

### Validation

`bash scripts/test-liquers-py.sh` (whole Python suite and the crate's Rust unit tests),
`python3 scripts/docs_index.py --check`. The diff touches `liquers-py/` and the listed documents
only; `specs/command_registry.yaml` does not change.
