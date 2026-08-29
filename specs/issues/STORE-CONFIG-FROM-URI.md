---
id: STORE-CONFIG-FROM-URI
kind: feature
title: A store cannot be configured from a URI
status: draft
priority: P3
complexity: M
area: [store/config]
design: store-config-uri
created: 2026-08-29
github:
---
## Problem

`StoreConfig` carries `type`, `prefix`, `config` and `metadata`. There is **no URI field and no URI
handling anywhere** in `liquers-store`: `create_opendal_store` builds a backend from
`config_as_string_map()` through `Operator::via_iter(scheme, pairs)`. A user who has an
`s3://bucket/path?region=…` URI — the form AWS tooling, documentation and colleagues hand around —
must decompose it by hand into `type`, `root` and `config` keys.

OpenDAL 0.55 supports the URI form natively (`Operator::from_uri`, and `Configurator::from_uri` on
61 of its 62 service configs), so the capability exists one layer down and is simply not exposed.

## Impact

A convenience gap, not a defect: everything expressible as a URI is expressible as `type` +
`config`, and the two produce an identical operator — verified, `s3://probe-bucket/data?region=…`
and the equivalent `config:` map both yield `name=probe-bucket`, `root="/data/"`. So this is P3.

The gap is most felt where a URI is the natural unit: a command-line flag (`--store s3://bucket`), a
single-store quick start, an environment variable. It is least felt in a multi-store YAML document,
where a `config:` map is more readable than a query string anyway.

## Direction chosen (maintainer, 2026-08-29)

**Unified URI**, not a per-interpreter field. The scheme is extracted from the URI and interpreted as
a store type — through an explicit mapping, since the Liquers store-type namespace and OpenDAL's
scheme namespace stay **distinct** — and then routed through the normal factory chain. The rejected
alternative was naming an interpreter per entry (`schemes: opendal` alongside `uri: "s3://…"`),
which is safer and simpler but messier and less ergonomic.

Backwards compatibility is **not** a constraint at this stage, so the two namespaces may be
harmonized where that reduces surprise. They are not merged: a mapping remains, because a scheme is
OpenDAL's vocabulary and a store type is ours.

Designed in [`design/store-config-uri/`](../design/store-config-uri/), whose stated purpose is as
much to **validate `store-factories-in-core` against this future extension** as to specify the
feature.

## Expected behaviour

A store entry may be written as a URI *instead of* `type` + `config`:

```yaml
stores:
  - uri: s3://my-bucket/datasets?region=eu-central-1
    prefix: remote
```

**`uri` and `type` are mutually exclusive, not complementary.** A URI's scheme *is* the store type,
so a document carrying both has two sources of truth that can disagree. Exactly one must be present;
`prefix` is orthogonal and stays in both forms.

## Design constraints, if this is built

Recorded from a critical assessment during
[`design/store-factories-in-core/`](../design/store-factories-in-core/) Phase 3. These are the
reasons the feature is not trivial, and they should be settled before implementation rather than
discovered during it.

**1. Resolution must go through the Liquers factory chain, never OpenDAL's registry.** The obvious
implementation — hand the URI to `Operator::from_uri` — bypasses `ChainedStoreFactory` entirely and
destroys the property the factory design exists for: that *Liquers* decides which factory serves a
type, first in the chain winning. A browser build could never override `http` that way. The URI must
instead be parsed for its scheme, the scheme mapped to a store type, the chain consulted as usual,
and the URI handed to the factory that claims it.

**2. The store-type namespace is not OpenDAL's scheme namespace, and they overlap confusingly.**

| Liquers `type` | Means | OpenDAL scheme | Means |
|---|---|---|---|
| `memory` | `liquers_core::store::AsyncMemoryStore` | `memory` | OpenDAL's in-memory service |
| `filesystem` | `liquers_core::store::AsyncFileStore` | `fs` | OpenDAL's local filesystem |
| `http` | in a browser build, a `fetch`-backed store | `http` | OpenDAL's HTTP service |

`memory://` and `http://` are ambiguous, and `filesystem` has no scheme at all. A URI form needs an
explicit, documented scheme-to-type mapping, which is a table to maintain and a source of surprise.

**3. Coverage is uneven whichever route is taken.** `Operator::from_uri` resolves through
`DEFAULT_OPERATOR_REGISTRY`, which registers **10** services (memory, fs, s3, azblob, b2, cos, gcs,
obs, oss, upyun), while `via_iter` has **62** arms. Calling `Configurator::from_uri` per config type
directly — using the same `store_type -> config type` mapping the factory needs anyway — reaches
**61 of 62** and is the better route. Either way, a configuration format where `uri:` works for some
types and fails for others is a trap: a user learns it from an S3 example and meets "scheme is not
registered" on FTP. Verified: `ftp://ftp.invalid:21` fails through `from_uri` while
`via_iter("ftp", …)` succeeds in the same build.

**4. Secrets and `${VAR}` expansion interact badly with URIs.** Expansion is currently per
configuration *value*, so `access_key_id: ${AWS_ACCESS_KEY_ID}` is a discrete field. Inside a query
string the expanded value would need URL-encoding — a new failure mode for any secret containing
`&`, `=` or `/` — and the form actively invites credentials in a single string that gets logged,
copied and pasted. The `config:` map is the safer place for secrets and should stay the recommended
one.

**5. Anything a URI cannot express still needs `config:`.** S3 alone has 26 fields; a URI carrying
ten query parameters is less readable than the map, not more. The feature earns its place for the
short case and should not be presented as a general alternative.

## Discovery

Raised by the maintainer during `design/store-factories-in-core/` Phase 3, after that design's
examples showed `Operator::from_uri` working for S3. The observation that settled the shape: **a URI
contains the type, so a `uri:` field duplicates or replaces `type:` rather than supplementing it.**

An earlier note in that design called a `uri:` field "sugar over the same `config:` map". That was
wrong and is corrected there: the URI carries the store type, which is the one field the factory
chain dispatches on, so it is an alternative spelling of the whole entry rather than of its
arguments.

Deliberately out of scope for that design, which moves existing code and adds a factory seam; a new
configuration field is a format change and belongs on its own.
