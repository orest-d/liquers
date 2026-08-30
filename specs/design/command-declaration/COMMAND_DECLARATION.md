---
title: Command Declaration Format
kind: reference
audience: internal
area: [core/commands, web, py]
reviewed: 2026-08-29
---
> **Not yet true at `HEAD`.** `reference/` documents describe how the system *is*
> (`DOCS_STRUCTURE_GUIDE.md` §2), and this format is designed but unimplemented. It is held here
> under its final name and **promoted to `specs/reference/COMMAND_DECLARATION.md` at Phase 5**,
> unchanged except for this banner. It is written now because the language-specific guides are to be
> built on it, and they need a stable thing to point at.

# Command Declaration Format

## 1. What this describes

A **command declaration** is the author-facing way to say that a function is a command. It is the
runtime counterpart of the `register_command!` macro: where a Rust author writes a macro invocation
that the compiler expands, an author in a dynamic language writes a declaration that
`liquers-core` turns into [`CommandMetadata`](REGISTER_COMMAND_FSD.md) at run time.

This document is the shared basis for the language-specific guides. It defines what a declaration
means; how a Python decorator, a JavaScript object literal or a YAML document expresses it is each
guide's subject.

**Declaration and metadata are different things and the difference is the point:**

| | Declaration | `CommandMetadata` |
|---|---|---|
| Describes | how a function implements a command | the command itself |
| Audience | the command author | the planner, the UI, the registry |
| Nature | ergonomic, **partial**, additive | precise, authoritative, **complete** |
| Role | input | result |

A declaration is not a serialization of `CommandMetadata`. It is a *fragment* of one, composed with
other fragments and completed by rule.

## 2. Composition

The central concept. A declaration is **not read alone**: it is layered over what the host already
knows, because most of what a command needs is discoverable from the function itself.

```
1. populate   the host inspects the callable and builds a baseline    host-specific
2. enhance    the author's declaration is merged over the baseline    shared
3. apply      conventions reinterpret the composed result             shared
4. fill       defaults are derived for whatever is still absent       shared
5. build      convert to CommandMetadata, or report what is wrong     shared
```

**Introspection is the basis, not a fallback.** Python's `inspect.signature` yields parameter names,
type annotations and defaults exactly; JavaScript's source parse yields names only; a YAML document
has no introspection at all and starts from an empty baseline. Whatever a host can discover, it
should — and the author then supplies only what discovery cannot know: a label, a documentation
string, a namespace, a widget hint, an enum domain.

The consequence for an author is the property worth remembering: **you write only the difference.**
Adding a label to one argument does not require restating that argument's type or default.

Stages 2–5 behave identically in every language. Only stage 1 differs, which is why they are shared
code and stage 1 is not.

**Stage 1 should be as dumb as it can be.** Report the parameters the callable actually has, in
order, and let stage 3 recognise which of them are not command arguments at all. An integration that
decides for itself that a parameter named `context` is the execution context has re-implemented a
convention that every other language would then have to re-implement identically.

### 2.1 Merge rules

Merging is defined on the serialized form, so that *absent* and *present-but-default* are
distinguishable — a declaration that says nothing about `cache` must not overwrite a discovered
`cache`.

| Shape | Rule |
|---|---|
| object over object | merged key by key, recursively |
| scalar or array over anything | replaces it |
| absent key | leaves the baseline untouched |
| `null` | an ordinary value, **not** a deletion marker |
| `arguments` | merged **by `name`**, never by position — see below |

**Arguments merge by name.**

- An entry naming an argument the baseline has **augments** it, field by field. Fields the entry
  omits keep their discovered values.
- An entry naming an argument the baseline does **not** have is **rejected**. Liquers binds query
  parameters positionally, so a misspelled argument name would silently misbind.
- **Exception:** when the baseline carries no `arguments` key at all — no introspection ran — the
  declaration establishes the list. A baseline with `arguments: []` means *a function with no
  parameters* and is subject to the reject rule.
- Order comes from the baseline where one exists. A declaration may not reorder arguments.

**There is no removal.** A declaration cannot delete a discovered argument or field. If a function
parameter should not become a command argument, the host omits it in stage 1 — discovery is the
host's, and that is where the decision belongs.

**Composition is associative**, so declarations may be layered: a module-level default, then a
per-command declaration, then a per-argument refinement.

## 3. Conventions

Introspection reports the parameters a function has. Some of them are not command arguments: they
are the execution context, or the input state. A **convention** recognises those by name and moves
them out of `arguments` into the place they belong.

Conventions are owned here rather than by each integration, because the recognition rule is the same
in every language and re-implementing it per host is how two hosts come to disagree about what
`context` means.

### 3.1 When they apply

