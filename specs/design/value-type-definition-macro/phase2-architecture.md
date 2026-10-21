# Phase 2: Solution & Architecture — value-type-definition-macro

## Overview

A function-like proc macro `define_value_types!` in `liquers-macro` reads one block per variant and
emits the `ExtValue` enum and all of its value-type impls. Facts (identifier, type name, defaults,
formats, cfg) are literals in the declaration; every behaviour (codecs, scalar reading, JSON, data
node) is a **named function** the declaration points at, with a standard refusal when omitted. The
existing arm bodies move, unchanged, into those functions (`liquers-lib/src/value/hooks.rs`).

Rejected: `macro_rules!` (optional keys, defaults and per-arm `#[cfg]` are impractical, errors
unreadable); a derive on a hand-written enum (cannot carry per-variant hook paths without heavy
attributes, and the enum would still be written twice — once as variants, once as attributes);
generating core `Value` too (bespoke untagged serde; Phase 1 question 3); a build script reading a
data file (the value-type-system design rejected shared data files).

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `CORE-VALUE-INTERFACE-CAPABILITY-SPLIT` (`value-interface-capability-split`) | accepted | P2 | Defines the emitted `ValueType` | yes | yes (designed around: required first) | Land first |
| `FOREIGN-VALUE-TEXT-AND-JSON-HOOKS-NEVER-CALLED` | draft | P3 | The generated `foreign` arms delegate, fixing it as a side effect | no | no | Fix separately if earlier; else closed by this |
| `TYPE-IDENTIFIED-IN-HAS-NO-IMPLEMENTATIONS` | draft | P3 | Generated impls make the guide's example true | no | no | Close with this for `ExtValue` |
| `DATA-FORMAT-CONSTANTS-AND-TOOLING` (`data-format-vocabulary`) | draft | P2 | `formats:` accepts constant paths as well as literals | no | no | None |
| `COMBINED-VALUE-DATA-NODE-COPIES-BASE-VALUE` | draft | P3 | `CombinedValue`, not generated | no | no | None |

## Interfaces

**DSL** (`liquers-macro/src/value_types.rs`, parsed with `syn`):

```text
define_value_types! {
    #[doc…] pub enum ExtValue (base = SimpleValue, crate_path = crate, accessors = ExtValueInterface) {
        <Variant>(<payload type>) { <key>: <value>, … },
        …
    }
}
```

| Key | Value | Default when absent |
|---|---|---|
| `identifier` | string literal; required unless `foreign` | — |
| `type_name` | string literal | identifier in snake case |
| `defaults` | `(data_format, extension, media_type, filename)` literals or const paths | `("bin", "bin", "application/octet-stream", "data.bin")` |
| `formats` | `[…]` literals or const paths | none (metadata-only type) |
| `cfg` | the inside of a `#[cfg(...)]` | always compiled |
| `rust_type` | type path for `TypeIdentifiedIn` | no impl |
| `accessor` | ident → `from_IDENT(Arc<T>)`, `as_IDENT() -> Result<Arc<T>, Error>`; payload must be `Arc<T>` | no accessor |
| `encode` | `fn(&Payload, &str) -> Result<Vec<u8>, Error>` | `SerializationError` "not supported by <type_name>" |
| `decode` | `fn(&[u8], &str) -> Result<Payload, Error>` | no arm; identifier falls through to the foreign decoder table, then refused |
| `as_base` | `fn(&Payload) -> Result<Base, Error>`; every scalar hook reads through the base value | `Error::conversion_error(identifier, target)` |
| `json` | `fn(&Payload) -> Result<serde_json::Value, Error>` | conversion error to `JSON` |
| `data_node` | `fn(&Payload) -> AnyNode` | JSON snapshot via `json`, else opaque |
| `foreign` | bare flag; payload is `Arc<dyn ForeignValue>` | — |

