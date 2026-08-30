Based on `HEAD`, read rather than remembered. Nothing here is implemented.

# Phase 4 — Implementation plan

## Overview

Ten steps, ordered so that each one is independently revertible and the riskiest work is isolated.
The sequence is **serde first, core module second, `liquers-web` last** — the core additions are new
code nothing depends on, while the `liquers-web` rewrite touches a public JavaScript API and is the
only step that can break a passing suite.

| # | Step | Crate | Risk |
|---|---|---|---|
| 1 | Serde defaults and aliases | core | low |
| 2 | `CommandParameterValue` permissive `Deserialize` | core | **medium** — the only change to an existing impl |
| 3 | Module skeleton, `CommandDeclaration`, the merge | core | low |
| 4 | Conventions | core | low |
| 5 | Derived defaults | core | low |
| 6 | `build` and validation | core | low |
| 7 | Integration tests | core | low |
| 8 | **Spike:** `serde_wasm_bindgen` conversion | web | **highest** — can change step 9 |
| 9 | `JsCommandSpec::parse` rewrite | web | medium |
| 10 | Documentation | specs | low |

**Step 8 is a spike and comes before step 9 deliberately.** Phase 2 records `serde_wasm_bindgen`'s
handling of a JavaScript declaration object as its largest unverified claim, and the fallback
(`js_sys::JSON::stringify` then `serde_json::from_str`) changes the code path. Learning that after
writing step 9 means writing it twice.

## Findings from `rust-best-practices`, applied

Run against the Phase 2 architecture before planning. Three findings, one blocking:

**BLOCKING — `CommandDeclaration` needs `#[serde(transparent)]`.** Phase 2 sketches

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommandDeclaration { doc: serde_json::Value }
```

A derived `Deserialize` on a one-field struct expects `{"doc": …}`, not the declaration itself. Every
document would fail to parse. Fix: `#[serde(transparent)]`, which is carried into step 3 below.

**ADVISORY — `registration()` needs a defined empty case.** Returning `&serde_json::Value` when the
key is absent needs something to borrow. `serde_json::Value::Null` is a unit variant and so is
`static`-constructible; the alternative is `Option<&Value>`, which pushes an `unwrap` onto every
caller and is worse. Step 3 uses a `static NULL`.

**ADVISORY — a command-level `hints` key is silently dropped.** `CommandMetadata` has no such field
(`COMMAND-METADATA-HAS-NO-COMMAND-LEVEL-HINTS`), and it has no `deny_unknown_fields`, so an author
writing one gets no error and no effect. Step 6 warns rather than fails, since failing would block
the day that field is added.

**Confirmed rather than assumed:** `Error::from_error<E: Display>` (`error.rs:129`) accepts a
`String`, so the planned construction is correct and no `Error::new` is needed;
`ErrorType::ParameterError` exists (`error.rs:18`); `serde_json` and `serde_yaml` are direct
dependencies of `liquers-core` (`Cargo.toml:58-59`), so no dependency is added.

## Implementation steps

### Step 1 — Serde defaults and aliases

**File:** `liquers-core/src/command_metadata.rs`

Deserialize-only attributes. **No field added, removed, renamed or retyped; no `Serialize`
behaviour changed.**

| Target | Attribute |
|---|---|
| `CommandMetadata::label` | `#[serde(default)]` |
| `CommandMetadata::cache` | `#[serde(default = "true_default")]` |
| `CommandMetadata::volatile` | `#[serde(default)]` |
| `CommandMetadata::definition` | `#[serde(default)]` |
| `ArgumentInfo::label` | `#[serde(default)]` |
| `ArgumentInfo::argument_type` | `#[serde(alias = "type")]` |
| `ArgumentType::{String,Integer,Float,Boolean}` | `#[serde(alias …)]` for `str`, `text`, `integer`, `number`, `boolean` |

