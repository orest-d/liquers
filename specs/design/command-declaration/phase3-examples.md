Based on `HEAD`, read rather than remembered. Nothing here is implemented; the test code is written
to be dropped into place at Phase 4.

# Phase 3 — Examples and use-cases

## High-level introduction

Phase 1 states the purpose: an author-facing way to say that a function is a command, which composes
over what the host discovered and produces a `CommandMetadata`. Phase 2 makes that a four-stage
pipeline whose middle three stages are shared. This phase shows what an author actually writes, and
pins the behaviour down with tests.

The examples progress deliberately. **Example 1** is the case that justifies the whole design — an
author adding one label to one argument without restating the rest — and it is the one to read if
only one is read. **Example 2** is the plain-document host, which has no introspection at all and so
exercises the opposite end of the same machinery. **Example 3** collects the ways this goes wrong,
because most of them are silent in a positional system and the diagnostics are the product.

**Example type:** conceptual examples (declaration in, metadata out) with test code written as real
Rust test functions, so Phase 4 drops them in rather than writing them again.

## Overview Table

| ID | Kind | Demonstrates or checks |
|---|---|---|
| **EX1** | Example | Composition: a Python author labels one argument; type and default survive |
| **EX2** | Example | A `commands.yaml` with no introspection — the declaration establishes everything |
| **EX3** | Example | Pitfalls: a misspelled argument, a reordering attempt, a `"None"` default, a lost hint |
| MERGE01 | Unit | An empty declaration is an identity |
| MERGE02 | Unit | `enhance` twice equals `enhance` once (idempotence) |
| MERGE03 | Unit | A declared scalar overrides a discovered one |
| MERGE04 | Unit | A declared field does **not** overwrite a discovered one it omits |
| MERGE05 | Unit | An argument entry augments by **name**, not position |
| MERGE06 | Unit | An entry naming an unknown argument is **rejected** |
| MERGE07 | Unit | …**unless** the baseline has no `arguments` key at all |
| MERGE08 | Unit | `arguments: []` in the baseline still triggers the reject rule |
| MERGE09 | Unit | `null` sets a null value; it does not delete |
| MERGE10 | Unit | Argument order comes from the baseline and cannot be changed |
| MERGE11 | Unit | Composition is associative — layered declarations |
| MERGE12 | Unit | Nested maps merge key-wise; arrays that are not `arguments` replace |
| DEF01 | Unit | Label derivation: `to_text`, `toText`, `toHTML`, `parseHTTPResponse` |
| DEF02 | Unit | Derivation never overwrites a declared or discovered label |
| DEF03 | Unit | `fill_defaults` is idempotent |
| DEF04 | Unit | Argument `gui_info` defaults to `TextField(40)`, not `ArgumentGUIInfo::None` |
| DEF05 | Unit | `cache`, `volatile`, `expires`, `definition`, `payload_required` defaults |
| DEF06 | Unit | Order is normative: derive after merge, never before |
| BUILD01 | Unit | `{"name":"greet"}` builds a complete `CommandMetadata` |
| BUILD02 | Unit | `type` is accepted as a spelling of `argument_type` |
| BUILD03 | Unit | `ArgumentType` aliases: `str`, `text`, `integer`, `number`, `boolean` |
| BUILD04 | Unit | `CommandParameterValue`: all six input shapes |
| BUILD05 | Unit | The `"None"` string trap: bare means no-default, `!Value 'None'` means the string |
| VAL01 | Unit | Empty or missing `name` |
| VAL02 | Unit | `multiple` argument that is not last |
| VAL03 | Unit | A default that does not fit its declared type |
| VAL04 | Unit | An unrecognised argument type names the command and argument |
| VAL05 | Unit | A global-enum reference is **not** resolved here and does not fail here |
| HINT01 | Unit | Registration hints merge like any other map |
| HINT02 | Unit | `build` drops `registration`; `registration()` still returns it |
| HINT03 | Unit | An unknown registration-hint key is carried, not rejected |
| HINT04 | Unit | A **usage** hint on an argument *does* reach the metadata |
| CONV01 | Unit | An argument named `context` leaves `arguments` and lands in `registration` |
| CONV02 | Unit | The **first** argument is always the state, whatever it is called |
| CONV03 | Unit | A *non-leading* `state`/`value`/`text` is an ordinary argument |
| CONV08 | Unit | Each delivery mode is recorded distinctly: `none`, `state`, `value`, `text` |
| CONV09 | Unit | `none`/`na` gives a source command — `first_command` semantics |
| CONV10 | Unit | An explicit `state_argument` is not touched by the convention |
| CONV11 | Unit | An unrecognised first-argument name is `Reserved` and behaves as `value` |
| CONV12 | Unit | A declared `registration.state` wins over the one derived from the name |
| CONV13 | Unit | A leading `context` is removed structurally *before* the delivery rule runs |
| CONV14 | Unit | With no introspection, the delivery rule does not apply |
| WARN01 | Unit | A reserved delivery name warns, and is still treated as `value` |
| WARN02 | Unit | A leading `context` warns that it shifted which argument became the state |
| WARN03 | Unit | No introspection + declared arguments + no `state_argument` warns |
| WARN04 | Unit | A dropped command-level `hints` warns rather than failing |
| WARN05 | Unit | Warnings are de-duplicated; re-running a stage does not multiply them |
| WARN06 | Unit | A warning is never fatal — `build` still succeeds |
| CONV04 | Unit | `conventions: { context: false }` keeps a genuine `context` argument |
| CONV05 | Unit | `conventions: false` disables every convention |
| CONV06 | Unit | A declared entry for a recognised name merges first, then is lifted — not rejected |
| CONV07 | Unit | Conventions are idempotent and consume no query parameter |
| INT01 | Integration | `command_registry.yaml` parses and re-serializes **byte-identically** |
| INT02 | Integration | A declaration and `register_command!` agree, `metadata_version` included |
| INT03 | Integration | `foo_bar` label parity: JavaScript verbatim, document derived |
| INT04 | Integration | The same declaration in YAML and in JSON builds the same metadata |
| INT05 | Integration | A JavaScript declaration object survives `serde_wasm_bindgen` conversion |
| INT06 | Integration | The whole `liquers-web` COMMAND conformance suite, unchanged |

