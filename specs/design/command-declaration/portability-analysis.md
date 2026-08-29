Validation evidence for [`phase2-architecture.md`](./phase2-architecture.md), prepared at the
approval gate. Nothing here is implemented.

# Portability analysis — how many languages can actually use this?

The design claims a *language-neutral* command declaration. This document tests that claim against
six host languages and reports where the claim holds, where it is thinner than it looks, and one
place where the design as specified would not serve Python.

**Method.** The design has three separable pieces of reuse, and they do not travel equally far:

| Piece | What it is |
|---|---|
| **M — the metadata format** | Part A: `CommandMetadata` deserializable from an author-written document |
| **C — the calling convention** | Part B: `CallingConvention` — state form, undeclared-async |
| **I — argument inference** | Deriving arguments from the callable itself |

For each language the question is which of the three it can use, not whether it "supports the
format". Evidence for the two languages that matter most (§Bar) is read from source; the four
others are assessed at the confidence stated in §Confidence.

## Prior art: the original Python decorator

`liquer/commands.py` (the pre-Rust implementation) is the strongest evidence available for what a
Python host actually needs, because it was built and used.

```python
def command(*arg, **kwarg):
    if len(arg) == 1:
        f = arg[0]
        if "ns" not in kwarg:
            kwarg["ns"] = "root"
        metadata = command_metadata_from_callable(f, attributes=kwarg)
        command_registry().register_command(f, metadata)
        return f
```

**Inferred from the callable:** `name` (`__name__`), `label` (`identifier_to_label`), `module`
(`__module__`), `doc` (`__doc__`), `version` (`callable_hash`), and the full argument list —
including type annotations, falling back to `type(p.default).__name__` when an annotation is absent.

**Supplied by the author:** `ns`, plus an open `**kwarg` → `attributes` dictionary that is *not
validated and largely not consumed* — only `ns` and `modify_command` are read.

Two absences matter to this design:

- **No async at all.** No coroutine detection, no `await`; the callable is invoked synchronously.
- **No value/text/state distinction.** There is `has_state_argument` (the difference between
  `@command` and `@first_command`), and `pass_state`, which is set only when the first parameter is
  literally named `state`. The three-way form the design treats as portable was never expressible.

So the Python precedent confirms the *shape* — infer the signature, declare the rest — while showing
that the "rest" was exactly the part that went unvalidated. That is the gap Part A closes.

## Per-language assessment

### JavaScript — uses M, C, I-specific. **Strong benefit.**

Verified from source. Inference is weak by necessity: `infer_arguments` (`spec.rs:281`) parses the
function's source text, requires every parameter to be a plain identifier, requires the token count
to equal `Function.length`, and **refuses** defaults, rest parameters, destructuring, and bound or
native functions. So the declaration is the primary path, not a fallback, and the host must be told
the state form and cannot always determine async ahead of the call (`IsAsync::Auto` exists precisely
because a plain function may return a Promise). JavaScript needs all of M and all of C.

### Python — uses M heavily, one field of C, none of I. **Clear benefit, differently distributed.**

`inspect.signature` gives parameter names, annotations, defaults and kinds *exactly*, and
`inspect.iscoroutinefunction` decides async deterministically. Python therefore needs:

- **M — yes, and this is the win.** Everything the old decorator dumped into an unvalidated
  `attributes` dict — `label`, `doc`, `namespace`, `cache`, `volatile`, `expires`, `filename`,
  `presets`, `next`, per-argument `gui_info` and enum types — becomes a typed, validated field set
  that agrees with Rust and JavaScript. `pythonize` or `serde-pyobject` deserializes the decorator's
  `**kwargs` dict straight into `CommandMetadata`, using the same passes `liquers-web` uses.
- **C — only `state`.** Whether the first parameter is the state, and in which form, cannot be
  inferred from a signature; `async` can be, so `is_async: None` is the permanent answer and the
  tri-state is never exercised.
- **I — no.** `inspect.signature` shares nothing with the JavaScript source parser, nor should it.

**The benefit is real but it is mostly Part A**, which is five serde attributes. That makes the
Python case cheap rather than weak.

### Rust, as an alternative to `register_command!` — uses M only. **Not a viable alternative.**

The macro's irreducible job is generating the *wrapper*: extracting each argument at its declared
Rust type, calling the function, converting the result. That is monomorphised per signature and
needs the types at compile time; Rust has no runtime reflection to recover them. A declaration read
at runtime can carry `ArgumentType::Integer` but cannot produce the `let x: i32 = …` that the
wrapper needs.

What is reusable is M: the macro already builds a `CommandMetadata`, and a lower-ceremony
front-end — a metadata literal plus a closure — becomes possible for commands whose arguments are
uniformly typed. That is a convenience, not a replacement, and it should not be sold as one.

### Starlark — uses M, C-minus-async. **Good fit; closest to the document model.**

Starlark is a Python dialect with parameter type annotations (`def fib(i: int) -> int:`), and it has
**no async or coroutines** — determinism and hermeticity are the point of the language. So the
async tri-state is dead weight there and `state` is the only convention field in play. Starlark has
no decorator syntax, so the natural registration form is `register_command({...}, fn)` — a dict
literal beside a callable, which is exactly the JavaScript shape and exactly the document model.

### Rhai — uses M, C-minus-async. **Fits the JavaScript model.**

