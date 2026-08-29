---
title: "Phase 3: Examples and tests — Store configuration and factories in liquers-core"
kind: design
audience: internal
area: [core/store, store/config, store/backends, web, docs]
---
# Phase 3: Examples & Use-cases — Store Configuration and Factories in `liquers-core`

## High-Level Introduction

Phase 1's purpose was that `liquers-core` should be able to describe a store without a backend in
the graph, and that `liquers-web` should stop depending on `liquers-store`. Phase 2 turned that into
a factory seam: a trait that *describes* the store types it claims, a first-wins chain, and a
builder with no built-in knowledge.

The examples follow the shape a developer actually meets them in:

- **Scenario 1** is the ordinary case — build a router from a configuration document using the
  default factory. It is one call, and the point is that it stays one call after the move.
- **Scenario 2** is the case the design exists for — an integration contributing its own store types
  and composing a chain, shown as `liquers-web` after the change.
- **Scenario 3** is the pitfalls, and they are unusually concentrated here because this design
  *inverts a documented rule*. Three currently-passing tests assert the old behaviour.

Because this is a refactor of code that already works and is already tested, the tests are the
primary deliverable of this phase, not an afterthought. The most important table in this document is
not the examples but §Test Plan's inventory of the 18 existing tests, of which **three assert
behaviour this design deliberately changes**.

## Example Type

**Runnable prototypes**, decided rather than asked. Conceptual examples would be the wrong artifact:
every scenario below corresponds to code that exists and is tested today, so a conceptual sketch
could not be checked against anything. Each example is written so it can become a test or a doc
example verbatim.

The one exception is Scenario 2, which is wasm32-only (`liquers-web`) and therefore runs under
`wasm-bindgen-test` rather than `cargo test`. It is shown as real code but its executable home is
the existing browser suite.

## Overview Table

| # | Type | Name | What it demonstrates / checks |
|---|---|---|---|
| 1 | Example | Router from a document, default factory | The ordinary path is unchanged in shape: parse a `StoreRouterConfig`, hand it a `default_store_factory()`, build |
| 2 | Example | An integration contributes store types | `WebStoreFactory` after the move: own factory describing its types, chained after core's, no `liquers-store` |
| 3 | Example | Pitfalls | First-wins is not last-wins; no built-in fallback; an unclaimed type is an error, not a silent miss |
| 4 | Example | OpenDAL store types | S3 (26 fields) and FTP (4) as `StoreTypeInfo`; S3 built from arguments *and* from a URI, both verified against OpenDAL 0.55; where our stringification and OpenDAL's parser disagree |
| 4a | Unit tests | S3 offline (2) | `s3_01` argument and URI forms agree; `s3_02` a missing `region` fails at construction — no credentials, no network |
| 4 | Unit tests | `store_config` (11 tests) | Moved verbatim: env-var expansion, YAML/JSON parsing, builder methods, `key_prefix` |
| 5 | Unit tests | `store_factory` — core (13 new) | `StoreTypeMap`, chain order, `store_types()` union, availability, error text |
| 6 | Unit tests | `store_factory` — `liquers-store` (6, 4 rewritten) | OpenDAL factory, default chain, the `factory01`–`factory04` suite restated |
| 7 | Integration | `liquers-core` router build (4 new) | A core-only build makes a working router with no `liquers-store` in the graph |
| 8 | Integration | `liquers-web` (existing, retargeted) | The browser suite passes with the dependency removed |
| 9 | Build matrix | `check-build-matrix.sh` (4 new rows) | `liquers-core` under default / no-default / `toml` / wasm32 |

## Example 1: A Router from a Configuration Document

### Connection to the High-Level Design

This is the path every existing consumer takes, and the test of whether the move cost anything. The
Phase 1 promise was a *relocation*, not a new way of working: the same document, the same builder,
one extra argument naming which store types you want.

### Scenario

A native application starts up, reads `stores.yaml`, and needs an `AsyncStoreRouter` routing `cache`
to memory and `data` to the filesystem. It wants OpenDAL types available too, because the same
document might later name `s3`.

### Sequence of Steps

1. The document is parsed into a `StoreRouterConfig` — pure data, `liquers-core`, no backend needed.
2. `liquers_store::default_store_factory()` is asked for the chain: core's store types first, then
   OpenDAL's.
3. `StoreRouterBuilder::new(config, factory)` pairs the two.
4. `build()` expands `${VAR}` references, then asks the chain to create each entry in order.
5. Each store is added to an `AsyncStoreRouter`, which routes by prefix, first match winning.

### Core Example Code

```rust
use liquers_core::store_config::StoreRouterConfig;
use liquers_core::store_factory::StoreRouterBuilder;
use liquers_store::store_factory::default_store_factory;

fn build_router(yaml: &str) -> Result<AsyncStoreRouter, Error> {
    let config = StoreRouterConfig::from_yaml(yaml)?;
    StoreRouterBuilder::new(config, Box::new(default_store_factory())).build()
}
```

with

```yaml
stores:
  - type: memory
    prefix: cache
  - type: filesystem
    prefix: data
    config:
      path: ./data
```

The convenience wrapper stays too, for the case with nothing to customise:

```rust
let router = liquers_store::store_factory::create_router_from_yaml(yaml)?;
```

### Guide and Executable Example

The primary scenario is small enough that a dedicated `examples/` binary would be ceremony around
four lines. Its executable home is `liquers-store`'s own test module — the existing
`test_store_router_from_yaml` becomes exactly this code — and `specs/guides/STORE_FACTORY_GUIDE.md`
links that test rather than duplicating it. Scenario 2 is the one the guide needs at length.

**Expected output:** a router where `cache/x.txt` resolves to the memory store and `data/x.txt` to
the filesystem store; `other/x.txt` is unsupported.

