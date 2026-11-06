# Phase 1: High-Level Design — liquers-py-modules

## Purpose

Let Python code kept in the Liquers store be imported by scripts and by commands, so a store
folder can carry its recipes, its data and the Python commands they call, and editing that code
expires the results it computed: the `MODULE` feature of `LANGUAGE-INTEGRATION_GUIDE.md`. It serves
`VISION.md`'s "make for data" pipelines and "trust the result" (a code change must not leave a
stale result standing). Milestone M7, with [`liquers-py-web-server`](../liquers-py-web-server/DESIGN.md)
and [`liquers-py-ui`](../liquers-py-ui/DESIGN.md).

## Design Readiness

- **Readiness:** `needs-decision`: Phase 1 written; the questions below wait for the Phase 1 gate.
- **Automatic fixing:** `not-eligible`: an `L` feature that loads executable code from the store,
  a security boundary.
- **Leading issue:** questions 2 and 4, which imports are allowed and when a module is re-executed.
  Source issue `PY-CANNOT-IMPORT-MODULES-FROM-THE-STORE` (P2 L, filed with this design).
- **Explanation:** no Rust change is expected beyond liquers-py: the store is read through the
  asset manager (`-R/<key>`), which already records what a command reads as a dependency
  (`Context::evaluate`, `DependencyRelation::ContextEvaluate`). The work is the Python import
  machinery, its trust rules, and doing the asynchronous read without holding Python's import lock.
- **Open questions:**
  1. **Proposed resolution: the API.** `env.module_path` (a list of `lq.Key`, empty by default),
     `env.import_module(name, reload=False)` and `context.import_module(name, cwd=None)` inside a
     command. Plain `import` statements also resolve, through one finder appended to the **end** of
     `sys.meta_path`, so the standard library and installed packages always win (MODULE06) and a
     store module never shadows them.
  2. **Open design: the trust boundary.** Recommended: code is executed only from a prefix in
     `env.module_path`. A relative import resolves against the command's `cwd` (a default link
     argument `cwd: lq.Key = lq.link("-R-key/.")`, never read from `Context`) and only when that
     `cwd` lies under a trusted prefix. Anything else raises `lq.LiquersError` with
     `NotSupported` (MODULE09), never `KeyNotFound`, so a refusal is not mistaken for absence.
     This makes the guide's MODULE03 sample, which sets no path, set `module_path = ["proj"]`;
     the deviation is recorded in the conformance matrix. Alternative: a cwd is trusted implicitly.
  3. **Proposed resolution: absent cwd** (MODULE04). An ad-hoc query has no recipe folder; a
     relative import then fails with a typed error naming the missing `cwd`, and absolute imports
     over `module_path` still work.
  4. **Open design: reload.** Recommended: `env.import_module` and `context.import_module` check
     the stored module's version on every call (a cached asset read) and re-execute it into a
     fresh module object when its bytes changed. A plain `import` statement keeps Python's usual
     `sys.modules` caching and never reloads. `reload=True` forces re-execution. Inside a command,
     the read records the module key as a dependency, so editing the module expires that
     command's results (MODULE07, MODULE08). Alternative: reload only when asked.
  5. **Proposed resolution: names and packages.** `pkg.sub` maps to `<prefix>/pkg/sub.py`, or to
     `<prefix>/pkg/sub/__init__.py` for a package, with `__path__` and `__file__` naming the store
     key, so a package's own relative imports work (MODULE05). Prefixes are tried in order. Store
     modules share `sys.modules` with the rest of the process; one name has one meaning per
     process, and the docs say so.
  6. **Proposed resolution: the synchronous import over an asynchronous store** (MODULE11). The
     module bytes are read before Python's import machinery is entered when the call is
     `import_module`, and with the GIL released when a plain `import` statement reaches the
     finder. An import on a liquers-py event-loop thread whose store is a Python `async def`
     store would wait on its own loop; it raises instead of deadlocking.
  7. **Proposed resolution: commands defined by store modules** (MODULE12). A module may register
     commands when imported or through a `register(env)` function it defines; such commands are
     versioned by their code object, as every Python command is, so a reloaded module gives them
     a new version.

## Problem Example

```python
import liquers as lq
env = lq.Environment()
env.store_set("code/scale.py", "def f(x):\n    return x * 2\n")
env.module_path = [lq.parse_key("code")]

@env.command
def scaled(state, context):
    return context.import_module("scale").f(state)

env.evaluate("number-2/scaled")            # 4
env.store_set("code/scale.py", "def f(x):\n    return x * 3\n")
env.evaluate("number-2/scaled")            # 6
```

**Today** `env.module_path` and `context.import_module` do not exist; the code must be on the host
filesystem, and changing it leaves `4` cached. **After** the second evaluation gives `6`, because
the module key is a dependency of the first result. `number` is a test source command returning
its argument; the queries validate with `number` and `scaled` declared.