---

## Example 1 — Composition: adding one label (primary)

### Connection to the high-level design

This is the case Phase 1 exists for. Without composition the author must choose: declare *nothing*
and accept discovered metadata, or declare *everything* and restate what discovery already knew.
`liquers-web` has exactly that limitation today (`spec.rs:171-174`).

### Scenario

A Python author has a working function. The signature already carries the argument's name, type and
default. What it cannot carry is a human label or a widget preference.

```python
@command(label="Repeat text", arguments={"count": {"gui_info": {"IntegerSlider": {"min": 1, "max": 9}}}})
def repeat(state, count: int = 2, context=None):
    """Repeat the input text."""
    return state.value * count
```

### Sequence

1. **populate** — the integration inspects the callable and **recognises nothing**: it reports
   `state`, `count` (annotated `int`, default `2`) and `context`, in order. `__doc__` yields the
   documentation, `__name__` the name.
2. **enhance** — the decorator's keyword arguments merge over that baseline. `label` is new. The
   `count` entry is matched **by name** and augments the discovered argument.
3. **apply** — conventions lift `state` out as `state_argument` and drop `context`, each leaving a
   trace in `registration`. Neither will consume a query parameter.
4. **fill** — what is still absent is derived: `count`'s label becomes `Count`; `cache` becomes
   `true`; `expires` becomes `never`. The command's own label was declared, so derivation leaves it.
5. **build** — validation passes; a `CommandMetadata` with **one** argument is produced.

### Core example

```yaml
# stage 1 — baseline from introspection; nothing is recognised, everything is reported
name: repeat
module: python
doc: Repeat the input text.
arguments:
  - { name: state }
  - { name: count, argument_type: int, default: !Value 2 }
  - { name: context }

# stage 2 — the declaration, which is all the author wrote
label: Repeat text
arguments:
  - { name: count, gui_info: !IntegerSlider { min: 1, max: 9 } }

# stage 5 — the result
name: repeat
label: Repeat text                 # declared
module: python
doc: Repeat the input text.        # discovered
state_argument: { name: state, label: state, gui_info: !TextField 40 }
arguments:
  - name: count
    label: Count                   # derived
    argument_type: int             # discovered, survived the merge
    default: !Value 2              # discovered, survived the merge
    gui_info: !IntegerSlider { min: 1, max: 9 }   # declared
cache: true                        # derived
volatile: false
expires: never
definition: Registered
# `registration` stays on the declaration and never reaches the metadata:
#   registration: { state: state, context: 2 }
```

**Two things to take from it.** The lines marked *"survived the merge"* are composition: under the
all-or-nothing rule this design replaces, declaring `arguments` at all would have discarded `int` and
`2`. And `state` and `context` never became command arguments, so neither consumes a query parameter
— that is conventions, and without them `repeat-3` would bind `3` to `state`.

---

## Example 2 — A document host with no introspection

### Scenario

The two-document setup from `COMMAND-DECLARATION-FORMAT`: one document configures the environment,
another declares commands. There is no host language and therefore no callable to inspect, so the
baseline is empty and the declaration must carry everything.

```yaml
# commands.yaml
commands:
  - name: to_upper
    doc: Convert the input text to upper case
    state_argument: { name: state }
    filename: upper.txt

  - name: repeat
    label: Repeat text
    state_argument: { name: state }
    arguments:
      - { name: count, type: int, default: 2 }
    hints:
      python: { state: text }
```

### What this exercises that Example 1 does not

- **The no-introspection exception (MERGE07).** With no `arguments` key in the baseline, `repeat`'s
  declaration *establishes* the argument list rather than being checked against it. This is the one
  place the reject rule stands down, and it exists precisely for this host.
- **Derived labels doing real work.** `to_upper` declares no label and gets `To upper`. An author
  writing thirty commands writes thirty fewer labels.
- **Shorthand defaults.** `default: 2` rather than `default: !Value 2` — the same value, written the
  way an author expects.
- **`type` rather than `argument_type`.** Both spellings reach the same field.
- **A hint on a command with no host present.** It is carried through the merge and dropped at
  `build`; a Python host loading this document later reads it from the declaration.

### Guide and executable example

This document is a guide candidate in its own right — it is the shortest complete answer to "how do
I declare commands without writing code". It should become a fixture used by INT04, so the guide's
example and the test's input are the same text.

---

## Example 3 — Pitfalls

### 3.1 A misspelled argument name is rejected, not added

```yaml
# baseline:    arguments: [{ name: count, argument_type: int }]
# declaration: arguments: [{ name: cnt,   label: Count }]
```

Rejected: `command "repeat": declared argument "cnt" does not exist; the callable declares "count"`.

Silently adding it would have produced a two-argument command whose second argument no function
parameter receives — and because Liquers binds positionally, the *first* query parameter would then
fill `count` while the second vanished. This is the reason the rule is reject rather than append.

### 3.2 A declaration cannot reorder

```yaml
# baseline:    arguments: [{name: a}, {name: b}]
# declaration: arguments: [{name: b, label: Bee}, {name: a, label: Ay}]
```

Both entries merge by name; the result is still `[a, b]`. The labels land correctly and the order
does not move. An author who expects the declaration's order to win gets a surprise — which is why
MERGE10 asserts it and why the reference states it.

### 3.3 A default of the string `"None"`

```yaml
- { name: mode, type: string, default: None }          # no default at all
- { name: mode, type: string, default: !Value 'None' } # the string "None"
```

Inherited from the exported form, where `None` is how the absent-default marker serializes. The
shorthand makes it reachable by accident, so it is documented and tested rather than left to be
discovered.

