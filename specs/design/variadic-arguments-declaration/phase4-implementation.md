# Phase 4: Implementation Plan - Declarable variadic command arguments

## Overview

Seven steps, in dependency order: liquers-core provides the accessor, liquers-macro emits it,
liquers-lib consumes it, then the generated registry and the documentation catch up.

Steps 1-3 are additive and independently landable — after each one the workspace compiles and every
existing test passes. Step 4 is the first behaviour change visible outside the design (the two
polars commands), step 5 regenerates a checked-in generated file, and steps 6-7 are documentation.

The plan is unusually confident for three reasons, all established earlier and not re-argued here:
the runtime half already works and is untouched; the plan-level behaviour was **measured** in Phase
3 with `liquers-validate` rather than predicted; and the macro rejections are parse-level, so they
are testable without new tooling.

**Estimated size:** ~120 lines of non-test code across three crates, ~350 lines of tests.

## Implementation Steps

### Step 1 — `CommandArguments::get_multiple` (liquers-core)

**File:** `liquers-core/src/commands.rs`, between `get` (`:102`) and `get_injected` (`:128`).

**Signature:**

```rust
pub fn get_multiple<T: FromParameterValue<T>>(
    &self,
    i: usize,
    name: &str,
) -> Result<Vec<T>, Error>
```

**Body shape** (not the implementation — the branch structure that must exist):

```rust
let p = self.get_parameter(i, name)?;
match p {
    ParameterValue::MultipleParameters(elements) => {
        let mut out = Vec::with_capacity(elements.len());
        for element in elements {
            out.push(Self::convert_multiple_element::<T>(element, name)?);
        }
        Ok(out)
    }
    // every other variant: the command declared `multiple`, the plan did not resolve it as such
    ParameterValue::DefaultValue(..) | ParameterValue::ParameterValue(..)
    | ParameterValue::OverrideValue(..) | ParameterValue::DefaultLink(..)
    | ParameterValue::ParameterLink(..) | ParameterValue::OverrideLink(..)
    | ParameterValue::EnumLink(..) | ParameterValue::Placeholder(_)
    | ParameterValue::Injected(_) | ParameterValue::None => Err(Error::general_error(format!(
        "Argument {i} '{name}' is declared as multiple but was not resolved as a parameter list"
    )).with_position(&self.action_position)),
}
```

with a private per-element helper so the two matches stay readable:

```rust
fn convert_multiple_element<T: FromParameterValue<T>>(
    element: &ParameterValue,
    name: &str,
) -> Result<T, Error>
```

**Requirements, each of which a test pins:**

- **Enumerate every `ParameterValue` variant explicitly.** No `_ =>` arm, in either match — this is
  a Liquers hard rule, and the point of it is that a new variant becomes a compile error here.
- **Ignore `self.values` entirely.** Do not add a fast path. `set_value` is populated only for
  parameters where `param.link()` is `Some` (`interpreter.rs:470`), and
  `MultipleParameters::link()` is `None` (`plan.rs:876`), so a variadic slot is never present
  there. A fast path would be dead code that looks load-bearing.
- **Attach the element's position**, not the action's, to every per-element failure — Phase 3
  measured that each element carries its own `Position`, and U5 asserts offset 23 rather than the
  action's 6.
- Link element variants → `Error::general_error` mentioning "link"; structurally impossible
  variants (`MultipleParameters`, `Injected`, `Placeholder`, `None` inside a list) →
  `Error::unexpected_error`. `pop_value` already refuses to place the latter group inside a list
  (`plan.rs:744-775`), so those arms are exhaustiveness, not defence.
- Doc comment states that this is for `multiple` arguments only and why the `TryFrom<E::Value>`
  bound of `get` is absent.

**Do NOT:** add any trait impl. The whole design rests on adding a method instead — see Phase 2,
"Trait Implementations".

**Validation:**
```bash
cargo test -p liquers-core --lib commands::
cargo test -p liquers-core --lib
```

**Agent:** Sonnet · skills: `rust-best-practices` · knowledge: this step, Phase 2 "Function
Signatures", `liquers-core/src/commands.rs:96-160`, `liquers-core/src/plan.rs:363-386` (the
`ParameterValue` variants), Phase 3 tests U1-U6.

