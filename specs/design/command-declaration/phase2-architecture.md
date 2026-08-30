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
4. use        convert to CommandMetadata, or error                     SHARED
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
3. **`state_argument` is descriptive, not a planner input.** Its only non-test consumers are
   `liquers-lib/src/egui/widgets.rs:705` (UI display), the registration sites, and the macro's
   compile-time wrapper generation (`registration.rs:1147`). Neither `plan.rs` nor the interpreter
   reads it. Whether a command receives its input state is decided by the executor closure, not by
   metadata — which is why the *form* of the state can be a hint rather than a specification.
4. **Two constructor/serde defaults disagree**, and both would silently change `metadata_version`:
   `ArgumentGUIInfo::Default` is `None` while `ArgumentInfo::any_argument` sets `TextField(40)`; and
   `CommandMetadata::new`/`from_key` set `state_argument: Some(..)` while the serde default is
   `None`. The second is filed as `STATE-ARGUMENT-CONSTRUCTOR-SERDE-DEFAULT-DISAGREE`.

## Chosen solution

**A function from loosely-specified JSON to `CommandMetadata`**, taking two inputs and composing
them. A new module `liquers-core/src/command_declaration.rs`, plus small additive changes to
`command_metadata.rs`. Four parts, in dependency order, each separately revertible.

The value is coordination rather than capability — ~136 lines leave `liquers-web` and ~300 enter
`liquers-core`, so it is net more code that is written and tested once. That makes it contingent on a
second consumer; see `purpose-and-semantics.md` §The test this design has to pass, which the gate
should apply before the open questions below.

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
    pub fn build(&self) -> Result<CommandMetadata, Error>;
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

`build` deserializes the composed document into `CommandMetadata` and reports what
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

### Part D — hints

Some facts about a command are neither metadata nor portable: which form of the state the callable
wants, whether a variadic reaches it spread or collected, whether it must be awaited. Each is
meaningful in some hosts and meaningless in others, and **`liquers-core` does not interpret any of
them.** They are carried, merged and preserved so that the host that understands them can read them
back.

There is precedent: `ArgumentInfo` already has
`hints: serde_json::Map<String, serde_json::Value>` — *"Free dictionary of hints for the argument"*
(`command_metadata.rs:399-403`). `CommandMetadata` has no command-level equivalent, and that
asymmetry is the natural place for this.

**Recommended: add a command-level `hints` map to `CommandMetadata`**, mirroring the argument-level
one:

```rust
/// Free dictionary of hints for the command. Not interpreted by `liquers-core`; a language
/// integration reads back what it wrote. Namespace keys by integration — `python`, `javascript` —
/// so two hosts declaring the same command cannot collide.
#[serde(skip_serializing_if = "serde_json::Map::is_empty")]
#[serde(default)]
pub hints: serde_json::Map<String, serde_json::Value>,
```

```yaml
name: repeat
arguments:
  - name: count
    type: int
hints:
  javascript: { state: text, variadic: spread, async: false }
```

The consequence is that **the declaration needs no structure of its own at all.** It is a partial
`CommandMetadata`, merged and defaulted; hints ride along as ordinary fields and the deep merge
composes them like any other map. Part D adds one field to one struct and no new types.

| | For | Against |
|---|---|---|
| Hints in `CommandMetadata` (recommended) | The declaration collapses to "partial metadata"; hints survive export and round-trip, so a rebuilt environment can recover how to call; mirrors `ArgumentInfo::hints` | A free-form field slightly dilutes "precise authoritative specification"; `metadata_version` moves when a hint changes — arguably correct, since the command really did change |
| Hints on the declaration only, dropped at `build` | Keeps `CommandMetadata` strict | The host must read them from the declaration before conversion, and they are lost on export — so an environment rebuild cannot recover them |

**Nothing about the hint *vocabulary* is designed here.** No key is reserved, none is validated, and
the examples above are illustrative. Fields are added as integrations need them, which is exactly
what a free dictionary is for. What this design commits to is only that there is somewhere to put
them and that the merge preserves them.

## The handover boundary