Stage 3 — **after** the merge, before defaults are derived. That ordering is deliberate: it means the
argument list is still complete while the declaration is merged, so an author who writes
`{ name: "context", label: "…" }` gets their entry matched by name rather than a confusing
"unknown argument" error. What the convention then does with that entry is the same whether it was
discovered or declared.

### 3.2 Two kinds of convention

They do different things and it is worth keeping them apart.

**Structural** — changes the argument list.

| Convention | Rule | Effect |
|---|---|---|
| `context` | an argument named `context` | Removed from `arguments`. Its position is recorded under `registration.context` so the integration can pass the context there at call time. It is **not** a `CommandMetadata` argument — matching `register_command!`, where a `context` parameter occupies no argument slot |

**Delivery** — classifies the first argument and fixes *how its value arrives*.

| Convention | Rule | Effect |
|---|---|---|
| `state` | the **first** argument, when named `state`, `value` or `text` | Removed from `arguments` and recorded as `state_argument`. The spelling selects a **delivery mode**, recorded under `registration.state` |

### 3.2.1 The three delivery modes

The mode is language-specific to *perform* — only the integration knows what its native values are —
but its **meaning is normative**, and every integration must honour it identically. This is the
whole reason the convention is owned here rather than per host.

| Spelling | The callable receives | Can it fail? |
|---|---|---|
| `state` | the `State` wrapper itself, so the callable reaches the metadata as well as the value | no |
| `value` | the value, **unwrapped to the language-native form wherever the value bridge can do it**, falling back to the `Value` wrapper only where it cannot | no |
| `text` | the value converted to a string, via `ValueInterface::try_into_string` | **yes** — at call time, not registration |

`value` is not a new mechanism: it delegates to the integration's existing value bridge (the `VALUE`
feature of the language integration guide). A Python `value` command receives a `str`, an `int`, a
`DataFrame` — whatever the bridge produces — and a `Value` object only for something the bridge
cannot unwrap. Writing `value` and receiving a wrapper for a plain string would be a bridge defect,
not a declaration one.

`text` is the only mode that can fail, and it fails **when the command runs**, not when it is
declared: `try_into_string` returns a `Result` and not every value has a string form. An integration
maps that failure through its error bridge like any other command error.

### 3.2.2 Only the first argument

The rule keys on position as well as name. An argument named `state` in any other position is an
ordinary command argument and keeps its slot.

**The consequence is worth stating plainly, because it surprises people:** a function whose first
parameter is named anything else — `data`, `df`, `x` — declares a *source* command, and that first
parameter becomes an ordinary query argument. Naming is the whole rule.

The escape hatch is to declare it rather than rely on the name:

```yaml
name: transform
state_argument: { name: df }      # explicit; the convention is not consulted for it
```

### 3.3 Opting out

A function may genuinely have an ordinary argument called `context`. The declaration disables
conventions by name, or all of them:

```yaml
conventions: { context: false }   # keep an argument literally named "context"
conventions: false                # apply none
```

### 3.4 Adding conventions

The set is expected to grow — a leading `self` in Python, a variadic parameter recognised as
`multiple`, further delivery modes. A new convention is a **behaviour change for existing
declarations**, so it is added deliberately, with a test, and named in the tables above rather than
left implicit in an implementation. A new *delivery mode* additionally needs its meaning fixed
normatively, as §3.2.1 does for the three that exist, or two integrations will read it differently.

## 4. Mapping to `CommandMetadata`

Every declaration key is a `CommandMetadata` field, with **one exception**: `hints` (§5), which is
declaration-only and does not reach the metadata, plus `registration` and `conventions` (§3, §6). Apart from those the declaration adds no vocabulary
of its own, so a field added to `CommandMetadata` is declarable immediately with no change here.

### 4.1 Command level

| Key | Metadata field | Notes |
|---|---|---|
| `name` | `name` | **Required.** The only field with no default |
| `namespace`, `realm` | same | Empty means the root namespace / default realm |
| `label` | `label` | Derived from `name` when absent — §4 |
| `doc` | `doc` | Free text; hosts usually discover this from a docstring |
| `module` | `module` | Informational. Integrations set it to the language name |
| `filename` | `filename` | Suggested filename for the result |
| `cache` | `cache` | Defaults to `true` |
| `volatile` | `volatile` | Defaults to `false`; forces re-execution |
| `expires` | `expires` | Expiration specification; defaults to `never` |
| `payload_required` | `payload_required` | `none` or `required` |
| `presets`, `next` | same | UI affordances: ready-made parameter sets, suggested follow-on commands |
| `state_argument` | `state_argument` | Present means the command transforms an input state; absent means a *source* command |
| `arguments` | `arguments` | A list; merged by name — §2.1 |
| `definition` | `definition` | `Registered` (default) or an `Alias` |
| `hints` | `hints` | **Usage** hints — how to *use* the command. Metadata; survives export — §6 |
| `registration` | *(none)* | **Registration** hints — how to *register and call* it. Declaration-only, dropped at build — §6 |
| `conventions` | *(none)* | Declaration-only; disables conventions — §3.3 |