### 3.4 A hint that nothing reads

```yaml
hints:
  javascript: { stat: text }    # "stat", not "state"
```

Carried through the merge, dropped at `build`, never read. Nothing in `liquers-core` can catch this,
by construction — `hints` is free-form. An integration that cares must check the keys it expects,
and its own conformance test is where that belongs.

---

## Corner cases

**Memory.** `CommandDeclaration` owns one `serde_json::Value` and nothing else; no `Arc`, no
lifetimes. A host retaining declarations for replay retains one per command. The `liquers-web`
aliasing hazard that `snapshot_declaration` exists for (`environment.rs:171-193`) disappears by
construction once a parsed declaration is retained instead of the caller's `JsValue`, though acting
on that belongs to `POST-INIT-COMMAND-REGISTRATION`.

**Concurrency.** None. Every stage is a pure function; registration is not a hot path. The
`INFERRED_ARGUMENTS` thread-local (`adapter.rs:26-37`) is *removed* rather than made concurrent —
the merge's own rule carries what it was tracking.

**Errors, and warnings.** Every failure is `Error::from_error(ErrorType::ParameterError, …)`; no new
error type, no `Error::new`. Non-fatal diagnostics go to a **collected** warning channel rather than
`eprintln!` — `liquers-web` is a wasm build where nothing reads stderr, so a printed warning is lost,
and a printed warning cannot be asserted on, which is exactly what WARN01–WARN06 need to do. Each message names the command, and the argument where there is one. Serde failures
from the `liquers-web` path are wrapped so the command name survives a message serde wrote.

**Serialization.** The one hard constraint: `specs/command_registry.yaml` must not move (INT01), and
it is checked byte-for-byte rather than by comparison of parsed values. Every serde change in Part C
is deserialize-only, so a `Serialize` change is a bug the test catches. `command_metadata.rs:1381`
asserts an exact JSON string and is the sharpest tripwire.

**Integration.** Two crates. `liquers-core` gains a module and eight deserialize-only serde rows;
`liquers-web` loses ~130 lines of hand-parsing. `liquers-py` is untouched until it opts in. The
`liquers-lib` registry export must stay green throughout.

---

## Test plan

Conventions per `liquers-unittest`: unit tests colocated in `#[cfg(test)] mod tests`, integration
tests in `liquers-core/tests/`, `#[tokio::test]` where async is needed — none is here.

### Unit tests — the merge laws

These are the substance. They are cheap, exhaustive, and each one is a property a host will depend
on without being told.

```rust
#[cfg(test)]
mod merge_tests {
    use super::*;
    use serde_json::json;

    fn baseline() -> serde_json::Value {
        json!({
            "name": "repeat",
            "arguments": [
                { "name": "count", "argument_type": "int", "default": { "Value": 2 } }
            ]
        })
    }

    #[test]
    fn merge01_empty_declaration_is_identity() {
        let mut d = CommandDeclaration::from_introspection(baseline());
        d.enhance(&json!({})).unwrap();
        assert_eq!(d.as_value(), &baseline());
    }

    #[test]
    fn merge02_enhance_is_idempotent() {
        let decl = json!({ "label": "Repeat text" });
        let mut once = CommandDeclaration::from_introspection(baseline());
        once.enhance(&decl).unwrap();
        let mut twice = CommandDeclaration::from_introspection(baseline());
        twice.enhance(&decl).unwrap();
        twice.enhance(&decl).unwrap();
        assert_eq!(once.as_value(), twice.as_value());
    }

    #[test]
    fn merge03_declared_scalar_overrides_discovered() {
        let mut d = CommandDeclaration::from_introspection(json!({"name":"x","doc":"discovered"}));
        d.enhance(&json!({ "doc": "declared" })).unwrap();
        assert_eq!(d.as_value()["doc"], json!("declared"));
    }

    #[test]
    fn merge04_omitted_field_leaves_discovered_value() {
        let mut d = CommandDeclaration::from_introspection(json!({"name":"x","doc":"discovered"}));
        d.enhance(&json!({ "label": "X" })).unwrap();
        assert_eq!(d.as_value()["doc"], json!("discovered"));
    }

    /// The case the design exists for: type and default survive an entry that mentions neither.
    #[test]
    fn merge05_argument_entry_augments_by_name() {
        let mut d = CommandDeclaration::from_introspection(baseline());
        d.enhance(&json!({ "arguments": [{ "name": "count", "label": "Count" }] })).unwrap();
        let arg = &d.as_value()["arguments"][0];
        assert_eq!(arg["label"], json!("Count"));
        assert_eq!(arg["argument_type"], json!("int"));
        assert_eq!(arg["default"], json!({ "Value": 2 }));
    }

    #[test]
    fn merge06_unknown_argument_name_is_rejected() {
        let mut d = CommandDeclaration::from_introspection(baseline());
        let err = d.enhance(&json!({ "arguments": [{ "name": "cnt" }] })).unwrap_err();
        let m = err.to_string();
        assert!(m.contains("cnt"), "names the offending argument: {m}");
        assert!(m.contains("repeat"), "names the command: {m}");
    }

    /// The plain-document host: no baseline to check against, so the declaration establishes it.
    #[test]
    fn merge07_no_arguments_key_lets_the_declaration_establish_the_list() {
        let mut d = CommandDeclaration::from_introspection(json!({ "name": "repeat" }));
        d.enhance(&json!({ "arguments": [{ "name": "count", "argument_type": "int" }] })).unwrap();
        assert_eq!(d.as_value()["arguments"][0]["name"], json!("count"));
    }

    /// An empty list means "introspected, no parameters" — a different thing from "not introspected".
    #[test]
    fn merge08_empty_arguments_list_still_rejects() {
        let mut d = CommandDeclaration::from_introspection(json!({"name":"f","arguments":[]}));
        assert!(d.enhance(&json!({ "arguments": [{ "name": "count" }] })).is_err());
    }

    #[test]
    fn merge09_null_sets_rather_than_deletes() {
        let mut d = CommandDeclaration::from_introspection(json!({"name":"x","filename":"a.txt"}));
        d.enhance(&json!({ "filename": null })).unwrap();
        assert!(d.as_value().get("filename").is_some(), "the key is present");
        assert_eq!(d.as_value()["filename"], serde_json::Value::Null);
    }

    #[test]
    fn merge10_declaration_cannot_reorder_arguments() {
        let base = json!({"name":"f","arguments":[{"name":"a"},{"name":"b"}]});
        let mut d = CommandDeclaration::from_introspection(base);
        d.enhance(&json!({ "arguments": [
            { "name": "b", "label": "Bee" },
            { "name": "a", "label": "Ay"  }
        ]})).unwrap();
        let args = d.as_value()["arguments"].as_array().unwrap();
        assert_eq!(args[0]["name"], json!("a"));
        assert_eq!(args[0]["label"], json!("Ay"));
        assert_eq!(args[1]["name"], json!("b"));
    }

    #[test]
    fn merge11_composition_is_associative() {
        let (d1, d2) = (json!({ "doc": "one" }), json!({ "label": "Two" }));
        let mut layered = CommandDeclaration::from_introspection(baseline());
        layered.enhance(&d1).unwrap();
        layered.enhance(&d2).unwrap();
        let mut combined = CommandDeclaration::from_introspection(baseline());
        combined.enhance(&json!({ "doc": "one", "label": "Two" })).unwrap();
        assert_eq!(layered.as_value(), combined.as_value());
    }

    #[test]
    fn merge12_nested_maps_merge_and_other_arrays_replace() {
        let base = json!({"name":"f","hints":{"js":{"state":"text","variadic":"spread"}},
                          "next":["a","b"]});
        let mut d = CommandDeclaration::from_introspection(base);
        d.enhance(&json!({ "hints": { "js": { "state": "value" } }, "next": ["c"] })).unwrap();
        assert_eq!(d.as_value()["hints"]["js"]["state"], json!("value"));
        assert_eq!(d.as_value()["hints"]["js"]["variadic"], json!("spread"), "sibling survives");
        assert_eq!(d.as_value()["next"], json!(["c"]), "a non-argument array replaces");
    }
}
```