`true_default` is a new private `fn true_default() -> bool { true }`; `false_default` already exists
(`:384`) as the precedent for the pattern.

**Validation**

```bash
cargo test -p liquers-core --lib command_metadata
cargo test -p liquers-lib --test registry_export
```

`command_metadata.rs:1381` asserts an exact JSON string and is the tripwire for an accidental
`Serialize` change. It must pass untouched.

**Agent:** haiku · skills `rust-best-practices` · knowledge: this step, `command_metadata.rs:155-520`
and `:772-940`.

**Rollback:** revert the file. Nothing depends on it yet.

---

### Step 2 — `CommandParameterValue` permissive `Deserialize`

**File:** `liquers-core/src/command_metadata.rs`

Replace the derived `Deserialize` with a hand-written one; **leave `Serialize` derived and
untouched**.

```rust
impl<'de> Deserialize<'de> for CommandParameterValue {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error>;
}
```

Accepted forms, per Phase 2 §Part C: `{"Value": x}` / `!Value x`; `{"Query": …}` / `!Query …`; the
bare string `"None"` → `None`; a bare scalar or array → `Value(...)`; `null` → `Value(Null)`; any
other map → an error naming the offending shape.

Implemented as a `Visitor` over `deserialize_any`. Two constraints to hold:

- **`deserialize_any` is required**, so a non-self-describing format would fail. JSON, YAML and
  `serde-wasm-bindgen` are all self-describing; step 8 confirms the third.
- A map is inspected for exactly one key, `Value` or `Query`. A single-key map with any other name
  is an error, not a silent `Value`.

**Validation**

```bash
cargo test -p liquers-core --lib command_parameter_value   # BUILD04, BUILD05
cargo test -p liquers-lib --test registry_export
diff <(git show HEAD:specs/command_registry.yaml) specs/command_registry.yaml   # must be empty
```

**Agent:** sonnet · skills `rust-best-practices`, `liquers-unittest` · knowledge: this step,
`CommandParameterValue` (`:293-330`), `ArgumentInfo::default`'s serde attributes, Phase 2 §Part C.

**Rollback:** restore the derive. This is the single most revertible step and the first to undo if
the registry moves.

---

### Step 3 — Module skeleton, `CommandDeclaration`, the merge

**Files:** `liquers-core/src/command_declaration.rs` (new), `liquers-core/src/lib.rs` (one `pub mod`)

```rust
/// A command declaration in the course of being composed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]                      // ← see the blocking finding above
pub struct CommandDeclaration {
    doc: serde_json::Value,
}

impl CommandDeclaration {
    pub fn from_introspection(baseline: serde_json::Value) -> Self;
    pub fn enhance(&mut self, declaration: &serde_json::Value) -> Result<(), Error>;
    pub fn as_value(&self) -> &serde_json::Value;
    pub fn registration(&self) -> &serde_json::Value;   // static NULL when absent
}
```

The merge, per Phase 2 §Part A: objects recurse; scalars and arrays replace; `arguments` merges by
`name`; an entry naming an unknown argument is an error **unless** the baseline has no `arguments`
key; order comes from the baseline; `null` is a value, not a deletion.

**Implementation notes.** No `unwrap()` — index a `Value` with `.get()` and propagate. Every error is
`Error::from_error(ErrorType::ParameterError, format!("command {name:?}: …"))`. Extract the command
name once at the top of `enhance` so every message can carry it.

**Validation**

```bash
cargo test -p liquers-core --lib merge_tests     # MERGE01-MERGE12
```

**Agent:** sonnet · skills `rust-best-practices`, `liquers-unittest` · knowledge: Phase 2 §Part A,
Phase 3 §merge tests (which are the specification, not an illustration).

**Rollback:** delete the module and its `pub mod` line.

---

### Step 4 — Conventions

**File:** `liquers-core/src/command_declaration.rs`

