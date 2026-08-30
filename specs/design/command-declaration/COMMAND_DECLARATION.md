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
3. fill       defaults are derived for whatever is still absent       shared
4. build      convert to CommandMetadata, or report what is wrong     shared
```

**Introspection is the basis, not a fallback.** Python's `inspect.signature` yields parameter names,
type annotations and defaults exactly; JavaScript's source parse yields names only; a YAML document
has no introspection at all and starts from an empty baseline. Whatever a host can discover, it
should — and the author then supplies only what discovery cannot know: a label, a documentation
string, a namespace, a widget hint, an enum domain.

The consequence for an author is the property worth remembering: **you write only the difference.**
Adding a label to one argument does not require restating that argument's type or default.

Stages 2–4 behave identically in every language. Only stage 1 differs, which is why they are shared
code and stage 1 is not.

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

## 3. Mapping to `CommandMetadata`

Every declaration key is a `CommandMetadata` field, with **one exception**: `hints` (§5), which is
declaration-only and does not reach the metadata. Apart from that the declaration adds no vocabulary
of its own, so a field added to `CommandMetadata` is declarable immediately with no change here.

### 3.1 Command level

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
| `hints` | *(none)* | **Declaration-only**, read by the integration and dropped at build — §5 |

Not declarable: `metadata_version` and `impl_version`. Both are computed — the first from the stored
metadata content, the second supplied at registration.

### 3.2 Argument level

| Key | `ArgumentInfo` field | Notes |
|---|---|---|
| `name` | `name` | **Required.** Also the merge key |
| `label` | `label` | Derived from `name` when absent — §4 |
| `type` | `argument_type` | `type` is an accepted spelling of `argument_type` |
| `default` | `default` | §3.3 |
| `multiple` | `multiple` | Variadic: consumes every remaining action parameter. Must be last |
| `injected` | `injected` | Supplied from the context, never from the query |
| `gui_info` | `gui_info` | Preferred entry widget |
| `hints`, `presets` | same | |

**Argument types.** The canonical names are `string`, `int`, `int_opt`, `float`, `float_opt`,
`bool`, `any`, `none`, plus the enum forms. These aliases are also accepted, so a declaration written
against a host's own vocabulary parses: `str` and `text` for `string`, `integer` for `int`, `number`
for `float`, `boolean` for `bool`.

### 3.3 Argument defaults

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

## 4. How defaults are created

Stage 3 fills what is still absent after the merge. It **never** overwrites a value that discovery or
the declaration supplied, and it is idempotent.

Ordering is normative: **merge, then derive, then build.** Deriving before merging would make a
derived value indistinguishable from a declared one and would block the author's own.

### 4.1 Labels

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

### 4.2 Other defaults

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

## 5. Hints

Some facts about a command are neither metadata nor portable — which form of the input state a
callable wants, whether a variadic reaches it spread or as one list, whether the result must be
awaited. Each is meaningful in some languages and meaningless in others.

`hints` is a free dictionary for exactly these, and it is **the one declaration key that is not a
`CommandMetadata` field**. It is composed like any other map through the merge, and the integration
reads it from the declaration — but `build` **drops it**. `CommandMetadata` stays a precise
specification of the command and says nothing about how to call it.

```yaml
name: repeat
arguments: [{ name: count, type: int }]
hints:
  javascript: { state: text, variadic: spread }
```

Namespace hint keys by integration, so two hosts declaring the same command cannot collide. No key
is reserved and none is validated; the vocabulary grows as integrations need it. Because nothing is
validated, a misspelled hint key is silently ignored — an integration that cares should check for
the keys it expects.

**Hints do not survive export.** A registry exported to `command_registry.yaml` carries metadata
only, so an environment rebuilt from an exported registry cannot recover how to call a declared
command. **An integration that replays registrations must retain the declaration, not the
metadata.** This is what `liquers-web` already does — `REGISTERED_SPECS` retains the declaration for
exactly this reason — so the requirement is not new, but it is now a rule rather than an accident.

## 6. Validation

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

## 7. What a language integration must do

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

## 8. Worked example

A Python function, with the host discovering the signature:

```python
@command(label="Repeat text", arguments={"count": {"gui": "int_slider"}})
def repeat(state, count: int = 2): ...
```

**Stage 1** — introspection produces the baseline:

```yaml
name: repeat
module: python
doc: ""
state_argument: { name: state }
arguments:
  - { name: count, argument_type: int, default: !Value 2 }
```

**Stage 2** — the declaration merges over it. `label` is new; the `count` entry augments the
discovered argument by name, leaving its type and default untouched:

```yaml
name: repeat
label: Repeat text
module: python
state_argument: { name: state }
arguments:
  - { name: count, argument_type: int, default: !Value 2, gui_info: int_slider }
```

**Stage 3** — derived defaults fill the rest: `count`'s label becomes `Count`, `cache` becomes
`true`, `expires` becomes `never`, `definition` becomes `Registered`. The command's own label is
already set, so the derivation rule does not touch it.

**Stage 4** — validation passes and a `CommandMetadata` is produced. Had the declaration named
`"cnt"` instead of `"count"`, stage 4 would have rejected it rather than adding a second argument.

## 9. Related documents

- [`REGISTER_COMMAND_FSD.md`](REGISTER_COMMAND_FSD.md) — the Rust compile-time counterpart
- [`COMMAND_REGISTRATION_GUIDE.md`](../guides/COMMAND_REGISTRATION_GUIDE.md) — registering commands in Rust
- [`LANGUAGE-INTEGRATION_GUIDE.md`](../guides/LANGUAGE-INTEGRATION_GUIDE.md) §COMMAND — what an integration must implement
- `specs/command_registry.yaml` — the exported metadata of every built-in command, in the same field vocabulary

## History

| Date | Change | Source |
|---|---|---|
| 2026-08-29 | Created. Not yet true at `HEAD`; promoted to `reference/` at Phase 5. | `design/command-declaration/` |