**Validation:**
- [x] Compiles against the Phase 2 signatures
- [x] Demonstrates the core workflow
- [x] Realistic document, defaults unchanged except where the scenario needs them
- [x] Expected output stated

## Example 2: An Integration Contributes Its Own Store Types

### Connection to the High-Level Design

This is what the design is *for*. It shows the per-crate convention from Phase 2 — one factory
describing only your own types, one default chaining what should be available — and it is the
scenario in which `liquers-web`'s `liquers-store` dependency disappears.

### What is new relative to Scenario 1

Scenario 1 consumed a chain someone else built. Here the chain is composed, and the composition is
the whole content: which factory goes first decides which implementation of a contested type name
wins.

### Core Example Code

`liquers-web` after the change. The factory now *describes* its types instead of merely naming them
— the arguments below are currently documented only in a doc-comment YAML block at the top of
`liquers-web/src/store/builder.rs`, which is exactly the drift this replaces:

```rust
use liquers_core::store_config::StoreConfig;
use liquers_core::store_factory::{
    ChainedStoreFactory, StoreArgumentInfo, StoreArgumentType, StoreFactory, StoreTypeInfo,
    core_store_factory,
};

impl StoreFactory for WebStoreFactory {
    fn store_types(&self) -> Vec<StoreTypeInfo> {
        vec![
            StoreTypeInfo::new("localstorage")
                .with_label("Browser localStorage")
                .with_doc("Persists in the page's localStorage. Bounded by the browser's quota.")
                .with_argument(
                    StoreArgumentInfo::new("namespace", StoreArgumentType::String)
                        .required()
                        .with_doc("Key prefix, so two applications on one origin do not collide."),
                )
                .with_argument(
                    StoreArgumentInfo::new("quota_bytes", StoreArgumentType::Number)
                        .with_doc("Refuse writes beyond this many bytes."),
                ),
            StoreTypeInfo::new("js")
                .with_doc("Delegates to a page object registered with registerStoreObject.")
                .with_argument(
                    StoreArgumentInfo::new("object", StoreArgumentType::String)
                        .required()
                        .with_doc("The registered name — a JS object cannot be written into YAML."),
                ),
            StoreTypeInfo::new("http")
                .with_doc("Read-only fetch() over a fixed key list.")
                .with_argument(
                    StoreArgumentInfo::new("url_prefix", StoreArgumentType::String).required(),
                )
                .with_argument(
                    StoreArgumentInfo::new("keys", StoreArgumentType::Array)
                        .with_doc("Keys this store serves; fetch cannot enumerate a directory."),
                ),
            // `https` as above.
        ]
    }

    fn create(&self, config: &StoreConfig) -> Result<Box<dyn AsyncStore>, Error> {
        match config.store_type.as_str() {
            LOCAL_STORAGE_TYPE => Self::create_local_storage(config),
            JS_TYPE => self.create_js(config),
            "http" | "https" => Self::create_fetch(config),
            other => Err(Error::general_error(format!(
                "the browser store factory does not handle store type {other:?}"
            ))),
        }
    }
}

/// The browser's convenience chain — core's store types, then this crate's.
/// Deliberately *not* OpenDAL's: it is not in this crate's graph at all.
pub fn default_store_factory(factory: WebStoreFactory) -> ChainedStoreFactory {
    ChainedStoreFactory::new()
        .chain(Box::new(core_store_factory()))
        .chain(Box::new(factory))
}
```

`keys` is the one argument needing `StoreArgumentType::Array`, and the only known user of that
variant — the evidence Phase 2 asked to collect.

**Expected output:** a document naming `localstorage`, `js`, `http` or `memory` builds; one naming
`s3` fails with an error listing the four supported types, because no OpenDAL factory is in this
chain.

**Validation:**
- [x] Compiles against the Phase 2 signatures
- [x] Shows the delta from Scenario 1 only
- [x] `liquers-store` appears nowhere

## Example 3: Pitfalls

### 1. First-wins reads like a prohibition; it is a default

**Symptom.** "I chained my factory after core's and my `memory` implementation is ignored."

**Cause.** First-wins. Core is first in every default chain, so it claims `memory`.

**Correct usage.** Compose your own chain and put yours first. The API permits it; only the
*default* ordering does not.

```rust
let factory = ChainedStoreFactory::new()
    .chain(Box::new(my_factory))          // wins for any type it claims
    .chain(Box::new(core_store_factory()));
```

**Test that protects it:** `chain03_earlier_factory_wins`.

### 2. There is no built-in fallback any more

**Symptom.** "`StoreRouterBuilder::new(config)` no longer compiles."

**Cause.** Deliberate. The builder has no store types of its own; the factory is a required argument
so that "which stores do I get" is answerable at the call site.

**Correct usage.** `StoreRouterBuilder::new(config, Box::new(default_store_factory()))`.

**Test that protects it:** compilation — there is no fallback to test.

### 3. The browser's `http` override no longer works by precedence

**Symptom.** None today, which is what makes it a pitfall: the behaviour is unchanged, the *reason*
is not.

**Cause.** `WebStoreFactory` used to beat the built-in OpenDAL `http` because factories were
consulted before built-ins. Under first-wins a later factory can never beat an earlier one. The
override survives only because `liquers-web` no longer has the OpenDAL factory in its chain at all.

**Why it matters.** Anyone adding the OpenDAL factory to a browser chain — or reading
`design/liquers-web-store/phase2-architecture.md`, which still argues the old rationale — would get
OpenDAL's `http`, which cannot run on wasm32. The guide must say this.

**Test that protects it:** `factory02` rewritten (see Test Plan), plus the existing browser suite.

### 4. An unclaimed type is an error, not a fall-through

**Symptom.** A configuration that used to build now fails with "Unknown store type".

