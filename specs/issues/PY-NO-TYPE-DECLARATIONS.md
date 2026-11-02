---
id: PY-NO-TYPE-DECLARATIONS
kind: feature
title: liquers-py ships no type declarations, so type checkers see the package as untyped
status: closed
priority: P2
complexity: L
area: [py]
design: liquers-py-stubs
created: 2026-11-01
github:
---
## Problem

`liquers-py/python/liquers/` has no `.pyi` file and no `py.typed` marker, and PyO3 emits no
declarations for the native extension `liquers._native`. A type checker therefore treats the whole
package as untyped: mypy reports `import-untyped` on `import liquers`, and every name, including a
command decorated with `lq.command`, is `Any`. `specs/reference/PYTHON_INTEGRATION.md` §8 lists
"no type stubs" as a limitation, and `specs/guides/PYTHON_GUIDE.md` §10 says the same.

## Impact

Python users get no completion, no signature help and no static checking for any `liquers` call;
a wrong argument (`lq.parse(42)`) is found only at run time. This is the guide's `STUBS` feature
(`specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §STUBS), selected for liquers-py in milestone M5.

## Expected behaviour

The package ships declarations for every public module, a `py.typed` marker, and decorators typed
so a command keeps its own signature; CI fails when the declarations and the runtime surface
drift (`STUBS01`-`STUBS07`). Design: `specs/design/liquers-py-stubs/`.

## Discovery

Filed 2026-11-01 when the M5 design folders were created (liquers-py plan, M5); the limitation was
documented but had no issue.

## Resolution

Closed 2026-11-01 by `design/liquers-py-stubs/` (PR #240). The package ships `py.typed`,
`__init__.pyi` and `_native.pyi`; the pure-Python modules are annotated.
`liquers-py/tests/test_STUBS_declarations.py` runs stubtest against the runtime, compares the native
names and the generated error classes, and checks typed samples under mypy `--strict` and pyright
`strict`. Contract: `reference/PYTHON_INTEGRATION.md` §13.
