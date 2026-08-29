Based on `HEAD`, read rather than remembered. Nothing here is implemented.

# Phase 2 — Solution and architecture

## Chosen solution

A new module `liquers-core/src/command_declaration.rs`, added to `lib.rs` alongside
`command_metadata` (`lib.rs:119`), containing the declarative half of a command and nothing else.

```rust
/// An author-facing command declaration: everything about a command except its implementation.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct CommandDeclaration {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")] pub namespace: String,
    #[serde(default, skip_serializing_if = "String::is_empty")] pub realm: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]  pub label: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")] pub doc: String,
    #[serde(default, skip_serializing_if = "String::is_empty")] pub module: String,
    #[serde(default, skip_serializing_if = "String::is_empty")] pub filename: String,
    /// Which form of the input state the implementation receives. Absent = `none`.
    #[serde(default)] pub state: StateMode,
    /// Absent means "not declared": the host may infer, or refuse.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Vec<ArgumentDeclaration>>,
    /// `None` = not declared; the host decides from the callable.
    #[serde(default, rename = "async", skip_serializing_if = "Option::is_none")]
    pub is_async: Option<bool>,
    #[serde(default, skip_serializing_if = "is_false")] pub volatile: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub cache: Option<bool>,
    #[serde(default)] pub expires: Expires,
    #[serde(default, skip_serializing_if = "PayloadRequirement::is_none")]
    pub payload_required: PayloadRequirement,
    /// Name of the implementation, resolved by the host against its own table of callables.
    #[serde(default, skip_serializing_if = "Option::is_none")] pub run: Option<String>,
}

/// One declared argument. A thin, defaulting mirror of [`ArgumentInfo`].
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct ArgumentDeclaration {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub label: Option<String>,
    #[serde(default, rename = "type")] pub argument_type: ArgumentType,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub default: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "is_false")] pub multiple: bool,
    #[serde(default, skip_serializing_if = "is_false")] pub injected: bool,
    #[serde(default)] pub gui_info: ArgumentGUIInfo,
}

/// Which form of the input state the implementation receives.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum StateMode {
    /// No state argument — a source command. Serde alias-free default.
    #[default] None,
    Value,
    #[serde(alias = "string")] Text,
    State,
}

impl CommandDeclaration {
    pub fn key(&self) -> CommandKey;
    /// Validated conversion. Fails on an empty name, a `multiple` argument that is not last,
    /// and an argument default that does not fit its declared type.
    ///
    /// It does **not** validate an `ArgumentType::GlobalEnum` reference: resolving one needs a
    /// `CommandMetadataRegistry` (`ArgumentType::resolve_global_enums`), which a declaration does
    /// not have. Global-enum resolution stays where it happens today, at registry insertion and
    /// plan building.
    pub fn to_metadata(&self) -> Result<CommandMetadata, Error>;
    /// The inverse, for the round-trip test and for describing a registered command.
    pub fn from_metadata(metadata: &CommandMetadata, state: StateMode) -> Self;
}
```

`to_metadata` builds on `CommandMetadata::from_key` (`command_metadata.rs:920`) so that every
default — `cache: true`, `definition: Registered`, `label` from the name — comes from the one place
that already owns it. `state: none` is the only case that clears `state_argument`;
`value`/`text`/`state` keep `Some(ArgumentInfo::any_argument("state"))`, which is exactly today's
behaviour for every JavaScript command.

**One default is not shared: `label`.** `CommandMetadata::from_key` derives it as
`key.name.replace("_", " ")` (`command_metadata.rs:925`), while `JsCommandSpec::parse` uses the name
**unchanged** (`spec.rs:166`). For a JavaScript command named `foo_bar` the two disagree —
`foo bar` against `foo_bar` — and because `metadata_version` is computed from the stored metadata
(`command_metadata.rs:1036`), adopting `from_key`'s default would silently change the version of
every underscored JavaScript command, against this design's own compatibility requirement. So
`liquers-web` sets `metadata.label` from `declaration.label.unwrap_or_else(|| name.clone())` after
conversion, preserving today's behaviour, and a parity test covers `foo_bar` specifically. See
open question 5 — normalising the two defaults instead is defensible, but it is a behaviour change
and must be chosen, not slipped in.

## Rejected alternatives

