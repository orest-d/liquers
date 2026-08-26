---
id: FOREIGN-VALUE-TYPE-REGISTRATION-PHASE1
kind: design
title: "Phase 1: High-level design — foreign and Python value types in the type registry"
status: in_review
phase: high-level
area: [core/value, lib/value, web, py]
created: 2026-08-26
---
# Phase 1: High-Level Design — Foreign Value Type Registration

## Feature Name

Foreign value type registration (fixes `FOREIGN-VALUE-TYPES-NOT-REGISTERED` and
`PY-VALUE-TYPE-DESCRIPTIONS-MISSING`)

## Purpose

A value whose type identifier is only known at *runtime* — a JavaScript, Python or Starlark handle
held in `ExtValue::Foreign` — cannot appear in the `TypeRegistry`, which is seeded from the
**static** `ValueInterface::type_descriptions()`. Since `value-type-system` step 6 the write path
refuses any identifier the registry does not contain, so such a value cannot be stored at all. This
design gives an integration a way to declare its type, and applies the same treatment to
`liquers-py`, whose `Value` describes no types at all.

**Confirmed against a build** (2026-08-26, native, mock `ForeignValue` returning `js.Value`):
`AssetManager::set_state` fails with `[General] Type identifier 'js.Value' is not registered in
this build`. The issue's "not verified" caveat is now settled.

## The governing rule: one identifier, one variant

**A type identifier corresponds one-to-one with a value variant.** `ExtValue::Foreign` is a *single*
variant, so it has exactly *one* `TypeInfo` — in a `liquers-web` build, `js.Value`. This settles the
issue's open choice: **there is no provider *family* entry** covering `js.*`, and no per-class
runtime typing such as `py.numpy.ndarray`. What varies per instance is `type_name`, which is
informational and never dispatched on — `JsOpaque` already does exactly this, reporting a constant
`js.Value` identifier and the JavaScript `constructor.name` as `type_name`.

Three consequences, all of which Phase 2 must honour:

1. **The identifier is a property of the `ForeignValue` implementation, not of an instance.** It
   must therefore be obtainable *without a value*, because registration happens at environment
   construction when no foreign value exists yet. `ForeignValue::identifier(&self)` is today an
   instance method; it needs a static counterpart, and the two must be proven to agree.
2. **One build carries one foreign implementation.** Two integrated languages in one build would be
   two identifiers for one variant, which the rule forbids; that case needs a second variant and is
   out of scope here.
3. **`liquers-py` is the last place the old many-to-one model survives.** Its `Value` maps `None`,
   `Bool`, `I32`, `I64`, `F64` and `Array` all onto `"generic"` — precisely the collapse
   `value-type-system` step 3 removed from `liquers-core::Value`. Its `Py` variant is its foreign
   variant and gets one identifier, `py.Object`.

### The rule is nowhere stated, and one formulation contradicts it

The rule is *enforced* — `type_descriptions_match_identifier` (`liquers-core/src/value.rs:1155`)
asserts "one description per variant, no more and no less" — but never *written down*:

| Where | Says | Needs |
|---|---|---|
| `liquers-core/src/value.rs:230` | "Several types can be linked to the same identifier." | **Wrong.** The doc comment on the very trait method now contradicts the test in the same file. Replace it. |
| `specs/reference/VALUE_TYPE_SYSTEM.md` | Cardinality "exactly one"; local name "normally the value variant's name" | Implies the rule; does not state it. State it, and add `Foreign`/`py.*` to the registered-identifier list. |
| `specs/guides/TYPE_SYSTEM_GUIDE.md` §2 | How to choose bare vs prefixed | Add: one identifier per variant, and how a runtime-typed variant declares itself. |
| `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §VALUE | "**Registration is an open problem**" | Becomes the procedure. |

Clarifying the formulation is part of this design's deliverable, not a side effect.

## Registry lifecycle: extend a base, then freeze at construction

**The registry stays essentially constant and is fixed once the environment is constructed.** It is
not mutated afterwards, and the `Environment` trait gains nothing — it keeps only
`get_type_registry(&self)`. An integration instead **builds on top of the existing registry**: it
takes the core or library registry, adds its own type, and hands the finished registry to the
environment constructor.

```rust
let mut types = TypeRegistry::from_value_type::<Value>();  // everything core and lib describe
types.register(JsOpaque::type_info())?;                    // one more: js.Value
let env = DefaultEnvironment::<Value>::new_with_type_registry(types);
```

`TypeRegistry::from_value_type::<V>()` and `register` both exist already; what is missing is the
constructor that accepts a registry. `new()` keeps seeding from the value type, so no existing call
site changes and `Default` is unaffected.

Three consequences for Phase 2:

- **Six constructors, not one.** `SimpleEnvironment`, `ImmediateEnvironment`,
  `SimpleEnvironmentWithPayload`, `ImmediateEnvironmentWithPayload` (`liquers-core/src/context.rs`),
  `DefaultEnvironment` (`liquers-lib/src/environment.rs`) and `liquers-py`'s each need the paired
  constructor. All are additive.
- **Naming needs care.** These types already use `with_*(&mut self) -> &mut Self` *mutators*
  (`with_async_store`, `with_recipe_provider`). A registry accepted at construction must not look
  like one of those, or it invites exactly the post-construction mutation this decision rules out —
  hence `new_with_type_registry(registry)` rather than `with_type_registry`.
- **A rebuild must replay the registrations.** `liquers-web` reconstructs its environment
  (`PENDING_ENV`, store-config replay); a type registered into the old registry and not replayed is
  silently lost — the trap store configuration already documents.

This also fixes *when* a foreign type must be describable: at construction, before any value exists.
That is why consequence 1 of the rule above — a static counterpart to `ForeignValue::identifier` —
is a requirement rather than a preference.

## Core Interactions

### Query System
None. No syntax, parsing or plan change.

### Store System
Indirect only: the refusal happens above the store, in asset-write validation. After the fix a
foreign value persists as metadata only, exactly as a UI element does.

### Command System
No new command required. A diagnostic command listing registered types (the `value-type-system`
Phase 2 open question) remains a candidate, not a commitment.

### Asset System
`AssetManager::set_state` / `set_binary` → `validate_metadata_hard` (`liquers-core/src/assets.rs:584`)
is the failing check. The read path already degrades gracefully on an unknown identifier
(`deserialize_stored_value`); that behaviour is deliberately left alone.

### Value Types
No new `ExtValue` variant. `ForeignValue` gains a `type_info()` derived from its identifier and
`default_*` methods, declaring no data formats, so the format check exempts it as it exempts
`UIElement`. `liquers-py`'s `Value` gains `type_descriptions()` with one entry per variant and
identifiers reconciled with `liquers-core`'s, so a store written from Python is readable from Rust.

### Web/API
`liquers-web` is the first consumer: `JsOpaque` declares `js.Value` where the environment is
constructed. Registrations must survive the environment rebuild that `PENDING_ENV` / store-config
replay performs, or they are silently lost — the trap store configuration already documents.

### UI
Not applicable.

## Crate Placement

- **liquers-core** — `type_system.rs` (extending a base registry), `context.rs` (four constructors
  that accept a finished registry), `value.rs` (correct the `identifier` doc comment). The registry
  stays lock-free and needs no lock because it is never written after construction.
- **liquers-lib** — `value/foreign.rs` (`ForeignValue::type_info`), `environment.rs` (registration
  entry point on `DefaultEnvironment`).
- **liquers-web** — registers `js.Value` at environment construction; no new abstraction.
- **liquers-py** — `value.rs`: `type_descriptions()`, identifiers split out of `"generic"`, and
  `python_value` → `py.Object`. Note `python_value` fails `identifier_naming_rule_holds` today
  (underscore is a reserved character), so it could not be registered even if it were described.

## Documentation Intent

**Reference:** Extend `specs/reference/VALUE_TYPE_SYSTEM.md` — state the one-identifier-per-variant
rule, add runtime registration, and extend the registered-identifier list with the foreign variant
and `liquers-py`'s set. Plus a `## History` row and a `reviewed:` bump. No new reference: this is
one mechanism inside an existing model.

