Based on `HEAD`, read rather than remembered. Nothing here is implemented.

# Phase 2 — Solution and architecture

> **Rewritten 2026-08-29** against [`purpose-and-semantics.md`](./purpose-and-semantics.md) and the
> decisions recorded there. Two earlier drafts are summarised in §Rejected alternatives: a parallel
> `CommandDeclaration` mirroring `CommandMetadata`, and a "fix `CommandMetadata` and add the
> residue" design. Both mistook the feature for a serialization problem. It is a *composition*
> problem.

## Diagnosis

A command declaration is the runtime equivalent of `register_command!`. Its substance is not a
struct but a **pipeline**, of which the middle is shareable:

```
1. populate   host introspection fills what it can discover          host-specific
2. enhance    the author's declaration is merged over it             SHARED
3. fill       defaults are derived for whatever is still absent      SHARED
4. use        convert to CommandMetadata + CallSpec, or error        SHARED
```

Stage 1 is irreducibly per-language — `inspect.signature`, a JavaScript source parse, `syn`, or
nothing at all in the plain-document case. Stages 2–4 are the deliverable.

Three facts about `HEAD` shape the design, all measured:

1. **`CommandMetadata` cannot be deserialized from a partial document.** Four fields lack
   `#[serde(default)]` — `label`, `cache`, `volatile`, `definition` — plus `ArgumentInfo::label`.
   Fourteen of its twenty fields already have one, so this is an oversight, not an invariant.
   Measured: `{"name":"greet"}` fails with `missing field 'label'`.
2. **Absence and default are not distinguishable in a typed representation.** `#[serde(default)]`
   collapses "the author said nothing about `cache`" into "the author said `true`" — which is
   exactly the distinction a merge needs. This is why stage 2 operates on the serialized form.
3. **Two constructor/serde defaults disagree**, and both would silently change `metadata_version`:
   `ArgumentGUIInfo::Default` is `None` while `ArgumentInfo::any_argument` sets `TextField(40)`; and
   `CommandMetadata::new`/`from_key` set `state_argument: Some(..)` while the serde default is
   `None`. The second is filed as `STATE-ARGUMENT-CONSTRUCTOR-SERDE-DEFAULT-DISAGREE`.

## Chosen solution

A new module `liquers-core/src/command_declaration.rs`, plus small additive changes to
`command_metadata.rs`. Four parts, in dependency order, each separately revertible.

### Part A — the merge (stage 2)

Stage 2 operates on `serde_json::Value`, so *absence is key-absence* and no representation has to
encode it. This is the decision that makes the rest simple.

```rust
/// A command declaration in the course of being composed: the baseline from introspection with
/// zero or more declarations merged over it. Not yet a command — call [`Self::build`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommandDeclaration {
    doc: serde_json::Value,
}

impl CommandDeclaration {
    /// Stage 1's result. `Value::Null` or an empty object means no introspection ran — the
    /// plain-document case — which is what relaxes the unknown-argument rule below.
    pub fn from_introspection(baseline: serde_json::Value) -> Self;

    /// Stage 2. Merges `declaration` over what is already here, by the rules below.
    /// May be called more than once; merging is associative, so layered declarations compose.
    pub fn enhance(&mut self, declaration: &serde_json::Value) -> Result<(), Error>;

    /// Stage 3. Idempotent; fills only what is still absent.
    pub fn fill_defaults(&mut self);

    /// Stage 4. Converts and validates, reporting every missing or inconsistent field.
    pub fn build(&self) -> Result<(CommandMetadata, CallSpec), Error>;
}
```

**Merge rules.**

| Shape | Rule |
|---|---|
| object over object | merged key by key, recursively |
| scalar or array over anything | replaces it |
| absent key | leaves the baseline untouched — this is the whole point |
| `null` | an ordinary value, **not** a deletion marker (no removal — decision Q2) |
| `arguments` | **special: merged by `name`, never by position** |

The `arguments` rule in full:

- An entry naming an argument the baseline has is **merged into it**, field by field. This is the
  case the whole design exists for: attaching a `gui_info` or a `label` to one argument without
  restating its type and default.
