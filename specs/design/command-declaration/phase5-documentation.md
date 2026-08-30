# Phase 5 — Documentation and outcome

## Completion Preconditions

All met before this document was written:

1. Steps 1-10 of `phase4-implementation.md` are complete; nothing was deferred.
2. Every command in the plan's testing table passes — see §Validation.
3. `git diff --exit-code specs/command_registry.yaml` is clean; the generated file did not move.
4. Every question the maintainer raised in review is answered in the design documents, and every
   correction is recorded rather than silently applied.
5. Both Phase 5 evidence items Phase 4 asked for are recorded: the conversion path step 8 selected,
   and where INT02 ended up living.

## Implementation Summary


A shared pipeline in `liquers-core::command_declaration` that turns an author's partial declaration
into a `CommandMetadata`, composing it over whatever the host discovered by introspection:

```
1. populate   host introspection fills what it can discover          host-specific
2. enhance    the author's declaration is merged over the baseline    shared
3. apply      conventions reinterpret the composed result             shared
4. fill       defaults are derived for whatever is still absent       shared
5. build      convert to CommandMetadata, or report what is wrong     shared
```

`liquers-web` now parses its declarations through stages 2-5, and about 150 lines of hand-written
`JsValue` parsing are gone: `get`, `get_string`, `get_bool`, `parse_arguments`,
`parse_argument_type`, `js_default_to_json`.

| Where | What |
|---|---|
| `liquers-core/src/command_declaration.rs` | New. `CommandDeclaration`, the by-name merge, `StateDelivery`, `Conventions`, the `Warning` channel, `derive_label`, build and validation |
| `liquers-core/src/command_metadata.rs` | Eight deserialize-only serde rows plus a permissive `Deserialize` for `CommandParameterValue`. No `Serialize` behaviour changed |
| `liquers-core/tests/command_declaration.rs` | New, with `fixtures/commands.yaml` — Phase 3's Example 2 verbatim |
| `liquers-web/src/command/spec.rs` | `parse` rewritten over the pipeline, plus `prepare_javascript_document` |
| `liquers-web/tests/commands_DECLARATION.rs` | New — the conversion spike, kept as a regression guard |
| `specs/reference/COMMAND_DECLARATION.md` | Promoted from this folder |

**Tests:** 51 unit and 5 integration in `liquers-core`, 3 in `liquers-web`. Nothing was excluded and
no existing test was changed to accommodate the work.

| Suite | Result |
|---|---|
| `liquers-core` lib | 720 |
| `liquers-core` integration | 5 |
| `liquers-lib` lib and all suites | 302 + all |
| `liquers-web`, wasm32 under Node | 144, including the 21-test COMMAND conformance suite |
| `specs/command_registry.yaml` | byte-identical |
| `scripts/check-build-matrix.sh` | 11 configurations |

## Deviations from the approved design

**Hints became two things.** An earlier draft used one `hints` key for facts about *calling* a
function, which collided with `ArgumentInfo::hints` — already documented as UI hints, already
round-tripping. They are separated: `hints` is metadata and survives export, `registration` is
declaration-only and is dropped at build.

**The state rule was inverted.** It began as "an argument *named* `state`/`value`/`text` is the
state", which made the name decide *whether* an argument was the state and left `def f(df, count)`
declaring a source command whose first query parameter bound to `df`. The first argument is now
always the state-derived argument, and its name selects only the delivery mode. That inversion is
what allows an unrecognised name to be a reserved extension point rather than an error.

**A warning channel was added**, unplanned. Three convention outcomes are silent decisions an author
cannot see in the document, and each warns rather than failing. Collected rather than printed, which
is a correctness point and not a preference: `liquers-web` is a wasm build where nothing reads
stderr, so a printed warning is lost, and a printed warning cannot be asserted on.

**`prepare_javascript_document` was added**, unplanned. JavaScript's label rules differ from the
shared ones at three points, and letting the shared derivation win would have moved every existing
JavaScript command's `metadata_version` and re-expired its dependent assets.

**`INFERRED_ARGUMENTS` was not removed**, contrary to Phase 2 and Phase 4. Both said the merge's own
rule carries what the thread-local tracked. It does not: the merge knows whether *this* declaration
spelled its arguments out, while the thread-local records that persistently per command key so
`describeCommand` can report `argumentsInferred` long after registration. Nothing else holds it and
removing it would break `COMMAND05`.

**`run` was dropped entirely.** Explored across three shapes — a callable, a name, a
`CommandDefinition::HostFunction` — and none survived. `CommandDefinition` already answers *which*
implementation; a callable cannot cross into portable data at all; and the `Alias` route would have
forced a command's metadata into the dispatcher's shape, discarding the per-argument metadata the
feature exists to carry. The reasoning is in Phase 2 §Rejected alternatives rather than deleted.

## Issues Filed

None was introduced by this work; all four were found by doing it.

| Issue | Priority | What |
|---|---|---|
| `MACRO-LEAVES-STALE-METADATA-VERSION` | P1 | `register_command!` stores a version computed from the bare skeleton before the macro fills in the label and arguments. Nothing recomputes it, so every macro-registered command's `metadata_version` reflects only its key. Invisible because the field is `#[serde(skip)]` |
| `ARGUMENT-GUI-INFO-HAS-THREE-DEFAULTS` | P2 | `TextField(20)` from the macro, `TextField(40)` from `any_argument`, `None` from serde |
| `STATE-ARGUMENT-CONSTRUCTOR-SERDE-DEFAULT-DISAGREE` | P2 | Constructing and deserializing the same command give different state arguments |
| `COMMAND-METADATA-HAS-NO-COMMAND-LEVEL-HINTS` | P3 | A usage hint can be attached to an argument but not to a command |