A host's native declaration is not portable data: a JavaScript object literal holds a
`js_sys::Function`, a Python decorator's kwargs hold a callable. **The host performs a handover**,
splitting its native structure into two:

```
native structure (JsValue, Python dict, Starlark dict, …)
        │
        ├── the callable and anything else non-portable  ──►  the host keeps it; out of scope here
        │
        └── the data part  ──►  CommandDeclaration  ──►  merge, defaults, build  ──►  CommandMetadata
```

`liquers-web` already does exactly this: it strips `run` before parsing (`spec.rs:130-140`). The
design's only requirement is that the boundary is explicit and that nothing non-portable crosses it.

Everything on the host's side of that line is **out of scope**: what to do with the callable, how to
register it, whether that becomes a `CommandDefinition::Registered` executor or something else, and
how the callable is ultimately invoked. `run` therefore does not appear in this design in any form.

`CommandDeclaration` is a **type, not a JSON convention**, even though it holds `serde_json::Value`
internally. That matters for the handover: a PyO3 or `wasm-bindgen` binding can expose the object and
let a host build it up incrementally — `with_label`, `add_argument`, `set_hint` — without
constructing JSON by hand, while the merge still gets its absence-tracking for free.

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
6. **Stages 2-4:** `enhance`, `fill_defaults`, `build` — yielding a `CommandMetadata`.
7. Override `metadata.label` with the name **verbatim** when the declaration gave none, and set
   `module = "javascript"`. The verbatim rule is JavaScript's, not the derived rule, so no existing
   command's `metadata_version` moves. A parity test covers `foo_bar`.
8. `StateMode` and `IsAsync` are read from the hints, by `liquers-web`, exactly as they are read
   today — both types **stay in `liquers-web`**, which is the point of the descope. An absent
   `async` hint means `IsAsync::Auto`; an absent `state` hint means `StateMode::None`. Core neither
   defines nor validates either.

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
| **A typed call specification in core** (`CallSpec { state, variadic, is_async }`, drafted here before the descope) | **Rejected by scope decision.** It was the part fighting portability, and `portability-analysis.md` had already measured why: the metadata half is usable by all six languages, while the call spec's fields were needed by two to five of them and meaningless in the rest. Typing it in core forces every host to agree on notions some of them do not have. As hints it costs one field and constrains nobody. |
| **`run`, in any form — a callable, a name, or a `CommandDefinition::HostFunction`** | **Out of scope.** A callable cannot cross into portable data at all, and what the host does with it — register an executor, build an alias — is host-specific. `HostFunction` would additionally have pushed runtime selection into the planner. See §The handover boundary. |
| **Keyword argument passing in the call spec** | Moot — there is no call spec. Should a host need it, it is a hint. |
| **Removal / deletion markers in the merge** | Rejected by decision Q2: handled in stage 1, which the host owns. |

## Data ownership, errors, sync/async

- `CommandDeclaration` owns one `serde_json::Value`; no lifetimes, no `Arc`. There are no other new
  types — that is the shape of the descope.
- Every failure is `liquers_core::error::Error` via
  `Error::from_error(ErrorType::ParameterError, …)` — no new error type, no `Error::new`.
  `liquers-web` wraps serde failures so the command name survives.
- Nothing async: composition is pure. The declared `async` flag describes the *implementation*.
- `enhance` and `build` return `Result`; `from_introspection` and `fill_defaults` are infallible,
  and `fill_defaults` is idempotent.
- No new enums, so no new matches; the existing no-`_ =>` rule is unaffected.

## Reuse

`CommandMetadata`, `CommandMetadata::from_key`, `ArgumentInfo`, `ArgumentInfo::any_argument`,
`ArgumentType`, `ArgumentGUIInfo`, `CommandParameterValue`, `CommandDefinition`, `Expires`,
`PayloadRequirement`, `CommandPreset`, `ParameterPreset` and `CommandKey` are reused unchanged as the
*output* of stage 4. The declaration never re-enumerates their fields, so adding a metadata field
never touches this module — the constraint that survives from draft 2's anti-duplication argument.

## Related open issues

- `ARGUMENT-DECLARATION-IS-ALL-OR-NOTHING` — **resolved by this design**; Part A's by-name argument
  merge is exactly what it asks for. To be closed when this lands.
