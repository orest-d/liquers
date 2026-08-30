Draft of the API documentation for `CommandDeclaration`, plus a critical evaluation of it.
Prepared at the approval gate from the maintainer's purpose statement of 2026-08-29.
Nothing here is implemented, and §Evaluation raises questions that must be settled before
[`phase2-architecture.md`](./phase2-architecture.md) can be rewritten against it.

# Purpose and semantics of a command declaration

## Part 1 — Specification (draft API doc)

### What it is

A **command declaration** is the runtime equivalent of `register_command!`: it says how a *function*
becomes a *command*. It is not a serialization of `CommandMetadata`, and the two answer different
questions.

| | `CommandDeclaration` | `CommandMetadata` |
|---|---|---|
| Describes | how a function implements a command | the command itself |
| Audience | the command author | the planner, the UI, the registry |
| Nature | ergonomic, partial, additive | precise, authoritative, complete |
| Role | input | result |
| Analogue | `register_command!` | what the macro emits |

A declaration is **language-neutral but function-facing**: it is the shared core that each
language-specific declaration form — a Python decorator's keyword arguments, a JavaScript object
literal, a Starlark dict — reduces to. `CommandMetadata` is language-agnostic in the stronger sense
that it describes commands without reference to any implementing function at all.

The declaration targets **dynamic hosts**. A Rust command is declared by `register_command!`, which
has the function's types at compile time and needs nothing at runtime. The plain-document case (a
`commands.yaml` beside the environment configuration) is served by the same type with no host
language present at all.

### The two things it carries

1. **A metadata contribution** — a *partial* `CommandMetadata`: whatever the author supplies that
   the host could not discover for itself. Labels, documentation, namespace, caching, expiration,
   presets, widget hints, enum domains.
2. **A call specification** — how to invoke the function once the plan has resolved the parameter
   values. This has no home in `CommandMetadata` by design, because it is about the function, not
   the command.

### Composition: a declaration adds to what is already known

A declaration is not a complete description and is not meant to be read alone. The normal flow is:

```
introspection  ──►  merge declaration over it  ──►  derive defaults  ──►  validate  ──►  CommandMetadata
(host-specific)     (shared)                       (shared)              (shared)
```

The host discovers what it can from the callable — Python's `inspect.signature` gives names, type
annotations, defaults and parameter kinds; JavaScript's source parse gives names only. The
declaration then **adds to and overrides** that, field by field, including inside nested structures:
an author may attach a widget hint to a single argument without restating that argument's type or
default.

Merge is therefore the central operation, and these are its rules:

- **Scalars**: a value present in the declaration overrides the discovered one. *Absent* and
  *default-valued* must be distinguishable — a declaration that says nothing about `cache` must not
  overwrite a discovered `cache`.
- **Maps**: merged key by key, recursively.
- **Argument lists**: merged **by `name`, never by position**. An entry naming a discovered argument
  augments it; an entry naming an unknown one adds it. Discovery establishes the order; added
  arguments append, because Liquers binds query parameters positionally and reordering would
  silently rebind them.

### Derived defaults

After merging, fields still empty are filled by rule rather than left blank. In particular a
**label is derived from the name**: `snake_case` and `camelCase` are both broken into
space-separated words and the first word is capitalised, so `to_text` and `toText` both yield
`To text`. The same rule derives argument labels from argument names.

Derivation runs **after** the merge, never before, and only fills what is still empty — otherwise a
derived value would be indistinguishable from a declared one and would block it.

### The call specification

**State.** Whether the function receives the input state, and in which form: not at all (a source
command), the converted value, its text form, or the `State` itself with its metadata.

**Argument passing.** Liquers arguments are positional by nature but always carry a name, so they can
be passed either way. A declaration says which, per argument: as part of the positional list, or by
keyword. Python needs this — an injected argument is naturally keyword-only — and Python can also
discover it, since `inspect.Parameter.kind` distinguishes positional-only, positional-or-keyword and
keyword-only.