| Option | Verdict |
|---|---|
| Use `CommandMetadata` directly as the declaration format, adding `#[serde(default)]` to `label`, `cache` and `definition` | Rejected. It makes the *export* format lossy in the other direction — a registry file missing `cache` would silently deserialize as `true` — and it still cannot carry `state`, tri-state `async`, or `run`. Measured evidence for the three missing defaults is in Phase 1. |
| Put `CommandDeclaration` inside `command_metadata.rs` | Rejected: that file is already 1397 lines, and declaration versus metadata is exactly the distinction this issue exists to draw. A sibling module makes the split legible. |
| A `#[serde(flatten)]` wrapper `NamedCommandDeclaration { decl, run }` | Rejected: `flatten` needs `deserialize_any`, which interacts badly with `serde-wasm-bindgen` and with self-describing-format assumptions. A plain `run: Option<String>` field costs nothing (**resolves Q1**). |
| Keep the format in `liquers-web` and have Python depend on it | Rejected by the issue: `liquers-web` is wasm32-only. |

## `liquers-web` re-implementation

`JsCommandSpec` (`spec.rs:81`) keeps its shape — `key`, `metadata`, `state_mode`, `is_async`, `run`,
`arguments_inferred` — and `register_js_command` (`adapter.rs:79`) is untouched. Only `parse`
changes:

1. `Reflect::get(spec, "run")`, checked with `is_function()` — unchanged, with its current message
   (`Command {name:?} must have a `run` function`).
2. Build a shallow copy of the declaration object **without** `run`, then deserialize it into
   `CommandDeclaration` with `serde_wasm_bindgen` (already a dependency:
   `liquers-web/Cargo.toml:32`). Functions and other non-data fields never reach serde.
3. Refuse `namespace == RESERVED_NAMESPACE` — unchanged, keeping the `"reserved"` wording that
   `command06_ns_reserved_namespace_is_refused` asserts.
4. `declaration.to_metadata()?`, then set `module = "javascript"`.
5. When `arguments` is `None`, run the existing `infer_arguments` (`spec.rs:281`) — unchanged,
   including every refusal message `command05_infer_refused_shapes` asserts.
6. Map `declaration.is_async` to `IsAsync`: `Some(true) → Async`, `Some(false) → Sync`,
   `None → Auto`. `IsAsync` stays in `liquers-web`, because "test whether the result is thenable" is
   a JavaScript notion; the wire format carries only the tri-state as `Option<bool>`.

Two incidental improvements fall out and are worth naming rather than discovering later:

- `snapshot_declaration` (`environment.rs:171-193`) exists only because `REGISTERED_SPECS` retains
  the caller's own `JsValue` and a caller may mutate it (`command14`). Retaining a parsed
  `(CommandDeclaration, js_sys::Function)` pair is immune by construction. **Deferred, not done** —
  it changes replay-on-rebuild and belongs with `POST-INIT-COMMAND-REGISTRATION`. Recorded here so
  the opportunity is not lost.
- A declaration may now express `filename`, `expires` and `payload_required`, which the current
  parser silently ignores. That is a widening of the JavaScript API; it needs a line in the
  TypeScript stubs (`liquers-web/tests/stubs/`) and a decision at the gate.

## Argument types and diagnostics (Q2)

`ArgumentType` (`command_metadata.rs:155`) serializes as `string | int | int_opt | float |
float_opt | bool | any | none`, plus externally-tagged `Enum` / `GlobalEnum`. `parse_argument_type`
(`spec.rs:236`) accepts `string|str|text`, `int|integer`, `float|number`, `bool|boolean`, `any` —
overlapping but not equal.

**Resolution:** add `#[serde(alias = …)]` to `ArgumentType` for `str`, `text`, `integer`, `number`,
`boolean`. Aliases affect deserialization only, so `specs/command_registry.yaml` and the
`registry_export` comparison are untouched. Two consequences, both to be accepted deliberately:

- A JavaScript declaration may now name `int_opt`, `float_opt`, `none`, and enum forms. This is a
  widening, and a welcome one — `COMMAND-METADATA-ENHANCEMENTS` wants richer argument types anyway.
- An **unknown** type name now produces serde's `unknown variant "zzz", expected one of …` instead
  of the current `Command "c", argument "n": unknown type "zzz"; expected "string", "int", "float",
  "bool" or "any"`. No test asserts that string. To keep the command and argument identifiable,
  `JsCommandSpec::parse` wraps every serde failure as
  `Error::from_error(ErrorType::ParameterError, format!("Command {name:?}: {e}"))`, so the
  diagnostic gains the serde detail and keeps the command name.

## Argument defaults

