---
title: Store Factory Guide
kind: guide
audience: internal
area: [core/store, store/config]
reviewed: 2026-08-29
---
# Store Factory Guide

How to define a store type, contribute one from another crate, choose which set of store types a
build gets, and override one someone else defined.

For *what* the configuration format is, see
[`reference/STORE_CONFIG_FSD.md`](../reference/STORE_CONFIG_FSD.md). This guide is the how-to.

## The one thing to know first

A `StoreRouterBuilder` has **no store types of its own**. Everything it builds comes from the
factory you give it, which is why the factory is a required argument:

```rust
use liquers_core::store_factory::StoreRouterBuilder;
use liquers_store::store_factory::default_store_factory;

let router = StoreRouterBuilder::from_yaml(yaml, Box::new(default_store_factory()))?.build()?;
```

If you take one thing from this guide: **reach for your crate's `default_store_factory()`**.
Compose a chain by hand only when you want a different order or a subset.

## Which factory do I want?

| Crate | Its own factory | `default_store_factory()` gives you |
|---|---|---|
| `liquers-core` | `core_store_factory()` — `memory`, `filesystem` | core only |
| `liquers-store` | `OpendalStoreFactory` — `s3`, `fs`, `ftp`, … | core, then OpenDAL |
| `liquers-web` | `WebStoreFactory` — `localstorage`, `js`, `http`, `https` | core, then browser |

Every crate contributing store types provides exactly two things: **one factory describing only its
own types**, and **one convenience chain** of its own after everything below it. Follow that shape
if you add a third.

`liquers-web`'s takes an argument, because `WebStoreFactory` holds page objects registered at
runtime. The convention is about what each crate provides, not a signature every crate must match.

## Add a store type to a crate that already has a factory

Use `StoreTypeMap`: a factory built from names and creation closures, no trait implementation.

```rust
use liquers_core::store_factory::{StoreArgumentInfo, StoreArgumentType, StoreTypeInfo, StoreTypeMap};

StoreTypeMap::new().with_store_type(
    StoreTypeInfo::new("mystore")
        .with_label("My store")
        .with_doc("What it stores and where.")
        .with_argument(
            StoreArgumentInfo::new("endpoint", StoreArgumentType::String)
                .required()
                .with_doc("Server address."),
        ),
    Box::new(|config| {
        let prefix = config.key_prefix()?;
        let endpoint = config.require_config_string_expanded("endpoint")?;
        Ok(Box::new(MyStore::new(&endpoint, &prefix)?))
    }),
)
```

Argument types are JSON's — `String`, `Number`, `Boolean`, `Array`, `Object`, `Any` — because a
configuration document is JSON or YAML. **Prefer scalars.** `Array` and `Object` exist for the cases
that genuinely need them; a `config:` block stays easiest to read and to pass on when its values are
flat.

## Contribute store types from your own crate

Implement `StoreFactory`. `liquers-web/src/store/builder.rs` is the worked example.

```rust
impl StoreFactory for MyFactory {
    fn store_types(&self) -> Vec<StoreTypeInfo> { /* one per type, with arguments */ }
    fn create(&self, config: &StoreConfig) -> Result<Box<dyn AsyncStore>, Error> { /* … */ }
}
```

Then chain it after the types you also want:

```rust
ChainedStoreFactory::new()
    .chain(Box::new(core_store_factory()))
    .chain(Box::new(MyFactory::new()))
```

**No `Send`/`Sync` bound on the trait, and do not add one.** A factory is consumed while the router
is built; only the `AsyncStore` it produces has thread requirements, which `AsyncStore` already
states. `WebStoreFactory` holds `js_sys::Object` handles and is `!Send`, so a bound would exclude
the browser.

## Override a store type someone else defined

Chain your factory **earlier**. The first factory to resolve an entry builds it.

```rust
ChainedStoreFactory::new()
    .chain(Box::new(my_factory))            // wins for every type it claims
    .chain(Box::new(core_store_factory()))
```

The default chains put core first so that `memory` and `filesystem` mean the same thing everywhere.
That is a *default ordering*, not a prohibition — first-wins reads like one, which is why this
section exists.