```rust
/// Which conventions are applied. All default to on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Conventions { pub context: bool, pub state: bool }

impl Default for Conventions { fn default() -> Self { Self { context: true, state: true } } }

impl CommandDeclaration {
    /// Stage 3. Reads the `conventions` key, applies what is enabled, and removes both keys'
    /// recognised arguments from `arguments`.
    pub fn apply_conventions(&mut self) -> Result<(), Error>;
}
```

- `context` — an argument named `context` is removed; its **index before removal** is written to
  `registration.context`.
- `state` — the **first** argument, if named `state`, `value` or `text`, is moved to
  `state_argument`; the spelling is written to `registration.state`.
- `conventions: false` disables both; `conventions: { context: false }` disables one.
- Idempotent: applying twice equals applying once (CONV07).

No `_ =>` arm anywhere — the two flags are matched explicitly.

**Validation**

```bash
cargo test -p liquers-core --lib convention_tests    # CONV01-CONV07
```

**Agent:** sonnet · skills `rust-best-practices` · knowledge: Phase 2 §Part E, Phase 3 CONV tests,
and `liquers-macro/src/registration.rs:489,1134` for the rule being reproduced.

**Rollback:** remove `apply_conventions` and `Conventions`; the pipeline still runs, producing
commands with `state` and `context` as ordinary arguments — the pre-design behaviour.

---

### Step 5 — Derived defaults

**File:** `liquers-core/src/command_declaration.rs`

```rust
impl CommandDeclaration {
    /// Stage 4. Idempotent; fills only what is absent.
    pub fn fill_defaults(&mut self);
}

/// snake_case and camelCase to a readable label. See the table in Phase 2 §Part B.
pub fn derive_label(name: &str) -> String;
```

`derive_label` is public because both the reference and any future integration will want the same
rule, and because DEF01 tests it directly. The four cases in the Phase 2 table are the
specification: `to_text` and `toText` → `To text`, `toHTML` → `To HTML`, `parseHTTPResponse` →
`Parse HTTP response`.

Other defaults per Phase 2 §Part B, including `gui_info: TextField(40)` for an argument that
declares none — matching `ArgumentInfo::any_argument`, **not** `ArgumentGUIInfo::Default`, which is
`None`. `namespace` is deliberately not normalised.

**Validation**

```bash
cargo test -p liquers-core --lib default_tests    # DEF01-DEF06
```

**Agent:** haiku · skills `rust-best-practices` · knowledge: Phase 2 §Part B, Phase 3 DEF tests,
`command_metadata.rs:414-426` (`any_argument`) and `:920-940` (`from_key`).

**Rollback:** remove `fill_defaults`; declarations then carry only what was written or discovered.

---

### Step 6 — `build` and validation

**File:** `liquers-core/src/command_declaration.rs`

```rust
impl CommandDeclaration {
    /// Stage 5. `registration` and `conventions` are declaration-only and do not reach metadata.
    pub fn build(&self) -> Result<CommandMetadata, Error>;
    /// Runs stages 3-5 in order. The normal entry point.
    pub fn finish(&mut self) -> Result<CommandMetadata, Error>;
}
```

`build` deserializes `self.doc` into `CommandMetadata` — the declaration-only keys are ignored
because `CommandMetadata` sets no `deny_unknown_fields` — then validates: empty or missing `name`; a
`multiple` argument that is not last; a default that does not fit its type; an unrecognised argument
type. Each message names the command and, where applicable, the argument. Global enums are **not**
resolved (VAL05).

A command-level `hints` key produces a **warning to `eprintln!`**, never a failure — see the
advisory finding. Library code must not touch stdout.

**Validation**

```bash
cargo test -p liquers-core --lib   # BUILD01-BUILD05, VAL01-VAL05, HINT01-HINT04
```

**Agent:** sonnet · skills `rust-best-practices`, `liquers-unittest` · knowledge: Phase 2 §Part C
and §Part D, Phase 3 BUILD/VAL/HINT tests.

