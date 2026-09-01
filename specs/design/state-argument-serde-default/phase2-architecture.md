# Phase 2: Solution and Architecture

## Chosen Solution

Add a private `default_state_argument() -> Option<ArgumentInfo>` in
`liquers-core/src/command_metadata.rs` and use it in `#[serde(default = ...)]` on
`CommandMetadata.state_argument`. Remove `skip_serializing_if = "Option::is_none"` so a source
command serializes explicit null; otherwise a round-trip would omit the field and deserialize it as
the conventional state argument. Reuse the helper in `CommandMetadata::new` and `from_key` to own
the fact once.

## Integration Points

`liquers-core/src/command_declaration.rs` explicitly lifts state conventions and must retain its
source-command cases. `liquers-web/src/command/spec.rs` currently inserts a conventional state
argument when absent; it should remain behaviourally equal and may be simplified only in a later
cleanup. `liquers-core/src/plan.rs` consumes `state_argument` and supplies the behavioural test.

## Alternatives and Compatibility

Reject constructor-to-`None`: it changes every macro and hand-built transforming command and is not
an S-sized compatibility change. Reject a new enum distinguishing omitted/null: serde already
preserves the needed distinction. The chosen path adds explicit `null` when serializing source
commands and changes the meaning of legacy documents that omitted the field; document both effects.

## Rust Feasibility

The helper returns owned metadata and adds no lifetime, async, dependency, or error concern.
Centralization prevents constructor/serde drift. No unwrap or default match arm is needed.

## Risk Assessment

| Concern | Assessment and control |
|---|---|
| Files/crates | Primarily `command_metadata.rs`; declaration, web, plan are validation callers. |
| Existing tests | Source declaration tests must keep explicit `None`; registry output should not drift. |
| New validation | JSON/YAML omission and null, constructor equality, planning source/transform cases. |
| Compatibility/data | Omitted legacy metadata changes semantics; source serialization gains explicit null. |
| Concurrency/performance/security | None. |
| Recovery | Restore `#[serde(default)]`; no stored data rewrite is performed. |
| Certainty | High technically; omission policy remains a proposed decision. |