### Unit tests — defaults

```rust
#[test]
fn def01_label_derivation() {
    for (name, want) in [
        ("to_text",           "To text"),
        ("toText",            "To text"),
        ("toHTML",            "To HTML"),
        ("parseHTTPResponse", "Parse HTTP response"),
        ("x",                 "X"),
    ] {
        assert_eq!(derive_label(name), want, "deriving from {name:?}");
    }
}

#[test]
fn def02_derivation_never_overwrites() {
    let mut d = CommandDeclaration::from_introspection(json!({"name":"to_text"}));
    d.enhance(&json!({ "label": "Textify" })).unwrap();
    d.fill_defaults();
    assert_eq!(d.as_value()["label"], json!("Textify"));
}

#[test]
fn def03_fill_defaults_is_idempotent() {
    let mut once = CommandDeclaration::from_introspection(json!({"name":"to_text"}));
    once.fill_defaults();
    let mut twice = CommandDeclaration::from_introspection(json!({"name":"to_text"}));
    twice.fill_defaults();
    twice.fill_defaults();
    assert_eq!(once.as_value(), twice.as_value());
}

/// `ArgumentGUIInfo`'s `Default` is `None`, but `ArgumentInfo::any_argument` sets `TextField(40)`.
/// Getting this wrong silently re-versions every JavaScript command with declared arguments.
#[test]
fn def04_argument_gui_info_defaults_to_text_field_40() {
    let m = build(json!({ "name": "f", "arguments": [{ "name": "a" }] }));
    assert_eq!(m.arguments[0].gui_info, ArgumentGUIInfo::TextField(40));
}

#[test]
fn def05_scalar_defaults_match_from_key() {
    let m = build(json!({ "name": "greet" }));
    let k = CommandMetadata::from_key(CommandKey::new("", "", "greet"));
    assert_eq!((m.cache, m.volatile, m.expires, m.definition, m.payload_required),
               (k.cache, k.volatile, k.expires, k.definition, k.payload_required));
}

/// Order is normative. Deriving before merging would make the derived label look "present".
#[test]
fn def06_derive_runs_after_merge() {
    let mut d = CommandDeclaration::from_introspection(json!({ "name": "to_text" }));
    d.fill_defaults();                                   // derives "To text"
    d.enhance(&json!({ "label": "Textify" })).unwrap();  // author still wins
    assert_eq!(d.as_value()["label"], json!("Textify"));
}
```

### Unit tests — build, validation, hints, conventions

