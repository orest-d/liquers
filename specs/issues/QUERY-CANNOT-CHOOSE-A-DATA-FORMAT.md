---
id: QUERY-CANNOT-CHOOSE-A-DATA-FORMAT
kind: feature
title: A query cannot choose a data format or a format refinement except through a filename
status: draft
priority: P2
complexity: M
area: [lib/commands, core/value]
design:
created: 2026-10-20
github:
---

## Problem

A query selects a data format only through a trailing filename's extension (level 2 of the seeding
cascade in `specs/reference/VALUE_TYPE_SYSTEM.md`). A refinement such as `csv:comma` cannot be
written at all: an unescaped action parameter accepts `ALNUM`, `_`, `+` and `.`, no escape decodes
to `:`, and `liquers-validate` rejects `abc/x-csv:comma` ("Can't parse query completely", checked
2026-10-20). The level-3 override (an explicit declaration) exists only for Rust callers setting
`MetadataRecord::data_format`.

## Impact

A user cannot ask for `csv` with a semicolon separator, or for a format whose extension the
filename should not carry, from a URL. Workaround: a Rust caller, or a filename whose extension is
the base format (which loses the refinement).

## Expected behaviour

Point 4 of `DATA-FORMAT-CONSTANTS-AND-TOOLING`, moved here when that feature's first slice was
designed (`specs/design/data-format-vocabulary/`): a command taking the base format and its
refinement as two ordinary parameters, e.g. `…/to_format-csv-comma/…` meaning `csv:comma`, that
sets the data format as a level-3 declaration. The design decides whether it applies to the
following step or to the terminal result, how it interacts with a filename in the same query, and
which namespace owns it; read `specs/guides/COMMAND_REGISTRATION_GUIDE.md` first. Validated
against the vocabulary (`liquers_core::data_format`).

## Discovery

Split out on 2026-10-20 in a bulk-design run from `DATA-FORMAT-CONSTANTS-AND-TOOLING` (point 4 was
added by the user on 2026-08-18, with the unwritability verified against `parse.rs` then).