`ArgumentDeclaration.default` is `Option<serde_json::Value>`, converted to
`CommandParameterValue::Value(...)` — the same representation `parse_arguments` produces today
(`spec.rs:227`), chosen for the same reason (the planner must resolve a default without re-entering
the host language). `CommandParameterValue::Query` is reachable from a Rust `register_command!`
default (`query "…"`) but **not** from a declaration in this issue: a query-valued default in a
document is a distinct feature and would need its own validation. Recorded as a known gap rather
than silently dropped — no command in `specs/command_registry.yaml`
carries one today (`grep -c '!Query'` returns 0), so the registry round-trip is unaffected — but a
future one would need handling, and the test should fail loudly rather than silently skip.

## Data ownership, errors, sync/async

- `CommandDeclaration` owns its data (`String`, `Vec`, `serde_json::Value`); no lifetimes, no `Arc`.
  It is `Clone` so a host may retain it for replay.
- Every failure is `liquers_core::error::Error` via `Error::from_error(ErrorType::ParameterError, …)`
  — no new error type, no `Error::new`.
- Nothing async: declaration parsing is pure. The declared `async` flag describes the
  *implementation*, and dispatch stays where it is, in `adapter.rs`.
- `to_metadata` returns `Result` rather than panicking; `from_metadata` is infallible.

## Reuse

`CommandMetadata::from_key`, `ArgumentInfo::any_argument`, `ArgumentType`, `ArgumentGUIInfo`,
`CommandParameterValue`, `Expires`, `PayloadRequirement` and `CommandKey` are all reused unchanged.
`infer_arguments`, `parameter_list`, `strip_comments` and `is_plain_identifier` stay in
`liquers-web`: they parse JavaScript source, which core has no business knowing.

## Related open issues

- `POST-INIT-COMMAND-REGISTRATION` (P3, `accepted`) — the other half of the document-driven
  ergonomics; not a prerequisite, and the `snapshot_declaration` cleanup belongs to it.
- `STORE-CONFIG-IN-CORE` (P0) — document #1; independent of this one.
- `COMMAND-METADATA-ENHANCEMENTS`, `REGISTER-COMMAND-ENUM` — both would extend the same field set;
  the declaration must not foreclose them, which is why `ArgumentType` is reused rather than
  re-enumerated.
- `REGISTRY-IMPL-VERSION-DRIFT-UNDETECTED` — relevant to the round-trip test: `impl_version` comes
  from registration, not from the declaration, so the round-trip must compare metadata *excluding*
  `impl_version`, or set it explicitly.

## Risk analysis