**Rollback:** with steps 3-6 reverted the module is gone; steps 1-2 stand alone as a defect fix.

---

### Step 7 — Integration tests

**File:** `liquers-core/tests/command_declaration.rs` (new), plus
`liquers-core/tests/fixtures/commands.yaml` — **Example 2 from Phase 3, verbatim**, so the
documented example and the tested input cannot drift.

INT01 (registry byte-identical), INT03 (label parity), INT04 (YAML and JSON agree) go here.

**INT02 needs a decision on placement.** It registers one command both ways and compares
`metadata_version`, which needs `register_command!` — a `liquers-macro` dependency that
`liquers-core`'s test targets may not have. If it does not build there, it moves to
`liquers-lib/tests/`, which already depends on both. Phase 3 flagged this; resolve it by trying the
core placement first and moving on failure, not by guessing now.

**Validation**

```bash
cargo test -p liquers-core --test command_declaration
cargo test -p liquers-lib --lib --tests
bash scripts/check-build-matrix.sh
```

**Agent:** sonnet · skills `liquers-unittest` · knowledge: Phase 3 §Integration tests, CLAUDE.md
§Testing.

**Rollback:** delete the test file. Rolling back a test is never a fix — if INT01 fails, the bug is
in step 1 or 2.

---

### Step 8 — Spike: `serde_wasm_bindgen` conversion (do this before step 9)

**File:** `liquers-web/tests/commands_DECLARATION.rs` (new)

One `wasm_bindgen_test` that builds a JavaScript declaration object — including a nested
`arguments` array, a bare `default: 2`, a tagged `default: {Value: 2}`, and a `registration` block —
converts it with `serde_wasm_bindgen::from_value::<serde_json::Value>`, and asserts the result
equals the `serde_json::json!` literal it should be.

**This step exists to answer a question, not to ship a feature.** Two outcomes:

- **Pass** → step 9 proceeds as Phase 2 describes.
- **Fail** → step 9 uses `js_sys::JSON::stringify` on the run-less copy plus `serde_json::from_str`.
  The cost, recorded in Phase 2: a non-JSON default becomes "absent" rather than an error. Record
  which path was taken in the Phase 5 summary either way.

**Validation**

```bash
cargo clean
cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles
```

`cargo clean` first — CLAUDE.md's build-matrix note: the wasm target and the native loop together
exhaust the disk allowance.

**Agent:** sonnet · skills `rust-best-practices` · knowledge: `liquers-web/Cargo.toml:32`
(`serde-wasm-bindgen` is present), `spec.rs:106-193`, Phase 2 §Certainty.

**Rollback:** delete the test. Its result is information, not a dependency.

---

### Step 9 — `JsCommandSpec::parse` rewrite

**File:** `liquers-web/src/command/spec.rs`, and `adapter.rs` for the thread-local removal

`JsCommandSpec` keeps its public shape. `parse` becomes the pipeline, per Phase 2
§`liquers-web` re-implementation:

1. object check; `name` pre-checked with `Reflect` so today's two messages survive verbatim;
2. `run` resolved with `is_function()` — unchanged wording;
3. shallow copy without `run`; convert to `serde_json::Value` by the path step 8 chose;
4. reserved-namespace refusal — unchanged `"reserved"` wording;
5. stage 1: `infer_arguments` when the copy has no `arguments` key — unchanged, with every refusal
   message `command05` asserts;
6. stages 2-5: `enhance`, `apply_conventions`, `fill_defaults`, `build`;
7. `metadata.label` overridden with the name **verbatim** when undeclared; `module = "javascript"`;
8. `IsAsync` from `registration`.

**Deleted:** `get`, `get_string`, `get_bool`, `parse_arguments`, `parse_argument_type`,
`js_default_to_json` (~130 lines), and `INFERRED_ARGUMENTS` (`adapter.rs:26-37`).
**Retained:** `infer_arguments`, `parameter_list`, `strip_comments`, `is_plain_identifier`.