- An entry naming an argument the baseline does **not** have is **rejected** (decision Q3), because
  Liquers binds query parameters positionally and a typo would silently misbind.
- **Exception, and it is load-bearing:** when the baseline has *no* `arguments` key at all,
  discovery did not run, and the declaration establishes the list. A baseline with
  `"arguments": []` means a function with no parameters and *is* subject to the reject rule. The
  serialized form gives this distinction for free; a typed one would have needed a separate flag.
- Order comes from the baseline when it exists, otherwise from the declaration. A declaration may
  not reorder.

**Removal is not supported, and costs nothing.** The case for it — a function parameter the command
should not expose — is handled in stage 1, which belongs to the host: introspection simply does not
emit that parameter. So `null` stays an ordinary value.

### Part B — derived defaults (stage 3)

Fills what is still absent, never what is present. Runs **after** the merge: deriving first would
make a derived value indistinguishable from a declared one and block it.

The label rule replaces `name.replace("_", " ")`, which appears in eight places
(`command_metadata.rs:417,440,453,466,487,508,893,925`), capitalises nothing, and does not handle
camelCase. Every readable label in `specs/command_registry.yaml` (`To text`,
`Commands documentation`) is hand-written today.

```
split on '_' and at lower→upper boundaries; a run of capitals followed by a lowercase
letter splits before the last capital; lowercase each word unless it is all-caps;
capitalise the first character of the result
```

| Name | Label |
|---|---|
| `to_text` | `To text` |
| `toText` | `To text` |
| `toHTML` | `To HTML` |
| `parseHTTPResponse` | `Parse HTTP response` |

The same rule derives argument labels. **It applies to the declaration path only** (decision Q5):
`register_command!` keeps `name.replace("_", " ")`, so no existing command's `metadata_version`
moves. Rust function names are snake case and the capitalisation is cosmetic there.

Remaining defaults come from `CommandMetadata::from_key`, plus `gui_info: TextField(40)` for an
argument that declares none — matching `ArgumentInfo::any_argument` rather than
`ArgumentGUIInfo::Default`, which is diagnosis point 3.

### Part C — conversion and validation (stage 4)

`build` deserializes the composed document into `CommandMetadata` and a `CallSpec`, and reports what
is missing or inconsistent. It needs `CommandMetadata` to deserialize from a document that omits what
stage 3 did not have to fill, so Part C carries the serde changes the previous draft called Part A:

| Target | Change |
|---|---|
| `CommandMetadata::{label, volatile, definition}` | `#[serde(default)]` |
| `CommandMetadata::cache` | `#[serde(default = "true_default")]` |
| `ArgumentInfo::label` | `#[serde(default)]` |
| `ArgumentInfo::argument_type` | `#[serde(alias = "type")]` — keeps today's JavaScript spelling |
| `ArgumentType` | `#[serde(alias)]` for `str`, `text`, `integer`, `number`, `boolean` |
| `CommandParameterValue` | hand-written `Deserialize` accepting the tagged form *and* a bare value |

All eight are **deserialize-only**. No field is added, removed, renamed or retyped and no
`Serialize` behaviour changes, so `specs/command_registry.yaml` stays byte-identical and
`registry_export` stays green.

`CommandParameterValue` accepts `!Value 2`/`{"Value":2}`, `!Query "a/b"`, the bare string `"None"`
(the None variant, as the exporter writes it), a bare scalar or array as shorthand for `Value(…)`,
and `null` as `Value(Null)` — preserving `js_default_to_json`'s treatment. Any other map is refused.
A default whose literal value is the string `"None"` must be written `!Value 'None'`.

Validation reports, with the command and argument named: an empty name; an argument entry naming an
unknown argument; a `multiple` argument that is not last; a default that does not fit its declared
type; an unknown argument type. Global-enum references are **not** resolved here — that needs a
`CommandMetadataRegistry` and stays where it happens today, at registry insertion and plan building.

### Part D — the call specification

What `register_command!` decides at compile time and a document must state. Per-command; keyword
argument passing is out of scope (decision C3), so nothing here is per-argument.

