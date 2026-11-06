# Phase 1: High-Level Design — liquers-py-modules

## Purpose

Let Python code kept in the Liquers store be imported by scripts and by commands, so a store folder
can carry its recipes, its data and the Python commands they call, and editing that code expires
the results it computed: the `MODULE` feature of `LANGUAGE-INTEGRATION_GUIDE.md`, for `VISION.md`'s
"make for data" pipelines and "trust the result". Milestone M7, with
[`liquers-py-web-server`](../liquers-py-web-server/DESIGN.md) and [`liquers-py-ui`](../liquers-py-ui/DESIGN.md).

## Design Readiness

- **Readiness:** `needs-decision`: Phase 1 written and reviewed twice; questions 2, 4 and 6 wait
  for the Phase 1 gate.
- **Automatic fixing:** `not-eligible`: an `L` feature that loads executable code from the store,
  a security boundary.
- **Leading issue:** question 2, which code may run. Source issue
  `PY-CANNOT-IMPORT-MODULES-FROM-THE-STORE` (P2 L, filed with this design).
- **Explanation:** no Rust change is expected outside liquers-py. A module is read as the asset
  `-R/<key>` through the command's `Context`, which records a dependency on it
  (`Context::evaluate`); `AssetManager::set_state` of that key then expires what depended on it.
  The work is Python's import protocol, the trust rule, and reading an asynchronous store from a
  synchronous import.
- **Open questions:**
  1. **Proposed resolution: the API.** `env.module_path` (a list of `lq.Key`, empty by default);
     `env.import_module(name, reload=False)`; inside a command `context.import_module(name,
     cwd=None)`; the `_async` forms of both; `lq.import_module` on the default environment. Plain
     `import` statements also resolve, through one finder appended to the **end** of
     `sys.meta_path`, so the standard library and installed packages always win (MODULE06).
  2. **Open design: the trust boundary.** Recommended: code runs only from a prefix in
     `module_path`, which is the trust list; there is no `search=` override. A relative import
     resolves against the `cwd` the command passes (its default link `cwd: lq.Key =
     lq.link("-R-key/.")`), only when that `cwd` lies under a trusted prefix, and never upward out
     of it (`from .. import` stops at the `cwd`). A refusal raises `lq.LiquersError` with
     `NotSupported` (MODULE09), so it is never mistaken for absence. The path is set per
     environment only, not per query. Because a server started with writes enabled could plant
     code under a trusted prefix, the docs recommend a read-only mount for code prefixes
     (`liquers-py-web-server` question 5). Alternative: a recipe folder's `cwd` is trusted implicitly.
  3. **Proposed resolution: `cwd` at the root** (MODULE04). An ad-hoc query has no recipe folder,
     so `-R-key/.` gives the root key. The root is under no trusted prefix unless the root itself
     is on the path, so a relative import is refused; absolute imports over `module_path` work.
  4. **Open design: reload and tracking.** Recommended: `import_module` checks the stored version
     on every call (a cached asset read) and re-executes the module into a fresh module object when
     its bytes changed; `reload=True` forces it. Inside a command the read records the module key
     as a dependency, so editing the module expires that command's results (MODULE07, MODULE08).
     A plain `import` statement keeps Python's `sys.modules` caching, never reloads, and records a
     dependency only when it runs inside a command of the environment (the finder knows the
     current command through a thread-local the command wrapper sets); outside one it is untracked,
     and the docs say so. Alternative: reload only when asked.
  5. **Proposed resolution: names and packages.** `pkg.sub` maps to `<prefix>/pkg/sub.py`, or
     `<prefix>/pkg/sub/__init__.py`; `__file__` and `__path__` name store keys, so a package's own
     relative imports resolve in the store (MODULE05). Prefixes are tried in order. Store modules
     share `sys.modules` with the process: one name has one meaning per process. With several
     environments, the finder uses the environment of the current command, else the default one.
  6. **Open design: the synchronous import over an asynchronous store** (MODULE11; the guide's
     central problem). `import_module` reads the bytes before it enters Python's import machinery,
     as the plan says. A plain `import` reaching the finder must read inside the import lock of that
     module, with the GIL released; Phase 2 shows no lock order can deadlock, or limits plain
     imports to modules already prefetched. In an `async def` command every blocking liquers-py
     call raises (`liquers-py/src/aio.rs`), so there the `_async` form is the way, and a plain
     store `import` raises a typed error.
  7. **Proposed resolution: commands defined by store modules** (MODULE12). A module may register
     commands in a `register(env)` function. Their version includes the module's stored version,
     not only the command's code object, so a change to a helper also gives a new version. A
     registration after the first evaluation rebuilds the environment
     (`LANGUAGE-INTEGRATIONS-REBUILD-ENVIRONMENT-ON-REGISTRATION`), so a reload re-registers only
     when a definition changed, and registering from inside a running command is refused.
- **Deviations from Appendix A** (copied to the conformance matrix): MODULE02 raises
  `ModuleNotFoundError`, Python's own absence error, not `lq.Error`/`KeyNotFound`; MODULE03 sets
  `module_path = [proj]`, imports `.helper` (an undotted name is absolute) and takes the `cwd` as a link argument, not `context.cwd_key`; MODULE04
  follows question 3; MODULE09 has no `search=` keyword (a prefix outside the path is requested
  through a relative import from an untrusted `cwd`); MODULE10 raises `ImportError` whose
  `__cause__` is the `lq.LiquersError`, as `importlib` requires of a finder.

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