```rust
#[test]
fn build01_minimal_declaration_builds() {
    let m = build(json!({ "name": "greet" }));
    assert_eq!(m.name, "greet");
    assert_eq!(m.label, "Greet");
    assert!(m.arguments.is_empty());
}

#[test]
fn build02_type_is_accepted_for_argument_type() {
    let m = build(json!({ "name": "f", "arguments": [{ "name": "a", "type": "int" }] }));
    assert_eq!(m.arguments[0].argument_type, ArgumentType::Integer);
}

#[test]
fn build03_argument_type_aliases() {
    for (spelling, want) in [
        ("str", ArgumentType::String),     ("text", ArgumentType::String),
        ("integer", ArgumentType::Integer), ("number", ArgumentType::Float),
        ("boolean", ArgumentType::Boolean),
    ] {
        let m = build(json!({"name":"f","arguments":[{"name":"a","type":spelling}]}));
        assert_eq!(m.arguments[0].argument_type, want, "spelling {spelling:?}");
    }
}

#[test]
fn build04_command_parameter_value_shapes() {
    let cases = [
        (json!(2),                 CommandParameterValue::Value(json!(2))),
        (json!("hello"),           CommandParameterValue::Value(json!("hello"))),
        (json!(true),              CommandParameterValue::Value(json!(true))),
        (json!(null),              CommandParameterValue::Value(json!(null))),
        (json!("None"),            CommandParameterValue::None),
        (json!({ "Value": 2 }),    CommandParameterValue::Value(json!(2))),
    ];
    for (input, want) in cases {
        let m = build(json!({"name":"f","arguments":[{"name":"a","default":input}]}));
        assert_eq!(m.arguments[0].default, want);
    }
}

/// Documented trap: the bare string "None" is the absent-default marker.
#[test]
fn build05_none_string_needs_the_tagged_form() {
    let bare = build(json!({"name":"f","arguments":[{"name":"a","default":"None"}]}));
    assert_eq!(bare.arguments[0].default, CommandParameterValue::None);
    let tagged = build(json!({"name":"f","arguments":[
        {"name":"a","default":{"Value":"None"}}]}));
    assert_eq!(tagged.arguments[0].default, CommandParameterValue::Value(json!("None")));
}

#[test]
fn val01_empty_name_is_refused() {
    assert!(try_build(json!({ "name": "" })).is_err());
    assert!(try_build(json!({})).is_err());
}

#[test]
fn val02_multiple_argument_must_be_last() {
    let err = try_build(json!({"name":"f","arguments":[
        {"name":"xs","multiple":true},{"name":"y"}]})).unwrap_err();
    assert!(err.to_string().contains("xs"));
}

#[test]
fn val03_default_must_fit_declared_type() {
    let err = try_build(json!({"name":"f","arguments":[
        {"name":"a","type":"int","default":"not a number"}]})).unwrap_err();
    let m = err.to_string();
    assert!(m.contains("f") && m.contains("a"), "names command and argument: {m}");
}

#[test]
fn val04_unknown_argument_type_names_command_and_argument() {
    let err = try_build(json!({"name":"f","arguments":[
        {"name":"a","type":"zzz"}]})).unwrap_err();
    let m = err.to_string();
    assert!(m.contains("f") && m.contains("a") && m.contains("zzz"));
}

/// Resolving one needs a registry, so it stays where it happens today.
#[test]
fn val05_global_enum_reference_is_not_resolved_and_does_not_fail() {
    let m = build(json!({"name":"f","arguments":[
        {"name":"a","argument_type":{"GlobalEnum":"colours"}}]}));
    assert!(matches!(m.arguments[0].argument_type, ArgumentType::GlobalEnum(_)));
}

#[test]
fn hint01_registration_hints_merge_like_any_map() {
    let mut d = CommandDeclaration::from_introspection(json!({"name":"f"}));
    d.enhance(&json!({ "registration": { "python": { "state": "text" } } })).unwrap();
    d.enhance(&json!({ "registration": { "python": { "variadic": "spread" } } })).unwrap();
    assert_eq!(d.registration()["python"]["state"], json!("text"));
    assert_eq!(d.registration()["python"]["variadic"], json!("spread"));
}

/// Registration hints are declaration-only: readable from the declaration, absent from the metadata.
#[test]
fn hint02_build_drops_registration() {
    let mut d = CommandDeclaration::from_introspection(json!({"name":"f"}));
    d.enhance(&json!({ "registration": { "python": { "state": "text" } } })).unwrap();
    let m = finish(&mut d);
    assert_eq!(serde_json::to_value(&m).unwrap().get("registration"), None);
    assert_eq!(d.registration()["python"]["state"], json!("text"), "still readable");
}

#[test]
fn hint03_unknown_registration_key_is_carried_not_rejected() {
    let mut d = CommandDeclaration::from_introspection(json!({"name":"f"}));
    d.enhance(&json!({ "registration": { "javascript": { "stat": "text" } } })).unwrap();
    assert_eq!(d.registration()["javascript"]["stat"], json!("text"));
    assert!(d.build().is_ok());
}

/// The other kind: a usage hint is ordinary metadata and must reach the built command.
#[test]
fn hint04_usage_hint_on_an_argument_reaches_the_metadata() {
    let m = build(json!({ "name": "f", "arguments": [
        { "name": "a", "hints": { "placeholder": "how many times" } }
    ]}));
    assert_eq!(m.arguments[0].hints["placeholder"], json!("how many times"));
}

// --- conventions ---------------------------------------------------------

/// A context parameter is not a command argument. register_command! gives it no argument slot
/// (registration.rs:489); a dynamic host needs this rule to reach the same place.
#[test]
fn conv01_context_leaves_arguments_and_lands_in_registration() {
    let mut d = CommandDeclaration::from_introspection(json!({"name":"f","arguments":[
        { "name": "count" }, { "name": "context" }]}));
    d.apply_conventions().unwrap();
    let m = finish(&mut d);
    assert_eq!(m.arguments.len(), 1);
    assert_eq!(m.arguments[0].name, "count");
    assert_eq!(d.registration()["context"], json!(1), "the position is recorded");
}

/// The first argument is *always* the state-derived argument; its name selects only the delivery
/// mode. `df` here is the state, delivered as `Reserved("df")` which behaves as `value`.
#[test]
fn conv02_the_first_argument_is_always_the_state() {
    for (first, want_mode) in [("state", "state"), ("value", "value"),
                               ("text", "text"), ("df", "df")] {
        let mut d = CommandDeclaration::from_introspection(json!({"name":"f","arguments":[
            { "name": first }, { "name": "count" }]}));
        d.apply_conventions().unwrap();
        let m = finish(&mut d);
        assert!(m.state_argument.is_some(), "first argument {first:?} is the state");
        assert_eq!(m.arguments.len(), 1, "only `count` remains");
        assert_eq!(d.registration()["state"], json!(want_mode));
    }
}

/// Position still matters: only the *first* argument is the state.
#[test]
fn conv03_a_non_leading_state_name_is_an_ordinary_argument() {
    let mut d = CommandDeclaration::from_introspection(json!({"name":"f","arguments":[
        { "name": "value" }, { "name": "state" }, { "name": "text" }]}));
    d.apply_conventions().unwrap();
    let m = finish(&mut d);
    assert_eq!(m.arguments.len(), 2);
    assert_eq!(m.arguments[0].name, "state");
    assert_eq!(m.arguments[1].name, "text");
}

/// Core records the mode and never performs it; every integration reads the same values.
#[test]
fn conv08_each_delivery_mode_is_recorded_distinctly() {
    for name in ["none", "na", "state", "value", "text"] {
        let mut d = CommandDeclaration::from_introspection(
            json!({"name":"f","arguments":[{ "name": name }]}));
        d.apply_conventions().unwrap();
        let want = if name == "na" { "none" } else { name };
        assert_eq!(d.registration()["state"], json!(want), "name {name:?}");
    }
}

/// `first_command` semantics: no state argument at all. Confirmed against liquer/commands.py:882,
/// where has_state_argument=False sets state_argument to None and dispatch calls f(*argv).
#[test]
fn conv09_none_gives_a_source_command() {
    for name in ["none", "na"] {
        let mut d = CommandDeclaration::from_introspection(json!({"name":"f","arguments":[
            { "name": name }, { "name": "count" }]}));
        d.apply_conventions().unwrap();
        let m = finish(&mut d);
        assert!(m.state_argument.is_none(), "{name:?} is a source command");
        assert_eq!(m.arguments.len(), 1, "the marker is not an argument either");
    }
}

/// The escape hatch: declaring it explicitly beats the naming rule.
#[test]
fn conv10_explicit_state_argument_is_left_alone() {
    let mut d = CommandDeclaration::from_introspection(json!({"name":"f","arguments":[
        { "name": "df" }]}));
    d.enhance(&json!({ "state_argument": { "name": "df" } })).unwrap();
    d.apply_conventions().unwrap();
    let m = finish(&mut d);
    assert_eq!(m.state_argument.as_ref().unwrap().name, "df");
}

/// The extension point. An unrecognised name is not an error — it means `value` until something
/// gives it meaning, so a declaration written today survives `df` acquiring one.
#[test]
fn conv11_reserved_name_behaves_as_value() {
    assert_eq!(StateDelivery::from_argument_name("df"),
               StateDelivery::Reserved("df".to_string()));
    assert_eq!(StateDelivery::from_argument_name("df").effective(), StateDelivery::Value);
}

/// A host implements its own first_command affordance this way, without depending on a name.
#[test]
fn conv12_a_declared_mode_wins_over_the_derived_one() {
    let mut d = CommandDeclaration::from_introspection(json!({"name":"f","arguments":[
        { "name": "value" }, { "name": "count" }]}));
    d.enhance(&json!({ "registration": { "state": "none" } })).unwrap();
    d.apply_conventions().unwrap();
    let m = finish(&mut d);
    assert!(m.state_argument.is_none(), "declared `none` wins over derived `value`");
}

/// Structural before delivery, or `def f(context, x)` would make the context the state.
#[test]
fn conv13_leading_context_is_removed_before_the_delivery_rule() {
    let mut d = CommandDeclaration::from_introspection(json!({"name":"f","arguments":[
        { "name": "context" }, { "name": "value" }, { "name": "count" }]}));
    d.apply_conventions().unwrap();
    let m = finish(&mut d);
    assert_eq!(d.registration()["context"], json!(0));
    assert_eq!(d.registration()["state"], json!("value"), "`value`, not `context`");
    assert_eq!(m.arguments.len(), 1);
}

/// The rule interprets a *function's parameters*. A document declaring public arguments where none
/// were discovered must not lose its first one to the state.
#[test]
fn conv14_no_introspection_means_no_delivery_rule() {
    let mut d = CommandDeclaration::from_introspection(json!({ "name": "f" }));
    d.enhance(&json!({ "arguments": [{ "name": "count" }] })).unwrap();
    d.apply_conventions().unwrap();
    let m = finish(&mut d);
    assert!(m.state_argument.is_none());
    assert_eq!(m.arguments.len(), 1);
    assert_eq!(m.arguments[0].name, "count");
}

#[test]
fn conv04_a_convention_can_be_disabled_by_name() {
    let mut d = CommandDeclaration::from_introspection(json!({"name":"f","arguments":[
        { "name": "context" }]}));
    d.enhance(&json!({ "conventions": { "context": false } })).unwrap();
    d.apply_conventions().unwrap();
    let m = finish(&mut d);
    assert_eq!(m.arguments.len(), 1, "a genuine `context` argument survives");
    assert_eq!(m.arguments[0].name, "context");
}

#[test]
fn conv05_all_conventions_can_be_disabled() {
    let mut d = CommandDeclaration::from_introspection(json!({"name":"f","arguments":[
        { "name": "state" }, { "name": "context" }]}));
    d.enhance(&json!({ "conventions": false })).unwrap();
    d.apply_conventions().unwrap();
    let m = finish(&mut d);
    assert_eq!(m.arguments.len(), 2);
    assert!(m.state_argument.is_none());
}

/// The reason conventions run *after* the merge: an author declaring metadata for a recognised
/// name gets it matched by name, not rejected as unknown.
#[test]
fn conv06_declared_entry_for_a_recognised_name_merges_before_it_is_lifted() {
    let mut d = CommandDeclaration::from_introspection(json!({"name":"f","arguments":[
        { "name": "state" }, { "name": "count" }]}));
    d.enhance(&json!({ "arguments": [{ "name": "state", "label": "Input" }] }))
        .expect("must not be rejected as an unknown argument");
    d.apply_conventions().unwrap();
    let m = finish(&mut d);
    assert_eq!(m.state_argument.as_ref().unwrap().label, "Input");
}

#[test]
fn conv07_conventions_are_idempotent() {
    let base = json!({"name":"f","arguments":[{ "name": "state" }, { "name": "context" }]});
    let mut once = CommandDeclaration::from_introspection(base.clone());
    once.apply_conventions().unwrap();
    let mut twice = CommandDeclaration::from_introspection(base);
    twice.apply_conventions().unwrap();
    twice.apply_conventions().unwrap();
    assert_eq!(once.as_value(), twice.as_value());
    assert_eq!(once.warnings(), twice.warnings(), "warnings too, not just the document");
}

// --- warnings ------------------------------------------------------------
//
// Three convention outcomes are silent decisions an author cannot see in the document. Each has a
// legitimate use, so each warns rather than failing. Collected, not printed: liquers-web is a wasm
// build where nothing reads stderr, and a printed warning cannot be asserted on.

fn kinds(d: &CommandDeclaration) -> Vec<WarningKind> {
    d.warnings().iter().map(|w| w.kind.clone()).collect()
}

#[test]
fn warn01_a_reserved_delivery_name_warns_and_still_means_value() {
    let mut d = CommandDeclaration::from_introspection(json!({"name":"f","arguments":[
        { "name": "df" }, { "name": "count" }]}));
    d.apply_conventions().unwrap();
    assert!(kinds(&d).contains(&WarningKind::ReservedStateDelivery));
    assert_eq!(d.registration()["state"], json!("df"), "recorded verbatim");
    assert_eq!(StateDelivery::from_argument_name("df").effective(), StateDelivery::Value);
    let w = &d.warnings()[0];
    assert!(w.message.contains("df") && w.command == "f");
}

/// The surprise: removing a leading context shifts which argument becomes the state, so
/// `def f(context, count)` makes `count` the state.
#[test]
fn warn02_a_leading_context_warns_that_it_shifted_the_state() {
    let mut d = CommandDeclaration::from_introspection(json!({"name":"f","arguments":[
        { "name": "context" }, { "name": "count" }]}));
    d.apply_conventions().unwrap();
    assert!(kinds(&d).contains(&WarningKind::ContextBeforeState));
    assert_eq!(d.registration()["state"], json!("count"), "`count` became the state");
}

/// Scoped to avoid noise: only when the declaration supplied arguments and declared no state, so a
/// plain-document host declaring `state_argument` explicitly stays quiet.
#[test]
fn warn03_no_introspection_warns_only_when_the_state_is_unstated() {
    let mut noisy = CommandDeclaration::from_introspection(json!({ "name": "f" }));
    noisy.enhance(&json!({ "arguments": [{ "name": "count" }] })).unwrap();
    noisy.apply_conventions().unwrap();
    assert!(kinds(&noisy).contains(&WarningKind::NoIntrospection));

    let mut quiet = CommandDeclaration::from_introspection(json!({ "name": "f" }));
    quiet.enhance(&json!({ "arguments": [{ "name": "count" }],
                           "state_argument": { "name": "state" } })).unwrap();
    quiet.apply_conventions().unwrap();
    assert!(!kinds(&quiet).contains(&WarningKind::NoIntrospection), "declared, so no warning");
}

#[test]
fn warn04_a_dropped_command_level_hints_key_warns() {
    let mut d = CommandDeclaration::from_introspection(json!({"name":"f"}));
    d.enhance(&json!({ "hints": { "category": "text" } })).unwrap();
    let m = finish(&mut d);
    assert!(kinds(&d).contains(&WarningKind::DroppedKey));
    assert_eq!(serde_json::to_value(&m).unwrap().get("hints"), None);
}

#[test]
fn warn05_warnings_are_deduplicated() {
    let mut d = CommandDeclaration::from_introspection(json!({"name":"f","arguments":[
        { "name": "df" }]}));
    d.apply_conventions().unwrap();
    d.apply_conventions().unwrap();
    assert_eq!(d.warnings().iter()
                 .filter(|w| w.kind == WarningKind::ReservedStateDelivery).count(), 1);
}

/// Every warned-about case has a legitimate use, so failing would block correct declarations to
/// catch incorrect ones.
#[test]
fn warn06_a_warning_is_never_fatal() {
    let mut d = CommandDeclaration::from_introspection(json!({"name":"f","arguments":[
        { "name": "context" }, { "name": "df" }]}));
    d.apply_conventions().unwrap();
    d.fill_defaults();
    assert!(!d.warnings().is_empty());
    assert!(d.build().is_ok(), "warnings do not fail the build");
}
```

