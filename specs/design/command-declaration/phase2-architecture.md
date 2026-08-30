Based on `HEAD`, read rather than remembered. Nothing here is implemented.

# Phase 2 — Solution and architecture

> **Revised 2026-08-29.** The first draft of this phase proposed a parallel `CommandDeclaration`
> struct mirroring `CommandMetadata` field for field. Review found that ~80% of it was a re-skin
> whose only substance was five missing `#[serde(default)]` attributes, and that the mirror
> introduced three *new* divergences (a renamed argument-type key, a different default
> representation, and the silent loss of `presets`/`next`/`hints`/`Alias`). This revision fixes
> `CommandMetadata` instead and adds only the genuinely host-specific residue. The measurements in
> Phase 1 are unchanged and still stand; the conclusion drawn from them is what changed. The
> superseded proposal is preserved in §Rejected alternatives so the reasoning is not lost.
>
> **Superseded in framing, 2026-08-29**, by
> [`purpose-and-semantics.md`](./purpose-and-semantics.md). A command declaration is the runtime
> equivalent of `register_command!` — a *partial* metadata contribution merged over what the host
> discovered by introspection, plus a call specification — not a serialization of `CommandMetadata`.
> This document's Part A survives as a prerequisite (and as a latent-defect fix) but not as the
> feature; Part B is roughly a third of the call specification actually required; and the headline
> round-trip test measures metadata serde rather than the declaration. **Do not implement from this
> document.** It is retained because its measurements, its rejected alternatives and its risk
> analysis remain valid, and because the reasoning that led here should not be lost.
>
> **Revised again 2026-08-29**, on the observation that the residue is not a grab-bag: it is the
> *wrapping model* — how a callable is adapted to serve as a command — which `register_command!`
> decides at compile time and which has no language-neutral equivalent. `CommandBinding` is renamed
> `CallingConvention` and given that frame in §Part B, which also settles what does and does not
> belong in it.

## Diagnosis

`CommandMetadata` is not an author-facing format today, but the gap is far narrower than Phase 1's
table suggests. Of its 20 fields, **14 already carry `#[serde(default)]`**. Exactly four do not —
`label`, `cache`, `volatile`, `definition` — plus `ArgumentInfo::label`. There is no principle
separating them from the rest: `is_async: bool` has a default and `volatile: bool`, three fields
away, does not. The strictness this format appears to enforce is not a designed invariant; it is an
oversight, and it is the whole of Phase 1's measured failure.

Three further mismatches were found while checking this, all verified at `HEAD`:

1. **`ArgumentGUIInfo`'s `Default` is `None`, but `ArgumentInfo::any_argument` sets
   `TextField(40)`** (`command_metadata.rs:213-283,422`). Plain serde deserialization of an
   argument that omits `gui_info` therefore produces different metadata than today's
   `parse_arguments` path — and since `metadata_version` is computed from stored content
   (`command_metadata.rs:1036`), that would silently re-version every JavaScript command with
   declared arguments.
2. **`CommandMetadata::new`/`from_key` set `state_argument: Some(any_argument("state"))`, but the
   serde default is `None`** (measured: deserializing `{"name":"greet",…}` yields
   `state_argument: None`). Constructing and deserializing the same command give **different
   commands**. This is a latent defect independent of this design; filed as
   `STATE-ARGUMENT-CONSTRUCTOR-SERDE-DEFAULT-DISAGREE`.
3. **`ArgumentInfo` serializes `argument_type`, while the JavaScript API declares `type`**
   (`spec.rs:212`, and `argument_type:` appears 101 times in `specs/command_registry.yaml`).

## Chosen solution

Three parts, in dependency order. Each is separately revertible.

### Part A — make `CommandMetadata` deserializable from a hand-written document

In `liquers-core/src/command_metadata.rs`, additive serde attributes only. **No field is added,
removed, renamed or retyped, and no `Serialize` behaviour changes**, so
`specs/command_registry.yaml` stays byte-identical and `registry_export` stays green.

| Target | Change | Why |
|---|---|---|
| `CommandMetadata::label` | `#[serde(default)]` | filled by `fill_declaration_defaults` |
| `CommandMetadata::cache` | `#[serde(default = "true_default")]` | matches `from_key` |
| `CommandMetadata::volatile` | `#[serde(default)]` | matches `from_key` |
| `CommandMetadata::definition` | `#[serde(default)]` | `CommandDefinition` already derives `Default = Registered` |
| `ArgumentInfo::label` | `#[serde(default)]` | filled by `fill_declaration_defaults` |
| `ArgumentInfo::argument_type` | `#[serde(alias = "type")]` | keeps `liquers.registerCommand({arguments:[{type:"int"}]})` working |
| `ArgumentType` | `#[serde(alias)]` for `str`, `text`, `integer`, `number`, `boolean` | keeps `parse_argument_type`'s vocabulary (**resolves Q2**) |
| `CommandParameterValue` | hand-written `Deserialize` (see below) | keeps `default: 2` working |