## Scope and Acceptance Criteria

- **AC-1** Import from a configured prefix
  WHEN `code/mymod.py` holds `VALUE = 42` and `env.module_path = [lq.parse_key("code")]`
  THEN `env.import_module("mymod").VALUE == 42`, and `import mymod` gives the same module (MODULE01)
- **AC-2** Nothing outside the path is loaded
  WHEN `other/secret.py` exists and `module_path` is `[code]`
  THEN `import_module("secret")` raises `ModuleNotFoundError` (an `ImportError`) (MODULE02)
- **AC-3** Relative import against cwd
  WHEN a command with `cwd: lq.Key = lq.link("-R-key/.")` runs from a recipe in trusted folder
  `proj` and calls `context.import_module("helper", cwd=cwd)`
  THEN `proj/helper.py` is loaded (MODULE03)
- **AC-4** Absent cwd
  WHEN the same call has no `cwd`, as in an ad-hoc query
  THEN it raises `lq.LiquersError` naming the missing `cwd` (MODULE04)
- **AC-5** Packages
  WHEN `code/pkg/__init__.py` and `code/pkg/sub.py` exist
  THEN `import_module("pkg.sub")` loads both, and `pkg.sub`'s `from . import x` resolves in the
  store (MODULE05)
- **AC-6** Installed modules win
  WHEN `code/json.py` exists
  THEN `import_module("json")` and `import json` return the standard library's `json` (MODULE06)
- **AC-7** Reload policy
  WHEN a module's bytes change after it was imported
  THEN `import_module` returns the new code, and `import` still returns the cached module (MODULE07)
- **AC-8** Code edits expire results
  WHEN the problem example's module is rewritten between two evaluations of `number-2/scaled`
  THEN the second result is computed again from the new code (MODULE08, problem example)
- **AC-9** Untrusted prefixes are refused
  WHEN a module is requested from a prefix not in `module_path`, or relative to an untrusted cwd
  THEN it raises `lq.LiquersError` with `NotSupported` and the module is never executed (MODULE09)
- **AC-10** Store failures are typed
  WHEN the store fails while a module is read
  THEN the caller gets an `ImportError` whose `__cause__` is the `lq.LiquersError` (MODULE10)
- **AC-11** No deadlock in a command
  WHEN a Python command imports a store module, in `def` and in `async def` form, under a 5 s timeout
  THEN it completes (MODULE11)
- **AC-12** Commands from a store module run
  WHEN a store module's `register(env)` registers `shout`
  THEN `hello/shout` evaluates to `"HELLO"`, `hello` being the test source command (MODULE12)

Non-goals: loading compiled extensions (`.so`, `.pyd`) or bytecode from the store; a sandbox
(trusted code runs with full rights); `pip`-style distribution from a store; reloading modules
already imported by plain `import` statements.

## Core Interactions

Store: module bytes read as `-R/<key>` assets. Assets: dependency records on the reading command.
Commands: link-default `cwd`, version by code object. Bindings: `Environment.module_path`,
`import_module`, `Context.import_module`, a `sys.meta_path` finder.

## Crate Placement

`liquers-py` only: a new `src/modules.rs` (the store read and its dependency record) and the
finder and loader in `python/liquers/_modules.py`, since `importlib`'s protocols are Python.

## Documentation Intent

- Reference: `PYTHON_INTEGRATION.md`, a new §Modules from the store, with the trust rule.
- Guide: a "Keep Python code in the store" chapter in `PYTHON_GUIDE.md`.
- Other documents: the `MODULE` row of `liquers-py/README.md` with the MODULE03 deviation;
  `LANGUAGE-INTEGRATION_GUIDE.md` §6 Python; `specs/README.md`.
- Documents to update: the user guide chapters `docs_index.py chapters-for` lists.
- API docs: the module docs of the new `liquers-py/src/modules.rs` and `_modules.py`, and of
  `context.rs`; docstrings on every new Python name.

## Design Dependencies

- `liquers-py-templates` (M6, PR #262, open): **requires** its `lq.link` default links, which
  give a command its `cwd` (AC-3). M6 lands before M7.
- `py-command-decorator-declaration-keywords` (in review): **overlaps** on argument declarations.
- `language-store-config-function`, `liquers-py-store`: **overlap** on Python stores serving
  modules (question 6).

## References

- Guide §MODULE and Appendix A `MODULE`; `COMMAND_REGISTRATION_GUIDE.md` (default link `cwd`)
- `liquers-core/src/context.rs` `Context::evaluate`; `liquers-core/src/dependencies.rs`
- Python `importlib.abc.MetaPathFinder`, `importlib.machinery.ModuleSpec`