### Integration tests

`liquers-core/tests/command_declaration_roundtrip.rs`:

```rust
/// The hard constraint: the committed registry must not move. Byte-for-byte, not value equality,
/// because a Serialize change that happens to round-trip is still a change to a committed file.
#[test]
fn int01_command_registry_yaml_is_byte_identical_after_a_parse_and_re_serialize() {
    let path = repo_root().join("specs/command_registry.yaml");
    let original = std::fs::read_to_string(&path).unwrap();
    let registry: CommandMetadataRegistry = serde_yaml::from_str(&original).unwrap();
    let again = serde_yaml::to_string(&registry).unwrap();
    assert_eq!(strip_comment_block(&original), again);
}

/// Equal content must give an equal version, since metadata_version is computed from stored
/// content by add_command_metadata (command_metadata.rs:1036,1064). Asserted, not assumed.
#[test]
fn int02_declaration_and_macro_agree_including_metadata_version() {
    let mut a = CommandMetadataRegistry::new();
    let mut b = CommandMetadataRegistry::new();
    a.add_command_metadata(metadata_from_macro_registration());
    b.add_command_metadata(build(declaration_for_the_same_command()));
    assert_eq!(a.get("to_text"), b.get("to_text"));
}

/// The two label rules must stay apart, or every underscored JavaScript command re-versions
/// and its dependent assets re-expire.
#[test]
fn int03_label_parity_between_the_two_paths() {
    assert_eq!(js_path_metadata("foo_bar").label, "foo_bar");
    assert_eq!(build(json!({ "name": "foo_bar" })).label, "Foo bar");
}

#[test]
fn int04_yaml_and_json_agree() {
    let yaml = include_str!("fixtures/commands.yaml");   // Example 2, verbatim
    let from_yaml: serde_json::Value = serde_yaml::from_str(yaml).unwrap();
    let from_json: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&from_yaml).unwrap()).unwrap();
    assert_eq!(build_all(&from_yaml), build_all(&from_json));
}
```