**Cause.** `factory03` used to assert that an unclaimed type "still reaches the built-ins". There are
no built-ins to reach.

**Correct usage.** Chain a factory claiming the type. The error lists what the chain does support,
so the fix is in the message.

**Test that protects it:** `chain05_unclaimed_type_lists_supported_types`.

## Example 4: OpenDAL Store Types — the Complex Configurations

### Why this example exists

Scenarios 1 and 2 use `memory`, `filesystem` and the browser types, all of which take one or two
arguments. They do not test whether `StoreTypeInfo` can describe a *real* backend. OpenDAL's are the
demanding ones, and describing them is what the `OpendalStoreFactory` must actually do.

Conceptual only — none of this runs here; the values are checked against OpenDAL 0.55's service
config structs (`opendal-0.55.0/src/services/*/config.rs`) rather than against a live backend.

### The size of the job

Field counts read from the 0.55 sources, not estimated:

| Type | Fields | Character |
|---|---|---|
| `ftp` | 4 | the simple case: `endpoint`, `root`, `user`, `password` |
| `http` | 5 | read-only, no credentials |
| `webdav` | 6 | two auth styles — `username`+`password` **or** `token` |
| `sftp` | 6 | key-file based |
| `azblob` | 9 | account name/key, SAS token |
| `gcs` | 13 | service-account JSON, scopes, predefined ACL |
| **`s3`** | **26** | the hardest: credentials, assume-role, four server-side-encryption modes, virtual-host style, request-payer, batch limits |

**≈134 fields across the 20 types in `OPENDAL_STORE_TYPES`.** That is the honest cost of "a factory
describes the arguments for each store type", and it is a scoping question for Phase 4 rather than a
design flaw — see §Scoping question below.

### FTP — the common simple case

```yaml
stores:
  - type: ftp
    prefix: archive
    config:
      endpoint: ftp.example.org:21
      root: /exports/liquers
      user: ${FTP_USER}
      password: ${FTP_PASSWORD}
```

```rust
StoreTypeInfo::new("ftp")
    .with_label("FTP")
    .with_doc("FTP server via OpenDAL. Credentials are sent by the protocol in cleartext.")
    .with_argument(
        StoreArgumentInfo::new("endpoint", StoreArgumentType::String)
            .required()
            .with_doc("host:port, e.g. ftp.example.org:21"),
    )
    .with_argument(
        StoreArgumentInfo::new("root", StoreArgumentType::String)
            .with_doc("Server-side directory treated as the store root."),
    )
    .with_argument(StoreArgumentInfo::new("user", StoreArgumentType::String))
    .with_argument(StoreArgumentInfo::new("password", StoreArgumentType::String))
```

All four are strings, so nothing is stressed. This is the shape most types have.

### S3 — the hard case

Every argument below is a real OpenDAL 0.55 field name. This is a *subset*; the full struct has 26.

```yaml
stores:
  - type: s3
    prefix: remote
    config:
      bucket: my-liquers-bucket          # the only required field
      region: eu-central-1
      endpoint: https://s3.eu-central-1.amazonaws.com
      access_key_id: ${AWS_ACCESS_KEY_ID}
      secret_access_key: ${AWS_SECRET_ACCESS_KEY}
      session_token: ${AWS_SESSION_TOKEN}
      root: datasets/2026
      server_side_encryption: aws:kms
      server_side_encryption_aws_kms_key_id: ${KMS_KEY_ARN}
      enable_virtual_host_style: true    # boolean
      allow_anonymous: false             # boolean
      disable_config_load: true          # boolean — do not read ~/.aws
      batch_max_operations: 1000         # number
      default_storage_class: INTELLIGENT_TIERING
```

An assume-role deployment instead uses `role_arn`, `external_id` and `role_session_name`; a
customer-managed-key deployment uses `server_side_encryption_customer_algorithm` and
`server_side_encryption_customer_key`. Those combinations are mutually exclusive in practice and
**nothing in `StoreTypeInfo` can express that** — see §Scoping question.

```rust
StoreTypeInfo::new("s3")
    .with_label("Amazon S3 (and S3-compatible)")
    .with_doc("S3 via OpenDAL. Also serves MinIO, Ceph and other S3-compatible endpoints;                set `endpoint` for those.")
    .with_argument(
        StoreArgumentInfo::new("bucket", StoreArgumentType::String)
            .required()
            .with_doc("Bucket name. The only argument S3 always needs."),
    )
    .with_argument(
        StoreArgumentInfo::new("region", StoreArgumentType::String)
            .with_doc("e.g. eu-central-1. Inferred from the environment when omitted."),
    )
    .with_argument(
        StoreArgumentInfo::new("access_key_id", StoreArgumentType::String)
            .with_doc("Use ${AWS_ACCESS_KEY_ID}; never write a literal key into a document."),
    )
    .with_argument(
        StoreArgumentInfo::new("enable_virtual_host_style", StoreArgumentType::Boolean)
            .with_default(serde_json::Value::Bool(false))
            .with_doc("Address the bucket as a subdomain rather than a path element."),
    )
    .with_argument(
        StoreArgumentInfo::new("batch_max_operations", StoreArgumentType::Number)
            .with_doc("Cap on operations per batch request. Whole number."),
    )
    // … 21 more
```

This is the first place `StoreArgumentType::Boolean` and `Number` have real users — the core and
browser types are all strings — so S3 is what justifies those variants existing.

### S3 two ways: from arguments, and from a URI

Both were **run against OpenDAL 0.55** in a scratch probe; the outputs below are recorded, not
predicted. The probe was removed afterwards — see §What was run and why it is not in the tree.

#### From arguments — what `StoreConfig` supports today