**Guide:** Extend `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §VALUE, "Typing an integrated value",
whose closing paragraph currently points at this issue as an open problem. Extend
`specs/guides/TYPE_SYSTEM_GUIDE.md` §2 with the cardinality rule and the runtime-typed variant.

**Other documents to create:** None. The fix is M-sized and its reasoning fits the Phase 5 summary.

**Specific documents to update:** `specs/issues/FOREIGN-VALUE-TYPES-NOT-REGISTERED.md` and
`specs/issues/PY-VALUE-TYPE-DESCRIPTIONS-MISSING.md` (status), `specs/README.md` (design link),
`CLAUDE.md` "Adding a Value Type" if the guide's step count changes.

Audience: an integration author bridging a language into Liquers, who must be able to type their
values without reading this design folder.

## An unregistered type stays a hard refusal

A type identifier that is not in the registry is still refused on write, as it is today. The fix is
that a foreign type *can now be registered*, not that an unregistered one becomes tolerated. The
pre-`value-type-system` degrade-to-metadata behaviour is not restored: it hid exactly the mistake
this design makes fixable, and an integration that forgets to register its type should hear about
it at once rather than discover months of assets carrying an identifier nothing can resolve.

### Realms: in scope to *not* obstruct, out of scope to implement

`js.Value` lives in the browser realm and not in the server realm, and some values are physically
untransferable — a JavaScript closure cannot be sent to a server at all. In the intended end state
**both sides hold a complete registry covering both realms**, so either can see that a type exists
elsewhere, identify the types not supported in every realm, and act on that per realm. The type
system's job is to make those cases *identifiable*; resolving each one is realm-specific work.

That is `TYPE-REGISTRY-NOT-REALM-AWARE` (P2, `L`, wants its own design), not this one. What this
design owes it is not to obstruct it:

- `TypeKey { realm, type_identifier }`, `TypeInfo::with_realm` and `get_in_realm` already exist, so
  a registration made here can name a realm from day one.
- `TypeInfo` already derives `Serialize`/`Deserialize`, so a realm's descriptions can be
  transmitted. `TypeRegistry` itself does not — worth adding when sharing is built, as a list of
  `TypeInfo` (each already carries its realm) rather than a map with a struct key.
- Accepting a **finished registry** at the environment constructor is the shape that admits this
  later: a registry assembled from descriptions received over the wire is just another registry.
  A post-construction registration point would not have been.

## Open Questions

**How is the static/instance agreement proven?** The registry is now built before any value exists,
so an implementation must expose its identifier *without* an instance — a static
`JsOpaque::type_info()` — while `ForeignValue::identifier(&self)` remains the instance method the
value path calls. That is one truth with two spellings, and if they ever disagree the value's
identifier is not the one that was registered: the exact failure this design fixes, reintroduced
silently by an integration author instead of structurally. Three ways to prevent it:

| | Approach | Cost |
|---|---|---|
| a | **One source of truth in the trait.** Mirror the existing `TypeIdentifiedIn<V>` (`const TYPE_IDENTIFIER` + `fn type_info()`) and write the instance method as a one-line delegation. A *default body* deriving one from the other is not available — `ForeignValue` must stay object-safe for `Arc<dyn ForeignValue>`, and a default body is type-checked with `Self: ?Sized`, so it cannot call an associated function. | Divergence is reduced to one line an author could still get wrong |
| b | **`debug_assert` on the value path**, comparing `value.identifier()` against the registry. | Fires only in debug, and only when the path runs |
| c | **A conformance test** in `LANGUAGE-INTEGRATION_GUIDE.md`'s suite, which already numbers its checks (`RUNTIME05`, …), invoked per integration on a sample value. | An integration that skips the suite is unprotected |

They compose. Recommendation: **a + c** — make the one-line delegation the documented shape and let
the conformance suite catch an author who writes it wrong. `b` earns its place only if the
comparison is free on a path that already reaches the registry.

## Prerequisite (not an open question)

`liquers-py/src/value.rs` and `context.rs` are among the eight files `lib.rs` never declares as
modules (`PY-MODULES-NOT-DECLARED-IN-LIB`, P2). Declaring `value` and `context` produces **four
compile errors in `value.rs`** alone (verified 2026-08-26): `try_into_query` returns the wrong
`Query` type, `from_asset_info` takes one `AssetInfo` where the trait wants a `Vec`, a `match` with
incompatible arms, and four unimplemented trait items (`from_command_metadata`, `try_into_bytes`,
`try_into_key`, `try_into_command_metadata`). The Python half of this design cannot be *verified*
until `value.rs` compiles, so repairing that file is inside this design's scope; the rest of
`PY-MODULES-NOT-DECLARED-IN-LIB` (`commands.rs`, `store.rs`, `interpreter.rs`, …) is not.

## References

- `specs/issues/FOREIGN-VALUE-TYPES-NOT-REGISTERED.md`, `specs/issues/PY-VALUE-TYPE-DESCRIPTIONS-MISSING.md`
- `specs/issues/PY-MODULES-NOT-DECLARED-IN-LIB.md` (prerequisite, partially in scope)
- `specs/design/value-type-system/phase2-architecture.md` ("Environment — one new method"),
  `phase4-implementation.md` step 3 (where `"generic"` was split in core)
- `specs/reference/VALUE_TYPE_SYSTEM.md`, `specs/guides/TYPE_SYSTEM_GUIDE.md`,
  `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §VALUE
- Code: `liquers-core/src/type_system.rs`, `liquers-core/src/value.rs:230` and `:1155`,
  `liquers-core/src/assets.rs:584`, `liquers-lib/src/value/foreign.rs`,
  `liquers-web/src/value.rs:67`, `liquers-py/src/value.rs:282`