```rust
/// How the callable is invoked once the plan has resolved the parameter values. The runtime
/// counterpart of the macro's `CommandSignature` and of `liquers-web`'s `CallableSpec`.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CallSpec {
    /// Which form of the input state the callable receives.
    #[serde(default)]
    pub state: StateMode,

    /// How a `multiple` argument's elements reach the callable.
    #[serde(default)]
    pub variadic: VariadicPassing,

    /// `None` = not declared; a host that can decide from the callable does so.
    #[serde(default, rename = "async", skip_serializing_if = "Option::is_none")]
    pub is_async: Option<bool>,
}

/// Mirrors the macro's `StateParameter` (`liquers-macro/src/registration.rs:785`) variant for
/// variant.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum StateMode {
    /// No state argument — a source command.
    #[default] None,
    Value,
    #[serde(alias = "string")] Text,
    State,
}

/// How the elements of a `multiple` argument are passed.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum VariadicPassing {
    /// One list argument. Rust's only mode — `CommandArguments::get_multiple`
    /// (`liquers-core/src/commands.rs:151`) always collects into `Vec<T>`.
    #[default] Collect,
    /// Spread across the call as individual arguments.
    Spread,
}
```

`Collect` is the default because it matches Rust and because it is the shape
`ParameterValue::MultipleParameters` already has. Nothing is grandfathered by the choice: today a
JavaScript declaration cannot express `multiple` or `injected` at all — `parse_arguments`
(`spec.rs:196-234`) reads only `name`, `type` and `default` — so both variadic modes are new surface.

**A host that cannot honour a declared mode must fail at registration**, with a message naming the
mode and the host, never silently ignore it. This is how the design stays honest about the
portability limits recorded in [`portability-analysis.md`](./portability-analysis.md): `Spread` is
meaningful in Python and JavaScript and impossible in Rust; the async tri-state is needed only where
the host cannot decide from the callable.

## `liquers-web` re-implementation

`JsCommandSpec` keeps its public shape — `key`, `metadata`, `state_mode`, `is_async`, `run`,
`arguments_inferred` — and `register_js_command` (`adapter.rs:79`) is untouched. `parse` becomes the
pipeline:

1. Object check, and `name` pre-checked with `Reflect` so today's messages survive verbatim.
2. `Reflect::get(spec, "run")` checked with `is_function()` — unchanged wording.
3. Shallow copy of the declaration **without** `run`, so no `js_sys::Function` reaches serde;
   convert it to `serde_json::Value` via `serde_wasm_bindgen`.
4. Refuse `namespace == RESERVED_NAMESPACE` — unchanged, keeping the `"reserved"` wording.
5. **Stage 1:** if the copy has no `arguments` key, run `infer_arguments` (`spec.rs:281`, unchanged,
   including every refusal message `command05` asserts) and build the baseline from it; otherwise the
   baseline has no `arguments` key and the declaration establishes the list. This is exactly today's
   declared-XOR-inferred behaviour, now expressed as the merge's own rule rather than as a
   thread-local — `INFERRED_ARGUMENTS` (`adapter.rs:26-37`) can be dropped.
6. **Stages 2–4:** `enhance`, `fill_defaults`, `build`.
7. Override `metadata.label` with the name **verbatim** when the declaration gave none, and set
   `module = "javascript"`. The verbatim rule is JavaScript's, not the derived rule, so no existing
   command's `metadata_version` moves. A parity test covers `foo_bar`.
8. `IsAsync` from `call_spec.is_async`: `Some(true) → Async`, `Some(false) → Sync`, `None → Auto`.
   `IsAsync` stays in `liquers-web` — "test whether the result is thenable" is a JavaScript notion.

Deleted: `get`, `get_string`, `get_bool`, `parse_arguments`, `parse_argument_type`,
`js_default_to_json` — about 130 lines. Retained: `infer_arguments`, `parameter_list`,
`strip_comments`, `is_plain_identifier` (~107 lines), which are stage 1 and are not shareable.

`snapshot_declaration` (`environment.rs:171-193`) exists because `REGISTERED_SPECS` retains the
caller's mutable `JsValue`. Retaining a composed `CommandDeclaration` is immune by construction, but
that changes replay-on-rebuild and stays **deferred** to `POST-INIT-COMMAND-REGISTRATION`.