Aliases and defaults affect **deserialization only**. Serialization is untouched by all eight rows.

Plus two idempotent normalisers, which are the one place the "what a document may omit" rule lives:

```rust
impl ArgumentInfo {
    /// Fills what a declaration may omit, matching [`ArgumentInfo::any_argument`].
    /// Idempotent; never overwrites a declared value.
    pub fn fill_declaration_defaults(&mut self);   // label <- name.replace('_'," "); gui_info None -> TextField(40)
}

impl CommandMetadata {
    /// Fills what a declaration may omit, matching [`CommandMetadata::from_key`].
    /// Idempotent; never overwrites a declared value. Does **not** touch `state_argument`
    /// (that is [`CallingConvention`]'s decision) or `namespace` (see note).
    pub fn fill_declaration_defaults(&mut self);   // label <- name.replace('_'," "); then each argument
}
```

`namespace` is deliberately **not** normalised. `CommandMetadata::new` sets `"root"` while
`from_key` passes the key's namespace through, and the exported registry writes no `namespace:` for
root commands — the two conventions already disagree and reconciling them is out of scope here.

### Part B — `CallingConvention`: the wrapping model, made portable

**The framing.** What is left over after Part A is not an assortment of leftovers; it is one
coherent thing — *how a callable is wrapped so that it can serve as a command*. Every binding
already has this concept and none of them shares it:

| Where | What it is called | Portable? |
|---|---|---|
| Rust | `CommandSignature` in `liquers-macro/src/registration.rs:1104`, consumed by codegen | no — it is `syn` types, gone after expansion |
| JavaScript | `CallableSpec` in `liquers-web/src/command/adapter.rs:130`, whose doc comment reads *"The retained callable and how to call it"* | no — wasm32-only |
| A document | — | **this is the gap** |

`CommandMetadata` describes *the command*: what it is called, what arguments it takes, whether its
result may be cached. It deliberately says nothing about how the implementation is invoked, which is
correct — but it means a document-driven host has nowhere to put the decisions the macro makes for a
Rust author.

**Which of the macro's decisions are wrapping, and which are portable.** Classifying every field of
`CommandSignature`:

| Decision | Recorded in `CommandMetadata`? | Portable? |
|---|---|---|
| `name`, `label`, `doc`, `namespace`, `realm`, `presets`, `next`, `filename`, `volatile`, `payload_required`, `expires`, `impl_version` | yes | — (metadata, not wrapping) |
| argument `injected` / `multiple` / default / label / gui / enum | yes, as `ArgumentInfo` | — (metadata) |
| `is_async` | yes, as `bool` | **yes**, but the *undeclared* state is not representable |
| `state_parameter: {Value, State, Text, None}` (`registration.rs:785`) | **no** — `state_argument` records only `None` versus the other three collapsed | **yes** |
| `result_type: {Value, Result}` (`:792`) | **no** | **no** — Rust-only. A JavaScript callable throws and a Python one raises; there is no second return form to choose between |
| `CommandParameter::Context` (`:489`) — whether the callable receives the execution context, and where in the parameter list | **no** | **yes in principle, absent in practice** — see below |
| parameter Rust type `ty`, driving conversion | as `ArgumentType`, a coarse projection | **no** — the projection is the portable part and metadata already carries it |
| `wrapper_version: WrapperVersion::V2` (`:1101`) | **no** | **yes** — see open question 8 |

So of the macro's wrapping decisions exactly **two** are both absent from metadata and portable
today: the state form, and async-when-undeclared. That is what `CallingConvention` carries, and the
narrowness is the point — it is a small type because most of the wrapping model either is already
metadata or is genuinely language-specific.

```rust
/// How a callable is wrapped so that it can serve as a command — the language-neutral counterpart
/// of what [`register_command!`] decides at compile time and of `liquers-web`'s `CallableSpec`.
///
/// This is deliberately *not* part of [`CommandMetadata`]: metadata describes the command, this
/// describes the call. It is also deliberately not [`CommandDefinition`], which answers *which*
/// implementation to use rather than *how* to invoke it.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CallingConvention {
    /// Which form of the input state the callable receives. `None` = the document did not say.
    ///
    /// Authoritative when present: it overwrites `state_argument`. When absent, whatever the
    /// document declared as `state_argument` stands — which is what lets a re-parsed
    /// `command_registry.yaml` round-trip.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<StateMode>,

    /// Whether the callable's result is awaited. `None` = the document did not say. A host that can
    /// decide from the callable — JavaScript's thenable test, Python's
    /// `inspect.iscoroutinefunction` — does so; one that cannot treats it as `false`.
    #[serde(default, rename = "async", skip_serializing_if = "Option::is_none")]
    pub is_async: Option<bool>,
}

/// Which form of the input state the callable receives. The portable half of the macro's
/// `StateParameter` (`liquers-macro/src/registration.rs:785`), which it mirrors variant for
/// variant.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum StateMode {
    /// No state argument — a source command.
    #[default]
    None,
    Value,
    #[serde(alias = "string")]
    Text,
    State,
}

impl CallingConvention {
    /// Applies the declared decisions onto metadata deserialized from the same document.
    /// Only fields the document actually declared are overwritten.
    pub fn apply(&self, metadata: &mut CommandMetadata);

    /// The concrete mode for dispatch. Absent means `StateMode::None`, i.e. a source command —
    /// today's `spec.rs:151-155` behaviour exactly.
    pub fn state_mode(&self) -> StateMode { self.state.unwrap_or_default() }
}
```

