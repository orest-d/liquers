---
id: DATA-FORMAT-CONSTANTS-AND-TOOLING
kind: feature
title: Data formats are bare string literals with no constants, validation, or generic serde path
status: draft
priority: P2
complexity: L
area: [core/value, lib/value, lib/commands, macro]
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

**4. A command for specifying the data format, including its argument.** This is also the level-3
override mechanism `value-type-system` deferred: metadata records an explicitly chosen format, and
a command is how a query chooses one.

It is not optional convenience — **a refinement cannot currently be written in a query at all.**
An unescaped action parameter accepts `ALNUM`, `_`, `+` and `.` (`parse.rs`, "String action
parameters"), and no entity in the escaping table decodes to `:`. So `csv:comma` is unwritable as a
parameter, escaped or not. A command taking the base format and its argument separately is what
makes the refinement reachable:

```
… /to_format-csv-comma/…      ->  data_format = "csv:comma"
```

Two arguments, both ordinary parameters, and the `:` never appears in the query. The design should
decide whether the command sets the format for the *following* step or for the terminal result, and
how it interacts with a filename in the same query — both are questions the seeding cascade in
`specs/reference/VALUE_TYPE_SYSTEM.md` already frames.

**5. A fast binary format.** Alongside the self-describing serde formats there should be a compact
binary one for caching and cross-process transfer, where JSON's size and parse cost are the
bottleneck. Candidates worth evaluating rather than a foregone conclusion: `postcard` (compact,
`no_std`, good wasm story), `bincode` 2.x, `borsh` (deterministic encoding), `rmp-serde`
(MessagePack — self-describing, so it sits between the two families), and `rkyv` (zero-copy, but
not `serde`, so it would not ride the generic path).

Selection axes the design should state explicitly: serde compatibility, wasm support, whether the
encoding is stable across crate versions — a cache written by one build and read by the next is the
whole point — and self-describing versus schema-dependent.

That last axis carries a design consequence worth naming: **a non-self-describing format cannot be
decoded without knowing the type.** `bincode` bytes do not say what they are. That makes the type
identifier load-bearing rather than advisory, which is the strongest validation of the
`value-type-system` invariants — and it means a binary format must refuse to decode when the
declared identifier is absent or unregistered, rather than guessing.

**6. Alignment with the type-defining macro.** `VALUE-TYPE-DEFINITION-MACRO` will generate value
types and their `TypeInfo`, so the declaration site is where a type's format capability belongs. The
macro should let a type:

- **enable or disable the generic serde formats** individually — a type that is `Serialize` but
  whose YAML form is meaningless should be able to say so;
- **supply its own encoder and decoder** for a format, which is how `polars.DataFrame` gets CSV and
  Parquet and `Image` gets PNG;
- **have `supported_data_formats` derived from those two choices rather than restated**, so the
  declaration and the implementation cannot disagree — which is the failure this issue documents.

The two issues therefore share a boundary and should not be designed independently: this one owns
the format vocabulary and the generic path, the macro owns the declaration syntax that selects from
it.

Wants a design: it spans `liquers-core` and `liquers-lib`, adds public API surface, adds a command
and a dependency, and the serde-generic path interacts with both `DefaultValueSerializer` and the
type-defining macro.

## Discovery

Raised by the user on 2026-08-18 during `value-type-system` implementation, after step 3 had to
declare `supported_data_formats` for every `Value` variant and the literals became impossible to
ignore. The `SimpleValue` write/read asymmetry (`toml` readable but not writable) was found while
gathering evidence for this issue and is not otherwise tracked.

Points 4 to 6 were added by the user on the same day. The unwritability of `csv:comma` as a query
parameter was verified against `parse.rs` while recording point 4, not assumed.

Related: `CORE-METADATA-FORMAT-TYPE-CONSISTENCY` established that a declared format must be
supported by the type; this issue is about the vocabulary that declaration is written in.
`COMBINED-VALUE-DISCRIMINATION` covers the identifier side of the same serializer.
`VALUE-TYPE-DEFINITION-MACRO` owns the declaration syntax that will select from this vocabulary.
