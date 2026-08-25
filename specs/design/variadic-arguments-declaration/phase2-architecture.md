# Phase 2: Solution & Architecture - Declarable variadic command arguments

## Overview

Three changes, one per crate, in dependency order. `liquers-core` gains one method —
`CommandArguments::get_multiple` — whose bounds avoid the coherence wall that blocks `Vec<T>`.
`liquers-macro` gains a `multiple` argument flag, infers `ArgumentType` from the `Vec` element
type, emits the new accessor, and rejects three malformed declarations at compile time.
`liquers-lib` converts `pl/select_columns` and `pl/drop_columns` to the real mechanism and deletes
their `split('-')` workaround.

No trait is added, no existing trait or signature changes, no enum gains a variant. The runtime
half of the feature (`pop_value`, `from_arginfo`, `materialize_nested_parameter`) is already
complete and is not touched.

## Known-Issue Preflight

Searched: issues linked from Phase 1; `specs/index.csv` filtered to locally open (`draft`,
`accepted`, `in_progress`) records in areas `macro`, `core/commands`, `core/plan`, `lib/polars`,
`lib/commands`, `py`; plus the two in-flight designs touching the macro DSL.

| Issue | Status | Priority | Relevance and solution impact | First? | Blocking? | Required action | Priority action |
|---|---|---|---|---|---|---|---|
| `COMMAND-VARIADIC-ARGUMENTS-NOT-DECLARABLE` | draft | P1 | The subject. Its stated fix direction is adopted with one change: the accessor is generic over `FromParameterValue` only, as it proposed, but the ordering guard moves to the macro | n/a | no | Closed by this design at Phase 5 | Keep P1 |
| `VARIADIC-ARGUMENT-STARVES-LATER-ARGUMENTS` | draft | P2 | Becomes reachable the moment `multiple` is declarable. Closed for macro-registered commands by the compile-time guard (D1); the `CommandMetadata::check()` route stays open for hand-built metadata | no | no | Narrow, do not close. Update its text at Phase 5 to say the macro path is guarded | Keep P2 — no longer reachable from the macro, so the residue is smaller than when filed |
| `COMMAND-REGISTRY-ISSUE-NAMESPACE-NAME-SWAPPED` | draft | P3 | Would corrupt any message emitted through `CommandRegistryIssue`. **Avoided entirely**: the macro guard emits a `syn::Error`, not a registry issue | no | no | None. Note the avoidance in the guide | Keep P3 |
| `COMMAND-CONTEXT-PARAM-ORDER` | accepted | P2 | Direct interaction: `context` must already be last, and now `multiple` must be last too. The two rules must be stated as one (see "Argument ordering rule") | no | no | State the combined rule; the macro guard implements it | Keep P2 |
| `REGISTER-COMMAND-ENUM` (design, `architecture`) | draft | — | Edits the same function this design edits — `impl Parse for CommandParameter` (`registration.rs:1539`). Textual conflict risk, no semantic conflict: enum specs are parsed from the parenthesised suffix, flags from the bare-identifier position | no | no | Monitor. Whichever lands second rebases | n/a |
| `COMMAND-METADATA-ENHANCEMENTS` | accepted | P2 | Enum arguments already work per element (`pop_value` calls `from_string` per parameter, which dispatches on `ArgumentType::Enum`). A variadic enum argument therefore works without extra effort — untested, so Phase 3 pins it | no | no | Add a test; no architecture change | Keep P2 |
| `MACRO-QUERY-VALIDATION-AND-HINTS` | accepted | P3 | Independent — concerns `query` defaults and hints, neither of which a variadic argument may carry (D4) | no | no | None | Keep P3 |
| `PY-MODULES-NOT-DECLARED-IN-LIB` | draft | P2 | **Filed by this preflight.** Changes what Phase 1 D1 claimed: see below | no | no | Filed; not fixed here | Set P2 |
| `ACTION-PARAMETER-SET-VALUE-DOUBLE-ENCODES` | draft | — | Concerns writing parameters back into a query — the operation a variadic list editor needs. Belongs with `UI-VARIADIC-ARGUMENT-LIST-EDITOR`, not here | no | no | Cross-link from the new UI issue | Keep |