## Rejected alternatives

| Option | Verdict |
|---|---|
| **Draft 1 — `CommandDeclaration` mirroring `CommandMetadata` field for field** | Rejected: ~80% identical fields, and it renamed `argument_type` to `type`, changed defaults to `serde_json::Value` (dropping `Query` defaults), and silently lost `presets`, `next`, `hints` and `Alias`. Its round-trip test would have passed only because the registry happens to contain none of those. |
| **Draft 2 — fix `CommandMetadata`'s serde and add the residue** | Superseded, and instructively: it mistook the feature for a serialization problem. The five serde attributes survive as Part C, but as a prerequisite of `build`, not as the feature. Its `CallingConvention` was about a third of the call specification actually needed. |
| **A typed partial: mirror struct with `Option` on every field** | Rejected: it is draft 1 with `Option`s, with the drift problem intact, and it still needs a side flag to distinguish "no introspection ran" from "introspected, no arguments". |
| **Hand-written `Deserialize` tracking presence per field** | Rejected: all the cost of the mirror plus a hand-written impl over twenty-odd fields. |
| **`#[serde(flatten)]` to compose the halves** | Rejected: needs `deserialize_any`, which `serde-wasm-bindgen` handles badly. |
| **Keyword argument passing in the call spec** | Out of scope by decision C3. The name is retained in the metadata, so adding it later is additive. |
| **Removal / deletion markers in the merge** | Rejected by decision Q2: handled in stage 1, which the host owns. |

## Data ownership, errors, sync/async

- `CommandDeclaration` owns one `serde_json::Value`; no lifetimes, no `Arc`. `CallSpec` is three
  `Copy` fields and derives `Copy`.
- Every failure is `liquers_core::error::Error` via
  `Error::from_error(ErrorType::ParameterError, …)` — no new error type, no `Error::new`.
  `liquers-web` wraps serde failures so the command name survives.
- Nothing async: composition is pure. The declared `async` flag describes the *implementation*.
- `enhance` and `build` return `Result`; `from_introspection` and `fill_defaults` are infallible,
  and `fill_defaults` is idempotent.
- Matches over `StateMode` and `VariadicPassing` are explicit — no `_ =>` arm.

## Reuse

`CommandMetadata`, `CommandMetadata::from_key`, `ArgumentInfo`, `ArgumentInfo::any_argument`,
`ArgumentType`, `ArgumentGUIInfo`, `CommandParameterValue`, `CommandDefinition`, `Expires`,
`PayloadRequirement`, `CommandPreset`, `ParameterPreset` and `CommandKey` are reused unchanged as the
*output* of stage 4. The declaration never re-enumerates their fields, so adding a metadata field
never touches this module — the constraint that survives from draft 2's anti-duplication argument.

## Related open issues

- `ARGUMENT-DECLARATION-IS-ALL-OR-NOTHING` — **resolved by this design**; Part A's by-name argument
  merge is exactly what it asks for. To be closed when this lands.
- `STATE-ARGUMENT-CONSTRUCTOR-SERDE-DEFAULT-DISAGREE` — not a blocker: `CallSpec.state` makes the
  state argument an explicit decision, but the inconsistency remains for direct deserializers.
- `JS-COMMAND-CANNOT-ACCESS-CONTEXT` — the most likely next `CallSpec` field; the type must stay open
  to it.
- `POST-INIT-COMMAND-REGISTRATION` — owns the `snapshot_declaration` cleanup.
- `COMMAND-METADATA-ENHANCEMENTS`, `REGISTER-COMMAND-ENUM` — would extend the same field set; reuse
  means they extend one place, and per-argument enums would arrive through the by-name merge.
- `REGISTRY-IMPL-VERSION-DRIFT-UNDETECTED` — `impl_version` comes from registration, not from a
  declaration.
- `WORKSPACE-NOT-RUSTFMT-CLEAN` — arrived on `main`; the `command_metadata.rs` edits should not be
  what first trips a formatting check.

## The `run` field