| Assessment | Record |
|---|---|
| **Files** | Source: `liquers-core/src/command_declaration.rs` (new, ~300 lines), `liquers-core/src/lib.rs` (one `pub mod`), `liquers-core/src/command_metadata.rs` (serde aliases on `ArgumentType`), `liquers-web/src/command/spec.rs` (rewrite of `parse`, deletion of `get_string`/`get_bool`/`parse_arguments`/`parse_argument_type`/`js_default_to_json`, ~150 lines removed). Tests: colocated tests in the new module, plus `liquers-core/tests/` for the registry round-trip. Specs: possibly `REGISTER_COMMAND_FSD.md` pointer + its `reviewed:`/History rows; `specs/index.csv` regenerated. Generated files: **none** — `specs/command_registry.yaml` must be byte-identical. |
| **Impact area** | `core/commands`, `web`. Downstream: every JavaScript command registration path, `describeCommand`, and the environment rebuild/replay path via `REGISTERED_SPECS`. `liquers-py` is unaffected until it opts in. |
| **Module/crate reach** | **Not confined to one module.** Crates crossed: `liquers-core` (two modules) and `liquers-web` (one module). This alone fails the automatic-clearance condition. |
| **Existing-test breakage** | Expected **0 assertion failures**, but the estimate is soft. At risk: the 20 `wasm_bindgen_test`s in `liquers-web/tests/commands_COMMAND.rs`, of which four assert error wording (`:66`, `:409`, `:422`, `:509`) — all four preserved by keeping their producing code in `liquers-web`. `liquers-core/src/command_metadata.rs`'s own tests (`:1210`, `:1236`, `:1254`) must be unaffected by the alias additions; aliases do not change serialization, so they should not be. `liquers-lib`'s `registry_export` must stay green. The honest number is "0 expected, ~24 tests in the blast radius". |
| **New validation** | (1) Registry round-trip: every command in `specs/command_registry.yaml` → `CommandDeclaration::from_metadata` → `to_metadata` → equality modulo `impl_version`/`metadata_version`. (2) Minimal declaration `{"name":"greet"}` yields `CommandMetadata` equal to `CommandMetadata::from_key`. (3) Parity with `register_command!` for one representative command including `metadata_version` after registry insertion. (4) YAML and JSON both parse the same declaration to the same value. (5) Malformed declarations: empty name, `multiple` not last, unknown argument type, non-array `arguments`. (6) The whole `liquers-web` COMMAND suite under Node. Commands to run: `cargo test -p liquers-core --lib`, `cargo test -p liquers-lib --lib --tests`, `bash scripts/check-build-matrix.sh`, and — after `cargo clean` — `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles`. |
| **Behavioural risk** | *Compatibility*: the JavaScript declaration surface widens (new accepted fields and type names) and one diagnostic's wording changes; both are deliberate and listed. *Persistence/data*: `specs/command_registry.yaml` is a committed generated file and must not move — enforced by `registry_export`. *Concurrency*: not applicable — parsing is pure and the `INFERRED_ARGUMENTS` thread-local is untouched. *Performance*: not applicable — registration is not a hot path. *Security*: a declaration is host-supplied data that becomes registered metadata; it cannot name a Rust implementation, so `run` resolution stays entirely with the host. *Error paths*: serde failures replace hand-written ones for malformed fields; wrapped so the command name survives. |
| **Recovery** | The core module is additive and can stay. The `liquers-web` rewrite is the risky half and is revertible on its own — `JsCommandSpec`'s public shape is unchanged, so reverting `parse` restores the old behaviour without touching `adapter.rs` or `environment.rs`. Sequencing the work as "core type + tests" then "web rewrite" keeps that boundary real. |
| **Certainty** | Q2 is resolved above but changes accepted input, which is a judgement the maintainer may want. Unverified: that `serde-wasm-bindgen` deserializes a JS object into `Option<serde_json::Value>` for argument defaults — that needs `deserialize_any`, which it supports, but it has not been executed here. Fallback if it does not hold: `js_sys::JSON::stringify` on the run-less copy, then `serde_json::from_str`; the cost is that a non-JSON default becomes "absent" instead of an error. The `!Query` default question was checked and is currently moot (no such command in the file). Two claims in an earlier draft were wrong and are corrected above, both found by a review bot and confirmed against the code: `to_metadata` cannot validate a global-enum reference without a registry, and the `label` default is not shared between the two registration routes. |

## Open questions for the gate

1. **Widening the JavaScript surface.** `filename`, `expires`, `payload_required`, and the extra
   `ArgumentType` variants become declarable. Accept, or restrict the JavaScript path to today's
   field set? **Recommendation:** accept — one format is the point of the issue, and a restricted
   subset would be a second format again.
2. **Diagnostic wording for an unknown argument type** changes from bespoke to serde-derived (with
   the command name prefixed). Acceptable? **Recommendation:** accept; no test asserts it and the
   serde message lists the valid names.
3. **Split into two changes?** "core type + tests" and "`liquers-web` rewrite" are separable, and
   the second is where the risk is. **Recommendation:** one PR, two commits, so the web half can be
   reverted alone.
4. **Query-valued defaults** are out of scope; a document cannot declare `default: query "…"`.
   Confirm that is acceptable for now.
5. **The `label` default split.** `liquers-web` keeps the name verbatim; Rust replaces underscores
   with spaces. Preserving the split (recommended, and what Phase 2 specifies) keeps every existing
   JavaScript command's `metadata_version` stable. Normalising to one rule is tidier and makes the
   two registration routes agree, at the cost of a one-off version change for underscored
   JavaScript commands — which re-expires their dependent assets. Raised by a review bot on this
   PR and verified against `spec.rs:166` and `command_metadata.rs:925`.

## Review record

*Against Phase 1:* every acceptance criterion maps to a named test; the non-goals (Python binding,
post-init registration, `register_command!`, exporter output) appear nowhere in the plan; the
`snapshot_declaration` cleanup is explicitly deferred rather than folded in.

*Against the codebase:* the `CommandMetadata` deserialization limits were measured, not assumed;
`ArgumentType`'s serde names, `CommandMetadata::from_key`'s defaults, `metadata_version`'s
computation site, `serde-wasm-bindgen`'s presence in `liquers-web`'s manifest, and the four
error-wording assertions in the conformance suite were all read at `HEAD`. Risk is **not**
understated: this crosses two crates and three modules, touches a public JavaScript API, and the
"0 broken tests" estimate is qualified with the 24 tests in range. It therefore fails the automatic
clearance conditions of the procedure and needs an explicit decision.