```yaml
- type: s3
  prefix: remote
  config:
    bucket: probe-bucket
    root: data
    region: eu-central-1
    allow_anonymous: true
    disable_config_load: true
```

reaching `Operator::via_iter("s3", …)`. Verified: builds, `name=probe-bucket`, `root="/data/"`.

#### From a URI — what OpenDAL supports and `StoreConfig` cannot express

```
s3://probe-bucket/data?region=eu-central-1&allow_anonymous=true&disable_config_load=true
```

Verified: builds, `name=probe-bucket`, `root="/data/"` — **byte-identical to the argument form.**
The URI's host is the bucket, its path is the root, and its query string is the remaining options.

**`StoreConfig` has no way to write this.** There is no `uri:` field, and adding one is a format
change beyond this design. It is worth recording as a possible future convenience because the
equivalence above is exact — a `uri:` entry would be sugar over the same `config:` map, not a second
mechanism.

#### Three findings from running it

**1. `region` is genuinely required, and the failure is offline and immediate.**

```
s3://probe-bucket          -> ERR ConfigInvalid at Builder::build: region is missing.
s3://probe-bucket?region=eu-central-1&allow_anonymous=true   -> OK
```

This is the clearest evidence for the Phase 1 decision to **validate on construction**: a missing
required argument is caught at build time, with no network and no credentials.

**2. Construction never touches the network.** `s3` with a nonexistent bucket, `ftp.invalid:21`, and
`https://example.invalid` all construct successfully. OpenDAL builders are lazy — an `Operator` is a
handle, and the first request is what fails. This confirms the "`create` must be fast and must not
fetch bulk data" constraint is a rule *implementations* must honour rather than something the
backends already violate, **and it is why S3 is unit-testable with no connection.**

**3. `from_uri` covers far fewer services than `via_iter`.** `ftp://ftp.invalid:21` fails with
"scheme is not registered", while `via_iter("ftp", …)` succeeds in the same build. Counted in the
source: `DEFAULT_OPERATOR_REGISTRY` registers **10** services (memory, fs, s3, azblob, b2, cos, gcs,
obs, oss, upyun), while `via_iter` has **62** arms — and 61 of 62 configs implement `from_uri`, so
the limit is the *registry*, not the configs. Any future `uri:` support would therefore be
narrower than `config:`, which is a reason to treat it as sugar rather than as the primary form.

### The offline S3 test

Asked for and written; it needs no credentials and no connection, per finding 2:

```rust
/// s3_01 — an S3 store is constructible offline, from arguments and from a URI, identically.
///
/// No credentials, no network: OpenDAL builders are lazy, so a bucket that does not exist still
/// yields an Operator. This is what makes the advertised-type coverage test below possible.
#[cfg(feature = "opendal")]
#[test]
fn s3_01_arguments_and_uri_agree() -> Result<(), Box<dyn std::error::Error>> {
    let from_args = StoreConfig::new("s3")
        .with_prefix("remote")
        .with_config("bucket", "probe-bucket")
        .with_config("root", "data")
        .with_config("region", "eu-central-1")
        .with_config("allow_anonymous", true)
        .with_config("disable_config_load", true);
    let store = default_store_factory().create(&from_args)?;
    assert_eq!(store.key_prefix(), parse_key("remote")?);

    let via_uri = opendal::Operator::from_uri(
        "s3://probe-bucket/data?region=eu-central-1&allow_anonymous=true&disable_config_load=true",
    )?;
    let via_args = opendal::Operator::via_iter("s3", from_args.config_as_string_map()?)?;
    assert_eq!(via_uri.info().name(), via_args.info().name());
    assert_eq!(via_uri.info().root(), via_args.info().root());
    Ok(())
}

/// s3_02 — a missing required argument fails at construction, not at first use.
#[cfg(feature = "opendal")]
#[test]
fn s3_02_missing_region_fails_at_construction() {
    let config = StoreConfig::new("s3").with_config("bucket", "probe-bucket");
    match default_store_factory().create(&config) {
        Ok(_) => panic!("S3 must not build without a region"),
        Err(e) => assert!(e.message.contains("region"), "got: {}", e.message),
    }
}
```

**`s3_01` is the interesting one**, because it asserts the argument and URI forms agree rather than
asserting a hard-coded root — so it keeps testing the property if OpenDAL changes how it derives
either.

**It will not compile as written until `services-s3` is enabled**, which is
[`STORE-OPENDAL-SERVICES-NOT-ENABLED`](../../issues/STORE-OPENDAL-SERVICES-NOT-ENABLED.md) — see
below. Phase 4 must either sequence that fix first or gate this test.

### The defect that answering this question exposed

Running the probe against every advertised type gave:

```
PROBE memory: OK      PROBE filesystem: OK      PROBE fs: OK
PROBE s3:   ERR ... scheme is not enabled or supported
PROBE ftp:  ERR ... scheme is not enabled or supported
PROBE http: ERR ... scheme is not enabled or supported
```