Not declarable: `metadata_version` and `impl_version`. Both are computed — the first from the stored
metadata content, the second supplied at registration.

### 4.2 Argument level

| Key | `ArgumentInfo` field | Notes |
|---|---|---|
| `name` | `name` | **Required.** Also the merge key |
| `label` | `label` | Derived from `name` when absent — §4 |
| `type` | `argument_type` | `type` is an accepted spelling of `argument_type` |
| `default` | `default` | §3.3 |
| `multiple` | `multiple` | Variadic: consumes every remaining action parameter. Must be last |
| `injected` | `injected` | Supplied from the context, never from the query |
| `gui_info` | `gui_info` | Preferred entry widget |
| `hints`, `presets` | same | `hints` here are **usage** hints and reach the metadata — §6 |

**Argument types.** The canonical names are `string`, `int`, `int_opt`, `float`, `float_opt`,
`bool`, `any`, `none`, plus the enum forms. These aliases are also accepted, so a declaration written
against a host's own vocabulary parses: `str` and `text` for `string`, `integer` for `int`, `number`
for `float`, `boolean` for `bool`.

### 4.3 Argument defaults

A default may be written either in the canonical tagged form the exporter produces, or as a bare
value — the shorthand an author expects:

| Written | Means |
|---|---|
| `2`, `"hello"`, `true`, `[…]` | that value |
| `null` | a null value |
| `None` (the bare string) | *no default* — the absent-default marker |
| `!Value 2` / `{"Value": 2}` | that value, explicitly |
| `!Query "a/b"` | a default computed by evaluating a query |

The one trap: a default whose literal value is the **string** `"None"` must be written
`!Value 'None'`, because the bare spelling is how "no default" is represented.

## 5. How defaults are created

Stage 3 fills what is still absent after the merge. It **never** overwrites a value that discovery or
the declaration supplied, and it is idempotent.

Ordering is normative: **merge, then derive, then build.** Deriving before merging would make a
derived value indistinguishable from a declared one and would block the author's own.

### 5.1 Labels

A label is derived from the name. Both `snake_case` and `camelCase` are broken into words and the
first is capitalised, so an author who names a function idiomatically in their own language gets a
readable label without writing one:

```
split on '_' and at lower→upper boundaries; a run of capitals followed by a lowercase
letter splits before the last capital; lowercase each word unless it is all-capitals;
capitalise the first character of the result
```

| Name | Label |
|---|---|
| `to_text` | `To text` |
| `toText` | `To text` |
| `toHTML` | `To HTML` |
| `parseHTTPResponse` | `Parse HTTP response` |

The same rule derives an argument's label from its name.

### 5.2 Other defaults

| Field | Default |
|---|---|
| `cache` | `true` |
| `volatile` | `false` |
| `expires` | `never` |
| `payload_required` | `none` |
| `definition` | `Registered` |
| `namespace`, `realm`, `doc`, `module`, `filename` | empty |
| `presets`, `next`, `arguments`, `hints` | empty |
| argument `argument_type` | `any` |
| argument `gui_info` | a text field of width 40 |
| argument `multiple`, `injected` | `false` |

So the minimal declaration is a name:

```yaml
name: to_text
```

which builds a complete `CommandMetadata`: label `To text`, cacheable, non-volatile, never expiring,
no arguments, registered.

## 6. Hints — two kinds

Two different things are called hints, they answer different questions, and they must not share a
key.

| | **Usage hints** | **Registration hints** |
|---|---|---|
| Answer | how do I *use* this command? | how do I *register and call* this function? |
| Key | `hints` | `registration` |
| Lives in | `CommandMetadata` | the declaration only |
| Read by | UI, documentation, tooling | the language integration |
| Survives export | **yes** | **no** — dropped at build |
| Example | a slider range, a placeholder, a category | which form of the state the callable wants |

### 6.1 Usage hints — `hints`

Part of the metadata, exactly as they are today: `ArgumentInfo::hints` is documented as *"Free
dictionary of hints for the argument… e.g. to provide additional hints for the UI"*
(`command_metadata.rs:399-403`). They describe the command to whoever presents it, and they are
declared like any other metadata field.

```yaml
arguments:
  - name: count
    type: int
    hints: { placeholder: "how many times" }
```

**Note a gap:** only `ArgumentInfo` has a `hints` field. `CommandMetadata` has none, so a *command-level*
usage hint cannot be expressed at all. This predates the declaration format and is not created by it;
filed as `COMMAND-METADATA-HAS-NO-COMMAND-LEVEL-HINTS`.

### 6.2 Registration hints — `registration`

