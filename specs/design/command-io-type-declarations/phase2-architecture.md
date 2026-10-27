# Phase 2: Solution & Architecture — commands declare the value types they accept and produce

## Overview

A small value type, `TypeSet`, holds an ordered list of type identifiers (empty = any). It is added
as `accepts` to `ArgumentInfo` — so `state_argument` and each argument carry it — and as `produces`
to `CommandMetadata`. Both are skipped when empty, so undeclared metadata serializes exactly as
today. `register_command!` gains `accepts:` / `produces:` statements and an `accepts:` parameter
attribute; the declaration format gets the keys from serde for free. `PlanBuilder` tracks the
declared type of the value flowing between steps and adds a warning when a declared input meets a
disjoint declared output. Two registry queries serve consumers: `commands_accepting` and
`check_declared_types`.

Rejected: an `ArgumentType::Value(String)` variant (question 1: conflates parsing with value type,
breaks exhaustive matches and bindings); a single `String` instead of a list (cannot say "frame or
CSV text", which every `ns-pl` command needs); refusing the plan on a mismatch (question 3);
inferring via `TypeIdentifiedIn` (no impls; question 5).

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `TYPE-IDENTIFIED-IN-HAS-NO-IMPLEMENTATIONS` | draft | P3 | Prevents inference | no | no | Inference is a non-goal |
| `TYPE-REGISTRY-NOT-REALM-AWARE` (`realm-type-registry`) | draft | P2 | Realm of declared identifiers | no | no | Command realm = type realm |
| `COMMAND-METADATA-ENHANCEMENTS` (`global-enum-registration`) | accepted | P2 | Same structs, other fields | no | no | Either order |
| `VALUE-CONVERSION-CAPABILITY` (`value-conversion-registry`) | draft | P2 | Purposes question | no | no | Groups deferred |

## Interfaces

```rust
// liquers-core/src/type_system.rs
/// Value types a command accepts or produces: ordered identifiers, the first principal.
/// Empty means any (undeclared). Serialized as a string for one entry, a list for several.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(try_from = "TypeSetRepr", into = "TypeSetRepr")]
pub struct TypeSet(Vec<String>);
impl TypeSet {
    pub fn any() -> Self;
    pub fn of(type_identifier: &str) -> Self;
    pub fn try_from_identifiers<I: IntoIterator<Item = S>, S: Into<String>>(ids: I)
        -> Result<Self, Error>;                   // refuses an empty or duplicated identifier
    pub fn is_any(&self) -> bool;
    pub fn identifiers(&self) -> &[String];
    pub fn principal(&self) -> Option<&str>;
    pub fn accepts(&self, type_identifier: &str) -> bool;   // true when any
    pub fn intersects(&self, other: &TypeSet) -> bool;      // true when either is any
}
#[derive(Serialize, Deserialize)] #[serde(untagged)]
enum TypeSetRepr { One(String), Many(Vec<String>) }      // private
```

```rust
// liquers-core/src/command_metadata.rs
pub struct ArgumentInfo {
    // …existing fields…
    /// Value types this argument (or the state) accepts. Empty: undeclared.
    #[serde(default, skip_serializing_if = "TypeSet::is_any")]
    pub accepts: TypeSet,
}
impl ArgumentInfo { pub fn with_accepts(mut self, accepts: TypeSet) -> Self; }

pub struct CommandMetadata {
    // …existing fields…
    /// Value types the command produces. Empty: undeclared.
    #[serde(default, skip_serializing_if = "TypeSet::is_any")]
    pub produces: TypeSet,
}
impl CommandMetadata {
    pub fn with_produces(&mut self, produces: TypeSet) -> &mut Self;
    pub fn with_accepts(&mut self, accepts: TypeSet) -> &mut Self; // on state_argument; no-op for a source
    /// `None` when the state is undeclared or the command is a source command.
    pub fn accepts_input(&self, type_identifier: &str) -> Option<bool>;
}
impl CommandMetadataRegistry {
    pub fn commands_accepting<'a>(&'a self, type_identifier: &'a str)
        -> impl Iterator<Item = &'a CommandMetadata> + 'a;  // executable, declared, accepting
    pub fn check_declared_types(&self, types: &TypeRegistry) -> IssueReport; // unknown id: Error
}
```

`CommandRegistry::register_alias` also copies `produces` from the target (beside
`state_argument`).

Macro (`liquers-macro/src/registration.rs`): command statements `accepts: "Id"` or
`accepts: ["A", "B"]` (state) and `produces: …`; parameter attribute `(accepts: …)`. Each emits
`TypeSet::try_from_identifiers([...])?` so a bad list fails registration, like `expires:`. Compile
errors: the statement twice, an empty list, a non-string element, `accepts:` without a state
parameter.

Plan (`liquers-core/src/plan.rs`, `PlanBuilder`): new private field
`flowing: Option<(TypeSet, CommandKey)>` — the declared type of the current value and who produced
it, `None` when unknown. `process_resource_query`, `UseQueryValue` and the `q` path set `None`.
`process_action`, after resolving metadata (for an alias, the alias's own metadata, which carries
the target's types): if `flowing` is `Some((produced, producer))` and the state's `accepts`
is declared and `!produced.intersects(accepts)`, `plan.init_warning("Type mismatch: '{cmd}' accepts
{accepts}, but '{producer}' produces {produced}")`; then `flowing = Some((produces, key))` when
`produces` is declared, else `None`.

## Integration Points

- `liquers-core/src/type_system.rs` (`TypeSet`), `command_metadata.rs` (fields; `Default` and the
  three hand-written `CommandMetadata { … }` literals; queries), `commands.rs` (`register_alias`),
  `plan.rs` (`PlanBuilder`).
- `liquers-macro/src/registration.rs` (statements, attribute, tests).
- `liquers-lib/src/image/commands.rs`, `polars/*.rs` registrations, `records/` `ns-rec`
  registrations: declarations only; function bodies untouched, so `impl_version` is unchanged.
- `liquers-py/src/command_metadata.rs`: read-only getters `accepts` / `produces` (JSON strings,
  like `argument_type`).
- No change: `command_declaration.rs` (serde carries the keys), `liquers-web` (same), validator.

## Error Handling

`TypeSet::try_from_identifiers` and deserialization: `Error::general_error("type set: empty
identifier")` / `"duplicate identifier 'X'"`. `check_declared_types`: one
`Issue::command_registry(IssueSeverity::Error, …, "declares unknown type 'X'")` per identifier. The
plan warning is a `Step::Warning`, never an `Error`.

## Relevant Commands

No command is added. Declarations on `ns-img` (47), `ns-pl` (32), `ns-rec` (35); examples:

```rust
// liquers-lib/src/image/commands.rs, `resize` (illustrative; existing statements unchanged):
//   accepts: "Image"  produces: "Image"
// liquers-lib/src/polars/selection.rs, `slice` (and so its alias pl/head):
//   accepts: ["polars.DataFrame", "RecordView", "Text", "Bytes"]  produces: "polars.DataFrame"
// rec/head: accepts: ["RecordView", "RecordSource", …as to_record accepts]  produces: "RecordView"
```

`specs/command_registry.yaml` is regenerated with a CHANGELOG line.

## Documentation Architecture

| Path | Change |
|---|---|
| `specs/reference/REGISTER_COMMAND_FSD.md` | `accepts:` / `produces:` statements, parameter attribute; History, `reviewed:` |
| `specs/reference/COMMAND_DECLARATION.md` | `accepts` on argument objects, `produces`; History, `reviewed:` |
| `specs/reference/VALUE_TYPE_SYSTEM.md` | §Declared command types (`TypeSet`, honesty rule, realm); History, `reviewed:` |
| `specs/guides/COMMAND_REGISTRATION_GUIDE.md` | "Declaring what a command accepts and produces"; History, `reviewed:` |

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | listed above, plus `specs/command_registry.yaml` |
| Crates and workflows affected | core, macro, lib, py (getters); validation output gains warnings |
| Existing tests likely to change | macro expansion snapshots in `registration.rs` tests that compare token strings; `registry_export` after regeneration |
| New validation | Phase 3 tests; `check-build-matrix.sh` (declarations sit inside `#[cfg]`-gated registrations) |
| Compatibility / data | Additive; undeclared metadata unchanged (AC-2). Declared commands get a new `metadata_version`, so results that depend on them are recomputed once. `CommandMetadata` does not deny unknown fields, so an older `liquers-validate` still reads the regenerated `command_registry.yaml` (it ignores the new keys) |
| Concurrency / performance / security | One `Option` comparison per planned action; none |
| Recovery | Revert; declarations are inert data |
| Certainty | High for the contract; per-command lists are reviewed by reading each command's input helper |