Two more were filed while designing: `JS-COMMAND-CANNOT-ACCESS-CONTEXT`,
`COMMAND-ALIAS-DEFINITION-UNTESTED`. One documentation gap:
`LANGUAGE-GUIDE-NO-DOCUMENTATION-SECTION`.

## Important Learning

**The spike earned its place, and nearly lied.** Phase 2 recorded `serde_wasm_bindgen`'s conversion
as its largest unverified claim, and Phase 4 hoisted the spike ahead of the rewrite it would affect.
It first failed for a fixture bug of mine — `js_sys::Array::from` copies, so a pushed element never
reached the object under test — and then for a wrong expectation, `json!(2.0)` where the library
correctly returns `Number(2)`. A spike that fails for its own bugs is how a sound design gets
abandoned; both failures had to be read rather than believed. The answer, once reached, was the good
one: the narrowing `js_default_to_json` did by hand is reproduced, so nothing re-versions and the
fallback path was not needed.

**Tests as specification worked.** The merge laws were written in Phase 3 before any code, and three
of them — a declared field not overwriting one it omits, `null` setting rather than deleting, a
declaration being unable to reorder — are cases a reasonable implementer would get wrong from the
prose alone. `CONV07` caught a genuine idempotence bug: after the first run recorded
`registration.state`, a second read its own recorded mode as a *declared* mode and consumed another
argument.

**Two regressions were caught by reading, not by tests.** The null-property filter (`label: null`
meant "no label", and handing that to serde is an error) and the `INFERRED_ARGUMENTS` claim were
both found by comparing the rewrite against the code it replaced. Neither would necessarily have
failed a test.

**The environment lied once too.** `check-build-matrix.sh` reported two `FAILED` configurations that
were simply the absent `wasm32-unknown-unknown` target — indistinguishable, in its output, from a
code failure. `CLAUDE.md` already tells the reader those rows need `rustup target add`; the script
could say so itself.

## Documentation Delivered

- **New:** `specs/reference/COMMAND_DECLARATION.md`, promoted from this folder unchanged but for
  its banner. Written before implementation deliberately, because the language-specific guides are
  to be built on it.
- **Updated:** `specs/reference/REGISTER_COMMAND_FSD.md` gains §The runtime counterpart, naming the
  one deliberate divergence and the test that holds the rest in agreement.
  `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` links resolve to `reference/`.
- **Closed:** `COMMAND-DECLARATION-FORMAT`, `ARGUMENT-DECLARATION-IS-ALL-OR-NOTHING`.
- **Not done, and filed:** the language guide still says nothing about the documentation an
  integration owes its own users — `LANGUAGE-GUIDE-NO-DOCUMENTATION-SECTION`.

## Conformance and Remaining Work

The design's acceptance criteria all hold. `CommandMetadata` deserializes from `{"name":"greet"}`;
`liquers-core` owns stages 2-5; field names agree because the output *is* `CommandMetadata`; the
registry round-trips byte-identically; a declaration and `register_command!` agree including
`metadata_version`; `JsCommandSpec::parse` is reimplemented with the conformance suite unchanged;
and a Python binding can deserialize the same document with no new parsing code.

### Remaining work

Nothing from the approved scope was deferred, so this design leaves no partial-implementation issue
behind. What remains is separate work, already filed:

- `MACRO-LEAVES-STALE-METADATA-VERSION` (P1) — the one worth acting on independently of this design.
- `LANGUAGE-GUIDE-NO-DOCUMENTATION-SECTION` — the guide section this reference is meant to be built
  on.
- `ARGUMENT-GUI-INFO-HAS-THREE-DEFAULTS`, `STATE-ARGUMENT-CONSTRUCTOR-SERDE-DEFAULT-DISAGREE` — the
  same class of defect on neighbouring fields, worth settling together.
- `JS-COMMAND-CANNOT-ACCESS-CONTEXT`, `COMMAND-ALIAS-DEFINITION-UNTESTED` — found while designing,
  neither blocking.

## Validation

```
cargo test -p liquers-core --lib                                 720 passed
cargo test -p liquers-core --test command_declaration              5 passed
cargo test -p liquers-lib --lib --tests                     302 + all passed
bash scripts/check-build-matrix.sh                        11 configurations
cargo test -p liquers-web --target wasm32-unknown-unknown \
           --features debug-handles                            144 passed
git diff --exit-code specs/command_registry.yaml                   no change
```

The wasm rows of the build matrix need `rustup target add wasm32-unknown-unknown`, and running the
`liquers-web` suite needs `cargo install wasm-bindgen-cli --version 0.2.127`. Neither was present in
this environment; both had to be installed, and the matrix reported the missing target as two bare
`FAILED` rows.

## What this does not do

No Python binding: `liquers-py` still has no Python-side registration path, and this design is the
prerequisite rather than the delivery. Nothing about how a host *calls* its callable is specified —
that was descoped deliberately, and survives only as uninterpreted registration hints. Query-valued
defaults are now expressible where the earlier draft would have lost them, but no command uses one.
