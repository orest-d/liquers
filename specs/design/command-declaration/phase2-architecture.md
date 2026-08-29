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
    /// (that is [`CommandBinding`]'s decision) or `namespace` (see note).
    pub fn fill_declaration_defaults(&mut self);   // label <- name.replace('_'," "); then each argument
}
```

`namespace` is deliberately **not** normalised. `CommandMetadata::new` sets `"root"` while
`from_key` passes the key's namespace through, and the exported registry writes no `namespace:` for
root commands — the two conventions already disagree and reconciling them is out of scope here.

### Part B — `CommandBinding`, the host-specific residue

A new module `liquers-core/src/command_declaration.rs` (added to `lib.rs:119` beside
`command_metadata`), holding **only** what `CommandMetadata` genuinely cannot say. Roughly 60
lines, not 300.

```rust
/// Which form of the input state the implementation receives.
///
/// `CommandMetadata::state_argument` records *whether* a command takes a state; this records
/// *which form* the callable wants, which is a host calling convention the metadata has no field
/// for. See open question 3 on promoting it to real metadata.
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

/// The part of a command declaration that is about *binding an implementation*, not about the
/// command. Deserialized from the same document as [`CommandMetadata`], over disjoint keys.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct CommandBinding {
    /// `None` = the document did not say. Authoritative when present: it overwrites
    /// `state_argument`. When absent, whatever the document declared as `state_argument`
    /// stands — which is what makes a re-parsed `command_registry.yaml` round-trip.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<StateMode>,

    /// `None` = the document did not say. A host that can decide from the callable — JavaScript's
    /// thenable test — does so; one that cannot treats it as `false`.
    #[serde(default, rename = "async", skip_serializing_if = "Option::is_none")]
    pub is_async: Option<bool>,

    /// Whether the document carried an `arguments` key at all — the one thing
    /// `CommandMetadata::arguments: Vec<_>` cannot distinguish from an empty list, and the reason
    /// `adapter.rs:26-37` keeps a thread-local today. Deserialization-only: the value is ignored,
    /// only its presence recorded. `arguments: null` reads as absent, matching `spec.rs`'s `get`.
    #[serde(default, rename = "arguments",
            deserialize_with = "field_is_present", skip_serializing)]
    pub arguments_declared: bool,
}

fn field_is_present<'de, D: Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    Ok(Option::<serde::de::IgnoredAny>::deserialize(d)?.is_some())
}

