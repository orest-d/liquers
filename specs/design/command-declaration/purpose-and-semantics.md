Draft of the API documentation for `CommandDeclaration`, plus a critical evaluation of it.
Prepared at the approval gate from the maintainer's purpose statement of 2026-08-29.
Nothing here is implemented. Every question raised in §Evaluation was settled by the maintainer on
2026-08-29 and the answers are recorded in §Decisions; `phase2-architecture.md` is rewritten against
them.

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

**The goal is plainly stated:** code that is reusable for Python and JavaScript language support,
and possibly some others. It is not a claim to universal portability. A Rust command is declared by
`register_command!`, which has the function's types at compile time and needs nothing at runtime, so
Rust is not a target. The plain-document case — a `commands.yaml` beside the environment
configuration — is served by the same type with no host language present at all, and is the case
with *no* introspection to compose over.

### In one sentence, and what that leaves out

*"A function that converts loosely-defined JSON into `CommandMetadata`."* That is close enough to be
useful, and it is the right mental model. Two corrections keep it from being misleading:

- It converts **more than one** input. The interesting call is not `json → metadata` but
  `(introspected baseline, author declaration) → metadata`. Composition, not parsing, is the
  substance; a single-input version of this would be an afternoon's work.
- "Loosely defined" is not a weakness to be tolerated — it *is* the deliverable. Being able to write
  `{"name": "to_text"}` and get a complete, valid `CommandMetadata` is the whole product.

### Added value, stated honestly

Ranked by how much of the case they carry:

1. **Composition over introspection.** The only part no host gets for free. Without it each host
   either writes its own merge or does what `liquers-web` does today — declared *or* inferred, never
   both — so an author who wants to label one argument must restate all of them. The rules
   (by-name argument merge, absence versus default, order from the baseline) are more valuable than
   the code that implements them, because their worth is that every host agrees on them.
2. **One vocabulary instead of N.** Which type names are accepted, what a missing field means, what
   an author is told when a declaration is wrong. Two hand-written parsers drift here immediately,
   and the drift is invisible until someone moves a declaration between languages.
3. **Defaults derived once.** Labels from names, `gui_info`, everything `CommandMetadata::from_key`
   already knows. Small code; the value is that Python and JavaScript produce identical metadata for
   identical input.
4. **`CommandMetadata` becomes partially specifiable at all.** Today it is not: four fields plus
   `ArgumentInfo::label` lack `#[serde(default)]`, so `{"name":"greet"}` is rejected. Five
   attributes, and nothing else here works without them — worth doing on its own as a latent-defect
   fix.

### What it is *not*

**It does not save code.** About 136 lines leave `liquers-web`; about 300 enter `liquers-core`. The
net is *more* code. What it saves is **divergence**: those 300 lines are written once, tested once,
and behave identically everywhere, instead of being written again, slightly differently, per binding.

**It does not make a language binding easy.** A Python binding still needs introspection,
callable handling, registration and dispatch — none of which this touches. It covers the metadata
slice, which `portability-analysis.md` measured as the slice Python benefits from most, but it is a
slice.

**It is not a hard technical problem.** It is a small amount of code whose value is coordination.

### The test this design has to pass — **passed**

Because the value is coordination rather than capability, it is contingent on there being at least
two consumers. With only `liquers-web` this would be a net loss, and the right change would be the
five serde attributes alone.

**Answered by the maintainer, 2026-08-30: Python *and* JavaScript support are both real, and
supporting both is likely the next major development goal.** That is the two-consumer case directly,
so the coordination value is concrete rather than speculative, and the design is justified on its
stated grounds. The plain-document host remains a third beneficiary and no longer has to carry the
argument alone.

Two things follow, and they are recorded rather than left as inference:

- **The `P0` on `COMMAND-DECLARATION-FORMAT` stands.** It was recorded as deliberate scheduling
  weight against `DOCS_STRUCTURE_GUIDE.md` §4.4, which reserves `P0` for wrong results, data loss,
  panics and broken documented features. It is still none of those, so the tension the issue file
  notes is real — but "prerequisite for the next major development goal" is now a fact rather than a
  projection, which is what the weight was claiming. Phase 1's Q3 is resolved in favour of leaving it.
- **Staying at `L` with the `liquers-project` workflow is right.** Phase 2's open question 4 asked
  whether the descope put this back at the M/L boundary. Two bindings landing on this code answers
  it: the merge, the defaulting rules and the diagnostics are what both will inherit, and they
  warrant Phase 3 examples and tests rather than being folded into an implementation commit.

Three filed issues also become more pressing, though none blocks this work:
`JS-COMMAND-CANNOT-ACCESS-CONTEXT` (Python will want context too, and the asymmetry becomes visible
across both bindings), `LANGUAGE-GUIDE-NO-DOCUMENTATION-SECTION` (two new user-facing bindings mean
two user guides), and `POST-INIT-COMMAND-REGISTRATION` (for a document-driven host, registering after
`to_ref` is the normal case rather than the exception).

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
1. populate   host introspection fills what it can discover          (host-specific)
2. enhance    the author's declaration is merged over it             (shared)
3. fill       defaults are derived for whatever is still absent      (shared)
4. use        convert to CommandMetadata + call spec;                (shared)
              report or error on missing required data
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