**No blockers.** Nothing above must be resolved before this design proceeds.

### Correction to Phase 1, decision 1

Phase 1 said the registry-level check "still covers hand-built metadata such as `liquers-py`'s
`argv` (`liquers-py/src/commands.rs:220`)". That file **is not compiled** —
`liquers-py/src/lib.rs` never declares `mod commands`, and seven other files are orphaned the same
way. Filed as `PY-MODULES-NOT-DECLARED-IN-LIB`. The proof is mechanical: `commands.rs:162` reads
`arg.parameters.0`, a `pub(crate)` field of another crate, which cannot compile.

The conclusion changes but does not reverse, because a *different* liquers-py file is compiled and
does set the flag: `CommandMetadataRegistry::add_python_command`
(`liquers-py/src/command_metadata.rs:430`) applies `multiple = true` to `cmd.arguments.last_mut()`.
So a live hand-built producer exists, it is reachable from Python, and — being explicitly
last-only by construction — it already satisfies the ordering rule this design enforces. That is
why `VARIADIC-ARGUMENT-STARVES-LATER-ARGUMENTS` is narrowed rather than closed.

## Data Structures

### New structs, enums, ExtValue variants

**None.** This is the architectural centre of the design and is worth stating explicitly:
`ParameterValue::MultipleParameters(Vec<ParameterValue>)` already exists (`plan.rs:381`),
`ArgumentInfo.multiple` already exists and already serializes (`command_metadata.rs:385`), and
`ResolvedParameterValues` already stores a variadic argument in one slot. Nothing new is
representable that was not representable before; what changes is who can *produce* and *consume* it.

### Modified struct: `CommandParameter::Param` (liquers-macro, private)

```rust
enum CommandParameter {
    Param {
        name: syn::Ident,
        ty: syn::Type,
        injected: bool,
        multiple: bool,          // NEW
        default_value: Option<DefaultValue>,
        label: Option<String>,
        gui: ArgumentGUIInfo,
        gui_explicit: bool,
        enum_spec: Option<EnumParameterSpec>,
    },
    Context,
}
```

**Ownership:** unchanged — all fields owned, the struct is built once during parse and consumed by
`quote!`. `multiple` is a plain `bool` for the same reason `injected` is: there are exactly two
states and no third is foreseen (unlike `PayloadRequirement`, which reserved one).

**Serialization:** none; this type never leaves the proc-macro.

**No default match arm:** `CommandParameter` is matched exhaustively in `parameter_extractor`,
`argument_info_expression` and `argument_type_expression`. Those matches currently end in `_ =>`
covering only `Context`; this design does not add a variant, so they are left as they are rather
than widening the diff. *(Advisory finding, recorded rather than fixed — see "Rust review".)*

## Trait Implementations

**None added, none changed.** This is the decisive architectural choice, so the reasoning is
recorded rather than assumed.

The obvious route — `impl<T: FromParameterValue<T>> FromParameterValue<Vec<T>> for Vec<T>` — is
rejected by coherence: it overlaps the existing
`impl<V: ValueInterface> FromParameterValue<Vec<V>> for Vec<V>` (`commands.rs:269`), because
nothing prevents a future type from implementing both `ValueInterface` and
`FromParameterValue<Self>`. The compiler cannot know the sets are disjoint, and there is no
negative bound to tell it.

Specialization would resolve it and is unstable. Removing the `ValueInterface` impl would break
`Vec<Value>` retrieval for hand-built registrations. So the design **adds a method, not an impl** —
which is exactly why it needs no coherence argument at all.

## Function Signatures

### liquers-core — `CommandArguments::get_multiple`

```rust
impl<E: Environment> CommandArguments<E> {
    /// Returns the elements of a variadic argument, converted to `T`.
    ///
    /// Only for an argument declared `multiple`. The parameter in slot `i` must be
    /// `ParameterValue::MultipleParameters`; anything else is a declaration/retrieval mismatch
    /// and is reported as such.
    pub fn get_multiple<T: FromParameterValue<T>>(
        &self,
        i: usize,
        name: &str,
    ) -> Result<Vec<T>, Error>;
}
```