`liquers-store` declares `opendal = { version = "0.55.0", optional = true }` with **no features**,
and OpenDAL's `default` enables only `services-memory`. `cargo tree -p liquers-axum -e features -i
opendal` confirms the server crate gets `services-memory` alone — not even `fs`. So **all 21 types in
`OPENDAL_STORE_TYPES` are unconstructible in any consumer build**, while
`specs/reference/STORE_CONFIG_FSD.md` documents S3, GCS, Azure Blob, FTP, SFTP and WebDAV with worked
examples.

`fs` passes in the crate's own tests only because dev-dependencies add `services-fs`, and Cargo
unifies features across normal and dev dependencies when building tests. **The suite is green
because of a dev-dependency**, which is the worst case: it conceals the defect rather than merely
missing it.

Filed as [`STORE-OPENDAL-SERVICES-NOT-ENABLED`](../../issues/STORE-OPENDAL-SERVICES-NOT-ENABLED.md)
(**P0**, S) — §4.4's "a documented feature that does not work". Independent of this design and not
fixed by it. The design does make the symptom *reportable*:
`StoreTypeAvailability::Unavailable("requires the 'services-s3' feature")` is the right shape for
saying so. Reporting it well is not the same as the type working.

### What was run and why it is not in the tree

A probe test and a temporary `services-s3`/`services-ftp`/`services-http` addition to
`liquers-store`'s dev-dependencies produced every result above. Both were reverted: enabling service
features is the *fix* for the P0 issue, and landing it inside an unapproved design would bury a
user-facing defect fix in a refactor. The tests above are specified here and implemented in Phase 4.

### What checking this against OpenDAL actually found

Everything reaching OpenDAL goes through `config_as_string_map`, which flattens each
`serde_json::Value` to a `String`, and then through OpenDAL's `ConfigDeserializer`, which parses it
back. The two must agree. Reading `opendal-0.55.0/src/raw/serde_util.rs`:

| Value | We send | OpenDAL expects | Agrees? |
|---|---|---|---|
| Boolean | `"true"` / `"false"` | `"true"`/`"on"`, `"false"`/`"off"`, case-insensitive | **yes** |
| Integer | `"1000"` | `parse::<usize>()` etc. | **yes** |
| Float `1000.0` | `"1000.0"` | `parse::<usize>()` on an integer field | **no** — parse error |
| Array | `["a","b"]` (JSON text) | `split(',')` — comma-separated, unquoted | **no** — splits into garbage |
| Null | `"null"` | a four-character string | **no** |

**The array row is a defect that exists today**, independent of this design, and it is filed as
[`STORE-OPENDAL-LIST-OPTION-MISPARSED`](../../issues/STORE-OPENDAL-LIST-OPTION-MISPARSED.md) (P2, S).
Its reach is currently one field — `endpoints: Option<Vec<String>>` on `tikv`, the only non-scalar
across all of OpenDAL 0.55's service configs — and `tikv` is reachable only through the
`opendal_tikv` escape hatch. `config_as_string_map` moves to `liquers-core` unchanged, so the
behaviour crosses the move intact; this design neither causes nor fixes it.

**It settles the `StoreArgumentType::Array` question, though.** An OpenDAL list option is spelled as
a *comma-separated string* in the document, so it is `StoreArgumentType::String` with the convention
in its `doc` — not `Array`. `Array` therefore still has exactly one legitimate user, the browser
`http` store's `keys`, which really is a YAML list because `parse_key_list` reads a
`serde_json::Value::Array`. And `Object` still has **none**: no OpenDAL service config has a
map-valued field.

### Resolved: how OpenDAL types are described without maintaining OpenDAL's documentation

The naive reading of "a factory describes the arguments for each store type" would have
`liquers-store` carry hand-written entries for ~134 fields it does not own. That is not merely
tedious — it is a **silent-drift trap**: when OpenDAL adds a field, changes a type or renames one,
our copy becomes wrong with nothing to detect it, and the wrongness is worse than absence because a
reader believes it.

Two mechanisms remove the trap, and they are independent — the first alone is sufficient.

Both are **committed to the design** (maintainer decision). `ArgumentCoverage` is required
regardless of OpenDAL: Liquers is meant to accept backends it does not own, and any such backend can
only be described incompletely, because its arguments change on someone else's release schedule.

#### 1. `ArgumentCoverage::Partial` — say that the list is guidance

Core and browser store types are `Complete`: Liquers owns them and the argument list *is* the
specification. OpenDAL types are `Partial { authority: "<OpenDAL's docs URL>" }`: the list is
guidance, unlisted keys pass through to the backend, and the truth lives upstream.

An incomplete list is only a lie if completeness was claimed. Under `Partial`, OpenDAL 0.56 adding a
field makes our description *less complete* — never *wrong* — and nothing has to be noticed for the
documentation to stay honest. This is what a user or a coding agent needs: enough to write a working
`config:` block, plus an unambiguous pointer to the authority for the rest.

#### 2. Derive the field names from the linked OpenDAL

Better than describing fewer fields by hand is describing them **without writing them down at all**.
Three properties of OpenDAL 0.55, each verified against the source rather than assumed:

| Fact | Evidence |
|---|---|
| Every service config is `Serialize` | `pub trait Configurator: Serialize + DeserializeOwned + Debug + 'static` (`src/types/builder.rs:123`) — a trait bound, not a convention |
| Every service config derives `Default` | all **62** of `src/services/*/config.rs` |
| No field is skipped on serialization | zero `skip_serializing_if` across those 62 files |

Therefore `serde_json::to_value(S3Config::default())` yields a JSON object containing **every field
name and its default value**, taken from the OpenDAL version actually linked:

```rust
fn derived_arguments<C: opendal::Configurator + Default>() -> Vec<StoreArgumentInfo> {
    match serde_json::to_value(C::default()) {
        Ok(serde_json::Value::Object(fields)) => fields
            .into_iter()
            .map(|(name, default)| StoreArgumentInfo::derived(name, default))
            .collect(),
        // A config that is not a JSON object cannot be described; `Partial` already says the
        // list may be incomplete, so an empty list is a correct answer rather than an error.
        Ok(_) | Err(_) => Vec::new(),
    }
}
```

**This cannot drift, because it is not written down.** A field added upstream appears; a field
removed disappears; a type change shows up in the default's JSON type.

What it yields and what it does not:

| | Derived? |
|---|---|
| Canonical field name (serde `alias`es collapse to it) | **yes** |
| Default value | **yes** |
| Type, where the default is not null — `bool` → `false`, `String` → `""` | **yes** |
| Type of an `Option<T>` field, whose default is `null` | no → `StoreArgumentType::Any` |
| Documentation text | no — Rust doc comments do not survive to runtime |
| Required-ness | no — every config is `#[serde(default)]`; requiredness is a backend runtime concern |

