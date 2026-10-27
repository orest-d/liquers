---
id: UI-WIDGET-DOES-NOT-CONVERT-ITS-VALUE
kind: feature
title: A UI view renders a value only by its own variant, never by converting it to a type it can show
status: draft
priority: P3
complexity: M
area: [lib/ui, lib/value]
design: ui-display-conversion
created: 2026-10-26
github:
---

## Problem

Both value backends choose a rendering by matching the value's variant: the HTML backend
(`liquers-lib/src/ui/web/html.rs`, `ext_to_html` / `simple_to_html`) and egui
(`liquers-lib/src/egui/mod.rs`, `UIValueExtension::show`). A `polars.DataFrame` and a `RecordView`
render as tables, while an `Array` of objects — the same data as a list of dictionaries — renders
as the bare label `Array` (`<span class="lq-value lq-array">Array</span>`; egui: an italic
`Array`), not even as JSON. Nothing consults the conversion registry planned by `value-conversion-registry`
(`can_convert`, `ConversionPolicy::Automatic`) to find a type the view can show.

## Problem Example

```
fetch_people
```

where `fetch_people` returns `[{"name": "Ann", "age": 31}, {"name": "Bo", "age": 27}]`. Today the
view shows the label `Array` (checked at `cd6d5da`). Should be: the view asks whether the value reaches a table type by an
automatic edge (`Array → RecordView`, `Structural`), converts for display only, and shows a table.

## Impact

Every command that wants a tabular display must return a frame or a record view itself; a Python
command returning a list of dictionaries is shown as text.

## Expected behaviour

A view that renders a kind of content ("a table", "an image") accepts any value reaching one of its
types by an automatic edge, converts for display without changing the asset, and falls back to
today's rendering otherwise. Whether "a table" is a *purpose* (a registered identifier nothing is
stored as, `type-conversion-draft.md` open question 1) is decided here, since this is the first
consumer that needs it. Requires `value-conversion-registry`; the egui backend has the same gap.

## Discovery

Split out of `VALUE-NOT-CONVERTED-AUTOMATICALLY` by its bulk design (2026-10-26,
`automatic-value-conversion`), which covers only the command state and link arguments.