impl CommandBinding {
    /// Applies the binding's decisions onto metadata deserialized from the same document.
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

## The pair, and why it is not a struct with a derived `Deserialize`

A host needs both halves out of one document. A `Deserializer` is consumed by a single pass, so a
combined type would need either `#[serde(flatten)]` (rejected: it requires `deserialize_any`, which
`serde-wasm-bindgen` handles badly — this reason survives from the first draft) or a hand-written
visitor over 20-odd fields (rejected: it re-creates the maintenance burden the whole revision
removes).

Instead, every real host has a **re-readable** source, so it makes two passes over disjoint keys:

```rust
pub struct CommandDeclaration {
    pub metadata: CommandMetadata,
    pub binding: CommandBinding,
}

impl CommandDeclaration {
    /// Two passes over one self-describing document. `&serde_json::Value` implements
    /// `Deserializer`, so this borrows rather than clones.
    pub fn from_json_value(value: &serde_json::Value) -> Result<Self, Error>;
}
```

`from_json_value` runs both passes, then `fill_declaration_defaults()` and `binding.apply(…)`, in
that order. `liquers-web` does the same two passes directly over its `&JsValue` with
`serde_wasm_bindgen`, never materialising JSON.

Neither struct sets `deny_unknown_fields` — each must ignore the other's keys. The cost is that a
typo'd `volatil: true` is silently ignored by both. That is already true of `JsCommandSpec::parse`
today, so it is not a regression; see open question 5.

## Rejected alternatives

| Option | Verdict |
|---|---|
| **A parallel `CommandDeclaration` struct mirroring `CommandMetadata`** (the first draft of this document) | **Rejected on review.** ~80% of its fields were identical; the substance was five missing `#[serde(default)]`. It renamed `argument_type` to `type`, contradicting Phase 1's own acceptance criterion 2; it changed `default` from `CommandParameterValue` to `serde_json::Value`, dropping `Query` defaults; and it silently omitted `presets`, `next`, `hints` and `CommandDefinition::Alias`, so `from_metadata`→`to_metadata` was not an identity. That round-trip passes at `HEAD` only by luck — the registry currently contains 0 of each (verified) — so the Phase 1 test would have certified a lossy format. A second 20-field struct with a renamed key and a different default representation is a second format wearing the first one's name. |
| Add `#[serde(default)]` and stop there | Insufficient: it leaves `state`, the async tri-state and declared-vs-inferred arguments homeless, which is the residue Part B exists for. |
| Make the export format tolerant is dangerous — "a registry file missing `cache` would silently deserialize as `true`" (first draft's objection to fixing `CommandMetadata`) | **Does not hold.** `cache` has no `skip_serializing_if`, so the exporter always writes it; the file is generated and never hand-edited; `registry_export` compares signatures and would catch a semantic drift; and the format already tolerates omission of 14 of 20 fields, so the strictness being protected does not exist. |
| Put `CommandBinding` in `command_metadata.rs` | Rejected: that file is 1397 lines, and metadata-versus-binding is exactly the distinction this issue draws. |
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
4. Two `serde_wasm_bindgen` passes over that copy: `CommandMetadata` and `CommandBinding`.
5. Refuse `namespace == RESERVED_NAMESPACE` — unchanged, keeping the `"reserved"` wording
   `command06_ns_reserved_namespace_is_refused` asserts.
6. `metadata.fill_declaration_defaults()`, then **override** `metadata.label` with
   `declared_label.unwrap_or_else(|| name.clone())` — the name **verbatim**, see below — then
   `metadata.module = "javascript"`, then `binding.apply(&mut metadata)`.
7. `if !binding.arguments_declared { infer_arguments(…) }` — unchanged, including every refusal
   message `command05_infer_refused_shapes` asserts. The thread-local `INFERRED_ARGUMENTS` in
   `adapter.rs:26-37` can now be dropped in favour of `binding.arguments_declared`, though that is
   an independent tidy-up and is **not** required by this change.
8. `IsAsync` from `binding.is_async`: `Some(true) → Async`, `Some(false) → Sync`, `None → Auto`.
   `IsAsync` stays in `liquers-web`, because "test whether the result is thenable" is a JavaScript
   notion; the wire format carries only the tri-state as `Option<bool>`.

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
`(CommandMetadata, CommandBinding, js_sys::Function)` triple is immune by construction.
**Deferred, not done** — it changes replay-on-rebuild and belongs with
`POST-INIT-COMMAND-REGISTRATION`. Recorded so the opportunity is not lost.

## Data ownership, errors, sync/async

- `CommandBinding` owns its data (`Option<StateMode>`, `Option<bool>`, `bool`); no lifetimes, no
  `Arc`, `Copy`-cheap in practice. `Clone` so a host may retain it for replay.
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
- `STATE-ARGUMENT-CONSTRUCTOR-SERDE-DEFAULT-DISAGREE` — filed by this design. Not a blocker: Part B
  makes `state_argument` explicitly the binding's decision, which sidesteps it for the declaration
  path, but the underlying inconsistency remains for anyone deserializing metadata directly.
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
| **Files** | Source: `liquers-core/src/command_metadata.rs` (8 serde attribute rows, 2 normalisers, 1 hand-written `Deserialize` — ~120 lines added, 0 removed), `liquers-core/src/command_declaration.rs` (new, ~60 lines), `liquers-core/src/lib.rs` (one `pub mod`), `liquers-web/src/command/spec.rs` (rewrite of `parse`, ~130 lines removed). Tests: colocated in both core modules, plus `liquers-core/tests/` for the registry round-trip. Specs: a pointer from `specs/reference/REGISTER_COMMAND_FSD.md` with its `reviewed:`/History rows; `specs/index.csv` regenerated. Generated files: **none** — `specs/command_registry.yaml` must be byte-identical. |
| **Impact area** | `core/commands`, `web`. Downstream: every JavaScript command registration path, `describeCommand`, and the environment rebuild/replay path via `REGISTERED_SPECS`. `liquers-py` is unaffected until it opts in. |
| **Module/crate reach** | **Not confined to one module.** `liquers-core` (two modules) and `liquers-web` (one). This alone fails the automatic-clearance condition of the autonomous procedure. |
| **Existing-test breakage** | Expected **0 assertion failures**; the estimate is soft. At risk: the 20 `wasm_bindgen_test`s in `liquers-web/tests/commands_COMMAND.rs`, of which four assert error wording (`:66`, `:409`, `:422`, `:509`) — all four preserved by keeping their producing code in `liquers-web`. `liquers-core/src/command_metadata.rs`'s own serde tests (`:1210`, `:1236`, `:1254`, `:1381`) must be unaffected; defaults and aliases do not change serialization, and `:1381` asserts an exact JSON string, so it is the sharpest tripwire for an accidental `Serialize` change. `liquers-lib`'s `registry_export` must stay green. Honest number: "0 expected, ~25 tests in the blast radius". |
| **New validation** | (1) Registry round-trip: `specs/command_registry.yaml` → parse → re-serialize → **byte-identical**. Stronger than the first draft's modulo-comparison, and possible only because nothing is dropped. (2) `{"name":"greet"}` after `fill_declaration_defaults` equals `CommandMetadata::from_key`, `state_argument` aside. (3) Parity with `register_command!` for one representative command including `metadata_version` after registry insertion. (4) `foo_bar` label parity: the JavaScript path yields `"foo_bar"`, the document path `"foo bar"`. (5) `gui_info` parity: a declared argument omitting `gui_info` yields `TextField(40)`, not `None`. (6) `CommandParameterValue`: all six input shapes in the Part C table. (7) YAML and JSON parse the same declaration to the same value. (8) Malformed: empty name, `multiple` not last, unknown argument type, non-array `arguments`, object-valued default. (9) The whole `liquers-web` COMMAND suite under Node. Commands: `cargo test -p liquers-core --lib`, `cargo test -p liquers-lib --lib --tests`, `bash scripts/check-build-matrix.sh`, and — after `cargo clean` — `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles`. |
| **Behavioural risk** | *Compatibility*: the JavaScript surface widens (`filename`, `expires`, `payload_required`, `presets`, `next`, `hints`, richer `ArgumentType`s, `Alias`, `Query` defaults all become declarable) and one diagnostic's wording changes; both deliberate and listed. *Persistence/data*: `specs/command_registry.yaml` must not move — now enforced by a byte-identical round-trip, not only by `registry_export`. *Metadata versions*: the `label` and `gui_info` parity tests exist precisely because a slip there silently re-expires assets. *Concurrency*: not applicable — parsing is pure. *Performance*: not applicable — registration is not a hot path. *Security*: a declaration is host-supplied data that becomes registered metadata; it cannot name a Rust implementation. *Error paths*: serde failures replace hand-written ones for malformed fields, wrapped so the command name survives. |
| **Recovery** | Part A is additive and behaviour-preserving on the serialize side; it can stay even if the rest is reverted. Part B is a new module nothing else depends on. Part C is the only change to an existing `Deserialize` and is the one to revert first if the registry moves. The `liquers-web` rewrite is separable and revertible on its own, since `JsCommandSpec`'s public shape is unchanged. Sequencing as A → B → C → web keeps every boundary real. |
| **Certainty** | Higher than the first draft, because the type being reused is the type already under test. Unverified and needing execution: (a) that `serde_wasm_bindgen` deserializes a JavaScript object into `CommandMetadata` including the `!Value`-tagged and shorthand default forms — the Part C visitor is written against `serde`, not against `serde-wasm-bindgen`'s value model, and its `deserialize_any` behaviour on a JS object is the specific unknown; (b) that `Option::<IgnoredAny>::deserialize` through `serde-wasm-bindgen` distinguishes an absent key from `null` the way `spec.rs`'s `get` does. Fallback for both: `js_sys::JSON::stringify` the run-less copy and go through `serde_json`, at the cost that a non-JSON default becomes an error rather than reaching the visitor. These are Phase 3 experiments, not assumptions. |

## Open questions for the gate

1. **`run` has no home here, and probably belongs to `CommandDefinition`.**
   `CommandDefinition` already *is* the "how is this command's implementation resolved" field:
   `Registered` means "look it up in the `CommandExecutor` by key" (`plan.rs:1555`), `Alias` means
   "rewrite to another key" (`plan.rs:1575`). A `run: "greet"` naming an entry in a host-supplied
   table is a third answer to that same question, so putting it in a sibling struct duplicates the
   concept. `run` is therefore **removed from this design** pending a decision between:
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
   not. Not decidable from the codebase — it depends on the shape of the intended host setup, so
   it is a maintainer decision.
2. **The `label` default split.** `liquers-web` keeps the name verbatim; Rust replaces underscores
   with spaces. Preserving the split (recommended, and what this document specifies) keeps every
   existing JavaScript command's `metadata_version` stable. Normalising to one rule is tidier at the
   cost of a one-off version change for underscored JavaScript commands, which re-expires their
   dependent assets.
3. **Should the value/text/state distinction become real metadata?** `StateMode` sits in
   `CommandBinding` because it is a calling convention. But "this command wants text, not the value"
   is something a planner or UI could use, and `state_argument.argument_type` is the field-shaped
   hole it would fit — `Any` for value, `String` for text, with no natural encoding for "the whole
   `State`". That is `COMMAND-METADATA-ENHANCEMENTS`'s territory (IO typing). **Recommendation:**
   leave it in the binding now, and record the question there rather than pre-empting it.
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

## Scope note

Fixing `CommandMetadata`'s serde defaults is a smaller change than the first draft's new type, which
weakens the issue's `P0`. The issue file itself already flags the tension with
`DOCS_STRUCTURE_GUIDE.md` §4.4 — it is scheduling weight, not a defect. With the fix at five
attributes plus a small module, **P1 looks right**, and the gate is the place to settle it.

## Review record

*Against Phase 1:* acceptance criteria 1-6 all still map to named tests, and criterion 2 ("field
names agree with `specs/command_registry.yaml`") is now satisfied literally rather than
approximately — they are the same struct. Criterion 3's round-trip strengthens from
"equal modulo `impl_version`" to byte-identical. Criterion 1's wording needs the amendment made in
the revised Phase 1: the minimal form is a `CommandMetadata`, not a new type. The non-goals (Python
binding, post-init registration, `register_command!`, exporter output) appear nowhere in the plan;
the `snapshot_declaration` cleanup stays deferred.

*Against the codebase:* every claim was read at `HEAD` and the deserialization limits were
re-measured, not carried over. Newly verified for this revision: the 14-of-20 default count; that
`ArgumentInfo::label` is also required, which Phase 1 missed; the `ArgumentGUIInfo::None` versus
`TextField(40)` mismatch; the `state_argument` constructor/serde disagreement; `argument_type`'s 101
occurrences in the registry; and that the registry contains 0 `presets`, 0 `hints`, 0 `next`,
0 `!Query` and 0 `GlobalEnum`, which is why the first draft's lossy round-trip would have passed.

Risk is **not** understated: this crosses two crates and three modules, touches a public JavaScript
API, changes an existing `Deserialize`, and the "0 broken tests" estimate is qualified with the 25
tests in range. It fails the automatic clearance conditions of the procedure and needs an explicit
decision.