**The maintenance boundary this draws is the important part.** What stays hand-written is a
`store_type → config type` mapping of about 20 entries, which changes only when a *service* is added
or removed — the same cadence as `OPENDAL_STORE_TYPES`, which is hand-maintained today anyway.
Field-level churn, which is where the volume and the volatility both are, becomes free. And the
failure mode of forgetting an entry is benign: that type reports no arguments, which under `Partial`
is honest rather than wrong.

On top of the derived list, hand-write `doc` text for only the two or three arguments per type where
guidance genuinely helps — `bucket`, `root`, `endpoint`, and the `${VAR}` convention for secrets.
That is a handful of sentences about *usage*, not a transcription of someone else's API.

#### Both, and how they compose

**2 fills in the names; 1 states that the result is still not a contract.** They are not
alternatives and neither subsumes the other: derivation gives an accurate list of *what OpenDAL 0.55
has*, and `Partial` says that list is guidance about a surface Liquers does not own — which stays
true even when derivation is perfect, because doc text, required-ness, and valid argument
*combinations* are still missing.

Order of work in Phase 4: `ArgumentCoverage` first (it is a `StoreTypeInfo` field and the browser
and core factories need it), then derivation (it only fills `arguments` for the OpenDAL factory).

**What is deliberately not attempted.** S3's credential modes are mutually exclusive in practice —
static keys, or assume-role (`role_arn` + `external_id`), or customer-managed SSE keys — and
`StoreTypeInfo` cannot express that a group of arguments is exclusive, nor that one argument
requires another. Encoding argument-group constraints is a much larger feature and is not proposed.
The guide should say plainly that these descriptions list arguments, not valid combinations.

## Corner Cases## Corner Cases

### Memory

Not a meaningful axis here. A factory is constructed, consumed while the router is built, and
dropped; `StoreTypeInfo` is a handful of `String`s per store type, on the order of a kilobyte for
the whole OpenDAL set. The one thing worth stating is a **constraint on implementations rather than
a property of this code**: `create` must not fetch bulk data, because every store in a document is
constructed at startup and construction *is* the validation. A store type that pre-fetches a remote
metadata database is making a trade-off it must document.

**Test:** none. This is a documented contract, not a checkable property — recorded in the guide.

### Concurrency

Also not a meaningful axis, and the notable thing is what is *absent*. The trait carries no
`Send`/`Sync` bound and must not gain one: `WebStoreFactory` holds `js_sys::Object` handles and is
`!Send`, and `WEB-NATIVE-IO-TIER2` will add an IndexedDB store with the same property. Factories are
never shared across threads — one is built, used, and dropped on one thread.

**Test:** the browser suite compiling at all is the assertion. A `Send` bound added by accident
fails `cargo test -p liquers-web --target wasm32-unknown-unknown` immediately.

### Errors

| Input | Expected |
|---|---|
| `type: postgress` (typo) | `NotSupported`, message lists supported types |
| `type: s3`, `opendal` feature off | `NotSupported`, message names the feature — never "unknown" |
| `type: filesystem` on wasm32 | `NotSupported`, message names the target |
| `filesystem` with no `path` | `General`, "Missing required configuration 'path'" (unchanged) |
| `${UNSET_VAR}` | `General`, names the variable (unchanged) |
| `${UNCLOSED` | `ParseError` (unchanged) |
| Malformed YAML/JSON/TOML | `ParseError` (unchanged) |

### Serialization

`StoreTypeInfo` / `StoreArgumentInfo` / `StoreArgumentType` / `StoreTypeAvailability` derive
`Serialize + Deserialize`; a round-trip test asserts they survive it, so a later exporter is not
starting from an unverified assumption. `StoreTypeMap` and `ChainedStoreFactory` are deliberately
not serializable — they hold `Box<dyn Fn…>` and `Box<dyn StoreFactory>`.

`StoreRouterConfig` round-trips YAML → JSON → YAML unchanged. That behaviour exists today; the test
re-asserts it **in `liquers-core`** after the move, which is the point.

### Cross-crate

The corner case that matters most and is easiest to miss: **a `liquers-core`-only build must produce
a working router.** If it cannot, the design has failed its stated purpose regardless of what the
other tests say. Test 7 below is that assertion, and it belongs in `liquers-core/tests/` precisely
because a test there cannot accidentally reach `liquers-store`.

## Documentation and Learning Log

Guide-worthy material identified while writing these examples, for
`specs/guides/STORE_FACTORY_GUIDE.md`:

| Guide section | Source | Executable evidence to link |
|---|---|---|
| "Build a router from a document" | Scenario 1 | `test_store_router_from_yaml` |
| "Contribute your own store types" | Scenario 2 | `liquers-web/src/store/builder.rs` |
| "Describe your arguments" | Scenario 2's `StoreTypeInfo` block | same |
| "Override someone else's store type" | Pitfall 1 | `chain03_earlier_factory_wins` |
| "Why your type is unavailable, not unknown" | Errors table | `factory04` rewritten |
| "`create` must be fast" | Corner cases §Memory | none — a contract |

Learning to carry to Phase 5:

- `StoreArgumentType::Array` has exactly one legitimate user — the browser `http` store's `keys` —
  and Scenario 4 explains why OpenDAL does not add a second: an OpenDAL list option is a
  comma-separated string, not a document list. `Object` still has **none**; no OpenDAL service config
  has a map-valued field. The open question is now answerable with evidence rather than absence.
- `Boolean` and `Number` earn their place through S3 and nothing else. Had Scenario 4 not been
  written, both would have looked speculative.