**Bound justification.** `T: FromParameterValue<T>` — required, because each element is converted
individually. That is the *whole* bound.

`get` additionally requires `T: TryFrom<E::Value, Error = Error>`, and that bound is the blockage
the issue identified. It exists solely for `get`'s pre-materialised fast path: when a top-level
link parameter has been resolved, the interpreter stores the resulting `E::Value` in
`self.values[i]` and `get` converts *from the value* rather than from the parameter. **A variadic
argument never takes that path**, because the interpreter populates `values` only for parameters
where `param.link()` is `Some` (`interpreter.rs:470`), and `MultipleParameters::link()` is `None`
(`plan.rs:876`). So `get_multiple` correctly ignores `self.values` entirely, and dropping the bound
is not a relaxation — it is the removal of a requirement that never applied.

**Why a method on `CommandArguments<E>` and not a free function.** It uses no `E` and could be
free. Keeping it a method makes the generated call site uniform with `get` / `get_injected` /
`get_value`, which matters because the macro chooses between them by a single branch.

**Behaviour, by `ParameterValue` variant of slot `i`:**

| Variant | Result |
|---|---|
| `MultipleParameters(elements)` | Convert each element (see below); collect into `Vec<T>`. Empty vector for an empty list — the normal no-arguments case (D4) |
| any other variant | `Error::general_error` naming the argument: the command declared it `multiple` but the plan did not resolve it as such. Reaching this means metadata and retrieval disagree |

**Per-element handling.** Elements are `ParameterValue`s produced by `pop_value`'s variadic branch
(`plan.rs:736-786`), which already refuses to place `MultipleParameters`, `Injected`, `None` or
`Placeholder` inside a variadic argument. So the reachable element variants are the value variants
and the link variants:

| Element variant | Result |
|---|---|
| `DefaultValue`, `ParameterValue`, `OverrideValue` | `T::from_parameter_value(element)`, position attached on failure |
| `DefaultLink`, `ParameterLink`, `OverrideLink`, `EnumLink` | `Error::general_error("Unresolved link parameter …")` with position — the interpreter materialises these first (`interpreter.rs:344`), so this fires only for a `CommandArguments` built without materialisation |
| `MultipleParameters`, `Injected`, `Placeholder`, `None` | `Error::unexpected_error` — unreachable via `pop_value`, but the match is exhaustive per the no-default-arm rule |

The link row is not defensive noise. `CommandArguments::new` is public and is called from four
places (`commands.rs:640`, `:672`, `:708`, `interpreter.rs:467`); only the interpreter materialises
first. A test constructing arguments directly will hit this, and the message should say what is
wrong rather than mis-converting.

**Errors:** `Error::general_error` and `Error::unexpected_error`, both typed constructors, both
with `.with_position(...)` where the element carries one. No `Error::new`, no new `ErrorType`.

### liquers-macro — modified functions

All in `liquers-macro/src/registration.rs`.

```rust
impl Parse for CommandParameter { fn parse(input: ParseStream) -> syn::Result<Self>; }
```
Replaces the single-flag probe at `:1564`

```rust
let injected = if input.peek(syn::Ident) {
    let flag: syn::Ident = input.parse()?;
    flag == "injected"          // any other identifier is silently discarded
} else { false };
```

with a loop accepting `injected` and `multiple` in either order, rejecting anything else with a
`syn::Error` on the offending identifier's span, and rejecting a repeat of either flag.

Peeking a bare `Ident` here is unambiguous, and the reason is worth recording because the enclosing
macro *does* use bare identifiers for something else. `CommandParameter::parse` reads from
`content` — the `syn::parenthesized!` stream holding the parameter list (`CommandSignature::parse`,
`:1645`) — where after a type the grammar admits only `,`, `)`, `=`, `(`, or a flag. The
command-level statements (`label:`, `doc:`, `namespace:`, `version:`) are bare identifiers too, but
they are parsed from `input`, outside those parentheses (`:1665`). The two identifier positions
cannot collide.