**A caution specific to this step.** `liquers-web` currently sets `state_argument` unconditionally
via `CommandMetadata::from_key`, and JavaScript's `state` mode is declared, not inferred. The
`state` convention keys on an argument *named* `state`, which a JavaScript declaration does not
have. **`liquers-web` must therefore keep reading its state mode from the declaration** and not rely
on the convention; the convention serves hosts whose introspection reports a state parameter.
`command13` asserts every state-passing mode delivers its documented content and will catch a slip.

**Validation**

```bash
cargo clean
cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles
```

All 20 `commands_COMMAND.rs` tests, with the four error-wording assertions (`:66`, `:409`, `:422`,
`:509`) unchanged.

**Agent:** sonnet · skills `rust-best-practices` · knowledge: `spec.rs` in full, `adapter.rs:26-37`
and `:79-120`, `commands_COMMAND.rs`, Phase 2 §`liquers-web` re-implementation.

**Rollback:** `git revert` this step alone. `JsCommandSpec`'s public shape is unchanged, so
`adapter.rs` and `environment.rs` are unaffected and the core module can stay.

---

### Step 10 — Documentation

- **Promote** `design/command-declaration/COMMAND_DECLARATION.md` to
  `specs/reference/COMMAND_DECLARATION.md`, removing only the not-yet-true banner. Its §9 worked
  example is replaced by Phase 3's Example 1, which shows the merge step by step.
- **`specs/reference/REGISTER_COMMAND_FSD.md`** gains a pointer to the runtime counterpart, plus a
  `## History` row and a `reviewed:` bump in the same commit.
- **`specs/guides/LANGUAGE-INTEGRATION_GUIDE.md`** links change from the design path to
  `reference/`; History row and `reviewed:` bump.
- **`specs/README.md`** — the design moves to complete.
- `python3 scripts/docs_index.py`.
- **Close** `ARGUMENT-DECLARATION-IS-ALL-OR-NOTHING` (resolved by the by-name merge) and
  `COMMAND-DECLARATION-FORMAT`, per `DOCS_STRUCTURE_GUIDE.md` §4.3.

**Agent:** sonnet · knowledge: `DOCS_STRUCTURE_GUIDE.md` §4.3, §9.2.

**Rollback:** documentation reverts independently of code, but a merged code change with reverted
docs is worse than neither. Revert together.

## Testing plan

| When | Command |
|---|---|
| After every core step | `cargo test -p liquers-core --lib` |
| After steps 1, 2, 7 | `cargo test -p liquers-lib --test registry_export` |
| After step 7 | `bash scripts/check-build-matrix.sh` |
| After steps 8, 9 | `cargo clean && cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles` |
| Before the PR | all of the above, plus `git diff --exit-code specs/command_registry.yaml` |

`cargo clean` before the wasm runs is not optional — CLAUDE.md records that the native and wasm
targets together exhaust the 30 GB allowance.

## Task splitting (Agent Assignment)

| Steps | Model | Why |
|---|---|---|
| 1, 5 | haiku | Mechanical, fully specified by a table |
| 2, 3, 4, 6, 7, 10 | sonnet | Judgement about error messages, merge edge cases, test placement |
| 8, 9 | sonnet | Cross-language, and step 9 touches a public API with wording assertions |

Every agent needs `rust-best-practices`; steps 2, 3, 6, 7 also need `liquers-unittest`. All need
`CLAUDE.md` §Code Conventions — the no-`unwrap`, no-`println!`, no-`Error::new`, no-`_ =>` rules are
the ones a generated implementation most often breaks.

## Rollback plan

**Per step:** each step above names its own. The dependency order is strictly
1 → 2 → 3 → 4 → 5 → 6 → 7, then 8 → 9, with 10 last; reverting step *n* requires reverting
everything above it in that chain, except that steps 8-9 revert independently of 1-7.