`apply` is:

```rust
if let Some(mode) = self.state {
    metadata.state_argument = match mode {
        StateMode::None => None,
        StateMode::Value | StateMode::Text | StateMode::State =>
            Some(ArgumentInfo::any_argument("state")),
    };
}
if let Some(is_async) = self.is_async {
    metadata.is_async = is_async;
}
```

Explicit match over every `StateMode` variant, no `_ =>` arm, per the codebase rule.

**`arguments` declared versus inferred is not part of the convention.** It is a *parse artifact* —
whether this particular document happened to spell its arguments out — not a fact about how the
callable is called, and it is not something a host would ever serialize. It moves to the parse
result (see §The pair), obtained by a presence probe:

```rust
/// Presence-only probe for the one thing `CommandMetadata::arguments: Vec<_>` cannot distinguish
/// from an empty list, and the reason `adapter.rs:26-37` keeps a thread-local today.
/// Deserialization-only; the value is ignored. `arguments: null` reads as absent, matching
/// `spec.rs`'s `get`.
#[derive(Deserialize, Default)]
struct ArgumentsPresence {
    #[serde(default, rename = "arguments", deserialize_with = "field_is_present")]
    declared: bool,
}

fn field_is_present<'de, D: Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    Ok(Option::<serde::de::IgnoredAny>::deserialize(d)?.is_some())
}
```

**A gap this framing exposes: a JavaScript command cannot reach the execution context.**
`register_js_command` receives one and discards it — `_context: Context<E>` (`adapter.rs:165`) —
while a Rust command declares `context` and gets it. So `CommandParameter::Context` is a wrapping
decision with no portable expression *and* no JavaScript implementation. Adding one is out of scope
here — it needs a JavaScript-side context object, which is its own design — but the convention type
is where it would land, and it is filed as `JS-COMMAND-CANNOT-ACCESS-CONTEXT` so the shape of
`CallingConvention` is not fixed in ignorance of it.

### Part C — permissive `CommandParameterValue` deserialization

Today `CommandParameterValue` is an externally tagged enum: the exporter writes `default: !Value ''`
and `default: None`, while the JavaScript API writes `default: 2`. A hand-written `Deserialize`
accepts both, and `Serialize` is left alone so the registry file does not move:

| Input | Reads as |
|---|---|
| `!Value 2` / `{"Value": 2}` | `Value(2)` — the exported form, unchanged |
| `!Query "a/b"` / `{"Query": …}` | `Query(…)` — unchanged, and now declarable |
| `None` (bare string `"None"`) | `None` — the exported form for the None variant |
| `2`, `"hello"`, `true`, `[…]` | `Value(…)` — shorthand, what JavaScript writes today |
| `null` | `Value(Null)` — preserves `js_default_to_json`'s treatment of `null` |
| any other map | refused, with the command and argument named |

Two consequences to accept deliberately:

- **A bare string `"None"` means the `None` variant, not the string `"None"`.** The escape hatch is
  the explicit `!Value 'None'`. This ambiguity is inherited from the current exported form, not
  introduced here — but the shorthand is where an author could trip over it. See open question 4.
- **Query-valued defaults become declarable from a document**, closing a gap the superseded draft
  recorded as permanently out of scope. `grep -c '!Query'` on the registry returns 0, so nothing
  existing changes.

### What the three parts buy

`CommandMetadata` becomes the declaration format. `{"name":"greet"}` deserializes; so does the
whole of `specs/command_registry.yaml`; so does today's JavaScript declaration object. `presets`,
`next`, `hints`, `ParameterPreset` and `CommandDefinition::Alias` keep working from a document for
free, because they were never removed. There is one format, which was the point of the issue.

And the wrapping model gets a name in `liquers-core` for the first time. Today it exists three
times over — as `CommandSignature` in the macro, as `CallableSpec` in `liquers-web`, and as nothing
at all in a document — and the three cannot be compared, tested against each other, or documented as
one concept. `CallingConvention` is small precisely because Part A moved everything that was really
metadata back into metadata, leaving only the two decisions that are genuinely about the call.

## The pair, and why it is not a struct with a derived `Deserialize`