**Variadic passing.** A `multiple` argument consumes every remaining action parameter. The
declaration says how the resulting sequence reaches the function: **spread** across the call as
individual arguments, or **collected** as one list argument.

**Asynchrony.** Whether the result is awaited. A host that can determine this from the callable
(Python's `inspect.iscoroutinefunction`) never needs it declared; one that cannot (JavaScript, where
a plain function may still return a Promise) does.

---

## Part 2 — Evaluation

### What this corrects in the current Phase 2

**The framing was wrong, and it was my recommendation that made it wrong.** Phase 2 argues that
`CommandMetadata` *is* the declaration format and that a separate type is a needless mirror. Under
this purpose statement that is only half right, and the wrong half matters:

- The rejection of a parallel type was aimed at a **mirror** — a struct re-listing `label`, `doc`,
  `namespace` and the rest, which duplicates fields and drifts. That objection stands.
- It does **not** reach a declaration that is *shaped differently*: a partial overlay plus a call
  specification. That is not a mirror; it is a different kind of thing whose output happens to be
  metadata. My earlier argument does not apply to it, and I should not have written Phase 2 as
  though the only alternatives were "reuse the struct" or "clone the struct".

The way to keep the anti-duplication concern while accepting the framing: **the declaration must not
re-enumerate metadata fields.** It carries them as a partial, so adding a field to `CommandMetadata`
never requires touching the declaration.

**Part A's justification changes, but Part A survives.** Its stated purpose — "so `CommandMetadata`
can be the declaration format" — is retired. But the merge requires a partial `CommandMetadata`, and
"every field has a defined behaviour when absent" is exactly what those five serde attributes
establish. Part A becomes a prerequisite of the merge rather than the feature itself, and it remains
worth doing on its own terms as a latent-defect fix.

**Part B is too small by a factor of about three.** `CallingConvention` as specified carries `state`
and `is_async`. The call specification above adds per-argument passing mode and variadic passing
mode, and the portability analysis' finding about all-or-nothing arguments turns out to be a
symptom of the missing merge rather than a separate defect.

### Where I agree without reservation

- **Declaration ≈ `register_command!`, not ≈ metadata.** This is the sharper framing and it explains
  something Phase 2 could not: why the residue kept feeling arbitrary. It was arbitrary because it
  was defined by subtraction.
- **"Describing commands rather than functions implementing the commands"** is the cleanest
  statement of the split available, and it belongs verbatim at the top of the API doc.
- **Merge over introspection is the core operation.** This is right and it is the part with real
  engineering content. It also independently confirms
  `ARGUMENT-DECLARATION-IS-ALL-OR-NOTHING`.
- **Better label derivation.** Today the rule is `name.replace("_", " ")` in eight places
  (`command_metadata.rs:417,440,453,466,487,508,893,925`) — no capitalisation, no camelCase. The
  registry's readable labels (`To text`, `Commands documentation`) are all hand-written. Deriving
  `To text` from both `to_text` and `toText` is a genuine improvement, and camelCase specifically
  matters for JavaScript and for Python method-style naming.
- **Variadic passing is a real gap.** Rust has only one mode: `get_multiple` always collects into
  `Vec<T>` (`commands.rs:151`). A dynamic host genuinely has both, and nothing today can express the
  difference.

### Concerns

**C1 — Scope. This is no longer an `M`.** The declaration now needs a merge algebra with
absence-tracking and name-keyed list merging, a defaults-derivation rule set, and a four-dimensional
call specification. That is `L`, which under `DOCS_STRUCTURE_GUIDE.md` §4.5 requires a design folder
— it has one — but it also means the simplified two-phase procedure this design is running under is
no longer the right fit. **Recommendation:** re-scope `COMMAND-DECLARATION-FORMAT` to `L` and
either adopt `liquers-project`, or split: the merge machinery is the substance, and Part A plus a
minimal call spec would still unblock the JavaScript rewrite.

**C2 — Absence-tracking forces a representation decision, and it is the crux.** To merge, "the
author said nothing about `cache`" must be distinguishable from "the author said `cache: false`".
`#[serde(default)]` collapses exactly that distinction, so Part A as written is *not* sufficient for
merge. Three options:

| Option | Assessment |
|---|---|
| Merge at the serialized level: introspection produces a JSON object, the declaration is a JSON object, deep-merge, then deserialize once | **Recommended.** Nesting and name-keyed argument merging fall out naturally, absence is simply key-absence, and it works for every host that can produce a JSON-shaped value — all six languages. Cost: an intermediate `serde_json::Value`, and merge errors are reported against JSON rather than typed fields |
| A mirror struct with `Option` on every field | This *is* the mirror Phase 2 rejects, with the drift problem intact |
| Hand-written `Deserialize` tracking presence per field | All the cost of the mirror plus a hand-written impl over 20-odd fields |

If the serialized-level merge is chosen, **the shared artifact is a merge function, not a struct** —
which is a different deliverable than Phase 2 describes, and a smaller one in type surface.

**C3 — The call specification is not uniformly portable, and the design should say so rather than
pretend.** Keyword passing is meaningful in Python and Starlark, meaningless in JavaScript (which has
no keyword arguments; `infer_arguments` already refuses destructured parameters), and irrelevant in
Rust. Variadic spread-versus-collect is meaningful in Python and JavaScript, fixed in Rust. So parts
of the call spec are inert or unrepresentable in some hosts. **Recommendation:** keep each dimension
a small closed enum, document per-language interpretation, and require a host that cannot honour a
declared mode to **fail at registration with a clear message** — never to silently ignore it. The
alternative, a general parameter-passing mechanism, would turn a portable type into a Python
emulator that JavaScript cannot implement.

**C4 — Order of operations must be normative, not incidental.** Merge, then derive, then validate.
Deriving before merging makes a derived label look "present" and block a declared one; validating
before merging rejects declarations that the merge would have completed. This is the classic bug in
layered-configuration systems and it should be stated in the API doc, not left to the implementation.

**C5 — Merge semantics need decisions that the purpose statement leaves open.** Listed as questions
below rather than assumed.

**C6 — Validation changes shape.** Phase 2's headline test — `command_registry.yaml` round-trips
byte-identically — tests metadata serde, not the declaration. A declaration-centric suite tests
**merge laws**: an empty declaration is an identity; merging twice equals merging once; a declared
scalar wins over a discovered one; an argument entry augments by name and never by position; a
declaration naming an unknown argument appends rather than reorders. These are cheap, exhaustive and
much closer to what can actually go wrong.

**C7 — A small tension in the purpose statement itself.** "Command declaration targets mainly dynamic
languages" and "declaration is the basis for language-specific declaration" together read as though a
declaration always sits under a host language. The original issue's motivating case — two documents,
one configuring the environment and one declaring commands — has no host language at all, and is the
case with *no* introspection to merge over, so every field must be declarable. That case should be
named explicitly as supported, or the merge design will quietly assume a discovery step that is not
always there.

### Questions to settle before Phase 2 is rewritten

1. **Representation for merge** — serialized-level deep merge (recommended, C2), or a typed partial?
   This decides whether the deliverable is a function or a struct.
2. **May a declaration remove** a discovered argument or field, or only add and override? Removal
   needs an explicit null-versus-absent convention, and the two are easy to confuse in YAML.
3. **Unknown argument names** — append (as specified above), or reject? Appending is friendlier;
   rejecting catches typos, which in a positional system silently misbind.
4. **Is argument passing per-argument or per-command?** Python's injected-keyword case needs
   per-argument; a per-command default with per-argument override is more code. Recommendation:
   per-argument, defaulting to positional.
5. **Does the label derivation apply retroactively** to commands registered by `register_command!`?
   Changing the rule changes `metadata_version` for every command relying on the default, which
   re-expires dependent assets. Recommendation: new rule for the declaration path only, and treat
   unifying it as a separate, deliberate change.
6. **Scope and procedure** (C1) — re-scope to `L` and adopt `liquers-project`, or split so the
   JavaScript rewrite is not blocked behind the merge machinery?
