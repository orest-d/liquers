# Phase 2: Solution & Architecture — value-interface-capability-split

## Overview

Add `liquers_core::type_system::ValueType`, an object-safe trait holding a value's identity and
description. `ValueInterface`, `ValueExtension` and `ForeignValue` each drop their own copies of
`identifier`, `type_name`, `default_*`, `type_info`, `type_descriptions` and
`supports_data_format` and take `ValueType` as a supertrait. An implementor writes two methods —
`type_identifier` (a cheap match) and `type_info` — and every other fact is derived from the
`TypeInfo`.

Rejected alternatives:
- **Split `ValueInterface` into scalar / core-payload / JSON sub-traits.** Every one is called on
  `E::Value` by the engine (`CommandArguments::get`, built-in commands, axum, templates), so each
  would reappear in the `Environment::Value` bound; no implementor could opt out.
- **Drop `DefaultValueSerializer` as a supertrait.** The asset layer persists every value
  (`State::as_bytes`, `AssetManager::set_state`), so the bound would move to ~every generic
  signature in `assets.rs` for no capability gained; refusal is already the opt-out.
- **Derive `type_identifier` from `type_info` too.** It is called on every `State` construction
  and on every `CombinedValue` read dispatch; a match on a borrowed `&'static str` stays cheapest.
- **Default `type_info` to a minimal fallback.** A described type that forgets to override it would
  silently report `bin` defaults — the drift this design removes. Required, with a helper.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `CORE-VALUE-ENUM-OVERSIZED` (`core-value-enum-oversized`, ready) | draft | P2 | Same `impl ValueInterface for Value` block | no | no | Either order; second rebases |
| `COMBINED-VALUE-DATA-NODE-COPIES-BASE-VALUE` | draft | P3 | `CombinedValue::data_node`, not touched here | no | no | None |
| `DATA-FORMAT-CONSTANTS-AND-TOOLING` (`data-format-vocabulary`) | draft | P2 | Rewrites `type_descriptions()` bodies with constants | no | no | Independent |
| `FOREIGN-VALUE-TEXT-AND-JSON-HOOKS-NEVER-CALLED` (filed now) | draft | P3 | Same `ExtValue::Foreign` arms, other methods | no | no | Separate fix |
| `TYPE-IDENTIFIED-IN-HAS-NO-IMPLEMENTATIONS` (filed now) | draft | P3 | `TypeIdentifiedIn` untouched | no | no | None |

## Interfaces

```rust
// liquers-core/src/type_system.rs — object safe; used as `dyn ForeignValue`'s supertrait.
pub trait ValueType {
    /// The dispatch key; one per variant. Was `identifier`.
    fn type_identifier(&self) -> Cow<'static, str>;
    /// This value's description, as registered (a foreign value refines `type_name`).
    fn type_info(&self) -> TypeInfo;
    /// Every type this implementor can hold; seeds `TypeRegistry::from_value_type`.
    fn type_descriptions() -> Vec<TypeInfo> where Self: Sized { Vec::new() }
    fn type_name(&self) -> Cow<'static, str> { self.type_info().type_name }
    fn default_data_format(&self) -> Cow<'static, str> { self.type_info().default_data_format }
    fn default_extension(&self) -> Cow<'static, str> { self.type_info().default_extension }
    fn default_filename(&self) -> Cow<'static, str> { self.type_info().default_filename }
    fn default_media_type(&self) -> Cow<'static, str> { self.type_info().default_media_type }
    fn supports_data_format(&self, data_format: &str) -> bool {
        self.type_info().supports_data_format(data_format)
    }
}

impl TypeInfo {
    /// The entry for `type_identifier` in `descriptions`, or a minimal description (identifier as
    /// type name, `bin` defaults, no formats) when absent. The body of every enum's `type_info`.
    pub fn find(descriptions: &[TypeInfo], type_identifier: &str) -> TypeInfo;
}
```

Changed traits (supertrait lists only; the listed methods are deleted from each):
`ValueInterface: ValueType + Debug + Clone + Sized + DefaultValueSerializer + MaybeSend + MaybeSync
+ 'static`; `ValueExtension: ValueType + …` (unchanged otherwise); `ForeignValue: ValueType +
Debug + MaybeSend + MaybeSync + 'static`. `TypeRegistry::from_value_type::<V: ValueType>()` (was
`V: ValueInterface`). All synchronous: pure functions of the value.

Performance: a concrete enum (`Value`, `SimpleValue`, `ExtValue`) keeps its descriptions in a
private `static DESCRIPTIONS: std::sync::OnceLock<Vec<TypeInfo>>` and its `type_info` clones one
entry (one small `Vec` clone), instead of rebuilding every description per call as the current
`ValueInterface::type_info` default does. `CombinedValue` delegates by arm, as today. `OnceLock`
works on `wasm32`.