`INT05` and `INT06` are `wasm_bindgen_test`s in `liquers-web`:

- **INT05** — a JavaScript declaration object, converted by `serde_wasm_bindgen`, builds the same
  metadata as the equivalent `serde_json::Value`. **This is the test that settles Phase 2's largest
  unverified claim** and should be written first in Phase 4, because the fallback if it fails
  (`js_sys::JSON::stringify` then `serde_json::from_str`) changes the `liquers-web` code path.
- **INT06** — the existing COMMAND suite, unchanged. Four tests assert error wording (`:66`,
  `:409`, `:422`, `:509`) and all four keep their producing code in `liquers-web`.

### Manual validation

```bash
cargo test -p liquers-core --lib                                    # unit tests
cargo test -p liquers-core --test command_declaration_roundtrip     # INT01-INT04
cargo test -p liquers-lib --lib --tests                             # registry_export stays green
bash scripts/check-build-matrix.sh                                  # 11 configurations
cargo clean && cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles
```

---

## Documentation and learning log

**Guide candidates.** Example 1 answers "how do I add metadata the signature cannot carry?" and
Example 2 answers "how do I declare commands without writing code?" — the two questions a language
guide must answer first. Both belong in `COMMAND_DECLARATION.md`, which already carries a worked
example in §8; Example 1 here is the same shape with the merge shown step by step, and §8 should be
replaced by it once this phase is approved.

