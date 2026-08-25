---
id: DATA-FORMAT-CONSTANTS-AND-TOOLING
kind: feature
title: Data formats are bare string literals with no constants, validation, or generic serde path
status: draft
priority: P2
complexity: L
area: [core/value, lib/value]
design:
created: 2026-08-18
github:
---
## Problem

A data format is a bare `&str` matched by literal, everywhere, with no shared vocabulary. Three
consequences follow, and all three are already visible in the code.

**1. No constants, so the sets have drifted.** `Value::as_bytes` accepts
`"txt" | "html" | "rs" | "py" | "css" | "js"` (`liquers-core/src/value.rs:935`) while
`SimpleValue::as_bytes` accepts only `"txt" | "html"` (`liquers-lib/src/value/simple.rs:539`) —
two value types disagreeing about what text is. Worse, `SimpleValue` disagrees with *itself*: its
`deserialize_from_bytes` accepts `"txt" | "html" | "toml"` (`simple.rs:632`), so `toml` can be read
but never written. Nothing catches either divergence, because nothing connects the arms.

The `value-type-system` work added `TypeInfo::supported_data_formats`, which makes the sets
*declared* rather than implicit — but it declares them with the same bare literals, and it had to
introduce a local `const TEXTUAL: [&str; 7]` in `value.rs` to avoid repeating them a fifteenth
time. That local constant is the shape of the fix, in the wrong place and at the wrong scope.

**2. No tooling, so an unknown format is indistinguishable from an unsupported one.** A typo
(`"jsonn"`) and a deliberate choice the current build cannot serve (`"parquet"` without the polars
feature) produce the same outcome, and neither can be diagnosed before evaluation. There is no way
to ask "is this a data format Liquers has ever heard of?", to list the formats a build knows, or to
suggest a near-match — the kind of thing `liquers-validate` should be able to answer offline.

**3. No generic path, so every serde-capable type hand-writes its serialization.** Most value types
are plain `serde` structures whose JSON support is a one-line `serde_json::to_vec`, repeated per
type per format. Adding YAML or CBOR today means editing every `as_bytes` and
`deserialize_from_bytes` arm in every value type, in every crate, which is why nobody has: `serde_yaml`
is a dependency of `liquers-py`, `liquers-axum` and `liquers-store` but not reachable as a Liquers
data format at all, and `toml` is an optional `liquers-store` dependency that leaks into
`SimpleValue`'s deserializer without a matching serializer.

## Impact

The sets diverge silently and asymmetrically, so a value can be written in a format it cannot be
read back from — the same class of failure as `CORE-METADATA-FORMAT-TYPE-CONSISTENCY`, one level
down. A user cannot discover what a build supports. And the formats Liquers *could* support almost
free — YAML, CBOR, MessagePack, TOML — stay unavailable because the cost is per-type rather than
per-format.

## Expected behaviour

**1. Named constants and a format vocabulary in `liquers-core`.** One module owning the identifiers
(`JSON`, `YAML`, `CBOR`, `TXT`, `BYTES`, …) with the aliases each covers — `b`/`bin`/`bytes` are one
format under three spellings today, which only the match arms know. Refinements (`csv:comma`) keep
the base/refinement split `TypeInfo::supports_data_format` already implements.

**2. Tooling over that vocabulary.** At minimum: `is_known(format) -> bool`, the list of formats a
build knows, and normalization of an alias to its canonical spelling. This is what lets a typo be
reported as a typo, and it belongs where `liquers-validate` can reach it without linking
`liquers-lib`.

**3. A generic serde path for types that can use it.** A type that is `Serialize + DeserializeOwned`
should get JSON, YAML, CBOR and friends without writing an arm per format — a blanket helper the
value type opts into per variant, so the declaration in `TypeInfo::supported_data_formats` and the
implementation come from the same place and cannot disagree.

This deliberately does **not** work for every type, and the design must make the opt-out
first-class rather than an afterthought: a Polars `DataFrame`, a `DynamicImage` and a foreign
language handle each need their own encoder, and an image written as YAML is not a sensible default
even where it would compile.

Wants a design: it spans `liquers-core` and `liquers-lib`, adds public API surface, and the
serde-generic path interacts with how `DefaultValueSerializer` is structured.

## Discovery

Raised by the user on 2026-08-18 during `value-type-system` implementation, after step 3 had to
declare `supported_data_formats` for every `Value` variant and the literals became impossible to
ignore. The `SimpleValue` write/read asymmetry (`toml` readable but not writable) was found while
gathering evidence for this issue and is not otherwise tracked.

Related: `CORE-METADATA-FORMAT-TYPE-CONSISTENCY` established that a declared format must be
supported by the type; this issue is about the vocabulary that declaration is written in.
`COMBINED-VALUE-DISCRIMINATION` covers the identifier side of the same serializer.