## Say a type exists but cannot be built here

A store type that is real and documented but compiled out — a Cargo feature is off, or the target
does not support it — must say so:

```rust
StoreTypeInfo::new("s3").unavailable("requires the 'opendal' feature of liquers-store")
```

The type is still *declared*, so a document naming it gets that reason instead of "unknown store
type". This is not politeness: reporting a real type as unknown sends the reader hunting for a typo
in something that exists. It is conformance item `STORE13` in
[`LANGUAGE-INTEGRATION_GUIDE.md`](LANGUAGE-INTEGRATION_GUIDE.md).

Live cases: every OpenDAL type without the `opendal` feature, and `filesystem` on wasm32.

## Describing arguments you do not own

If your store type wraps another project's backend, its arguments change on that project's release
schedule. Do not copy them:

```rust
StoreTypeInfo::new("s3")
    .with_arguments(the_two_or_three_that_actually_need_guidance())
    .partial("https://opendal.apache.org/docs/rust/opendal/services/index.html")
```

`ArgumentCoverage::Partial` says the list is guidance, unlisted keys pass through to the backend,
and the authority names where the truth lives. **An incomplete list is only a lie if completeness
was claimed** — under `Partial`, an upstream release adding a field makes your description less
complete, never wrong, and nobody has to notice for it to stay honest.

Leave the default `Complete` for a type you own: there the list *is* the specification.

## Rules for a `create` implementation

- **Be fast, and do no bulk I/O.** Every store in a document is constructed at startup, and
  construction *is* the validation — there is no separate validate-without-building pass. A store
  type that benefits from pre-fetching (a remote metadata database) is making a trade-off it must
  document.
- **Validate there.** A missing required argument, an unreachable configuration, a name that does
  not resolve: fail in `create` with a message naming what is wrong.
- **Trust `config.store_type`.** The chain sets it to whatever `resolve` returned before calling.
- **Expand `${VAR}`** with `require_config_string_expanded` rather than `require_config_string`, for
  anything that might carry a secret.

## Inferring the store type

`resolve` defaults to an exact match on `type`, which is what every factory in the tree uses. You
*may* override it to infer the type from the entry instead — the store type is the resolved identity
of an entry, and what identifies it is input to that.

Two rules keep inference from becoming magic in a routing decision:

- **Resolve only to a type you declare** in `store_types()`. The declared set stays the vocabulary;
  inference chooses within it.
- **Key on something whose purpose is identification** — a `uri` scheme, an explicit type — never on
  the incidental presence of an argument. A factory inferring `filesystem` from a `path` key would
  silently reroute any other type that later gained a `path`.

Nothing in the tree needs this yet. It exists so that a URI form
([`STORE-CONFIG-FROM-URI`](../issues/STORE-CONFIG-FROM-URI.md)) can be added without changing the
trait.

## What these descriptions do not express

The argument list says what a type accepts, **not which combinations are valid**. S3's credential
modes — static keys, or assume-role, or customer-managed encryption keys — are mutually exclusive in
practice, and nothing in `StoreTypeInfo` says so. Encoding argument-group constraints is a larger
feature that has not been designed. Document exclusivity in an argument's `doc` if it matters.

## Executable evidence

| What | Where |
|---|---|
| A factory implemented from scratch, with described arguments | `liquers-web/src/store/builder.rs` |
| A map-built factory | `core_store_factory` in `liquers-core/src/store_factory.rs` |
| Chain order, unions, unclaimed and unavailable types | `chain01`–`chain06` in the same file |
| Resolution, including inference | `resolve01`–`resolve04` |
| `Partial` accepting an undescribed key | `coverage02` in `liquers-store/src/store_factory.rs` |
| The gated-feature message | `factory04` — runs **only** under `--no-default-features` |

## History

| Date | Change | Source |
|---|---|---|
| 2026-08-29 | Created with the factory model: choosing a chain, adding a type by map or by trait, overriding by chaining earlier, declaring unavailability, `ArgumentCoverage` for externally-owned arguments, the `create` contract, and the inference rules. | `design/store-factories-in-core/` |