```rust
impl CommandParameter {
    fn parameter_extractor(&self, i: usize) -> proc_macro2::TokenStream;   // :459
    fn argument_type_expression(&self) -> proc_macro2::TokenStream;        // :567
    fn argument_info_expression(&self) -> Option<proc_macro2::TokenStream>;// :666
}
```

| Function | Change |
|---|---|
| `parameter_extractor` | New first branch: `multiple` emits `let #var: #ty = arguments.get_multiple(#i, #name_str)?;`. Placed before the `is_value_or_any` test, which would otherwise never fire for a `Vec` anyway (`Vec<Value>` has no bare ident) |
| `argument_type_expression` | When `multiple`, unwrap `Vec<T>` to `T` **before** the existing inference. `Vec<String>` → `String`, `Vec<i32>` → `Integer`, `Vec<Option<i32>>` → `IntegerOption` |
| `argument_info_expression` | `multiple: false` (`:717`) becomes `multiple: #multiple` |

New private helper:

```rust
/// Returns the element type of `Vec<T>`, or `None` for any other type.
fn vec_element_type(ty: &syn::Type) -> Option<&syn::Type>;
```

Matches `syn::Type::Path` with a single segment `Vec` and one angle-bracketed type argument —
the same shape the existing `is_option_of` (`:583`) uses for `Option`.

**Why the element type matters beyond metadata.** `pop_value`'s variadic branch converts each
action parameter through `ParameterValue::from_string(arginfo, s, pos)` (`plan.rs:741`), which
dispatches on `arginfo.argument_type`. Leaving it `Any` — which is where `Vec<String>` currently
falls (`:658`) — makes every element a `Value::String`, so `Vec<i32>` would parse as strings and
fail at `from_parameter_value`. Inferring the element type is what makes non-string variadic
arguments work at all.

### Compile-time rejections (new)

Four malformed declarations, each a `syn::Error` at the right span:

| Declaration | Message |
|---|---|
| `a: i32 foobar` | ``Unknown argument flag `foobar`; expected `injected` or `multiple` `` |
| `a: String multiple` | ```multiple` requires a `Vec<T>` parameter type `` |
| `a: Vec<String> injected multiple` | ```injected` and `multiple` cannot be combined`` |
| `a: Vec<String> multiple = "x"` | ``a `multiple` argument cannot have a default value; it defaults to the empty list`` |

The first is the pre-existing silent-typo hazard the issue named, and fixing it is a prerequisite
for the rest: without it, `multiple` misspelled is `injected`-shaped nonsense that compiles.

The third is required because an injected argument is represented by `ParameterValue::Injected`,
never `MultipleParameters` (`from_arginfo`, `plan.rs:480` — the `multiple` branch is taken first and
never produces `Injected`), so the combination has no coherent runtime meaning.

### Argument ordering rule

Stated once, covering both this design and `COMMAND-CONTEXT-PARAM-ORDER`:

> A `multiple` argument must be the last argument that consumes a query parameter. Arguments
> marked `injected`, and the `context` parameter, may follow it.

Enforced in `impl Parse for CommandSignature` after the parameter list is collected: if any
`Param { multiple: true, .. }` is followed by a `Param { injected: false, .. }`, error on the
**later** parameter's span — that is the starved argument, and it is where the author must look.
`CommandParameter::Context` never triggers it.

Rationale for the exemption: injected arguments consume no query parameter, so nothing starves
them. `VARIADIC-ARGUMENT-STARVES-LATER-ARGUMENTS` states this requirement explicitly.

### liquers-lib — converted commands

`liquers-lib/src/polars/selection.rs`:

```rust
pub fn select_columns(state: &State<Value>, columns: Vec<String>) -> Result<Value, Error>;
pub fn drop_columns(state: &State<Value>, columns: Vec<String>) -> Result<Value, Error>;
```

Registration (`:105`, `:114`):

```rust
register_command!($cr,
    fn select_columns(state, columns: Vec<String> multiple) -> result
    namespace: "pl"
    label: "Select columns"
    doc: "Select columns by name, one parameter per column"
    version: auto
)?;
```

Three behavioural changes inside the functions, each deliberate:

1. **`columns.split('-')` is deleted.** Keeping it would make `select_columns-a~_b` (one escaped
   parameter) and `select_columns-a-b` (two parameters) both work by two different mechanisms, and
   would keep silently splitting a column whose name really contains a dash. The issue requires the
   deletion.
2. **`.trim()` is dropped with it.** It exists to clean up `"a - b"` after splitting. With one
   parameter per column there is nothing to clean, and trimming would silently alter a column name
   with significant whitespace.
3. **An empty list is rejected**, with `Error::general_error` naming the command. `from_arginfo`
   makes `select_columns` with no parameters well-formed at plan level (D4), and neither
   `df.select([])` nor `drop_many([])` is a meaningful request. The error is raised by the command,
   not the plan builder, because the plan builder cannot know a given variadic argument requires at
   least one element.

**Ownership:** `columns: Vec<String>` is owned, matching what `get_multiple` returns; the existing
`check_column_exists(&df, col)` loop borrows from it unchanged.

**Expected side effect on the registry diff.** Both functions carry
`#[liquers_macro::command_version]`, which hashes the function's entire token stream
(`liquers-macro/src/versioning.rs:15-21`, blake3 over `item.to_string()`) and is registered as
`version: auto`. Changing the signature and body therefore changes each command's `impl_version`.
That is correct — the command's behaviour genuinely changed — but it means the regenerated
`specs/command_registry.yaml` will differ in more than `multiple: true` and `argument_type: string`,
and the implementer should not read the version churn as a mistake.

## Sync vs Async Decisions

| Function | Async? | Rationale |
|---|---|---|
| `CommandArguments::get_multiple` | No | Pure in-memory conversion over already-resolved parameters. `get` and `get_injected` are sync for the same reason. Every link inside a variadic argument is resolved *before* `CommandArguments` is constructed (`interpreter.rs:462-466`), which is precisely what keeps this accessor sync |
| `select_columns`, `drop_columns` | No | Unchanged; CPU-bound DataFrame operations with no I/O |
| Macro-generated code | Unchanged | `get_multiple` is emitted identically in the sync and async wrappers; the async wrapper's `async move` block already contains the extractor statements |

No new async surface, no blocking I/O introduced.

## Integration Points

| Crate | File | Change |
|---|---|---|
| liquers-core | `src/commands.rs` | Add `get_multiple` beside `get` / `get_injected` (~`:102-130`) |
| liquers-macro | `src/registration.rs` | `multiple` field; flag loop (`:1564`); `vec_element_type`; three emission sites (`:459`, `:567`, `:717`); ordering check in `CommandSignature::parse`; update two expected-token test strings (`:2336`, `:2406`) |
| liquers-lib | `src/polars/selection.rs` | Two signatures, two bodies, two registrations, two doc comments |
| specs | `command_registry.yaml` | Regenerate |

**Dependencies:** none added, in any crate.

**Backward compatibility.** Every existing `register_command!` invocation parses and expands
identically: the flag loop accepts zero flags, `multiple` defaults to `false`, and
`argument_info_expression` emits `multiple: false` as before. The only source-breaking change is
intentional — a previously-swallowed unknown flag now fails the build. A workspace grep finds no
such usage.

## Documentation Architecture

### Reference Plan

**No new reference.** This closes a gap in a documented mechanism.

| Path | Change |
|---|---|
| `specs/reference/REGISTER_COMMAND_FSD.md` (`reviewed:` is `overdue`) | Argument grammar (`:102`) gains `[multiple]`; attribute table (`:109`) gains a `multiple` row; generated-`ArgumentInfo` example (`:388`) shows `multiple` as variable; new subsection stating the four compile-time rejections, the ordering rule, the element-type inference, and that a variadic argument takes no default. Audience: internal. Area: `macro`, `core/commands` |
| `specs/reference/POLARS_COMMAND_LIBRARY.md` | Revert the `~_` spelling introduced by `design/excess-action-parameters-error/` at `:60`, `:83`, `:84`, `:86`, `:263`, `:275`, `:468`, `:711`, `:793` to the plain `select_columns-date-amount-status` form. The arity-error note (`:90-94`) is replaced: `a-b` is now two columns, and `a~_b` is how you name one column that contains a dash. Update the worked example at `:394`. Area: `lib/polars` |