`get_fn_metadata_list` returns metadata for script-defined functions including parameter names, and
`Engine::gen_fn_signatures` (behind the `metadata` feature) does the same for registered natives.
Rhai has no native `async`, so again only `state` is in play. Metadata would be supplied as an
object map.

### Rune — uses M and all of C. **Fits the JavaScript model, async included.**

Rune has first-class async — async functions, closures, blocks and generators — so unlike Starlark
and Rhai it is a second language where the async tri-state earns its place. Its introspection story
for script function signatures is the least certain of the six.

## Result

| Language | M — metadata | C — `state` | C — async tri-state | I — inference shared |
|---|---|---|---|---|
| JavaScript | ✔ | ✔ | ✔ | ✘ (source regex) |
| Python | ✔ **primary win** | ✔ | ✘ (`iscoroutinefunction`) | ✘ (`inspect.signature`) |
| Rust (macro) | ✔ (already) | compile-time | compile-time | ✘ (`syn`) |
| Starlark | ✔ | ✔ | ✘ (no async) | ✘ |
| Rhai | ✔ | ✔ | ✘ (no async) | ✘ |
| Rune | ✔ | ✔ | ✔ | ✘ |

Three conclusions, in order of how much they should affect the gate decision:

1. **The metadata half is portable to all six.** This is the part that is nearly free (five serde
   attributes) and it is the part every host uses. The design's value is concentrated where its cost
   is lowest.
2. **The calling convention is portable to five, but its two fields are unevenly justified.**
   `state` is needed by every dynamic host. `is_async: Option<bool>` is needed by two of six —
   JavaScript and Rune — because the rest either have no async or can decide it from the callable.
   The field earns its place, but this is an argument against ever growing the type, and it
   strengthens open question 8's additive-only rule.
3. **Argument inference is shared by none of them**, and should not be. Every host infers from its
   own reflection mechanism. This is worth stating because the issue's framing ("a Python binding
   must write all of it again") implicitly counts the inference code as duplicated work; it is not
   duplicated, it is genuinely per-language, and ~140 of `spec.rs`'s 389 lines are it.

## Bar: is there clear benefit for Python and JavaScript?

**JavaScript: yes, unambiguously** — it uses all three pieces of M and C, the declaration is its
primary registration path, and ~136 lines of hand-rolled parsing are replaced by shared code.

**Python: yes, but the benefit is Part A rather than Part B.** The precedent shows a Python host
supplying its non-inferable metadata as decorator kwargs into an unvalidated free-form dict. Sharing
`CommandMetadata` turns that into a validated field set that agrees across languages — which is the
drift argument, applied to the half of the problem Python actually has. A Python binding written
against this design writes a `**kwargs` → `CommandMetadata` conversion and a `state` field, and
nothing else.

The bar is met. But it is met by Part A far more than by Part B, and the design should say so rather
than implying a symmetric win.

## Finding: `arguments_declared` is too coarse for Python

The design treats declared and inferred arguments as **mutually exclusive** — `arguments_declared`
is one boolean, and `liquers-web` runs `infer_arguments` only when no `arguments` key is present.
That matches JavaScript, where inference is all-or-nothing and refuses anything it cannot parse
exactly.

It does not match Python. The natural Python decorator infers names, types and defaults from the
signature *and* wants the author to augment individual arguments with what a signature cannot
carry — a label, a `gui_info` widget hint, an enum domain:

```python
@command(label="Repeat text", arguments={"count": {"label": "Repeat count", "gui": ...}})
def repeat(state, count: int = 2): ...
```

Under the design as specified, supplying `arguments` at all would switch inference off and force the
author to restate every argument in full — losing the type and default information that
`inspect.signature` had for free. The same limitation would bite Starlark and Rhai.

This is not a blocker: a Python binding can merge before handing the metadata over, and the shared
type is unaffected. But **per-argument merge is the natural semantics for three of the six
languages**, and the boolean forecloses expressing it in the shared layer. Filed as
[`ARGUMENT-DECLARATION-IS-ALL-OR-NOTHING`](../../issues/ARGUMENT-DECLARATION-IS-ALL-OR-NOTHING.md).

## Confidence

| Claim | Basis |
|---|---|
| JavaScript inference is weak and refuses many shapes; declaration is primary | read at `HEAD` (`spec.rs:281-344`) |
| `liquers-py` has no Python-side registration today; it registers Rust functions | read at `HEAD` (`liquers-py/src/commands.rs:187-190`) |
| The old decorator's inferred/declared split, its lack of async, its binary state flag | read from `liquer/commands.py` at `master` |
| Rust cannot build a typed wrapper from a runtime declaration | follows from the absence of runtime reflection; `registration.rs`'s per-type extractors read at `HEAD` |
| Starlark has no async; parameters carry type annotations | `starlark-rust/docs/types.md`; the absence of async is a language property, not a version detail |
| Rhai exposes script function metadata incl. parameter names; has no native async | Rhai book, `fn-metadata` and `gen_fn_sig` pages |
| Rune has first-class async | Rune project documentation |

**Not verified, and worth checking before leaning on these two rows:** whether a Rust host can read a
Starlark `def`'s parameter spec at runtime (the `starlark::docs` module exists for documentation
generation; whether it covers a live `def` value was not confirmed — `docs.rs` is blocked from this
environment), and whether Rune exposes a stable reflection API for script function signatures.
Neither affects the M column, which is where the benefit is; both affect only whether that language
could infer arguments instead of declaring them.