Facts about calling the function: which form of the input state it wants, whether a variadic reaches
it spread or as one list, whether the result must be awaited, where the context parameter sits. Each
is meaningful in some languages and meaningless in others, so **`liquers-core` does not interpret
any of them.** It carries and merges them; the integration reads them back.

```yaml
name: repeat
arguments: [{ name: count, type: int }]
registration:
  javascript: { state: text, variadic: spread }
```

Namespace by integration, so two hosts declaring the same command cannot collide. Nothing is
reserved and nothing is validated, so a misspelled key is silently ignored — an integration that
cares should check for the keys it expects. Conventions (§3) also write here: that is where a
recognised `context` parameter's position ends up.

**Registration hints do not survive export.** A registry exported to `command_registry.yaml` carries
metadata only, so an environment rebuilt from an exported registry cannot recover how to call a
declared command. **An integration that replays registrations must retain the declaration, not the
metadata.** `liquers-web` already does this — `REGISTERED_SPECS` retains the declaration for exactly
this reason — so the requirement is not new, but it is now a rule rather than an accident.

## 7. Validation

Stage 4 reports, naming the command and where relevant the argument:

- an empty or missing `name`;
- an argument entry naming an argument that does not exist (§2.1);
- a `multiple` argument that is not the last argument;
- a default that does not fit its declared type;
- an unrecognised argument type.

Global enum references are **not** resolved here — that needs the command registry, and it happens at
registration and at plan building, as it does for Rust commands.

Unknown keys are ignored rather than refused, which matches existing behaviour but means a
misspelled field is silently dropped. Inside `hints` nothing can be checked at all, by construction.

## 8. What a language integration must do

The declaration is portable data; a callable is not. An integration performs a **handover**:

```
native declaration (JS object, Python kwargs, Starlark dict)
   │
   ├── the callable and anything else non-portable  →  the integration keeps it
   │
   └── the data part  →  declaration  →  merge, derive, build  →  CommandMetadata
```

Nothing non-portable crosses that line. What the integration does with the callable — how it
registers an executor, how it passes the state, how it invokes the function — is the integration's
own concern and is **outside this format**; `hints` is where it may record its answers.

An integration therefore owns: stage 1 (introspection), the handover, the callable, registration, and
dispatch. It shares: stages 2–4.

## 9. Worked example

A Python function, with the host discovering the signature:

```python
@command(label="Repeat text", arguments={"count": {"gui_info": {"IntegerSlider": {"min": 1, "max": 9}}}})
def repeat(state, count: int = 2, context=None):
    """Repeat the input text."""
```

**Stage 1** — introspection reports the parameters as it finds them, and recognises nothing:

```yaml
name: repeat
module: python
doc: Repeat the input text.
arguments:
  - { name: state }
  - { name: count, argument_type: int, default: !Value 2 }
  - { name: context }
```

**Stage 2** — the declaration merges over it. `label` is new; the `count` entry is matched by name
and augments the discovered argument, leaving its type and default untouched.

**Stage 3** — conventions apply. `state` is the first argument and is named `state`, so it becomes
`state_argument`; `context` is recognised and removed. Both leave a trace in `registration`:

```yaml
name: repeat
label: Repeat text
module: python
doc: Repeat the input text.
state_argument: { name: state }
arguments:
  - { name: count, argument_type: int, default: !Value 2,
      gui_info: !IntegerSlider { min: 1, max: 9 } }
registration:
  state: state          # the delivery mode — here, the State wrapper itself
  context: 2            # the parameter position to pass the context at
```

**Stage 4** — derived defaults: `count`'s label becomes `Count`, `cache` becomes `true`, `expires`
becomes `never`, `definition` becomes `Registered`. The command's label was declared, so derivation
leaves it.

**Stage 5** — validation passes. The resulting `CommandMetadata` has **one** argument, `count`. The
`registration` block does not reach it; the Python integration reads it from the declaration and
knows to call `repeat(state_value, count, context=ctx)`.

Two things to take from this. The discovered `int` and default `2` **survived** an entry that
mentioned neither — that is composition. And `state` and `context` never became command arguments,
so no query parameter is consumed by either — that is conventions.

## 10. Related documents

- [`REGISTER_COMMAND_FSD.md`](REGISTER_COMMAND_FSD.md) — the Rust compile-time counterpart
- [`COMMAND_REGISTRATION_GUIDE.md`](../guides/COMMAND_REGISTRATION_GUIDE.md) — registering commands in Rust
- [`LANGUAGE-INTEGRATION_GUIDE.md`](../guides/LANGUAGE-INTEGRATION_GUIDE.md) §COMMAND — what an integration must implement
- `specs/command_registry.yaml` — the exported metadata of every built-in command, in the same field vocabulary

## History

| Date | Change | Source |
|---|---|---|
| 2026-08-29 | Created. Not yet true at `HEAD`; promoted to `reference/` at Phase 5. | `design/command-declaration/` |