### Hints, and what is out of scope

**How to call the function is out of scope** (decision, 2026-08-29). It is the part that fought
portability: which form of the state a callable wants, whether a variadic arrives spread or
collected, whether the result is awaited — each is meaningful in some hosts and meaningless in
others, and typing it in `liquers-core` forces every host to agree on notions some of them do not
have. What the host does with its callable — register it, wrap it, invoke it — is likewise the
host's business.

What remains is a place to **collect** such facts without interpreting them: a free `hints`
dictionary, mirroring the one `ArgumentInfo` already has (`command_metadata.rs:399-403`). A host
writes what it needs and reads it back; `liquers-core` only carries and merges it. No hint key is
reserved or validated, and the vocabulary is expected to grow as integrations need it.

**Decided 2026-08-30: hints live on the declaration only** and are dropped when the metadata is
built, so `CommandMetadata` stays a precise specification of the command and says nothing about how
to call it. The cost, accepted deliberately: hints do not survive export, so an integration that
replays registrations must retain the declaration rather than the metadata.

```yaml
name: repeat
arguments: [{ name: count, type: int }]
hints:
  javascript: { state: text, variadic: spread }
```

### The handover boundary

A host's native declaration is not portable data — a JavaScript object holds a `js_sys::Function`, a
Python decorator's kwargs hold a callable. The host splits its native structure: the callable and
anything else non-portable stays with the host, and the data part becomes a `CommandDeclaration`.
`liquers-web` already does this, stripping `run` before parsing (`spec.rs:130-140`). Nothing
non-portable crosses the boundary, which is what keeps the shared layer shared.

`CommandDeclaration` is a **type**, not a JSON convention, though it holds `serde_json::Value`
internally — so a PyO3 or `wasm-bindgen` binding can expose the object and let a host build it up
incrementally rather than assembling JSON by hand.

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

## Part 3 — Decisions (maintainer, 2026-08-29)

| # | Question | Decision |
|---|---|---|
| C1 | Scope | **Agreed.** Re-scoped `M → L`; `design/command-declaration/` adopts `liquers-project`. |
| C2 | Merge representation | **Agreed with the recommendation:** deep merge at the serialized level. |
| C3 | Keyword argument passing | **Out of scope for now.** Positional only. |
| C4 | Order of operations | **Agreed.** The four stages above are normative. |
| C7 | Portability framing | **Restated:** the goal is code reusable for Python and JavaScript support, and possibly some others. Not a universality claim. |
| Q1 | Representation | Recommended option accepted (same as C2). |
| Q2 | May a declaration remove? | **No removal.** See below. |
| Q3 | Unknown argument names | **Reject.** |
| Q4 | Per-argument or per-command passing | Moot — C3 removes the dimension. |
| Q5 | Retroactive label rule | **Not necessary.** Rust function names are normally snake case and the capitalisation is cosmetic; the new rule applies to the declaration path only. |
| Q6 | Procedure | Agreed with C1. |
| Q1' | `run` and `CommandDefinition` | **Out of scope entirely.** A callable cannot cross into portable data, and registration is host-specific. Superseded by the descope below; the `HostFunction`/`Alias` analysis is kept in Phase 2 §Rejected alternatives. |
| — | **Descope, 2026-08-29** | Defining *how to call the function* is out of scope. The declaration's job is the originally stated one: an ergonomic author-facing format that fills defaults and produces `CommandMetadata`. Call-related facts survive as **uninterpreted hints**; the vocabulary is not designed now and grows as needed. |

### Consequences worth stating

**Q4 is answered by C3, not independently.** Dropping keyword passing removes the dimension that
per-argument-versus-per-command was about. Nothing per-argument remains in the call specification, so
the call spec is per-command: state form, variadic passing, asynchrony. Recorded so that a later
reader does not reinstate a per-argument passing mode believing it was already decided.

**No removal is the right call, and it costs nothing, because stage 1 belongs to the host.** The
exotic case raised against it — a function parameter with a default that the command should not
expose — is handled where it arises: the host's introspection simply does not emit that parameter
into the baseline. The shared layer never needs a deletion marker, and `null` therefore stays an
ordinary value rather than becoming a sentinel. This is worth keeping in the API doc, because "how do
I hide a parameter?" is a question that will be asked.

**Rejecting unknown argument names needs one exception, and it is the plain-document case.** With no
introspection there is no baseline to validate against and every name is "unknown". The rule is
therefore conditional on the baseline *having* an argument list at all: an absent `arguments` key in
the baseline means discovery did not run, and the declaration establishes the list; a present
`arguments` key — including an empty one, meaning a function with no parameters — makes the
declaration's entries subject to the reject rule. The serialized-level merge gives this distinction
for free, since absence is key-absence; a typed representation would have needed a separate flag.