- `STATE-ARGUMENT-CONSTRUCTOR-SERDE-DEFAULT-DISAGREE` — **now more relevant, not less.** With no
  `CallSpec` to decide `state_argument`, the declaration sets it as an ordinary metadata field, so a
  document that omits it gets the serde default (`None`) while a constructor gives `Some(..)`.
  Worth fixing alongside, though still not a blocker: `fill_defaults` can settle it explicitly.
- `JS-COMMAND-CANNOT-ACCESS-CONTEXT` — no longer touches this design; whatever it needs is a hint or
  host-side work.
- `COMMAND-ALIAS-DEFINITION-UNTESTED` — no longer touches this design; filed independently and
  still worth fixing.
- `POST-INIT-COMMAND-REGISTRATION` — owns the `snapshot_declaration` cleanup.
- `COMMAND-METADATA-ENHANCEMENTS`, `REGISTER-COMMAND-ENUM` — would extend the same field set; reuse
  means they extend one place, and per-argument enums would arrive through the by-name merge.
- `REGISTRY-IMPL-VERSION-DRIFT-UNDETECTED` — `impl_version` comes from registration, not from a
  declaration.
- `WORKSPACE-NOT-RUSTFMT-CLEAN` — arrived on `main`; the `command_metadata.rs` edits should not be
  what first trips a formatting check.

## Risk analysis

| Assessment | Record |
|---|---|
| **Files** | Source: `liquers-core/src/command_declaration.rs` (new, ~300 lines: the merge, defaults derivation, `build`), `liquers-core/src/command_metadata.rs` (8 deserialize-only rows plus a hand-written `Deserialize`, ~120 lines added, 0 removed), `liquers-core/src/lib.rs` (one `pub mod`), `liquers-web/src/command/spec.rs` (rewrite of `parse`, ~130 lines removed), `liquers-web/src/command/adapter.rs` (drop `INFERRED_ARGUMENTS`). Tests: colocated, plus `liquers-core/tests/` for the registry round-trip. Specs: a pointer from `REGISTER_COMMAND_FSD.md`; `specs/index.csv` regenerated. Generated files: **none**. |
| **Impact area** | `core/commands`, `web`. Downstream: every JavaScript registration path, `describeCommand`, and the rebuild/replay path via `REGISTERED_SPECS`. `liquers-py` is unaffected until it opts in — it has no Python-side registration today. |
| **Module/crate reach** | Two crates, three modules. Fails the automatic-clearance condition. |
| **Existing-test breakage** | Expected **0 assertion failures**, estimate soft. At risk: the 20 `wasm_bindgen_test`s in `commands_COMMAND.rs`, four of which assert error wording (`:66`, `:409`, `:422`, `:509`) — all preserved by keeping their producing code in `liquers-web`; `command_metadata.rs`'s serde tests (`:1210`, `:1236`, `:1254`, `:1381`), where `:1381` asserts an exact JSON string and is the sharpest tripwire for an accidental `Serialize` change; and `registry_export`. Honest number: "0 expected, ~25 in the blast radius". |
| **New validation** | **Merge laws** (the substance): an empty declaration is an identity; `enhance` twice equals once; a declared scalar wins; an argument entry augments by name and preserves untouched fields; an unknown argument name is rejected when the baseline has an `arguments` key and accepted when it does not; `null` sets rather than deletes; order never changes. **Defaults**: the four label cases in the Part B table; `gui_info` yields `TextField(40)`. **Conversion**: `{"name":"greet"}` builds; `command_registry.yaml` parses and re-serializes **byte-identically**; `CommandParameterValue`'s six input shapes; malformed inputs each name the command and argument. **Hints**: a hint map merges like any other map and survives `build` and re-serialization untouched. **Parity**: the JavaScript path yields `foo_bar`, the document path `Foo bar`; `register_command!` and a declaration agree on `metadata_version` for one representative command. **Suites**: `cargo test -p liquers-core --lib`, `cargo test -p liquers-lib --lib --tests`, `bash scripts/check-build-matrix.sh`, and after `cargo clean` `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles`. |
| **Behavioural risk** | *Compatibility*: the JavaScript surface widens substantially — `filename`, `expires`, `payload_required`, `presets`, `next`, `hints`, `multiple`, `injected`, `Alias`, richer `ArgumentType`s and query-valued defaults all become declarable; needs a line in the TypeScript stubs. One diagnostic's wording changes from bespoke to serde-derived with the command name prefixed. *Persistence*: `command_registry.yaml` must not move — enforced by a byte-identical round-trip. *Metadata versions*: the `foo_bar` and `gui_info` parity tests exist because a slip there re-expires assets. *Concurrency, performance*: not applicable — composition is pure, registration is not a hot path. *Security*: a declaration is host-supplied data that becomes registered metadata; it cannot name a Rust implementation. |
| **Recovery** | Parts A and B are a new module nothing depends on; Part D is one field on one struct. Part C is the only change to existing `Deserialize` impls and is the one to revert first if the registry moves. The `liquers-web` rewrite is separable, since `JsCommandSpec`'s public shape is unchanged. Sequence C → A → B → D → web. |
| **Certainty** | The merge and defaults are pure functions over `serde_json::Value` and are exhaustively testable, which is where most of the new code is — this is more certain than either earlier draft. Unverified and needing execution in Phase 3: (a) `serde_wasm_bindgen`'s conversion of a JavaScript declaration object to `serde_json::Value`, in particular `deserialize_any` on nested objects and on the argument-default forms; fallback is `js_sys::JSON::stringify` plus `serde_json::from_str`, at the cost that a non-JSON default becomes an error. (b) That the camel-split rule's acronym handling is what is wanted — the table in Part B is the specification, and `parseHTTPResponse` is the case to confirm. |

