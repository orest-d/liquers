---
id: REGISTER-COMMAND-EXPIRES-AND-VERSION-STATEMENTS-UNDOCUMENTED
kind: issue
title: The `expires:` and `version:` metadata statements of register_command! are undocumented
status: draft
priority: P3
complexity: S
area: [docs, macro, core/commands]
design:
created: 2026-10-04
github:
---

## Problem

`register_command!` accepts two metadata statements that no reference or guide lists:

- `expires: "<expiration spec>"` (`liquers-macro/src/registration.rs`, parse arm `"expires"`),
  emitted as `cm.expires = "<spec>".parse()?`;
- `version: auto | now | "<string>" | <integer>` (parse arm `"version"`), which sets the
  command's implementation version (`CommandImplVersionSpec`). It is used throughout
  `liquers-lib/src/commands.rs` (`version: auto`).

`specs/reference/REGISTER_COMMAND_FSD.md` §Metadata Statements has a table of every statement and
lists neither; `CLAUDE.md`'s DSL Syntax Reference and `specs/guides/COMMAND_REGISTRATION_GUIDE.md`
do not mention them either. The only way to learn the accepted forms (including that `version`
takes a bare `auto`/`now` identifier, a string that is hashed, or an integer) is the macro source.

## Impact

Command authors cannot discover how to declare a command's expiration or bump its implementation
version, which feeds `metadata_version` and dependency freshness. Silent rather than wrong: the
statements work. Low severity.

## Expected behaviour

The FSD's table lists both statements with their accepted forms and effects; `CLAUDE.md`'s DSL
list and the registration guide mention them.

## Discovery

Found 2026-10-04 while verifying `REGISTER-COMMAND-PAYLOAD-STATEMENT-UNDOCUMENTED` for
`design/register-command-payload-docs/`; kept separate so that design names one source.