**The proposal.** `run` carries a callable. Registration turns it into one of two things: the
function registered as an executable command in `CommandRegistry` (**branch 1**,
`CommandDefinition::Registered`), or a `CommandDefinition::Alias` onto a per-runtime dispatch command
(**branch 2** — a `pycall` / `jscall` registered once, using commands themselves as the pluggable
runtime). This avoids a `CommandDefinition::HostFunction { runtime, module, name }`, which would
require the interpreter to support pluggable runtimes.

**Rejecting `HostFunction` is right.** It would put runtime selection into the planner, which today
knows only `Registered` and `Alias` (`plan.rs:1555-1600`), and every consumer of `CommandDefinition`
would have to learn what a runtime identifier means. The payoff would be a metadata field the
planner immediately delegates anyway. Agreed, and it is now recorded as rejected rather than open.

**`run` cannot be a field of the shared declaration.** A `js_sys::Function` or `Py<PyAny>` is not
serde-able, which is the observation the issue opens with. So `run` belongs to the *host's*
declaration surface — the JavaScript object, the Python decorator's argument — and is stripped
before the document enters stage 2, exactly as `spec.rs` strips it today. The shared pipeline never
sees a callable; it sees the `CommandDefinition` that registration chose. This is compatible with the
proposal and preserves the merge.

### Branch 1 works, for every language assessed

`CommandRegistry::register_command` (`commands.rs:613-625`) accepts any closure
`Fn(&State, CommandArguments, Context) -> Result<Value, Error>` meeting the `MaybeSend`/`MaybeSync`
bounds, and returns `&mut CommandMetadata` for the caller to overwrite with the built metadata. The
host captures its callable in that closure. No interpreter change, no metadata pollution, and the
command's own metadata governs both planning and execution because they are the same command.

| Host | Captured value | Assessment |
|---|---|---|
| JavaScript | `js_sys::Function` | **Proven** — this is what `register_js_command` does today |
| Python | `Py<PyAny>` | Works; `Py<T>` is `Send + Sync`, GIL acquired inside the closure |
| Rhai | `FnPtr` + `AST` | Both `'static` and clonable |
| Rune | `Function` | `'static`; Rune's async maps onto the async registration path |
| Starlark | `FrozenValue` in a `FrozenModule` | Works on the *frozen* path only — a live `Value` is heap-borrowed. Worth confirming before relying on it |

### Branch 2 does not work as proposed, and the reason is specific

`ResolvedParameterValues::from_action_extended` (`plan.rs:995-1011`) zips `head_parameters` against
**the alias command's own `arguments`**, then resolves the alias's remaining arguments into
individual slots:

```rust
let mut values = head_parameters.iter()
    .zip(command_metadata.arguments.iter())     // the ALIAS's arguments, not the target's
    .map(|(x, arginfo)| ParameterValue::from_command_parameter_value(&arginfo.name, x))
    .collect_vec();
let n = values.len();
for a in command_metadata.arguments.iter().skip(n) { … }
```

That is the right semantics for a genuine alias — `head` is `slice` with its first argument
pre-filled, so the two share an argument list. It is the wrong shape for a dispatcher.

A `pycall(state, fn_id, *args)` registered through `register_command!` reads its variadic with
`CommandArguments::get_multiple(i, …)` (`commands.rs:151`), which requires **one**
`ParameterValue::MultipleParameters` slot at index `i`. That slot is produced by `pop_value` only
when the *alias's* metadata declares a `multiple` argument. So `foo`'s metadata would have to be
shaped `[fn_id, args multiple]` — pycall's shape — discarding `foo`'s per-argument types, labels,
defaults and widget hints. **That is precisely the metadata this feature exists to carry.**

There is an escape hatch, and it should be judged on its costs rather than dismissed: register the
dispatcher with a **hand-written** closure instead of the macro, ignoring its own declared shape and
iterating `0..args.len()` by index. Then `foo` keeps its real arguments, with the dispatch id as
argument 0 marked `injected` — `accepted_parameter_count` (`plan.rs:918-925`) filters injected, so
the user-facing arity stays right, and `lib/commands.rs:172` filters them out of documentation. Its
costs:

- a synthetic argument in **every** declared command's metadata, which lands in any exported registry;
- it depends on `CommandArguments`'s public accessors being sufficient outside `liquers-core`
  (`parameters` is `pub(crate)`; `len()` is public);