- The browser store types' arguments were documented only in a module doc-comment. Moving them into
  `StoreTypeInfo` is the first time they are machine-readable — a benefit Phase 1 did not claim.
- `factory02`'s doc comment is a small essay on why factories precede built-ins. Rewriting it is the
  clearest single artifact of what this design changes, and worth quoting in the guide.

## Test Plan

### Existing tests: what moves, what changes, what goes

18 tests exist in the code being moved. **This inventory is the most important content of this
phase**, because three of them assert behaviour this design deliberately inverts, and a refactor
that quietly rewrites its own assertions is indistinguishable from a regression.

#### Move verbatim to `liquers-core/src/store_config.rs` (11)

`test_expand_env_vars_simple`, `_multiple`, `_no_vars`, `_missing`, `_unclosed`,
`test_store_router_config_yaml`, `test_store_router_config_json`, `test_store_config_builder`,
`test_key_prefix_parsing`, `test_key_prefix_empty`, and the `expand_env_vars` doc-test (its `use`
line changes crate). No assertion changes — if any needs changing, the move was not behaviour-
preserving and that is a finding.

#### Stay in `liquers-store` (2)

`test_is_opendal_store_type`, `test_get_opendal_scheme` — they test the OpenDAL type tables, which
do not move. Their module home changes from `config.rs` to `store_factory.rs`.

#### Rewritten, because the design changes what is true (3)

| Test | Asserts today | Must assert after |
|---|---|---|
| `factory02_factory_precedes_builtin` | A factory claiming `http` beats the built-in OpenDAL dispatch, "consulted **before** the built-in types" | A factory chained **before** the OpenDAL factory wins. Renamed `chain03_earlier_factory_wins`. Its doc comment — an argument that consulting factories second "would make that impossible" — is replaced by the real mechanism |
| `factory03_unclaimed_type_falls_through` | An unclaimed type "still reaches the built-ins" | **Deleted and replaced.** There are no built-ins. `chain05_unclaimed_type_lists_supported_types` asserts the error and that its message names the supported set |
| `factory04_gated_type_names_the_feature` | `create_store` on `s3` with `opendal` off names the feature | Same guarantee via `StoreTypeAvailability::Unavailable`; the call becomes the chain's `create`. Keeps its `#[cfg(not(feature = "opendal"))]` gate and both assertions, including "a gated-off type is not an unknown type" |

`factory01_custom_type_is_created` survives with only an API update (`with_factory` → a chain), as do
`test_create_memory_store`, `test_create_filesystem_store`, `test_store_router_from_yaml`,
`test_store_router_from_json`, `test_unknown_store_type`, `test_filesystem_missing_path`.

### New unit tests — `liquers-core/src/store_factory.rs` (13)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // --- StoreTypeMap ---
    #[test] fn map01_claims_only_registered_types();
    #[test] fn map02_create_dispatches_to_the_registered_constructor();
    #[test] fn map03_store_types_is_sorted();          // BTreeMap: deterministic error text
    #[test] fn map04_unregistered_type_errors();

    // --- ChainedStoreFactory ---
    #[test] fn chain01_empty_chain_claims_nothing();
    #[test] fn chain02_single_factory_behaves_as_itself();
    #[test] fn chain03_earlier_factory_wins();          // replaces factory02
    #[test] fn chain04_store_types_is_the_union_first_wins();
    #[test] fn chain05_unclaimed_type_lists_supported_types();
    #[test] fn chain06_unavailable_type_reports_its_reason();

    // --- core factory ---
    #[test] fn core01_claims_memory_and_filesystem();
    #[test] fn core02_memory_store_is_constructed();
    #[cfg(target_arch = "wasm32")]
    #[test] fn core03_filesystem_is_listed_but_unavailable_on_wasm();
    #[test] fn core04_complete_type_rejects_an_unknown_key();   // ArgumentCoverage::Complete
}
```

Two are worth explaining because they are the ones that could be written vacuously:

**`chain04_store_types_is_the_union_first_wins`.** Two factories both claiming `memory`, with
different `doc` strings. The union must contain `memory` **once**, with the *first* factory's
description. Without this, `store_types()` could advertise a description belonging to a factory that
will never be called — and since that list is what the error message prints, the message would lie.

**`chain05_unclaimed_type_lists_supported_types`.** Asserts on message *content*, not just
`is_err()` — the supported names appear, the unclaimed one appears, and (per `factory04`'s surviving
rule) an unavailable type is not described as unknown. `test_unknown_store_type` today asserts only
`is_err()`, which would pass against an empty message.

### New unit tests — `liquers-store/src/store_factory.rs` (10)

```rust
#[test] fn opendal01_claims_the_opendal_type_table();
#[test] fn opendal02_claims_the_opendal_underscore_prefix();
#[cfg(feature = "opendal")]
#[test] fn opendal03_constructs_a_store();
#[cfg(not(feature = "opendal"))]
#[test] fn factory04_gated_type_names_the_feature();     // preserved, retargeted
#[test] fn default01_chain_is_core_then_opendal();
#[test] fn default02_core_types_are_not_shadowed_by_opendal();  // `fs` != `filesystem`

// --- ArgumentCoverage ---
#[test] fn coverage01_opendal_types_are_partial_with_an_authority();
#[test] fn coverage02_partial_type_accepts_an_undescribed_key();