**Generated items** (all synchronous, all `match` arms explicit, each carrying the variant's `#[cfg]`):

1. `#[derive(Debug, Clone)] pub enum ExtValue { Variant { value: Payload }, … }` (variant shape
   unchanged, so pattern matches elsewhere still compile).
2. `impl ValueType for ExtValue` — `type_identifier` (literal per arm; `foreign` delegates),
   `type_info` (cached lookup; `foreign` delegates), `type_descriptions` (`TypeInfo` builder chains).
3. `impl ValueExtension for ExtValue` — `try_into_string`, `_option`, `try_into_i32`, `try_into_i64`,
   `_option`, `try_into_f64`, `_option`, `try_into_bool` (via `as_base`, or delegation for
   `foreign`), `try_into_json_value`, `data_node`.
4. `impl DefaultValueSerializer for ExtValue` — `as_bytes` (`encode`), `deserialize_from_bytes`
   (`match type_identifier { "<id>" => decode … , _ => foreign decoder or refusal }` — the one
   permitted `_` arm, matching a `&str`, as today).
5. `pub trait ExtValueInterface` with the accessor pairs, `impl … for ExtValue`, and
   `impl<B: ValueInterface + Default> … for CombinedValue<B, ExtValue>` (base arm refuses).
6. `impl TypeIdentifiedIn<ExtValue> for T` and `impl<B> TypeIdentifiedIn<CombinedValue<B, ExtValue>>
   for T` per `rust_type` (orphan-safe: the value type is local).

`liquers-macro/src/lib.rs`: `#[proc_macro] pub fn define_value_types(input: TokenStream) ->
TokenStream`. Generated paths use `::liquers_core::…` and `<crate_path>::value::…`.

## Integration Points

- `liquers-macro/src/value_types.rs` (new): `ValueTypesDecl`, `VariantDecl`, `parse`, `expand`.
- `liquers-lib/src/value/hooks.rs` (new): the arm bodies of today's `mod.rs`, renamed per hook —
  `encode_image`, `decode_image`, `encode_polars`, `decode_polars`, `encode_record_view`,
  `decode_record_view`, `encode_record_source`, `decode_record_source`, `record_view_as_base`,
  `record_view_json`, `record_view_node`, `polars_node` (gated as their types).
- `liquers-lib/src/value/mod.rs`: the enum and the five impls replaced by one `define_value_types!`;
  helper fns (`record_view_cell_as_base`, …) move to `hooks.rs`.
- Accessor call sites: `from_polars_dataframe(df)` → `from_polars_dataframe(Arc::new(df))` (29).
- Unchanged consumers: `liquers-lib/src/ui/web/html.rs`, `egui/mod.rs`, `liquers-web/src/default_value.rs`.

## Error Handling

Runtime errors are today's (`Error::conversion_error`, `Error::from_error(ErrorType::SerializationError, …)`).
Declaration errors are `syn::Error` → `compile_error!` naming variant and key: missing `identifier`,
duplicate identifier or variant, unknown key, `accessor` on a non-`Arc` payload, `foreign` with
`identifier`.

## Relevant Commands

None; `specs/command_registry.yaml` is unchanged.

## Documentation Architecture

| Path | Change |
|---|---|
| `specs/reference/DEFINE_VALUE_TYPES.md` (new, reference, internal, `macro`, `lib/value`) | DSL keys, defaults, generated items |
| `specs/reference/VALUE_TYPE_SYSTEM.md` | `ExtValue` is declared; History, `reviewed:` |
| `specs/guides/TYPE_SYSTEM_GUIDE.md` | the four steps become "declare, write hooks, describe"; §A gated variant via `cfg:`; History, `reviewed:` |
| `specs/README.md` | link the new reference (done in the PR) |

Proposed `affects_docs`: the three documents above.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `liquers-macro/src/{lib,value_types}.rs`; `liquers-lib/src/value/{mod,hooks}.rs`; polars/image/records command files (accessor calls) |
| Crates and workflows affected | macro, lib; every workflow through `ExtValue`, none intentionally changed |
| Existing tests likely to change | only accessor call sites in tests |
| New validation | parser unit tests in `liquers-macro`; `scripts/check-build-matrix.sh` (every cfg row, wasm32); the AC tests of Phase 3 |
| Compatibility / data / concurrency / performance / security | API change for `from_polars_dataframe`; no stored-data change; generated matches are what was hand-written, so runtime cost is equal; compile time +small |
| Recovery | The macro output can be inspected with `cargo expand` if installed; fallback is reverting `mod.rs` to the hand-written impls (kept in git) |
| Certainty and open questions | Medium-high: every current arm maps to a key; `RecordView` scalar reading is exactly "via base value" at HEAD. Two scope/signature decisions open |
