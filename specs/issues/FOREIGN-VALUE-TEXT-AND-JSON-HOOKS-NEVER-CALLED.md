---
id: FOREIGN-VALUE-TEXT-AND-JSON-HOOKS-NEVER-CALLED
kind: issue
title: ForeignValue's text and JSON hooks are never called, so a foreign value cannot offer either
status: draft
priority: P3
complexity: S
area: [lib/value]
design:
created: 2026-10-20
github:
---

## Problem

`ForeignValue` (`liquers-lib/src/value/foreign.rs`) declares `try_into_string` and
`try_into_json_value`, documented as "if the language can provide a faithful one". But
`ExtValue`'s `ValueExtension` impl (`liquers-lib/src/value/mod.rs`) refuses for the `Foreign` arm
in both methods (`ExtValue::Image { .. } | ExtValue::UIElement { .. } | ExtValue::Foreign { .. } =>
Err(ext_scalar_refusal(..))`) instead of delegating to the payload. The two trait methods are dead:
`JsOpaque` (`liquers-web/src/value.rs`) implements both, and neither can ever run.

## Impact

Harmless today, because every implementation refuses. An integration that implements a faithful
JSON or text form (Python could, for a dataclass) would see it silently ignored, and a value that
could bind to a string argument or be returned over HTTP would be refused.

## Expected behaviour

The `Foreign` arms of `try_into_string`, `try_into_string_option` and `try_into_json_value`
delegate to `ForeignValue`; a test with a mock foreign value returning JSON proves it. Alternatively
remove the two methods from `ForeignValue` if delegation is not wanted. A generated dispatch
(`value-type-definition-macro`) would make this class of arm impossible.

## Discovery

Bulk design of `VALUE-TYPE-DEFINITION-MACRO` (2026-10-20), reading every `ExtValue` match arm.