Both need a `## History` row and a `reviewed:` bump in the same commit (§9.2).

### Guide Plan

**No new guide.**

`specs/guides/COMMAND_REGISTRATION_GUIDE.md` §"Accepting a variable number of parameters"
(`:156-175`) currently states the feature cannot be declared and prescribes the `~_` workaround. It
is rewritten as the how-to: the declaration form, the ordering rule (merged with the existing
context-last rule), the no-default rule, the element-type inference, and the two spellings — the
worked `select_columns-a-b` (two columns) against `select_columns-a~_b` (one column named `a-b`),
which is now a genuine distinction rather than a workaround. Audience: internal. Area:
`core/commands`, `macro`. `## History` row and `reviewed:` bump required.

### Other Documents to Create

**None.** Two issues to file at this phase (Phase 1 D5), which are records, not documents:

| Issue | Covers |
|---|---|
| `UI-VARIADIC-ARGUMENT-LIST-EDITOR` | Argument→parameter-range mapping and insert/remove/move on an action's parameters; cites `orest-d/egui-midi-test` `src/editor.rs` and cross-links `ACTION-PARAMETER-SET-VALUE-DOUBLE-ENCODES` |
| `COMMAND-COMPOSITE-VARIADIC-ARGUMENTS` | Tuple / key-value element types and the GUI they need |

`PY-MODULES-NOT-DECLARED-IN-LIB` is already filed.

### Existing Documents to Review or Update

| Path | In `affects_docs`? | Change |
|---|---|---|
| `specs/reference/REGISTER_COMMAND_FSD.md` | yes | As above |
| `specs/reference/POLARS_COMMAND_LIBRARY.md` | yes | As above |
| `specs/guides/COMMAND_REGISTRATION_GUIDE.md` | yes | As above |
| `CLAUDE.md` | yes | DSL Syntax Reference: `multiple` beside `injected`; note that a variadic argument takes no default |
| `specs/command_registry.yaml` | no (generated) | Regenerate; add a dated CHANGELOG line |
| `specs/README.md` | no | List this design folder |
| `specs/issues/COMMAND-VARIADIC-ARGUMENTS-NOT-DECLARABLE.md` | no | `design:` now; `closed` at Phase 5 |
| `specs/issues/VARIADIC-ARGUMENT-STARVES-LATER-ARGUMENTS.md` | no | `design:` now; narrowed at Phase 5, not closed |

**Candidates considered and discarded:** `specs/reference/PROJECT_OVERVIEW.md` — describes query
and asset concepts, not argument declaration, and gains nothing from a macro flag.
`specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` (area `py`, `core/commands`) — the liquers-py finding
belongs to `PY-MODULES-NOT-DECLARED-IN-LIB`, not to this design's documentation.
`specs/reference/api/DOC_08_RECIPES_PLANS.md` — recipe overrides of a variadic argument are
untouched (`OverrideValue` already flows through `MultipleParameters`).

### Design and Capability Links

`specs/README.md` gains the design folder on creation and, at Phase 5, points readers at the two
extended documents rather than at this folder. No capability entry is warranted — this restores a
documented capability rather than adding one.

### Evidence to Collect During Implementation

- Whether `syn`'s `Type` parse leaves the flag identifier cleanly peekable in every real
  declaration form (the parenthesised-suffix and `= default` combinations).
- The exact compiler message a user sees for `Vec<i32>` when `ArgumentType` inference is wrong —
  it motivates the element-type change and belongs in the guide.
- Whether the `~_` escape actually round-trips a column name containing a dash through a variadic
  argument, which is the new capability and the one worth a worked example.
- Any further orphaned-file symptom in liquers-py encountered while regenerating the registry.

## Relevant Commands

### New Commands

**None.** No command is added by this design.

### Modified Commands

| Command | Namespace | Before | After |
|---|---|---|---|
| `select_columns` | `pl` | `columns: String`, split on `-` internally | `columns: Vec<String> multiple` |
| `drop_columns` | `pl` | `columns: String`, split on `-` internally | `columns: Vec<String> multiple` |