Example 2's `commands.yaml` should be a **test fixture used by INT04**, so the documented example and
the tested input cannot drift.

**Learning to carry into Phase 5.**

- The merge laws are the specification. Prose describing "declaration overrides introspection" is
  not precise enough to implement from — MERGE04, MERGE09 and MERGE10 are each a case a reasonable
  implementer would get wrong from the prose alone.
- Two default mismatches found while designing (`ArgumentGUIInfo::None` versus `TextField(40)`;
  `state_argument`'s constructor versus serde default) both silently change `metadata_version`.
  DEF04 exists because of the first; the second is `STATE-ARGUMENT-CONSTRUCTOR-SERDE-DEFAULT-DISAGREE`
  and `fill_defaults` should settle it explicitly rather than inherit it.
- The `"None"` default is the sharpest edge in the format and is inherited, not introduced.
- Two things called "hints" answer different questions. Using one key for both would have put a
  calling convention into the exported registry under a name that already means "hints for the UI".
- Conventions are the reason stage 1 can be dumb. An integration that recognises `context` itself
  has re-implemented a rule every other language must then re-implement identically.
- The first argument is *always* the state; its name selects only the delivery mode. The earlier
  rule — recognise an argument *named* `state`/`value`/`text` — was a trap: `def f(df, count)` would
  have declared a source command whose first query parameter bound to `df`. Making the name select
  the mode instead is what lets an unknown name be a reserved extension point rather than an error.
- The two convention kinds are worth keeping apart: `context` is *structural* — it changes the
  argument list — while the state rule is *delivery*, fixing how a value arrives. The second has normative
  meanings (`value` means "unwrap through the value bridge where possible") that core records but
  cannot enforce, so each integration's conformance suite is where they are actually checked.
- Silent decisions want warnings, and the three the conventions make are the ones that have
  surprised people. A collected channel beats `eprintln!` for a library two hosts wrap — and the
  wasm case makes it not a preference but a correctness point, since stderr goes nowhere there.
- `text` is the only delivery mode that can fail, and it fails at call time. Nothing in
  `liquers-core` can catch it: `try_into_string` returns a `Result` and the integration's error
  bridge carries it.

## Review record

*Against Phase 1:* every acceptance criterion has a test — criterion 1 is BUILD01, criterion 3 is
INT01, criterion 5 is INT02, criterion 6 is INT06, criterion 7 is what the merge laws make possible.
Criterion 2 is HINT01–HINT04, CONV01–CONV14 and WARN01–WARN06: registration hints declaration-only, usage hints in
the metadata, and conventions owned by the declaration layer.

*Against Phase 2:* the five stages appear as `from_introspection`, `enhance`, `apply_conventions`,
`fill_defaults`, `build`; every merge rule in Part A has a numbered test, including the
no-introspection exception that Part A calls load-bearing; the Part B label table is DEF01 verbatim;
the Part C `CommandParameterValue` table is BUILD04; Part D's two hint kinds are HINT02 (registration
dropped) and HINT04 (usage kept); and Part E's conventions are CONV01–CONV14, with CONV06 asserting
the after-the-merge ordering that Part E calls its design decision, CONV08 pinning the delivery
modes and CONV13 the structural-before-delivery ordering. Part E's warning channel is
WARN01–WARN06.

*Against the codebase:* no query strings appear in these examples, so query validation does not
apply. Every cited line was read at `HEAD`. Commands named in examples (`repeat`, `to_upper`) are
illustrative declarations, not registry lookups.

*Review passes* were run inline rather than by sub-agents, this session not having been asked to
spawn them; the conformity checks the workflow assigns to separate reviewers are folded into this
record.

*Gaps I would not hide:* INT05 is the one test whose outcome could change the design, and it is the
one that cannot be written until Phase 4 puts code in `liquers-web`. INT02 assumes a representative
command can be registered both ways in one test binary; if `register_command!` cannot be invoked
from an integration test it becomes a unit test inside `liquers-lib` instead.