- a **state-mode mismatch** that branch 1 cannot have: if `foo` declares `state: none` its
  `state_argument` is `None`, but the dispatcher it retargets to is registered *with* a state. Under
  branch 1 the metadata and the executor are the same command, so the question never arises.

**And `CommandDefinition::Alias` is untested.** A repository-wide search finds only its two match
arms — `plan.rs:1575` and `lib/commands.rs:165` — with no test, no `register_command!` statement that
produces one, and no entry in `specs/command_registry.yaml` (`grep -c 'Alias'` returns 0). Building
the Python and JavaScript story on it means productionising an untested path whose head-parameter
semantics have never been exercised.

### Recommendation

**v1: branch 1 only.** It works for all six languages, changes nothing in the planner, keeps every
declared argument's metadata intact, and is already proven in `liquers-web`.

**Keep branch 2 as a designed-for future**, but for its *real* payoff, which is not dispatch:
**serializability**. A `Registered` command's implementation is invisible in metadata, so an exported
registry cannot say which function a command was. `Alias { pycall, [id] }` records the binding, which
is what would let an environment rebuild or a registry export reconstruct declared commands — the
concern behind `POST-INIT-COMMAND-REGISTRATION` and `snapshot_declaration`. Pursuing it needs the
head-parameter semantics settled and `Alias` given tests first, so it belongs to that issue rather
than to this one.

## Risk analysis

| Assessment | Record |
|---|---|
| **Files** | Source: `liquers-core/src/command_declaration.rs` (new, ~400 lines: the merge, defaults derivation, `CallSpec`, `build`), `liquers-core/src/command_metadata.rs` (8 deserialize-only rows plus a hand-written `Deserialize`, ~120 lines added, 0 removed), `liquers-core/src/lib.rs` (one `pub mod`), `liquers-web/src/command/spec.rs` (rewrite of `parse`, ~130 lines removed), `liquers-web/src/command/adapter.rs` (drop `INFERRED_ARGUMENTS`). Tests: colocated, plus `liquers-core/tests/` for the registry round-trip. Specs: a pointer from `REGISTER_COMMAND_FSD.md`; `specs/index.csv` regenerated. Generated files: **none**. |
| **Impact area** | `core/commands`, `web`. Downstream: every JavaScript registration path, `describeCommand`, and the rebuild/replay path via `REGISTERED_SPECS`. `liquers-py` is unaffected until it opts in — it has no Python-side registration today. |
| **Module/crate reach** | Two crates, three modules. Fails the automatic-clearance condition. |
| **Existing-test breakage** | Expected **0 assertion failures**, estimate soft. At risk: the 20 `wasm_bindgen_test`s in `commands_COMMAND.rs`, four of which assert error wording (`:66`, `:409`, `:422`, `:509`) — all preserved by keeping their producing code in `liquers-web`; `command_metadata.rs`'s serde tests (`:1210`, `:1236`, `:1254`, `:1381`), where `:1381` asserts an exact JSON string and is the sharpest tripwire for an accidental `Serialize` change; and `registry_export`. Honest number: "0 expected, ~25 in the blast radius". |
| **New validation** | **Merge laws** (the substance): an empty declaration is an identity; `enhance` twice equals once; a declared scalar wins; an argument entry augments by name and preserves untouched fields; an unknown argument name is rejected when the baseline has an `arguments` key and accepted when it does not; `null` sets rather than deletes; order never changes. **Defaults**: the four label cases in the Part B table; `gui_info` yields `TextField(40)`. **Conversion**: `{"name":"greet"}` builds; `command_registry.yaml` parses and re-serializes **byte-identically**; `CommandParameterValue`'s six input shapes; malformed inputs each name the command and argument. **Parity**: the JavaScript path yields `foo_bar`, the document path `Foo bar`; `register_command!` and a declaration agree on `metadata_version` for one representative command. **Suites**: `cargo test -p liquers-core --lib`, `cargo test -p liquers-lib --lib --tests`, `bash scripts/check-build-matrix.sh`, and after `cargo clean` `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles`. |
| **Behavioural risk** | *Compatibility*: the JavaScript surface widens substantially — `filename`, `expires`, `payload_required`, `presets`, `next`, `hints`, `multiple`, `injected`, `Alias`, richer `ArgumentType`s and query-valued defaults all become declarable; needs a line in the TypeScript stubs. One diagnostic's wording changes from bespoke to serde-derived with the command name prefixed. *Persistence*: `command_registry.yaml` must not move — enforced by a byte-identical round-trip. *Metadata versions*: the `foo_bar` and `gui_info` parity tests exist because a slip there re-expires assets. *Concurrency, performance*: not applicable — composition is pure, registration is not a hot path. *Security*: a declaration is host-supplied data that becomes registered metadata; it cannot name a Rust implementation. |
| **Recovery** | Parts A, B and D are a new module nothing depends on. Part C is the only change to existing `Deserialize` impls and is the one to revert first if the registry moves. The `liquers-web` rewrite is separable, since `JsCommandSpec`'s public shape is unchanged. Sequence C → A → B → D → web. |
| **Certainty** | The merge and defaults are pure functions over `serde_json::Value` and are exhaustively testable, which is where most of the new code is — this is more certain than either earlier draft. Unverified and needing execution in Phase 3: (a) `serde_wasm_bindgen`'s conversion of a JavaScript declaration object to `serde_json::Value`, in particular `deserialize_any` on nested objects and on the argument-default forms; fallback is `js_sys::JSON::stringify` plus `serde_json::from_str`, at the cost that a non-JSON default becomes an error. (b) That the camel-split rule's acronym handling is what is wanted — the table in Part B is the specification, and `parseHTTPResponse` is the case to confirm. |

