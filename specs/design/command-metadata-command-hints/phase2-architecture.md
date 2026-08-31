# Phase 2: Solution and architecture

## Chosen solution

Add `pub hints: serde_json::Map<String, serde_json::Value>` to `CommandMetadata` beside its UI-facing metadata, with `#[serde(default)]` and `skip_serializing_if = "serde_json::Map::is_empty"`; initialize it in `new` and `from_key`, and add `with_hint(&mut self, key: &str, value: Value) -> &mut Self`. In `liquers-macro/src/registration.rs`, introduce a command-level statement parser and apply each accepted string hint as `Value::String`; reject duplicate keys at macro expansion with a useful span error. The recommended syntax is `hint icon: "play"` in the command declaration block.

Parameter `Hint` remains out of scope except that command and parameter grammar should share the existing parsing helper if practical. Rejected: attaching command facts to an arbitrary argument (wrong ownership), serializing an untyped string map (precludes present JSON values), and silently accepting duplicate keys (hides author mistakes).

`CommandMetadata` is cloned and serialized by the registry, so the map moves and derives with existing serde traits. No async, persistence, or executor ownership changes occur. Nonempty hints deliberately alter metadata version; empty maps do not serialize.

## Risks and validation

| Risk | Affected files/workflow | Validation and containment | Certainty |
|---|---|---|---|
| Registry bytes drift | `command_metadata.rs`, `specs/command_registry.yaml` | existing byte-identical export test | High |
| Public macro ambiguity | `registration.rs`, command authors | decide spelling before implementation; compile tests | Medium |
| Argument hint regression | macro parameter handling | retain/add parameter hint test when companion issue lands | Medium |
| UI assumes hints are strings | `liquers-lib` consumers | no consumer change in this design; document JSON values | High |

## Questions resolved from code

`CommandMetadata` derives `Serialize`, `Deserialize`, `Clone`, and `PartialEq`, so the same map type already used by `ArgumentInfo` fits without custom code. The macro parses parameter hints but deliberately drops them today; command-level support must not claim to complete that separate issue.
