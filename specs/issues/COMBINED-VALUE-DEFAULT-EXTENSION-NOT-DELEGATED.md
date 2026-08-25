---
id: COMBINED-VALUE-DEFAULT-EXTENSION-NOT-DELEGATED
kind: issue
title: CombinedValue::default_extension returns "ext" for every extended value
status: closed
priority: P2
complexity: S
area: [lib/value]
design: value-type-system
created: 2026-08-18
github:
---
## Problem

`liquers-lib/src/value/extended.rs:150` — `CombinedValue::default_extension` does not delegate to
the extension:

```rust
fn default_extension(&self) -> Cow<'static, str> {
    match self {
        CombinedValue::Base(base) => base.default_extension(),
        _ => "ext".into(),
    }
}
```

Every sibling method on the same impl delegates properly — `identifier` (`:136`), `type_name`
(`:143`), `default_filename` (`:161`), `default_media_type` (`:167`). Only this one substitutes the
constant `"ext"`.

The default match arm is also against the codebase convention (`CLAUDE.md`, "Match Statements"):
an explicit `CombinedValue::Extended(ext)` arm would have made this a compile error to overlook
rather than a silent constant.

## Impact

An extended value reports a self-contradictory set of defaults: `ExtValue::Image` yields
`default_filename() == "image.png"` and `default_media_type() == "image/png"`, but
`default_extension() == "ext"`. Since `ValueInterface::default_data_format` derives from
`default_extension` (`liquers-core/src/value.rs:209`), the default *data format* of every extended
value is also `"ext"` — a format no serializer implements.

This bites exactly where the type/format consistency work lands: an extended value with no
explicitly declared `data_format` seeds level 1 from the value's default, which would be `"ext"`
rather than `"png"`, so serialization fails or is misrecorded.

## Expected behaviour

Delegate to `ext.default_extension()`, matching every other method on the impl, and enumerate the
variants explicitly instead of using `_ =>`.

## Resolution

Fixed 2026-08-18 as step 0a of `value-type-system`. `CombinedValue::default_extension` now
delegates to `ext.default_extension()` through an explicit `CombinedValue::Extended(ext)` arm,
matching its four siblings; the `_ =>` arm is gone, so a future variant is a compile error here as
the codebase convention intends.

Regression tests in `liquers-lib/tests/value_type_system.rs`: `combined_value_delegates_all_defaults`
pins the delegated values, and `combined_value_defaults_are_mutually_consistent` pins the invariant
the constant violated — that `default_filename` ends in `default_extension` and
`default_data_format` derives from it. Both were confirmed failing before the fix
(`left: "ext"`, `right: "png"`) and passing after.

## Discovery

Found while checking `CombinedValue`'s identifier delegation during `value-type-system` Phase 1,
2026-08-18. The delegation claim being verified was correct; this neighbouring method was not.
