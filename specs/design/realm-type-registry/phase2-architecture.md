# Phase 2: Solution & Architecture — a type registry that describes every realm

## Overview

Give `TypeRegistry` a serialized form (a list of `TypeInfo`), a way to import another side's
registry under a realm name, and give `TypeInfo` a declared transfer mode. Everything stays in
`liquers-core/src/type_system.rs`; `liquers-web` declares `js.Value` untransferable.

Rejected: serializing the internal `BTreeMap<TypeKey, TypeInfo>` (a struct key does not map to
JSON object keys, and the issue asks for a list); a separate `RealmTypeRegistry` type (every
consumer would have to choose between two; the existing key already holds realms); a per-realm
action table outside `TypeInfo` (the builder was added precisely so this field could go on
`TypeInfo` — `value-type-system` Phase 2, "Forward compatibility").

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `CORE-MULTI-REALM-INTERPRETER` | accepted | P3 | Consumer of the transfer modes | no | no | follow-up `CROSS-REALM-TRANSFER-NOT-PLANNED` |
| `VALUE-CONVERSION-CAPABILITY` | draft | P2 | Executes `ConvertTo` | no | no | note |
| `VALUE-TYPE-DEFINITION-MACRO` | draft | P2 | A future macro would emit `with_transfer` | no | no | note |

## Interfaces

```rust
// liquers-core/src/type_system.rs

/// What happens to a value of this type at a realm boundary.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum TransferPolicy {
    /// Crosses serialized in one of `supported_data_formats`.
    #[default]
    Serialize,
    /// Must be converted before crossing.
    ConvertTo { type_identifier: Cow<'static, str>, data_format: Cow<'static, str> },
    /// Cannot cross in any encoding; steps consuming it stay in its realm.
    Untransferable,
}

pub struct TypeInfo {
    // … existing fields …
    #[serde(default)]
    pub transfer: TransferPolicy,
}

impl TypeInfo {
    pub fn with_transfer(self, transfer: TransferPolicy) -> Self;
    /// `transfer`, except that `Serialize` with no supported format is `Untransferable`.
    pub fn transfer_mode(&self) -> TransferPolicy;
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(into = "Vec<TypeInfo>", try_from = "Vec<TypeInfo>")]
pub struct TypeRegistry { types: BTreeMap<TypeKey, TypeInfo> }

impl From<TypeRegistry> for Vec<TypeInfo> { … }            // in key order
impl TryFrom<Vec<TypeInfo>> for TypeRegistry { type Error = Error; … } // via `register`

impl TypeRegistry {
    /// Copies `other`'s default-realm entries into `realm` (relabelled). Entries `other` holds
    /// in named realms are skipped. A clash with an existing entry is an error, nothing applied.
    pub fn import_realm(&mut self, realm: &str, other: &TypeRegistry) -> Result<(), Error>;
    /// Realm names present, the default realm as "".
    pub fn realms(&self) -> BTreeSet<String>;
    /// Identifiers present in some other realm and absent from `realm`.
    pub fn types_not_in(&self, realm: &str) -> Vec<&TypeInfo>;
}
```

All synchronous: the registry is frozen at construction and read without locks (unchanged).
`import_realm` is called by whoever assembles the registry before `with_type_registry`; nothing
mutates a live environment's registry. `TryFrom`'s error is `liquers_core::error::Error`, which
serde reports through its `Display` (the type already implements it).

`import_realm` validates `realm` with `TypeKey::new` normalisation: importing into `""` is refused
with `Error::general_error` ("cannot import into the default realm"), because it would mix two
hosts' types under one name.

## Integration Points

- `liquers-core/src/type_system.rs`: the items above; tests in its `mod tests`.
- `liquers-web/src/value.rs` `js_value_type_info`: `.with_transfer(TransferPolicy::Untransferable)`.
- `liquers-py/src/value.rs` `py_type_info`: unchanged; inferred `Untransferable` without pickle,
  `Serialize` with it (a pickle crosses to another Python host; whether a non-Python realm accepts
  it is the receiver's registry's answer, not this type's).
- No change to `Environment`, `EnvironmentBuilder` or bindings' public surface.

## Error Handling

Duplicate key: the existing `register` error (`Error::general_error`). Import clash: the same
message, naming the realm. Import into `""`: `Error::general_error`. No `Error::new`, no new
`ErrorType`.

## Relevant Commands

None.

## Documentation Architecture

- `specs/reference/VALUE_TYPE_SYSTEM.md` (reference, internal, `core/value`): "Realms" — serialized
  form, `import_realm`, `TransferPolicy` and inference; History row, `reviewed:` bump.
- `specs/reference/REALMS.md` (`core/plan`): the open question "Types per realm" now points to the
  registry's realms and to `CROSS-REALM-TRANSFER-NOT-PLANNED`.
- `specs/guides/TYPE_SYSTEM_GUIDE.md`: one paragraph on declaring `with_transfer`.
- `affects_docs`: the first two (the guide is a one-line addition).

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `liquers-core/src/type_system.rs`, `liquers-web/src/value.rs` |
| Crates and workflows affected | core (registry construction), web (one entry); py, lib, axum recompile only |
| Existing tests likely to change | none; `vts4.*` tests stay valid |
| New validation | the Phase 3 tests in `type_system.rs` and one web conformance assertion |
| Compatibility / data / concurrency / performance / security | `TypeInfo`'s serialized form gains a field (`#[serde(default)]` keeps old input readable); an imported registry is untrusted data from another host, read only, never executed |
| Recovery | revert the commit; nothing persists the registry today |
| Certainty and open questions | High. Questions 1-3 are proposals; none changes the code shape |