## Open questions for the gate

1. **Where do hints live?** A command-level `hints` map on `CommandMetadata` (recommended — the
   declaration then needs no structure of its own, and hints survive export), or on the declaration
   only, dropped at `build`? See the table in Part D.
2. **Widening the JavaScript surface.** The pipeline's output is `CommandMetadata`, so `filename`,
   `expires`, `payload_required`, `presets`, `next`, `multiple`, `injected`, `Alias`, richer
   `ArgumentType`s and query-valued defaults all become declarable from JavaScript at once.
   **Recommendation:** accept, and add a line to the TypeScript stubs — a restricted subset would be
   a second format again.
3. **Unknown-field tolerance.** The merge cannot reject unknown *metadata* keys without a key list,
   so a typo'd `volatil: true` merges in and is dropped at `build`. Today's parser ignores it too, so
   this is not a regression, but a document-driven host makes typos likelier. **Recommendation:**
   accept now; `build` is the natural place to warn later. Note this interacts with hints: a
   free-form map means a typo inside `hints` can never be caught by core at all.
4. **Complexity, re-checked.** The descope removes Part D's types and the whole `run` question. What
   remains — the merge, defaults derivation, serde fixes, validation — sits at the **M/L boundary**
   rather than clearly at `L`. The `liquers-project` conversion is already made and the merge still
   warrants Phase 3 examples and tests, so **recommendation: stay at `L` and keep the workflow**;
   raised only so the re-scope is deliberate rather than inherited.
5. **Commit split.** C → A → B → D → web is five commits; the web rewrite carries the risk.
   **Recommendation:** one PR, so the web half can be reverted alone.

## Review record

*Against the purpose statement, as descoped:* all four stages appear, with 2-4 shared and 1
host-specific; the declaration carries a metadata contribution as a partial and never re-enumerates
metadata fields; merge is by name and handles nesting; defaults derivation runs after merge and
includes the camel/snake label rule; the output is `CommandMetadata` and nothing else. How to call
the function is out of scope, retained only as uninterpreted hints, and the handover boundary is
stated so nothing non-portable enters the shared layer. Q3's no-introspection exception is still
required and still stated; Q4 and C3 are moot along with the call spec.

*Against `portability-analysis.md`:* the descope keeps exactly the column that analysis found
portable to all six languages (metadata) and drops the one it found unevenly justified (the call
spec, needed by two to five of them). The evidence supported this pivot before it was made, which is
the strongest argument for it.

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