**Today** neither `module_path` nor `context.import_module` exists; the code must be on the host
filesystem, and changing it leaves `4` cached. **After** the second evaluation gives `6`. `number`
is a test source command returning its argument; the queries validate with `number` and `scaled`
declared.

## Scope and Acceptance Criteria

- **AC-1** Import from a configured prefix
  WHEN `code/mymod.py` holds `VALUE = 42` and `env.module_path = [lq.parse_key("code")]`
  THEN `env.import_module("mymod").VALUE == 42`, and `import mymod` gives the same module (MODULE01)
- **AC-2** Nothing outside the path is loaded
  WHEN `other/secret.py` exists and `module_path` is `[code]`
  THEN `import_module("secret")` raises `ModuleNotFoundError` (MODULE02)
- **AC-3** Relative import against cwd
  WHEN a recipe command in trusted folder `proj` with `cwd: lq.Key = lq.link("-R-key/.")` calls
  `context.import_module(".helper", cwd=cwd)`
  THEN `proj/helper.py` is loaded (MODULE03)
- **AC-4** Root cwd
  WHEN the same command runs from an ad-hoc query, so `cwd` is the root
  THEN the relative import raises `lq.LiquersError` with `NotSupported` (MODULE04)
- **AC-5** Packages
  WHEN `code/pkg/__init__.py` and `code/pkg/sub.py` exist
  THEN `import_module("pkg.sub")` loads both, and a `from . import x` in `pkg.sub` resolves in the
  store (MODULE05)
- **AC-6** Installed modules win
  WHEN `code/json.py` exists
  THEN `import_module("json")` and `import json` give the standard library's `json` (MODULE06)
- **AC-7** Reload policy
  WHEN a module's bytes change after it was imported
  THEN `import_module` returns the new code, and `import` returns the cached module (MODULE07)
- **AC-8** Code edits expire results
  WHEN the problem example's module is rewritten between two evaluations of `number-2/scaled`
  THEN the second result is computed from the new code (MODULE08)
- **AC-9** Untrusted code is refused
  WHEN a relative import from an untrusted `cwd`, or one escaping its `cwd`, is attempted
  THEN it raises `lq.LiquersError` with `NotSupported`, and no module code runs (MODULE09)
- **AC-10** Store failures are typed
  WHEN the store fails while a module is read
  THEN the caller gets an `ImportError` whose `__cause__` is the `lq.LiquersError` (MODULE10)
- **AC-11** No deadlock in a command
  WHEN a `def` command calls `context.import_module` and an `async def` command awaits
  `context.import_module_async`, each under a 5 s timeout
  THEN both complete (MODULE11)
- **AC-12** Commands from a store module run
  WHEN a store module's `register(env)` registers `shout`
  THEN `hello/shout` evaluates to `"HELLO"`, `hello` being the test source command (MODULE12)

Non-goals: compiled extensions or bytecode from the store; a sandbox (trusted code has full rights);
package distribution from a store; reloading modules imported by plain `import`.

## Core Interactions

Store and Assets: modules read as `-R/<key>`, recorded as dependencies. Commands: the `cwd` link
default. Bindings: `module_path`, `import_module`, `Context.import_module`, a `sys.meta_path` finder.

## Crate Placement

`liquers-py` only: a new `src/modules.rs` (the store read and its dependency record) and the finder
and loader in `python/liquers/_modules.py`, since `importlib`'s protocols are Python.

## Documentation Intent

- Reference: `PYTHON_INTEGRATION.md`, a new §Modules from the store, with the trust rule.
- Guide: a "Keep Python code in the store" chapter in `PYTHON_GUIDE.md`.
- Other documents: the `MODULE` row and deviations in `liquers-py/README.md`;
  `LANGUAGE-INTEGRATION_GUIDE.md` §6 Python; `specs/README.md`.
- Documents to update: the user guide chapters `docs_index.py chapters-for` lists.
- API docs: the module docs of the new `liquers-py/src/modules.rs` and `_modules.py`, and of
  `liquers-py/src/context.rs`; docstrings on every new Python name.

## Design Dependencies

- `liquers-py-templates` (M6, PR #262, open): **requires** its `lq.link` default links (AC-3).
- `language-late-registration-without-rebuild`, `post-init-command-registration` (in review):
  **overlap** question 7.
- `liquers-py-web-server`: **overlaps** question 2 (writes over HTTP).
- `py-command-decorator-declaration-keywords` (in review): **overlaps** on argument declarations.
- `liquers-py-store`, `language-store-config-function`: **overlap** question 6 (Python stores).

## References

Guide §MODULE and Appendix A; `COMMAND_REGISTRATION_GUIDE.md` (default link `cwd`);
`PROJECT_OVERVIEW.md` (a root `cwd`); `liquers-core/src/context.rs` `Context::evaluate`;
`liquers-py/src/aio.rs`, `_commands.py`; Python `importlib.abc.MetaPathFinder`.
