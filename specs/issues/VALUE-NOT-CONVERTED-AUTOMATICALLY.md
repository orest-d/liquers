---
id: VALUE-NOT-CONVERTED-AUTOMATICALLY
kind: feature
title: A command argument, state, widget or HTTP client cannot receive a value converted to the type it declares
status: draft
priority: P2
complexity: L
area: [core/commands, core/value, macro, lib/value]
design:
created: 2026-10-20
github:
---

## Problem

Once a conversion registry exists (`specs/design/value-conversion-registry/`), nothing consults it
automatically. A command parameter `df: polars::DataFrame` still binds through `TryFrom<V>`
(`liquers-core/src/commands.rs` `CommandArguments::get`) and fails on a `RecordView`; a state of the
wrong type reaches the command unconverted; a table widget cannot accept both a frame and a list of
dictionaries; a web client's `Accept: text/csv` is not negotiated.

Example: `-R/data/sales.csv/-/ns-pl/head-5` works only because `try_to_polars_dataframe` parses the
CSV itself; a command written in plain Rust types gets no such help.

## Impact

Commands keep accepting `Any` and converting by hand, each with its own rules for lossy cases.

## Expected behaviour

Automatic conversion at the four sites of `type-conversion-draft.md` §"Where automatic conversion
applies", using `ConversionPolicy::Automatic` (refuses `Lossy` and `Fallible`): argument binding
converts to the identifier derived from the parameter's Rust type (`to_type_identifier`), then
extracts; state conversion is opt-in per command; a converted state drops its `data_format` and logs
the edge. Open: whether a purpose (`table`) is a registered identifier nothing is stored as.
Requires `COMMAND-INPUT-OUTPUT-TYPES-ARE-UNDECLARED` (argument and state types in metadata) and
`TYPE-IDENTIFIED-IN-HAS-NO-IMPLEMENTATIONS`.

## Discovery

Split out of `VALUE-CONVERSION-CAPABILITY` by its bulk design (2026-10-20), which delivers the
explicit half.
