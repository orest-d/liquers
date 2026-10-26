---
id: VALUE-NOT-CONVERTED-AUTOMATICALLY
kind: feature
title: A command's state or link argument is not converted to the type the command declares
status: draft
priority: P2
complexity: L
area: [core/commands, core/value, core/plan]
design: automatic-value-conversion
created: 2026-10-20
github:
---

## Problem

Once a conversion registry exists (`specs/design/value-conversion-registry/`), nothing consults it
automatically. The interpreter (`liquers-core/src/interpreter.rs`, `do_step`, `Step::Action`)
hands a command its input state and its link-resolved arguments exactly as they arrived; the
command's wrapper extracts arguments with `TryFrom<V>` (`CommandArguments::get` in
`liquers-core/src/commands.rs`). A command that works on a `RecordView` and receives an `Array` of
objects must call the record bridge itself; one written in Python has no such helper.

## Problem Example

```
fetch_people/summarize
```

`summarize` declares (once `COMMAND-INPUT-OUTPUT-TYPES-ARE-UNDECLARED` exists) that its state is a
`RecordView`; `fetch_people` returns an `Array` of objects. Today `summarize` receives the `Array`.
Should be: the interpreter converts it through the `Structural` edge `Array → RecordView`, logs the
edge, and `summarize` receives a `RecordView`; a `Lossy` or `Fallible` edge is never taken
automatically.

## Impact

Commands keep accepting `Any` and converting by hand, each with its own rules for lossy cases.

## Expected behaviour

Automatic conversion at the two core sites of `type-conversion-draft.md` §"Where automatic
conversion applies" — the state and link arguments — using `ConversionPolicy::Automatic` (refuses
`Lossy` and `Fallible`), driven by the types the command declares, so it works for every language.
A converted state drops its `data_format` and logs the edge. Requires
`COMMAND-INPUT-OUTPUT-TYPES-ARE-UNDECLARED` and `VALUE-CONVERSION-CAPABILITY`. The two other sites
the draft names are separate items: `UI-WIDGET-DOES-NOT-CONVERT-ITS-VALUE` and
`HTTP-ACCEPT-HEADER-NOT-NEGOTIATED`.

## Discovery

Split out of `VALUE-CONVERSION-CAPABILITY` by its bulk design (2026-10-20), which delivers the
explicit half.

Re-scoped by the bulk design of 2026-10-26 (`automatic-value-conversion`): the widget and HTTP
sites were split off, and binding is driven by declared metadata rather than by
`to_type_identifier` of the Rust parameter type, so `TYPE-IDENTIFIED-IN-HAS-NO-IMPLEMENTATIONS` is
no longer a prerequisite.