## Integration Points

- `liquers-core/src/type_system.rs` — `ValueType`, `TypeInfo::find`, `from_value_type` bound.
- `liquers-core/src/value.rs` — `ValueInterface` loses 9 methods and the TODOs at the trait head
  and on `identifier` / `type_name`; `impl ValueType for Value` (one match, cached lookup); the
  `type_name` / `default_*` matches (60 arms) deleted. A doc comment on `DefaultValueSerializer`
  records why it stays a supertrait.
- `liquers-core/src/state.rs` `sync_metadata_with_value`, `effective_data_format`;
  `assets.rs`, `metadata.rs`, `context.rs` and tests: `identifier()` → `type_identifier()`
  (32 call sites in core); add `use crate::type_system::ValueType` where a concrete type is used.
- `liquers-lib/src/value/simple.rs` — as core `Value`.
- `liquers-lib/src/value/extended.rs` — `ValueExtension` loses its 7 copies; `CombinedValue`
  implements `ValueType` with two delegating arms (was six); test extensions updated.
- `liquers-lib/src/value/mod.rs` — `impl ValueType for ExtValue` (identifier match; `type_info`
  match delegating `Foreign` to the payload, cached lookup otherwise); delete `type_name` and the
  three `default_*` matches (36 cfg-gated arms) and `default_ext_type_info`.
- `liquers-lib/src/value/foreign.rs` — `ForeignValue` loses `identifier`, `default_extension`,
  `default_filename`, `default_media_type` and the derived `type_info` default; keeps `type_name`
  (per instance) through `ValueType`'s default override. ~92 `identifier()` call sites in lib.
- `liquers-py/src/value.rs` `impl ForeignValue for PyObjectValue` — implement `ValueType`
  (`type_identifier`, `type_info` = `py_type_info(..).with_type_name(..)`, `type_name`); delete
  three methods; two call sites.
- `liquers-web/src/value.rs` `JsOpaque` (same shape); `liquers-web/src/lib.rs` getter
  `identifier` → `typeIdentifier` (question 4); `bridge.rs`; `tests/second_value_type.rs`
  `MatrixExt` gains a `type_info`.
- `liquers-axum`, `liquers-mcp`, `liquers-agent`, `liquers-records`, `liquers-template`: no call
  sites (checked by grep); a `cargo check` confirms.

## Error Handling

No new failure. `TypeInfo::find`'s fallback is infallible, as `ValueInterface::type_info`'s is today.

## Relevant Commands

None added or changed; `specs/command_registry.yaml` is unaffected.

## Documentation Architecture

| Path | Change |
|---|---|
| `specs/reference/VALUE_TYPE_SYSTEM.md` | §Two axes / §Registering a type an integration owns: `ValueType`, derived defaults, `type_identifier()`; History row, `reviewed:` |
| `specs/guides/TYPE_SYSTEM_GUIDE.md` | §3 "Add the arms" becomes two methods; §Verifying it names the new test; History, `reviewed:` |
| `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` | §VALUE `ForeignValue` listing and the `JsOpaque` example; History, `reviewed:` |
| `specs/reference/PYTHON_INTEGRATION.md` | method names where `ForeignValue` is described; History, `reviewed:` |

Proposed `affects_docs`: the four paths above. No `specs/README.md` link changes.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `type_system.rs`, `value.rs`, `state.rs`, `assets.rs` (renames), `simple.rs`, `extended.rs`, `mod.rs`, `foreign.rs`, lib command and UI files calling `identifier()`, `liquers-py/src/value.rs`, `liquers-web/src/{value,lib,bridge}.rs`, web test |
| Crates and workflows affected | core, lib, py, web; every workflow reads identifiers, none changes behaviour |
| Existing tests likely to change | renames only; `combined_value_delegates_all_defaults` and the foreign mocks lose methods |
| New validation | Phase 3's three tests; `scripts/check-build-matrix.sh` (cfg arms deleted); `scripts/test-liquers-py.sh`; the `liquers-web` Node loop and `check-stubs.sh` |
| Compatibility / data / concurrency / performance / security | API break by design (no compatibility kept). Stored data unchanged (AC-4). `type_name()` now clones one cached `TypeInfo` per call — on `State` construction, beside a whole `Metadata` clone, so negligible. No concurrency or security effect |
| Recovery | Mechanical; revert the commit. Partial landing is not possible (one trait change) — do it in one PR |
| Certainty and open questions | High on feasibility (every implementor inspected); four naming/scope decisions open (Phase 1) |