---

### Step 2 — Unit tests for `get_multiple` (liquers-core)

**File:** `liquers-core/src/commands.rs`, in the first `#[cfg(test)] mod tests` (`:614`).

Write U1-U6 exactly as given in Phase 3, using `type TestEnv = SimpleEnvironment<Value>` (the alias
the file's second test module already uses — `mod` at `:724`, alias at `:733`).

U5 is the one that must not be softened: it asserts `err.position.offset == 23` — the *element's*
offset — with `args.action_position` deliberately set to a different value, so a regression that
attaches the action position instead fails loudly.

**Validation:** `cargo test -p liquers-core --lib commands::tests`

**Agent:** Haiku · skills: `liquers-unittest`, `rust-best-practices` · knowledge: Phase 3 unit-test
section, Step 1's signature, `liquers-core/src/commands.rs:614-660` for the existing test style.

---

### Step 3 — The `multiple` flag in `register_command!` (liquers-macro)

**File:** `liquers-macro/src/registration.rs`. Six edits, all in one crate, no dependency change.

**3a. Field.** `CommandParameter::Param` (enum at `:444`) gains `multiple: bool`, threaded through the
struct literal at `:1623`.

**3b. Flag parsing.** Replace `:1564-1569`:

```rust
let injected = if input.peek(syn::Ident) {
    let flag: syn::Ident = input.parse()?;
    flag == "injected"          // any other identifier silently discarded
} else { false };
```

with a loop that sets `injected` / `multiple`, rejects an unknown identifier, rejects a duplicate,
and rejects the combination. Messages exactly as in Phase 2's table.

This edit is the prerequisite for everything else in the step: until an unknown flag is rejected,
`multipel` is silently discarded and a typo becomes a silent behaviour change.

**3c. Container check.** New private helper beside `is_option_of` (`:582`):

```rust
/// Element type of a container a `multiple` argument may be declared as.
/// Recognises `Vec<T>`; this is the single place a future container is added.
fn variadic_element_type(ty: &syn::Type) -> Option<&syn::Type>
```

If `multiple` is set and this returns `None`, error on the **type's** span, rendering the declared
type via `quote!(#ty).to_string()` and suggesting the `Vec<…>` form.

**3d. No default.** After `default_value` is parsed (`:1570-1575`): if `multiple` and
`default_value.is_some()`, error. The implicit default is the empty list, which `from_arginfo`
(`plan.rs:480`) already produces for `CommandParameterValue::None`.

**3e. Emission.** Three sites:

| Site | Change |
|---|---|
| `parameter_extractor` (`:459`) | New **first** branch: `multiple` → `arguments.get_multiple(#i, #name_str)?`. Before the `injected` and `is_value_or_any` branches |
| `argument_type_expression` (`:567`) | When `multiple`, unwrap through `variadic_element_type` before the existing `is_option_of` inference |
| `argument_info_expression` (`:717`) | `multiple: false` → `multiple: #multiple` |

**3f. Ordering rule.** In `impl Parse for CommandSignature` (`:1640`), after the parameter-collecting
`while` loop (`:1650-1661`): if any `Param { multiple: true, .. }` is followed by a
`Param { injected: false, .. }`, error on the **later** parameter's span.

`CommandParameter::Context` is exempt, and so is any `injected` parameter — neither consumes a query
parameter, so neither is starved. This is the compile-time closure of
`VARIADIC-ARGUMENT-STARVES-LATER-ARGUMENTS` for macro-registered commands.

**Two expected-token test strings** at `:2336` and `:2406` contain `multiple: false` and keep
passing unchanged — they assert the non-variadic path still emits `false`, which is exactly the
regression guard wanted.

**Validation:**
```bash
cargo test -p liquers-macro
cargo build -p liquers-core --tests   # every existing register_command! still expands
```

**Agent:** Sonnet · skills: `rust-best-practices` · knowledge: this step, Phase 2 "liquers-macro —
modified functions" and the rejection table, `registration.rs:449-470`, `:459-500`, `:567-665`,
`:666-725`, `:1539-1637`, `:1640-1665`.

---

### Step 4 — Macro unit tests (liquers-macro)

**File:** `liquers-macro/src/registration.rs`, existing `mod tests` (`:1819`).

U7-U17 from Phase 3. `quote` and `syn` are already imported (`:1822-1823`); the rejection tests use
`syn::parse2` rather than `parse_quote!`, because the latter panics on error.

U10 (`scalar_parameter_expansion_is_unchanged`) is the regression guard for the claim that every
existing declaration expands identically. U17 pins the injected/context exemption.

**Validation:** `cargo test -p liquers-macro`

**Agent:** Haiku · skills: `liquers-unittest` · knowledge: Phase 3 macro-test section, Step 3,
`registration.rs:2290-2360` for the existing assertion style and the `fuzzy` helper.

---

### Step 5 — Convert the two polars commands (liquers-lib)

**File:** `liquers-lib/src/polars/selection.rs`.

**5a.** `select_columns` (`:14`) and `drop_columns` (`:36`): signature `columns: String` →
`columns: Vec<String>`; **delete** `columns.split('-').map(|s| s.trim())`; add the empty-list
rejection; keep the `check_column_exists` loop.

Deleting `.trim()` is deliberate and is not an oversight to be re-added: it existed only to clean up
after splitting, and with one parameter per column it would silently alter a column name containing
significant whitespace.

**5b.** Doc comments on both functions: describe one parameter per column, and note that `a~_b`
names the single column `a-b`.

**5c.** Registrations (`:105`, `:114`): `columns: String` → `columns: Vec<String> multiple`, and
`doc:` from "(separated by dashes)" to "one parameter per column".

**Validation:**
```bash
cargo build -p liquers-lib --features polars
cargo test -p liquers-lib --lib --tests
```

**Agent:** Sonnet · skills: `rust-best-practices` · knowledge: this step, Phase 3 Example 1,
`liquers-lib/src/polars/selection.rs:1-50` and `:95-125`, `liquers-lib/src/polars/util.rs`.

---

### Step 6 — Integration tests and registry regeneration

**6a. Replace the two bypassing tests.** `liquers-lib/tests/polars_commands.rs`: add the
`eval_over_csv` helper from Phase 3 and replace `test_select_columns` (`:105`) and
`test_drop_columns` (`:122`) with I1-I5.

The helper's store setup is **verified**, closing the last unverified API in the plan:
`AsyncStore::set(&self, key: &Key, data: &[u8], metadata: &Metadata)`
(`liquers-core/src/store.rs:740`), called exactly as the existing suites call it —
`.set(&key, text.as_bytes(), &Metadata::new())`
(`liquers-core/tests/expiration_integration.rs:318`). Note `set` calls `key.as_absolute()?`, so the
key must be absolute; copy the surrounding pattern from that file rather than inventing one.

The other eleven tests in the file stay as they are; they are
`POLARS-COMMAND-TESTS-BYPASS-COMMANDS`, filed and out of scope.

**6b. Registry round-trip.** Add I6 to `liquers-lib/tests/registry_export.rs`.

**6c. Regenerate:**
```bash
cargo run -p liquers-lib --features cli --bin export-command-registry -- \
  --format yaml -o specs/command_registry.yaml
```
then add a dated line inside the `# CHANGELOG-BEGIN` / `# CHANGELOG-END` markers — the only
hand-maintained part of that generated file.

**Expect the diff to include changed `impl_version` values** for both commands.
`#[command_version]` blake3-hashes each function's whole token stream
(`liquers-macro/src/versioning.rs:15-21`), so a signature change necessarily changes it. That is
correct, not a mistake.

**6d. Corner cases** C1 (trailing dash) and C2 (variadic in an aliased command). C3 (recipe
override) only if the existing recipe suite does not already cover override flow — check first.

**Validation:**
```bash
cargo test -p liquers-lib --lib --tests
cargo test -p liquers-lib --test registry_export
```

**Agent:** Sonnet · skills: `liquers-unittest`, `liquers-validate` · knowledge: Phase 3 integration
section, `liquers-lib/tests/polars_commands.rs`, `liquers-lib/tests/registry_export.rs:60-140`,
`liquers-core/tests/expiration_integration.rs:143` and `:318` for the store setup and `set` call patterns.

---

### Step 7 — Documentation

Per the Phase 2 documentation architecture. Each reference and guide edit needs a `## History` row
and a `reviewed:` bump **in the same commit** (`DOCS_STRUCTURE_GUIDE.md` §9.2).

| File | Change |
|---|---|
| `specs/reference/REGISTER_COMMAND_FSD.md` | Grammar `:102` → `[injected \| multiple]`; attribute table `:109` gains a row; `:388` stops showing `multiple: false` as invariant; new subsection: the six rejections, the ordering rule, element-type inference, no defaults |
| `specs/reference/POLARS_COMMAND_LIBRARY.md` | Revert `~_` to plain dashes at `:60`, `:83`, `:84`, `:86`, `:263`, `:275`, `:468`, `:711`, `:793`; rewrite the arity note `:90-94` as the dash-escaping note; update `:394` |
| `specs/guides/COMMAND_REGISTRATION_GUIDE.md` | Rewrite §"Accepting a variable number of parameters" (`:156-175`) from "cannot be declared" to the how-to, using the Phase 3 guide-candidate table |
| `CLAUDE.md` | DSL Syntax Reference: `multiple` beside `injected`; note the no-default rule |
| `specs/README.md` | List this design folder |

**Every query written into any of these must be re-checked with `liquers-validate` against the
regenerated registry** — by then no overlay is needed, since the commands really are variadic.

**Validation:**
```bash
cargo run -p liquers-core --features cli --bin liquers-validate -- --detail summary -- \
  'ns-pl/select_columns-date-amount-status' 'ns-pl/select_columns-a~_b' 'ns-pl/drop_columns-b'
python3 scripts/docs_index.py
```

**Agent:** Sonnet · skills: `liquers-validate` · knowledge: Phase 2 "Documentation Architecture",
Phase 3 "Documentation and Learning Log", the four target documents.

---

## Testing Plan

| When | Command | Must pass |
|---|---|---|
| After Step 1 | `cargo test -p liquers-core --lib` | Existing suite unchanged |
| After Step 2 | `cargo test -p liquers-core --lib commands::tests` | U1-U6 |
| After Step 3 | `cargo test -p liquers-macro` and `cargo build -p liquers-core --tests` | Existing macro tests; every existing `register_command!` still expands |
| After Step 4 | `cargo test -p liquers-macro` | U7-U17 |
| After Step 5 | `cargo build -p liquers-lib --features polars` | Compiles; the two commands are the first real `get_multiple` callers |
| After Step 6 | `cargo test -p liquers-lib --lib --tests` | I1-I7, C1-C2 |
| After Step 7 | `liquers-validate` on every documented query | All `Ok` |
| Final | `cargo test -p liquers-lib --lib --tests` (the CLAUDE.md default loop) | Everything |

**Not run:** the browser and `liquers-web` loops. Nothing in this design touches wasm, the web
bindings or the UI — `gui_info` has no consumer, and `liquers-web` is excluded from
`default-members`. Running them would cost a `cargo clean` cycle for no coverage.

**Disk:** the default loop is the ~4.2 GB / ~3 min configuration recorded in `CLAUDE.md`. If a
build hits "No space left on device", `cargo clean` and continue; deletes still succeed while
writes fail.

## Agent Assignment

| Step | Model | Skills | Why |
|---|---|---|---|
| 1 | Sonnet | `rust-best-practices` | Exhaustive matching over 11 enum variants with correct position propagation; the one step where a wrong bound or a stray `_ =>` undoes the design |
| 2 | Haiku | `liquers-unittest`, `rust-best-practices` | Tests fully specified in Phase 3; mechanical transcription against a fixed signature |
| 3 | Sonnet | `rust-best-practices` | Six coordinated edits in one file, `syn` parsing, span selection for messages. The highest-risk step |
| 4 | Haiku | `liquers-unittest` | Fully specified; the assertion style already exists in the file |
| 5 | Sonnet | `rust-best-practices` | Small but semantic: deleting a workaround, and the empty-list decision |
| 6 | Sonnet | `liquers-unittest`, `liquers-validate` | Needs a new test helper and one unverified API (`AsyncStore::set`) |
| 7 | Sonnet | `liquers-validate` | Judgement about what the documents should say, not transcription |

Steps 1-2, 3-4, and 5-6 pair naturally; 3 must not start before 1 lands, since Step 3's generated
code calls Step 1's method.

## Rollback Plan

Each step is independently revertible, which is a consequence of the ordering rather than a
separate effort.

| Step | Revert | Blast radius |
|---|---|---|
| 1 | Delete the method | None — nothing calls it until Step 3 |
| 2 | Delete the tests | None |
| 3 | Revert `registration.rs` | Any variadic declaration stops compiling; no existing declaration is affected, since the non-variadic path is byte-identical |
| 4 | Delete the tests | None |
| 5 | Revert `selection.rs` | The two commands return to `String` + `split('-')`; queries revert to needing `a~_b` |
| 6 | Revert the tests; re-run the exporter | `registry_export` fails until the registry matches the code — revert 5 and 6 together |
| 7 | Revert the documents | Documents claim the feature is undeclarable |

**The one coupling to respect:** Steps 5 and 6c move together. `specs/command_registry.yaml` is
generated from the registered commands and `cargo test -p liquers-lib --test registry_export`
enforces agreement, so reverting the commands without regenerating (or the reverse) fails that test.

**No migration concern.** No serialized format changes meaning: `multiple` and `argument_type`
already existed and already serialized. An older registry file reads unchanged; a newer one adds
two fields on two arguments.

## Review Findings Applied

Four conformity passes (Phase 1, 2, 3, codebase) plus a final cross-document pass, run sequentially
rather than as parallel agents. No conformity drift was found: every step traces to a Phase 2
integration point, and every test named traces to Phase 3. The codebase pass corrected six line
anchors and closed one open API:

| Finding | Resolution |
|---|---|
| `get` / `get_injected` boundary misstated | `get` at `:102`, `get_injected` at `:128` |
| `mod tests` in `commands.rs` given as `:620` | Two test modules: `:614` and `:724`; the `SimpleEnvironment<Value>` alias is at `:733` in the second |
| `test_drop_columns` given as `:124` | `:122` |
| `CommandParameter::Param` given as `:449` | Enum at `:444` |
| `AsyncStore::set` left unverified | Verified: `set(&self, key: &Key, data: &[u8], metadata: &Metadata)` (`store.rs:740`), pattern at `expiration_integration.rs:318`. Note it requires an **absolute** key |
| `Metadata::new()` used by the helper | Confirmed (`metadata.rs:1495`) |

The plan now contains no API it has not checked against the source.

## Phase 5 Entry Criteria

Phase 5 begins when all of these hold:

1. Steps 1-7 complete; `cargo test -p liquers-lib --lib --tests` and `cargo test -p liquers-macro`
   green.
2. `cargo test -p liquers-lib --test registry_export` green — the committed registry matches the
   code.
3. Every query in the changed documents re-validated with `liquers-validate` **without** the
   proposal overlay.
4. All review comments on the PR resolved.
5. `specs/design/variadic-arguments-declaration/variadic-proposal.registry.yaml` re-examined: it
   was Phase 3's measurement instrument, and once the real registry carries the signatures it is
   either deleted or kept with a note saying it is superseded by the real thing.

Phase 5 must then, at minimum:

- Close `COMMAND-VARIADIC-ARGUMENTS-NOT-DECLARABLE` (`status: closed`, resolution note).
- **Narrow, not close, `VARIADIC-ARGUMENT-STARVES-LATER-ARGUMENTS`**: record that macro-registered
  commands are guarded at compile time and that hand-built metadata — `liquers-py`'s compiled
  `add_python_command` (`command_metadata.rs:430`) is the live example — remains unguarded.
- Confirm the four issues filed during this design are accurate at HEAD:
  `PY-MODULES-NOT-DECLARED-IN-LIB`, `POLARS-COMMAND-TESTS-BYPASS-COMMANDS`,
  `UI-VARIADIC-ARGUMENT-LIST-EDITOR`, `COMMAND-COMPOSITE-VARIADIC-ARGUMENTS`.
- Carry forward the four learning points recorded in Phase 3.