## Open questions for the gate

1. **`run`** — *proposal evaluated 2026-08-29; see §The `run` field below. One sub-question remains
   for the gate: accept branch 1 only for v1 (recommended), or productionise `Alias` now?*
2. **Widening the JavaScript surface.** Reusing `CommandMetadata` as the output makes a large field
   set declarable from JavaScript at once. **Recommendation:** accept, and add the TypeScript stubs
   line — a restricted subset would be a second format again.
3. **Unknown-field tolerance.** The merge cannot reject unknown *metadata* keys without a key list,
   so a typo'd `volatil: true` merges in and is dropped at `build`. Today's parser ignores it too, so
   this is not a regression — but a document-driven host makes typos likelier. **Recommendation:**
   accept now; `build` is the natural place to warn later.
4. **`CallSpec` versioning.** The macro versions its wrapping model (`WrapperVersion::V2`,
   `registration.rs:1101`) as a codegen seam. A document outlives the binary that reads it.
   **Recommendation:** defer, and record that every future field must be optional with a
   backward-compatible default so additive evolution stays available.
5. **Commit split.** C → A → B → D → web is five commits; the web rewrite is where the risk is.
   **Recommendation:** one PR, so the web half can be reverted alone.

## Review record

*Against the purpose statement:* all four stages appear, with 2–4 shared and 1 host-specific; the
declaration carries a metadata contribution as a partial and never re-enumerates metadata fields;
merge is by name and handles nesting; defaults derivation runs after merge and includes the
camel/snake label rule; the call specification covers state, variadic passing and asynchrony, and
omits keyword passing per C3. Every recorded decision is honoured, and the two places where a
decision has a non-obvious consequence — Q4 being answered by C3, and Q3 needing the
no-introspection exception — are stated rather than left implicit.

*Against the codebase:* every cited line was read at `HEAD`. Verified for this rewrite: that
`parse_arguments` (`spec.rs:196-234`) reads only `name`, `type` and `default`, so `multiple` and
`injected` are genuinely new JavaScript surface; that `get_multiple` (`commands.rs:151`) always
collects, so Rust has one variadic mode; and the eight `name.replace("_", " ")` sites.

*Review passes* were run inline rather than by sub-agents, since this session has not been asked to
spawn them; the conformity checks the workflow assigns to separate reviewers (against Phase 1, the
purpose statement, and the codebase) are folded into this record.

*Risk is not understated:* two crates, three modules, a public JavaScript API, a changed
`Deserialize`, and a "0 broken tests" estimate qualified with the 25 tests in range. This needs an
explicit decision at the gate.
