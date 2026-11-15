---
id: PY-ERROR-INSTANCES-CANNOT-BE-PICKLED
kind: issue
title: Liquers error instances cannot be pickled in Python
status: draft
priority: P3
complexity: S
area: [py]
design: 
created: 2026-11-15
github:
---
## Problem

`pickle.dumps(e)` of a caught `liquers.LiquersError` (or any generated subclass, such as
`KeyNotFoundError`) raises `TypeError: cannot pickle 'ErrorType' object`. `BaseException.__reduce__`
pickles the instance `__dict__`, and `LiquersError.__init__` (`liquers-py/python/liquers/_errors.py`
lines 67-73) stores `self.type`, a PyO3 `ErrorType` (`liquers-py/src/error.rs`), which has no
`__reduce__`. The classes themselves pickle by name.

## Impact

An error raised in a worker process (`multiprocessing`, `concurrent.futures.ProcessPoolExecutor`)
cannot be sent back to the parent, so the parent sees a pickling failure instead of the Liquers
error. Workaround: catch it in the worker and send `str(e)` or its fields.

## Expected behaviour

A Liquers error instance survives `pickle.loads(pickle.dumps(e))` with its type, message, key,
query and traceback, for example through a `__reduce__` on `LiquersError` that stores the
`ErrorType` by name, or by making `ErrorType` picklable.

## Discovery

Found by the codebase review of Phase 3 of `py-package-submodules` on 2026-11-15, which checked
whether moving the error classes to `liquers.error` keeps them picklable.
