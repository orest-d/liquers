---
id: LANGUAGE-STORE-TYPE-NOT-DEFINABLE
kind: feature
title: A store type cannot be defined in the integrated language
status: draft
priority: P2
complexity: M
area: [web, py, core/store, store/config]
design: 
created: 2026-08-29
github:
---
## Problem

An integrated language can supply a store **instance**, but not a store **type**.

`registerStoreObject(name, object)` maps a name to a JavaScript object, and a document reaches it
through the single Rust-defined type `js`:

```yaml
- type: js
  prefix: custom
  config: { object: myStore }
```

There is no way for a page to define `type: myprotocol` with its own arguments. Doing so requires
implementing `StoreFactory` — `store_types`, `resolve`, `create` — and that trait has no binding in
any integrated language. `WebStoreFactory` is Rust, in `liquers-web/src/store/builder.rs`, and it is
the only factory a browser build has.

**The asymmetry with commands is the sharpest way to see it.** `registerCommand(spec)` lets a page
define a genuinely new command, which then appears in the command registry like any other. Stores
have no equivalent: `registerStoreObject` is closer to registering one command *implementation*
under a fixed name than to defining a command.

## Impact

There is a workaround and it is not terrible — a page can put arbitrary behaviour behind a `js`
object — so this is P2 rather than higher. What the workaround costs:

- **The store type list cannot include page-defined types.** `StoreFactory::store_types()` is what
  the unclaimed-type error enumerates and what a generated table or configuration UI would list. A
  page-defined store is invisible to all of it: a document naming `myprotocol` is told the type is
  unknown and offered a list that could never contain it.
- **A page-defined store cannot declare its arguments.** `StoreTypeInfo` carries name, type,
  documentation, required-ness and defaults per argument. A `js` store's real arguments live inside
  the page's object, undescribed, so `config:` for it is unvalidated and undocumented.
- **The configuration document leaks an implementation detail.** `type: js, config: {object: foo}`
  says *how* the store is provided; `type: foo` would say what it is. A document written against a
  browser build cannot be read as naming the same store type a native build might implement in Rust.

`specs/design/store-factories-in-core/` made this sharper rather than causing it. That design put
"a factory declares the types it can build, with their arguments and availability" at the centre of
the model — and the one category of factory that cannot do any of it is a language-defined one.

## Expected behaviour

An integrated language should be able to contribute a store *type*, not only a store object:
something like `registerStoreType(spec)`, where the spec carries the type name, its argument
descriptions, and a constructor callback returning a store object.

The Rust side is a bridging factory that implements `StoreFactory` over a table of registered
language specs: `store_types()` returns the declared `StoreTypeInfo` values, `resolve` matches their
names, and `create` calls back into the language and adapts the result with the existing `JsStore`
machinery.

Questions the design has to answer, none of them settled here:

- **Where does it live?** `liquers-web` for JavaScript, but Python will want the same shape, and the
  language-neutral half — a serde-shaped store-type declaration — probably belongs in
  `liquers-core` beside `StoreTypeInfo`. Compare `COMMAND-DECLARATION-FORMAT`, which is the same
  problem for commands and has already concluded that every binding hand-parsing its own spec is
  the wrong answer.
- **What is `ArgumentCoverage` for such a type?** `Complete` seems right — the page owns the type
  and its arguments — but nothing verifies that a page-declared list matches what its constructor
  actually reads.
- **When may types be registered?** Registration after the environment is built has the same
  rebuild problem as `POST-INIT-COMMAND-REGISTRATION`.
- **Does `js` survive?** Probably yes, as the quick path for a one-off store, with the new mechanism
  for a store type that deserves a name.

## Discovery

Raised by the maintainer while reviewing `LANGUAGE-INTEGRATION_GUIDE.md` after
`design/store-factories-in-core/` landed: does the guide address integrating a foreign store into
the store configuration and factories, and would that require implementing a factory in the
integrated language? It does not, and it would.

The guide's §STORE names three directions — a store written in the integrated language, stores the
*integration* provides, and composition/configuration — and its extension-seam guidance is about the
*integration* (a Rust crate) contributing types. A store type contributed by the *language* is not a
direction it offers.