// --- derived arguments ---
#[cfg(feature = "opendal")]
#[test] fn derive01_s3_arguments_come_from_the_linked_opendal();
#[cfg(feature = "opendal")]
#[test] fn derive02_default_value_determines_the_argument_type();
```

**`derive01` must not assert an exhaustive list** — that would reintroduce by the back door exactly
the maintenance burden derivation removes, failing on every OpenDAL upgrade that adds a field.
Assert instead that a few long-stable names are *present* (`bucket`, `region`, `root` for `s3`) and
that the list is non-empty. The test then checks the mechanism, not the dependency's contents.

**`derive02`** asserts the inference rule: `enable_virtual_host_style` has default `false`, so it
comes back `Boolean`; an `Option<String>` field defaults to `null`, so it comes back `Any`. That
second half is the honest limit of derivation, and asserting it stops someone "fixing" it later
without understanding why it is there.

**`coverage02`** is the behavioural half of `ArgumentCoverage`: a configuration naming a key the
factory did not describe must still build for a `Partial` type. Its counterpart on the core side is
`core04_complete_type_rejects_an_unknown_key`.

`default02` guards a real near-miss: OpenDAL claims `fs`, core claims `filesystem`. They do not
collide today, and a future rename of either would silently change which factory serves a document.

### New integration tests — `liquers-core/tests/store_router_STORE.rs` (4)

```rust
#[tokio::test] async fn core_router01_builds_from_yaml_without_liquers_store();
#[tokio::test] async fn core_router02_routes_by_prefix_first_match_wins();
#[tokio::test] async fn core_router03_env_expansion_applies_on_build();
#[test]        fn core_router04_type_info_round_trips_through_json();
```

`core_router01` is the design's thesis stated as an assertion, and its value is structural: a test in
`liquers-core/tests/` **cannot** reach `liquers-store` — it is not a dependency — so the test cannot
pass by accident.

### Existing suites that must keep passing unchanged

| Suite | Why it matters here |
|---|---|
| `cargo test -p liquers-web --target wasm32-unknown-unknown` | `STORE11`, `c12`, and the environment-rebuild tests exercise the configuration path end to end. Their assertions must not change — only import paths |
| `cargo test -p liquers-lib --test registry_export` | Must stay green untouched: proof this design did not leak into the command surface |
| `cargo test -p liquers-axum` | The one other `liquers-store` consumer |

### Commands

```bash
cargo test -p liquers-core --lib                                    # moved + new unit tests
cargo test -p liquers-core --test store_router_STORE                # new integration tests
cargo test -p liquers-store                                         # factory suite, opendal on
cargo test -p liquers-store --no-default-features --features async_store   # factory04's gate
cargo test -p liquers-lib --lib --tests                             # regression, incl. registry_export
cargo test -p liquers-axum
bash scripts/check-build-matrix.sh                                  # + the 4 new liquers-core rows
cargo clean && cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles
```

The `--no-default-features` run is not optional: `factory04` is `#[cfg(not(feature = "opendal"))]`
and **never executes in the default configuration**. It is the only test covering the message
`StoreTypeAvailability` exists to preserve.

### Coverage summary

| Area | Tests | Note |
|---|---|---|
| Configuration data | 11 moved | Verbatim; any change is a finding |
| OpenDAL type tables | 2 moved | New module, same assertions |
| Factory machinery | 14 new | Chain order, union, availability, error text, `Complete` rejection |
| `liquers-store` factories | 12 (4 rewritten) | Gated-feature message, coverage behaviour, derivation, and the two offline S3 tests |
| Core-only router | 4 new | The design's thesis, structurally unfakeable |
| Browser | existing, retargeted | Import paths only |
| Build matrix | 4 new rows | `liquers-core` has none today |

**Total: 43 tests + 4 matrix rows**, of which 20 assert behaviour that does not exist yet and 3
replace assertions this design invalidates.

Two of them — `s3_01` and `s3_02` — cannot compile until
[`STORE-OPENDAL-SERVICES-NOT-ENABLED`](../../issues/STORE-OPENDAL-SERVICES-NOT-ENABLED.md) is fixed,
so Phase 4 must either sequence that first or gate them.

## Inline Review Findings

The three reviewer roles were run as sequential passes in this session rather than spawned as
parallel agents; the same independent concerns were covered and the outcomes are recorded here.

**Pass 1 — Phase 1 conformity.** No scope drift. Phase 3 introduces no capability Phase 1 did not
approve; the examples are the Phase 1 interactions made concrete. The two questions Phase 1 left open
for this phase are both answered by Scenario 2: `StoreArgumentType::Array` has exactly one user
(`keys`), and `Object` still has none.

**Pass 2 — Phase 2 conformity.** Every signature used in the examples was checked against Phase 2.
Two gaps found and fixed in Phase 2 rather than worked around here:

1. `StoreArgumentInfo`'s builder methods were never specified — Phase 2 gave them for
   `StoreTypeInfo` only, while Scenario 2 needs `new`/`required`/`with_doc`. Added.
2. `liquers-web`'s `default_store_factory` takes a `WebStoreFactory` argument, unlike core's and
   `liquers-store`'s, which take none. That is not an error — `WebStoreFactory` is stateful, holding
   page objects registered at runtime — but the convention as written implied a uniform signature.
   Recorded as a deliberate deviation with its reason.

**Pass 3 — codebase and query validation.** Test names and line references checked against
`liquers-store/src/store_builder.rs`, `liquers-store/src/config.rs` and
`liquers-web/tests/store_js_STORE.rs`; the 18-test inventory is exhaustive against those files.
`liquers-validate` was **not** run and is not applicable: this design contains no Liquers query. The
strings that look path-like (`cache/x.txt`, `data/x.txt`) are store keys in prose, not queries, and
no example evaluates anything. No command is registered, so there is nothing to check against
`specs/command_registry.yaml`.

The finding from this pass that shaped the document: the existing `factory01`–`factory04` suite was
discovered by reading the source rather than by searching for it, and `factory02` turned out to
assert — with a doc comment arguing the case at length — precisely the rule this design inverts.
That is why the test inventory leads the Test Plan instead of trailing it.
