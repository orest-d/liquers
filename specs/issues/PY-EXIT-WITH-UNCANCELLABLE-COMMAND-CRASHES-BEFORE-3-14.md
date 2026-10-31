---
id: PY-EXIT-WITH-UNCANCELLABLE-COMMAND-CRASHES-BEFORE-3-14
kind: issue
title: Exiting Python while a command ignores cancellation can still abort the process before CPython 3.14
status: draft
priority: P3
complexity: M
area: [py]
design:
created: 2026-10-31
github:
---
## Problem

When the interpreter exits, liquers-py's `atexit` hook (`liquers-py/src/exit.rs`) cancels the
Python commands still running on liquers threads and waits up to 5 s for them. A command that
does not check `context.is_cancelled()` and runs longer (a `time.sleep(3600)`, a long C call) is
left behind with a `RuntimeWarning`. On CPython before 3.14 the interpreter then ends that thread
when it takes the GIL back, and the unwind through liquers-py's Rust frames releases a thread state
that is no longer current: "Fatal Python error: PyGILState_Release", or a segfault. CPython 3.14
parks such a thread instead, so it is not affected.

## Expected

Exiting never crashes, whatever a command does. Options to weigh:

- keep the thread from unwinding through PyO3's guards (attach and release without a Rust drop
  guard on the threads liquers-py owns);
- wait without a bound, as Python waits for non-daemon threads, with a warning after the grace;
- document it as a limitation of the supported versions below 3.14.

## Found by

`design/language-binding-tests-in-ci/`, when the first CI run on 3.10 aborted at exit
(2026-10-31). The fix there covers every command that honours cancellation.