**Full rollback:** revert steps 3-10 and keep steps 1-2. Those two are a standalone latent-defect
fix — `{"name":"greet"}` becoming deserializable is worth having whether or not the declaration
lands — and they change no serialized output.

**Partial completion:** if steps 1-7 land and 8-9 do not, the core type exists, is tested, and is
usable by a future Python binding, while `liquers-web` keeps its hand-written parser. That is a
coherent stopping point and the natural place to split if the PR grows too large. File the remainder
as an issue rather than leaving it implied — a design that ships in part leaves an issue, per
`DOCS_STRUCTURE_GUIDE.md` §5.6.

## Documentation updates

Step 10 covers the whole set. Two notes for Phase 5 evidence: record which conversion path step 8
selected, and whether INT02 could live in `liquers-core/tests/` or had to move to `liquers-lib`.
Neither is knowable now and both will be asked later.

`CLAUDE.md` needs no change: no new build command, no new feature, no new crate. If step 9 removes
the last use of a dependency in `liquers-web`, its `Cargo.toml` gets a cleanup — check rather than
assume.

## Phase 5 Entry Criteria

Phase 5 starts when **all** of these hold, and not before:

1. Steps 1-9 are complete, or a deliberate stopping point is agreed and the remainder is filed as an
   issue (see §Rollback plan, *Partial completion*).
2. Every command in §Testing plan passes, including the wasm loop after `cargo clean`.
3. `git diff --exit-code specs/command_registry.yaml` is clean — the generated file did not move.
4. All review comments on the PR are answered or incorporated.
5. The two Phase 5 evidence items are recorded: which conversion path step 8 selected, and where
   INT02 ended up living.

Phase 5 then promotes `COMMAND_DECLARATION.md` to `specs/reference/`, writes
`phase5-documentation.md`, reviews the affected documents, and closes
`COMMAND-DECLARATION-FORMAT` and `ARGUMENT-DECLARATION-IS-ALL-OR-NOTHING`. Under
`workflow: liquers-project` it is **mandatory** and normally lands in the same PR before merge.

## Review record

*Against Phase 1:* every acceptance criterion has a step that satisfies it and a test that proves
it. Criterion 4's byte-identical round-trip is enforced twice — by INT01 in step 7 and by a
`git diff --exit-code` before the PR.

*Against Phase 2:* Parts A-E map to steps 3, 5, 2+6, 6, 4. The `liquers-web` re-implementation is
step 9. The sequencing Phase 2 recommends (C → A → B → D → web) is preserved, with the conventions
inserted and the spike hoisted ahead of the rewrite.

*Against Phase 3:* every numbered test has a step that makes it pass — MERGE in 3, CONV in 4, DEF in
5, BUILD/VAL/HINT in 6, INT in 7-9. No test is orphaned and no step is untested.

*Against the codebase:* `Error::from_error<E: Display>` (`error.rs:129`), `ErrorType::ParameterError`
(`:18`), `false_default` as the precedent for `true_default` (`command_metadata.rs:384`), and
`serde_json`/`serde_yaml` as direct core dependencies (`Cargo.toml:58-59`) were all read at `HEAD`.

*`rust-best-practices` was applied to the Phase 2 architecture* and found one blocking issue — the
missing `#[serde(transparent)]` — which would have made every document fail to parse. It is fixed in
step 3 rather than left for the implementer to discover.

*Review passes were run inline* rather than by sub-agents, this session not having been asked to
spawn them.

*Certainty.* High for steps 1-7: they are pure functions over `serde_json::Value` with the tests
written. **Step 8 is the one genuine unknown**, and it is isolated as a spike precisely so its answer
arrives before any code depends on it. Step 9 is medium: the mechanical part is clear, but four
error-wording assertions and `command13`'s state-mode coverage are the kind of thing a rewrite
breaks quietly.
