---
id: CWD-DEFAULT-LINK-WARNS-WITHOUT-WORKING-DIRECTORY
kind: issue
title: The recommended cwd default link warns on every evaluation without a working directory
status: draft
priority: P3
complexity: S
area: [core/query]
design: 
created: 2026-10-17
github:
---
## Problem

`COMMAND_REGISTRATION_GUIDE.md` ("Passing the working directory") recommends a working-directory
argument declared as `dir: Key = query "-R-key/."`. When the query sets no working directory, the
link resolves `.` against the logical root and logs `RELATIVE_WITHOUT_CWD_WARNING`
("Relative key/query has no CWD; using logical root '/'.", `liquers-core/src/query.rs`) as a
warning of the result. The root is the intended default there, so the warning reports nothing wrong.

## Impact

Every command that follows the guide (`template/render`, `template/expand`, any alias of them)
writes a spurious warning into the log of every result evaluated without `-R-cwd/…`. A reader of
the log, or a test counting warnings, has to filter it out; a real warning is easier to miss. No
wrong value results.

## Expected behaviour

A default link that only asks for the working directory (`-R-key/.`) resolves to the root without a
warning, or the guide names a form that does; an explicit relative operand with no working
directory can keep warning.

## Discovery

`liquers-lib/tests/template_commands.rs` `error_modes_log_warnings` (design/liquers-mustache, Step
13) counted two warnings for one failing tag; the second was this one.