### Relevant Existing Namespaces

| Namespace | Relevance | Key commands |
|---|---|---|
| `pl` (polars) | Contains both converted commands; the rest of the namespace appears in the doc examples being respelled | `from_csv`, `select_columns`, `drop_columns`, `head`, `gt`, `eq` |
| `root` (core) | The macro change applies to every namespace; `root` commands are the regression surface for "existing declarations still expand identically" | `to_text`, `json`, `yaml` |
| `lui`, `egui` | Registered through the same macro, so they exercise the unchanged path. `egui` also holds the registry inspector that renders the `multiple` badge (`egui/widgets.rs:733`) | — |

**Ask user:** are `pl` and `root` the right namespaces to exercise, or should `lui`/`egui`
declarations be treated as a regression surface worth explicit tests in Phase 3?

## Web Endpoints

**None.** No route, handler or response type changes. The only externally visible difference is
`specs/command_registry.yaml`, which gains `multiple: true` and `argument_type: string` on two
arguments.

## Error Handling

No new error types; no `Error::new`.

| Scenario | Constructor | Where |
|---|---|---|
| Argument declared `multiple` but slot holds another variant | `Error::general_error` | `get_multiple` |
| Element is an unresolved link | `Error::general_error(...).with_position(...)` | `get_multiple` |
| Element is a structurally impossible variant | `Error::unexpected_error` | `get_multiple` |
| Element fails conversion to `T` | `Error::conversion_error_with_message` | existing `from_parameter_value` |
| Empty column list | `Error::general_error` | `select_columns` / `drop_columns` |
| Malformed declaration | `syn::Error` (compile time) | macro |

Position is attached wherever the element carries one, so a failure in `select_columns-a-9-c`
points at `9` rather than at the action.

## Serialization Strategy

No serde annotation changes. `ArgumentInfo.multiple` already carries
`skip_serializing_if = "is_false"` / `default = "false_default"`, and `argument_type` already
carries `skip_serializing_if = "ArgumentType::is_any"`. Setting both on one argument is a
combination that has never occurred in the exported registry — `grep multiple
specs/command_registry.yaml` currently returns nothing — so the round trip is asserted by test in
Phase 3 rather than assumed. The fields are independent, so this is a confirmation, not a risk.

## Rust Review (rust-best-practices)

**BLOCKING** — none.

**ADVISORY**

- `commands.rs` opens with `#![allow(warnings)]` (`:3`). New code should still be warning-clean on
  its own terms; do not let the blanket allow hide an unused import added for `get_multiple`.
- The `_ =>` arms in `parameter_extractor` / `argument_info_expression` / `argument_type_expression`
  cover only `CommandParameter::Context` and predate this design. They violate the no-default-arm
  rule, but tightening them is unrelated churn in a diff that must stay reviewable. Recorded here,
  not fixed.
- `get_multiple` returns `Vec<T>` by value. Correct: the caller (a command function) needs
  ownership, and elements are freshly converted, so there is nothing to borrow from.
- `T: FromParameterValue<T>` is a self-referential bound, which reads oddly but is the trait's
  existing shape (`pub trait FromParameterValue<T>`), used identically by `get`. Matching it is
  right; changing the trait is out of scope.

**QUESTIONS** — none outstanding; all four Phase 1 questions are settled as decisions.

## Open Questions Carried Forward

Both Phase 1 open questions are resolved by inspection and become Phase 3 test obligations rather
than design unknowns:

1. **`Vec<Value>` remains retrievable for hand-built registrations.** `get_multiple` adds a method,
   not an impl, so `impl<V: ValueInterface> FromParameterValue<Vec<V>>` is untouched and still
   reachable via `T::from_parameter_value`. Note the limitation the other way: `get_multiple::<Value>`
   does **not** compile, because no `impl FromParameterValue<Value> for Value` exists — so
   `columns: Vec<Value> multiple` is not declarable through the macro. Phase 3 pins the diagnostic;
   whether to add that impl is deliberately out of scope.
2. **The registry round-trips.** Confirmed by inspection of the serde attributes above; pinned by a
   Phase 3 test rather than left as an assumption.
