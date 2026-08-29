For [`issues/STORE-CONFIG-FROM-URI.md`](../../issues/STORE-CONFIG-FROM-URI.md). Nothing here is
implemented.

# Phase 1 — High-level design

## Problem and evidence

A store entry is written as a `type` plus a `config` map. There is **no URI support of any kind** in
`liquers-store`: `StoreConfig` (`liquers-store/src/config.rs:39`) carries `store_type`, `prefix`,
`config` and `metadata`, and `create_opendal_store` (`store_builder.rs:190`) builds a backend from
`config_as_string_map()` through `Operator::via_iter(scheme, pairs)`. No code path anywhere accepts a
URI.

The capability exists one layer down and is unexposed. Verified against OpenDAL 0.55:

```
s3://probe-bucket/data?region=eu-central-1&allow_anonymous=true  ->  name=probe-bucket root="/data/"
via_iter("s3", {bucket, root, region, allow_anonymous})          ->  name=probe-bucket root="/data/"
```

Identical operators. `Configurator::from_uri` is implemented by **61 of 62** service configs, though
`Operator::from_uri`'s own registry resolves only **10** schemes.

The ergonomic gap is real: `s3://my-bucket/datasets?region=eu-central-1` is one line that people
already copy from AWS tooling, against four lines of YAML that must be assembled by hand.

## Expected behaviour and acceptance criteria

A store entry may be written as a URI **instead of** a type and its arguments:

```yaml
stores:
  - uri: s3://my-bucket/datasets?region=eu-central-1
    prefix: remote
```

1. `uri` and `type` are **mutually exclusive**; exactly one is present, and a document with both is
   an error naming both.
2. `prefix` is orthogonal and works with either form.
3. A URI entry and the equivalent `type` + `config` entry produce the same store.
4. Scheme resolution goes through the **Liquers factory chain**, never OpenDAL's registry, so
   first-wins ordering decides which factory serves a scheme exactly as it decides a store type.
5. A scheme no factory in the chain claims is an error listing the schemes the chain does support —
   the URI analogue of the unclaimed-store-type error.
6. `${VAR}` expansion keeps working, and a secret containing `&`, `=` or `/` survives it.

## Scope and non-goals

**In scope:** a `uri` field on a store entry; scheme-to-store-type mapping; routing a URI entry
through the existing chain; the error paths above; and the compatibility audit of
`store-factories-in-core` that is this design's second purpose.

**Non-goals.** Replacing `type` + `config`, which stays the primary and the only sensible form for a
backend with many arguments — S3 has 26. Exposing `Operator::from_uri` directly. A URI form for
`prefix` or for the router as a whole. Credentials-in-URI as a recommended practice.

## Affected systems

**Store.** The configuration format gains a field; `StoreRouterBuilder` gains a normalization step
before dispatch. **Query, Commands, Assets, UI:** unaffected — this is configuration, not evaluation.
**Bindings:** `liquers-web` benefits without change, since it consumes `StoreRouterConfig`; a browser
page could configure `localstorage://ns` once the browser factory declares a scheme.

## Compatibility constraints

Backwards compatibility is **not** a constraint at this stage (maintainer decision), so the Liquers
store-type namespace and OpenDAL's scheme namespace may be harmonized where that reduces surprise.
They are **not merged**: a scheme is OpenDAL's vocabulary and a store type is ours, so a mapping
remains even where the two names coincide.

The binding constraint is the other direction: this design must not require a change to
[`design/store-factories-in-core/`](../store-factories-in-core/), which is at its own approval gate.
Phase 2 audits that explicitly.

## Open questions

1. **Does core's `filesystem` claim `fs://`?** Harmonization argues yes; Phase 2 argues no, and the
   reason is the sharpest finding here — see the `fs://` trap.
2. **Where does `${VAR}` expansion happen** — on the URI string before parsing, or per value after?
   Phase 2 recommends after.
3. **Do the browser store types get schemes** (`localstorage://`, `js://`)? Desirable, not required.
4. **Is `uri` also accepted at the router level** as a one-store shorthand? Out of scope; noted
   because a CLI flag is the most natural home for a URI.

## Documentation assessment

Small maintenance: `specs/reference/STORE_CONFIG_FSD.md` gains the `uri` field, the scheme mapping
table and the exclusivity rule. Potentially substantive, to revisit at Phase 5: whether the
scheme-to-type mapping deserves its own section rather than a table, and whether
`STORE_FACTORY_GUIDE.md` (created by `store-factories-in-core`) needs a "declare a URI scheme"
task.

## Critical review

- **Duplication:** none. No existing mechanism accepts a URI.
- **Scope:** narrow and testable; every criterion above is assertable offline, since OpenDAL
  construction is verified not to touch the network.
- **Architecture fit:** the unified-URI direction *reinforces* the layered design rather than
  cutting across it — resolution stays inside the Liquers chain, and the store type remains the
  dispatch key.
- **Blocking unknowns:** none. Question 1 is a recommendation Phase 2 makes and the gate can
  overrule; 2–4 do not block.
