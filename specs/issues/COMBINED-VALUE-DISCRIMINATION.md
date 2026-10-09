---
id: COMBINED-VALUE-DISCRIMINATION
kind: feature
title: Deserialization cannot discriminate base from extended values
status: closed
priority: P2
complexity: M
area: [core/value, lib/value]
design: combined-value-identifier-dispatch
created: 2026-08-08
github:
---
# COMBINED-VALUE-DISCRIMINATION


## Summary
Improve combined value deserialization so type identifiers drive correct base-vs-extended value decoding.

## Problem
Current combined value deserialization does not consistently use type discriminator/type identifier to select the intended decoding branch, risking ambiguous or incorrect reconstruction.

## Goals
1. Use type identifier as primary discriminator during decode.
2. Provide deterministic fallback rules when discriminator is absent/unknown.
3. Preserve backward compatibility for existing serialized data where feasible.

## Proposed Scope
1. Introduce dispatch table keyed by type identifier.
2. Apply deterministic decode order (known extended, then base fallback).
3. Add roundtrip tests for base and extended value families.

## Acceptance Criteria
1. Extended values deserialize through intended branch when identifier is known.
2. Base values remain decodable with stable behavior.
3. Roundtrip tests validate discriminator-driven behavior.

## Evidence (2026-10-07)

Measured through `liquers_lib::value::Value` (`CombinedValue`), whose `deserialize_from_bytes`
asks `SimpleValue` first and the extension only when the base refuses: `SimpleValue` reads
`txt` / `html` / `toml` as `Text`, `json` as plain JSON and `yaml` as a tree **whatever the type
identifier**. So a `RecordView` written as `json` reads back as an `Array`, one written as `html`
as `Text`, and a `RecordSource` manifest (`yaml`) as an `Object`. Every other `RecordView` format
reads back correctly, because the base refuses it.

Found while adding `md` to `Text` (`specs/design/text-value-markdown-format/`), which hit the same
trap and avoided it by refusing every identifier but `Text` in `SimpleValue`'s `md` arm. The
general fix is that rule for every arm: the base reads an identifier it owns, or an empty one,
and refuses the rest.

## Resolution

Fixed in `specs/design/combined-value-identifier-dispatch/`. `CombinedValue::deserialize_from_bytes`
(`liquers-lib/src/value/extended.rs`) now routes by the declared identifier: one the extension
declares in `type_descriptions()` is read by the extension alone, and its refusal is final; every
other identifier keeps the base-first order; when both halves refuse a named identifier, the error
is the one from the half that declares it, which replaces the hard-coded `polars.DataFrame` branch.

The issue's suggested route — making every `SimpleValue` arm refuse identifiers it does not own —
was not taken: the dispatch in `CombinedValue` covers every extension, including `liquers-web`'s
combinations, and leaves the base lenient for empty and undeclared identifiers. Goal 1 is the
dispatch, goal 2 the base-first fallback, goal 3 unchanged base and untyped reads.

Evidence: `record_view_json_reads_back_as_a_record_view`,
`record_source_manifest_reads_back_as_a_record_source` and
`record_view_html_refuses_instead_of_reading_as_text` (`liquers-lib/tests/record_typeinfo.rs`),
`extension_identifiers_never_read_as_base_values` (`liquers-lib/tests/value_type_system.rs`), and
five unit tests in `liquers-lib/src/value/extended.rs`.