A host needs both halves out of one document. A `Deserializer` is consumed by a single pass, so a
combined type would need either `#[serde(flatten)]` (rejected: it requires `deserialize_any`, which
`serde-wasm-bindgen` handles badly — this reason survives from the first draft) or a hand-written
visitor over 20-odd fields (rejected: it re-creates the maintenance burden the whole revision
removes).

Instead, every real host has a **re-readable** source, so it makes one pass per struct over
disjoint keys:

```rust
/// One command as a document declares it: what it is, how it is called, and — for the host's
/// benefit — whether it spelled its arguments out.
pub struct CommandDeclaration {
    pub metadata: CommandMetadata,
    pub convention: CallingConvention,
    /// A parse artifact, not part of the wire format: `true` when the document carried an
    /// `arguments` key at all. A host that infers arguments from the callable consults this.
    pub arguments_declared: bool,
}

impl CommandDeclaration {
    /// Three passes over one self-describing document. `&serde_json::Value` implements
    /// `Deserializer`, so this borrows rather than clones.
    pub fn from_json_value(value: &serde_json::Value) -> Result<Self, Error>;
}
```

`from_json_value` runs the three passes (`CommandMetadata`, `CallingConvention`,
`ArgumentsPresence`), then `fill_declaration_defaults()` and `convention.apply(…)`, in that order.
`liquers-web` does the same three passes directly over its `&JsValue` with `serde_wasm_bindgen`,
never materialising JSON. Each pass is over disjoint keys, so their order does not matter except
that `apply` runs last.

No struct sets `deny_unknown_fields` — each must ignore the others' keys. The cost is that a
typo'd `volatil: true` is silently ignored by all of them. That is already true of
`JsCommandSpec::parse` today, so it is not a regression; see open question 5.

## Rejected alternatives

| Option | Verdict |
|---|---|
| **A parallel `CommandDeclaration` struct mirroring `CommandMetadata`** (the first draft of this document) | **Rejected on review.** ~80% of its fields were identical; the substance was five missing `#[serde(default)]`. It renamed `argument_type` to `type`, contradicting Phase 1's own acceptance criterion on field-name agreement; it changed `default` from `CommandParameterValue` to `serde_json::Value`, dropping `Query` defaults; and it silently omitted `presets`, `next`, `hints` and `CommandDefinition::Alias`, so `from_metadata`→`to_metadata` was not an identity. That round-trip passes at `HEAD` only by luck — the registry currently contains 0 of each (verified) — so the Phase 1 test would have certified a lossy format. A second 20-field struct with a renamed key and a different default representation is a second format wearing the first one's name. |
| Add `#[serde(default)]` and stop there | Insufficient: it leaves `state`, the async tri-state and declared-vs-inferred arguments homeless, which is the residue Part B exists for. |
| Make the export format tolerant is dangerous — "a registry file missing `cache` would silently deserialize as `true`" (first draft's objection to fixing `CommandMetadata`) | **Does not hold.** `cache` has no `skip_serializing_if`, so the exporter always writes it; the file is generated and never hand-edited; `registry_export` compares signatures and would catch a semantic drift; and the format already tolerates omission of 14 of 20 fields, so the strictness being protected does not exist. |
| Put `CallingConvention` in `command_metadata.rs` | Rejected: that file is 1397 lines, and *what a command is* versus *how it is called* is exactly the distinction this issue draws. Keeping them in one file would blur it again. |
| Keep the format in `liquers-web` and have Python depend on it | Rejected by the issue: `liquers-web` is wasm32-only. |

## `liquers-web` re-implementation

`JsCommandSpec` (`spec.rs:81`) keeps its public shape — `key`, `metadata`, `state_mode`, `is_async`,
`run`, `arguments_inferred` — and `register_js_command` (`adapter.rs:79`) is untouched. Only `parse`
changes:

1. Object check, and `name` pre-checked with `Reflect` so the current messages survive verbatim
   (`"A command declaration must have a string \`name\`"`, `"A command \`name\` must not be
   empty"`). Serde would otherwise say `missing field \`name\``.
2. `Reflect::get(spec, "run")`, checked with `is_function()` — unchanged, with its current message.
3. Shallow copy of the declaration **without** `run`, so no `js_sys::Function` reaches serde.
4. Three `serde_wasm_bindgen` passes over that copy: `CommandMetadata`, `CallingConvention` and the
   `ArgumentsPresence` probe.
5. Refuse `namespace == RESERVED_NAMESPACE` — unchanged, keeping the `"reserved"` wording
   `command06_ns_reserved_namespace_is_refused` asserts.
6. `metadata.fill_declaration_defaults()`, then **override** `metadata.label` with
   `declared_label.unwrap_or_else(|| name.clone())` — the name **verbatim**, see below — then
   `metadata.module = "javascript"`, then `convention.apply(&mut metadata)`.
7. `if !arguments_declared { infer_arguments(…) }` — unchanged, including every refusal message
   `command05_infer_refused_shapes` asserts. The thread-local `INFERRED_ARGUMENTS` in
   `adapter.rs:26-37` can now be dropped in favour of the probe, though that is an independent
   tidy-up and is **not** required by this change.
8. `IsAsync` from `convention.is_async`: `Some(true) → Async`, `Some(false) → Sync`, `None → Auto`.
   `IsAsync` stays in `liquers-web`, because "test whether the result is thenable" is a JavaScript
   notion; the wire format carries only the tri-state as `Option<bool>`. `CallableSpec` is then
   constructible straight from `(run, convention.state_mode(), is_async, name)` — it becomes the
   JavaScript *instance* of the convention rather than a parallel definition of it.

Deleted: `get`, `get_string`, `get_bool`, `parse_arguments`, `parse_argument_type`,
`js_default_to_json` — approximately 130 lines. Retained: `infer_arguments`, `parameter_list`,
`strip_comments`, `is_plain_identifier` (~107 lines), which parse JavaScript source and which a
Python binding would replace with `inspect.signature` rather than share.

**The `label` default is not shared, and must not be.** `CommandMetadata::from_key` derives
`key.name.replace("_", " ")` (`command_metadata.rs:925`); `JsCommandSpec::parse` uses the name
**unchanged** (`spec.rs:166`). For a command named `foo_bar` the two disagree, and adopting the Rust
rule would change `metadata_version` for every underscored JavaScript command, re-expiring their
dependent assets. Step 6 above therefore overrides after `fill_declaration_defaults`, and a parity
test covers `foo_bar` specifically. See open question 2.

`snapshot_declaration` (`environment.rs:171-193`) exists only because `REGISTERED_SPECS` retains the
caller's own `JsValue`, which the caller may mutate (`command14`). Retaining a parsed
`(CommandMetadata, CallingConvention, js_sys::Function)` triple is immune by construction.
**Deferred, not done** — it changes replay-on-rebuild and belongs with
`POST-INIT-COMMAND-REGISTRATION`. Recorded so the opportunity is not lost.

## Data ownership, errors, sync/async

- `CallingConvention` is two `Option`s of `Copy` types, so it derives `Copy`; no lifetimes, no
  `Arc`, and a host may retain it for replay at no cost.
- Every failure is `liquers_core::error::Error` via
  `Error::from_error(ErrorType::ParameterError, …)` — no new error type, no `Error::new`.
- `liquers-web` wraps serde failures as
  `Error::from_error(ErrorType::ParameterError, format!("Command {name:?}: {e}"))`, so a serde
  diagnostic keeps the command name.
- Nothing async: parsing is pure. The declared `async` flag describes the *implementation*, and
  dispatch stays in `adapter.rs`.
- `fill_declaration_defaults` is infallible and idempotent; `from_json_value` returns `Result`.

## Reuse

`CommandMetadata`, `CommandMetadata::from_key`, `ArgumentInfo::any_argument`, `ArgumentInfo`,
`ArgumentType`, `ArgumentGUIInfo`, `CommandParameterValue`, `CommandDefinition`, `Expires`,
`PayloadRequirement`, `CommandPreset`, `ParameterPreset` and `CommandKey` are all reused **as-is** —
this revision reuses the whole type rather than mirroring it. `infer_arguments`, `parameter_list`,
`strip_comments` and `is_plain_identifier` stay in `liquers-web`.

## Related open issues

- `POST-INIT-COMMAND-REGISTRATION` (P3, `accepted`) — the other half of the document-driven
  ergonomics; not a prerequisite. The `snapshot_declaration` cleanup belongs to it.
- `STORE-CONFIG-IN-CORE` (P0) — document #1; independent of this one.
- `JS-COMMAND-CANNOT-ACCESS-CONTEXT` — filed by this design. A wrapping decision the macro supports
  (`CommandParameter::Context`) that JavaScript neither expresses nor implements. Not a blocker, but
  it is the most likely next field of `CallingConvention`, so the type must not be sealed against it.
- `STATE-ARGUMENT-CONSTRUCTOR-SERDE-DEFAULT-DISAGREE` — filed by this design. Not a blocker: Part B
  makes `state_argument` explicitly the convention's decision, which sidesteps it for the
  declaration path, but the underlying inconsistency remains for anyone deserializing metadata directly.
- `COMMAND-METADATA-ENHANCEMENTS` (P2, `accepted`, wants IO typing) — open question 3 is squarely in
  its territory; this design must not foreclose it, and reusing `ArgumentType` rather than
  re-enumerating it is why it does not.
- `REGISTER-COMMAND-ENUM` — would extend the same field set; reuse means it extends one place.
- `REGISTRY-IMPL-VERSION-DRIFT-UNDETECTED` — relevant to the round-trip test: `impl_version` comes
  from registration, not from the declaration, so the round-trip compares metadata *excluding*
  `impl_version`, or sets it explicitly.

## Risk analysis

| Assessment | Record |
|---|---|
| **Files** | Source: `liquers-core/src/command_metadata.rs` (8 serde attribute rows, 2 normalisers, 1 hand-written `Deserialize` — ~120 lines added, 0 removed), `liquers-core/src/command_declaration.rs` (new, ~70 lines: `StateMode`, `CallingConvention`, the `ArgumentsPresence` probe, `CommandDeclaration`), `liquers-core/src/lib.rs` (one `pub mod`), `liquers-web/src/command/spec.rs` (rewrite of `parse`, ~130 lines removed). Tests: colocated in both core modules, plus `liquers-core/tests/` for the registry round-trip. Specs: a pointer from `specs/reference/REGISTER_COMMAND_FSD.md` with its `reviewed:`/History rows; `specs/index.csv` regenerated. Generated files: **none** — `specs/command_registry.yaml` must be byte-identical. |
| **Impact area** | `core/commands`, `web`. Downstream: every JavaScript command registration path, `describeCommand`, and the environment rebuild/replay path via `REGISTERED_SPECS`. `liquers-py` is unaffected until it opts in. |
| **Module/crate reach** | **Not confined to one module.** `liquers-core` (two modules) and `liquers-web` (one). This alone fails the automatic-clearance condition of the autonomous procedure. |
| **Existing-test breakage** | Expected **0 assertion failures**; the estimate is soft. At risk: the 20 `wasm_bindgen_test`s in `liquers-web/tests/commands_COMMAND.rs`, of which four assert error wording (`:66`, `:409`, `:422`, `:509`) — all four preserved by keeping their producing code in `liquers-web`. `liquers-core/src/command_metadata.rs`'s own serde tests (`:1210`, `:1236`, `:1254`, `:1381`) must be unaffected; defaults and aliases do not change serialization, and `:1381` asserts an exact JSON string, so it is the sharpest tripwire for an accidental `Serialize` change. `liquers-lib`'s `registry_export` must stay green. Honest number: "0 expected, ~25 tests in the blast radius". |
| **New validation** | (1) Registry round-trip: `specs/command_registry.yaml` → parse → re-serialize → **byte-identical**. Stronger than the first draft's modulo-comparison, and possible only because nothing is dropped. (2) `{"name":"greet"}` after `fill_declaration_defaults` equals `CommandMetadata::from_key`, `state_argument` aside. (3) Parity with `register_command!` for one representative command including `metadata_version` after registry insertion. (4) `foo_bar` label parity: the JavaScript path yields `"foo_bar"`, the document path `"foo bar"`. (5) `gui_info` parity: a declared argument omitting `gui_info` yields `TextField(40)`, not `None`. (6) `CommandParameterValue`: all six input shapes in the Part C table. (7) YAML and JSON parse the same declaration to the same value. (8) Malformed: empty name, `multiple` not last, unknown argument type, non-array `arguments`, object-valued default. (9) `CallingConvention` parity with the macro: for each of `state`/`value`/`text`/`none`, the metadata a declaration produces equals what the equivalent `register_command!` `StateParameter` produces. (10) The whole `liquers-web` COMMAND suite under Node. Commands: `cargo test -p liquers-core --lib`, `cargo test -p liquers-lib --lib --tests`, `bash scripts/check-build-matrix.sh`, and — after `cargo clean` — `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles`. |
| **Behavioural risk** | *Compatibility*: the JavaScript surface widens (`filename`, `expires`, `payload_required`, `presets`, `next`, `hints`, richer `ArgumentType`s, `Alias`, `Query` defaults all become declarable) and one diagnostic's wording changes; both deliberate and listed. *Persistence/data*: `specs/command_registry.yaml` must not move — now enforced by a byte-identical round-trip, not only by `registry_export`. *Metadata versions*: the `label` and `gui_info` parity tests exist precisely because a slip there silently re-expires assets. *Concurrency*: not applicable — parsing is pure. *Performance*: not applicable — registration is not a hot path. *Security*: a declaration is host-supplied data that becomes registered metadata; it cannot name a Rust implementation. *Error paths*: serde failures replace hand-written ones for malformed fields, wrapped so the command name survives. |
| **Recovery** | Part A is additive and behaviour-preserving on the serialize side; it can stay even if the rest is reverted. Part B is a new module nothing else depends on. Part C is the only change to an existing `Deserialize` and is the one to revert first if the registry moves. The `liquers-web` rewrite is separable and revertible on its own, since `JsCommandSpec`'s public shape is unchanged. Sequencing as A → B → C → web keeps every boundary real. |
| **Certainty** | Higher than the first draft, because the type being reused is the type already under test. Unverified and needing execution: (a) that `serde_wasm_bindgen` deserializes a JavaScript object into `CommandMetadata` including the `!Value`-tagged and shorthand default forms — the Part C visitor is written against `serde`, not against `serde-wasm-bindgen`'s value model, and its `deserialize_any` behaviour on a JS object is the specific unknown; (b) that `Option::<IgnoredAny>::deserialize` through `serde-wasm-bindgen` distinguishes an absent key from `null` the way `spec.rs`'s `get` does. Fallback for both: `js_sys::JSON::stringify` the run-less copy and go through `serde_json`, at the cost that a non-JSON default becomes an error rather than reaching the visitor. These are Phase 3 experiments, not assumptions. |

## Open questions for the gate

1. **`run` has no home here, and belongs to `CommandDefinition`.**
   Part B's framing makes the split precise. There are two separate questions about an
   implementation:

   - **Which one?** — `CommandDefinition`. `Registered` = look it up in the `CommandExecutor` by key
     (`plan.rs:1555`); `Alias` = rewrite to another key (`plan.rs:1575`).
   - **How is it called?** — `CallingConvention`. State form, async.

   `run: "greet"`, naming an entry in a host-supplied table, answers **which**. So it belongs beside
   `Registered` and `Alias`, not beside `state` and `async`; a sibling field would have duplicated
   `CommandDefinition`'s job while sitting in the type that answers the other question entirely.
   `run` is therefore **removed from this design** pending a decision between:
   - **(a) Drop it.** In today's JavaScript API `run` is the function itself, and
     `register_js_command` keys it by `CommandKey`. A document-driven host can key its table the
     same way, and `run` is redundant.
   - **(b) Keep it as an override only**, defaulting to the command name, for the case where one
     callable serves several commands.
   - **(c) Add `CommandDefinition::HostFunction { name: String }`.** Then it *is* metadata: it
     deserializes with everything else, `describeCommand` can report it, and the codebase's
     no-`_ =>` rule forces `plan.rs:1555` and `lib/commands.rs:163` to state what they do with it
     (both would plan it exactly as `Registered`; only the executor differs). Cost: a public enum
     gains a variant, which is a breaking change for any external match.

   **Recommendation:** (c) if the document-driven host genuinely needs indirection, (a) if it does
   not. The which/how split above settles *where* it goes but not *whether* it is needed, and that
   depends on the shape of the intended host setup — a maintainer decision, not one the codebase
   answers.
2. **The `label` default split.** `liquers-web` keeps the name verbatim; Rust replaces underscores
   with spaces. Preserving the split (recommended, and what this document specifies) keeps every
   existing JavaScript command's `metadata_version` stable. Normalising to one rule is tidier at the
   cost of a one-off version change for underscored JavaScript commands, which re-expires their
   dependent assets.
3. **Should the value/text/state distinction become real metadata?** `StateMode` sits in
   `CallingConvention` because it is a calling convention. But "this command wants text, not the value"
   is something a planner or UI could use, and `state_argument.argument_type` is the field-shaped
   hole it would fit — `Any` for value, `String` for text, with no natural encoding for "the whole
   `State`". That is `COMMAND-METADATA-ENHANCEMENTS`'s territory (IO typing). **Recommendation:**
   leave it in the convention now, and record the question there rather than pre-empting it. Note
   the tension: if IO typing ever lands, `state` becomes metadata and `CallingConvention` is down to
   one field — at which point it may be worth reconsidering whether it should exist as a type.
4. **The `"None"` shorthand ambiguity.** A bare string `"None"` reads as the `None` variant, so a
   default whose literal value is the string `"None"` must be written `!Value 'None'`. Accept, or
   refuse the bare-string form for that one word? **Recommendation:** accept and document; refusing
   creates a special case that is harder to explain than the escape hatch.
5. **Unknown-field tolerance.** Two-pass deserialization means neither struct can set
   `deny_unknown_fields`, so a typo'd `volatil: true` is silently ignored. Today's parser has the
   same behaviour, so it is not a regression — but a document-driven host makes typos likelier than
   an inline object literal does. Accept now and revisit with a validating pass later, or build
   the known-key check into `from_json_value` from the start? **Recommendation:** accept now; the
   check is cheap to add later and needs a key list that Part A does not yet stabilise.
6. **Widening the JavaScript surface.** Reusing `CommandMetadata` means `filename`, `expires`,
   `payload_required`, `presets`, `next`, `hints`, `Alias` and the richer `ArgumentType` variants all
   become declarable from JavaScript. **Recommendation:** accept — one format is the point of the
   issue — and add a line to the TypeScript stubs (`liquers-web/tests/stubs/`).
7. **Split into commits?** Parts A, B, C and the `liquers-web` rewrite are separable, and the last
   is where the risk is. **Recommendation:** one PR, four commits, so the web half can be reverted
   alone.
8. **Should `CallingConvention` be versioned?** The macro already versions its wrapping model —
   `WrapperVersion::V2` (`registration.rs:1101`), currently a single-variant enum that is never
   parsed from the DSL but exists as a codegen seam for changing the convention without breaking
   existing registrations. A *document-driven* convention has the same problem in a harder form: the
   document outlives the binary that reads it, and a host cannot recompile it. Options: add a
   `version` field now with one value; rely on additive-only evolution (every new field optional,
   old documents keep meaning what they meant); or defer until a second version actually exists.
   **Recommendation:** defer, but record the constraint that every future field must be optional
   with a backward-compatible default, so additive evolution stays available. Raised because the
   macro's seam makes it clear the maintainers already expect the convention to change.
10. **Per-argument merge.** `arguments_declared` is all-or-nothing, matching JavaScript. Python,
    Starlark and Rhai can infer names, types and defaults exactly and would want to augment single
    arguments with a label or widget hint without restating the rest. Leave the boolean and let each
    host merge before it hands metadata over (recommended — it keeps this change minimal and matches
    today's JavaScript behaviour exactly), or design the merge into the shared layer now?
    **Recommendation:** leave it, and treat `ARGUMENT-DECLARATION-IS-ALL-OR-NOTHING` as the place it
    gets settled — before `COMMAND-METADATA-ENHANCEMENTS` lands per-argument enums, which would be
    supplied the same way.
9. **The name.** `CallingConvention` was chosen over `CommandBinding` because it says what the type
   is rather than what it is not, and matches the macro's own `WrapperVersion` vocabulary.
   `CommandWrapper` and `WrappingSpec` are equally defensible. Cheap to change before implementation
   and expensive after; flagged only so the choice is deliberate.

## Portability validation

[`portability-analysis.md`](./portability-analysis.md) tests the language-neutrality claim against
JavaScript, Python, Rust, Starlark, Rhai and Rune. Three results bear on this architecture:

1. **Part A is portable to all six**, and it is the cheap half. The design's value is concentrated
   where its cost is lowest.
2. **Part B's two fields are unevenly justified.** `state` is needed by every dynamic host;
   `is_async: Option<bool>` is needed by two of six — JavaScript and Rune — because Starlark and
   Rhai have no async and Python and Rust determine it from the callable. The field earns its place
   and stays, but this is evidence against ever growing `CallingConvention`, and it strengthens open
   question 8's additive-only rule.
3. **Argument inference is shared by none of them**, and should not be — each host infers from its
   own reflection mechanism. Worth stating because the issue's framing implicitly counts that code
   as duplicated; it is not, and it is ~140 of `spec.rs`'s 389 lines.

The analysis also found that `arguments_declared` being a single boolean matches JavaScript but not
Python, Starlark or Rhai, which want to *augment* inferred arguments rather than replace them. Not a
blocker — a host can merge before handing metadata over — but it means the shared layer cannot
express the semantics three of six languages would want. Filed as
`ARGUMENT-DECLARATION-IS-ALL-OR-NOTHING`; see open question 10.

## Scope note

Fixing `CommandMetadata`'s serde defaults is a smaller change than the first draft's new type, which
weakens the issue's `P0` (Phase 1, Q3). The issue file itself already flags the tension with
`DOCS_STRUCTURE_GUIDE.md` §4.4 — it is scheduling weight, not a defect. With the fix at five
attributes plus a small module, **P1 looks right**, and the gate is the place to settle it.

## Review record

*Against Phase 1:* all seven acceptance criteria map to named tests in the validation row above.
Criterion 3 ("field names agree with `specs/command_registry.yaml`") is now satisfied literally
rather than approximately — they are the same struct. Criterion 4's round-trip strengthens from
"equal modulo `impl_version`" to byte-identical. Criterion 2 is what Part B exists for, and is the
only place a new type is introduced — now framed as the wrapping model rather than as a residue,
which is what fixed its scope: two fields, because everything else the macro decides is either
already metadata or genuinely language-specific. The non-goals (Python binding, post-init registration,
`register_command!`, exporter output) appear nowhere in the plan; the `snapshot_declaration` cleanup
stays deferred.

*Against the codebase:* every claim was read at `HEAD` and the deserialization limits were
re-measured, not carried over. Newly verified for this revision: the 14-of-20 default count; that
`ArgumentInfo::label` is also required, which Phase 1 missed; the `ArgumentGUIInfo::None` versus
`TextField(40)` mismatch; the `state_argument` constructor/serde disagreement; `argument_type`'s 101
occurrences in the registry; and that the registry contains 0 `presets`, 0 `hints`, 0 `next`,
0 `!Query` and 0 `GlobalEnum`, which is why the first draft's lossy round-trip would have passed.
Verified for the wrapping frame: `CommandSignature`'s full field list (`registration.rs:1104-1123`),
`StateParameter` and `ResultType` (`:785`, `:792`), `CommandParameter::Context` (`:489`) and the
comment at `:1134` confirming it consumes no argument slot, `WrapperVersion` (`:1101`) being
internal-only, `CallableSpec` (`adapter.rs:130`), and that `register_js_command` discards the
context it is handed (`adapter.rs:165`).

Risk is **not** understated: this crosses two crates and three modules, touches a public JavaScript
API, changes an existing `Deserialize`, and the "0 broken tests" estimate is qualified with the 25
tests in range. It fails the automatic clearance conditions of the procedure and needs an explicit
decision.
