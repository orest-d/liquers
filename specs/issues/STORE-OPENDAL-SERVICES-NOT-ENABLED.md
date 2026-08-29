---
id: STORE-OPENDAL-SERVICES-NOT-ENABLED
kind: issue
title: No OpenDAL service features are enabled, so every advertised OpenDAL store type fails to build
status: closed
priority: P0
complexity: S
area: [store/backends]
design: 
created: 2026-08-29
github:
---
## Problem

`liquers-store` declares its OpenDAL dependency with no features:

```toml
opendal = { version = "0.55.0", optional = true }
```

OpenDAL's `default` feature set enables exactly one backend — `services-memory` — plus
`executors-tokio` and `reqwest/rustls-tls`. Every service is behind its own
`services-*` feature, and `Operator::via_iter` matches the scheme against `#[cfg]`-gated arms, so an
unenabled service has no arm to match.

Meanwhile `OPENDAL_STORE_TYPES` (`liquers-store/src/config.rs`) advertises **21** types — `fs`, `s3`,
`ftp`, `gcs`, `azblob`, `sftp`, `webdav`, `github`, `hdfs`, `webhdfs`, `http`, `https`, `redis`,
`mongodb`, `postgresql`, `mysql`, `sqlite`, `dropbox`, `onedrive`, `gdrive`, `ipfs` — and
`create_store` dispatches every one of them to OpenDAL. **None of them can be constructed by a
consumer.**

Measured with `cargo tree -p liquers-axum -e features -i opendal`:

```
├── opendal feature "default"
├── opendal feature "executors-tokio"
└── opendal feature "services-memory"
```

A consumer linking `liquers-store` normally gets `services-memory` and nothing else — not even `fs`.

**Precision about who is hit.** No in-tree crate is currently broken by this, and the issue should
not claim otherwise. `liquers-axum` declares the dependency but references nothing from it
(`grep -rn liquers_store liquers-axum/src/` is empty), `liquers-web` builds with `opendal` off and
never reaches an OpenDAL type, and `liquers-lib`'s `ui_query_console_app` example constructs
`AsyncOpenDALStore` directly rather than through `create_store`. The crate's own tests are the only
in-tree caller, and dev-dependencies mask them.

The defect therefore bites **external consumers and anyone following the documented configuration
format** — which is the entire audience for `STORE_CONFIG_FSD.md`. That is enough for §4.4's
"documented feature that does not work", but the absence of an in-tree victim is exactly why it
survived unnoticed, and is worth stating rather than glossing.

Confirmed by running `create_store` directly:

```
PROBE memory: OK
PROBE filesystem: OK
PROBE fs: OK                      <- only because dev-dependencies add services-fs
PROBE s3:   ERR ... scheme is not enabled or supported
PROBE ftp:  ERR ... scheme is not enabled or supported
PROBE http: ERR ... scheme is not enabled or supported
```

## Impact

Configuring any OpenDAL-backed store fails at startup with

> Failed to create OpenDAL operator for scheme 's3': Unsupported (permanent) … scheme is not
> enabled or supported

This is the store-configuration feature's headline capability — `specs/reference/STORE_CONFIG_FSD.md`
documents S3, GCS, Azure Blob, FTP, SFTP and WebDAV with worked YAML examples — and none of it works
in any build a user would produce. There is no workaround inside Liquers: the consumer would have to
add their own `opendal` dependency with the right features and rely on Cargo feature unification,
which is neither documented nor obvious.

**Why it was not noticed.** `liquers-store`'s dev-dependencies declare
`opendal = { version = "0.55", features = ["services-fs"] }`. Cargo unifies features across normal
and dev dependencies when building tests, so the test binary gets `services-fs` while the library a
consumer links does not. Every OpenDAL test in the crate exercises `fs` — the one service the
dev-dependency happens to add — so the suite is green while the shipped artifact is broken. A test
suite that passes because of a dev-dependency is the worst case: it actively conceals the defect.

The type table is also enforced nowhere. `OPENDAL_STORE_TYPES` is a hand-written list with no
compile-time or test-time connection to the `services-*` features actually enabled, so the two can
disagree silently and indefinitely.

## Expected behaviour

A store type Liquers advertises should be constructible, or should say precisely why not.

Two decisions, and they are separable:

1. **Which services to enable by default.** Enabling all 21 is the simple answer and costs
   compile time and binary size in a workspace that already documents disk pressure
   (`CLAUDE.md` §"Building and testing"). A reasonable default is the common set — `services-fs`,
   `services-s3`, `services-gcs`, `services-azblob`, `services-http`, `services-webdav`,
   `services-ftp`, `services-sftp` — with the rest behind opt-in features of `liquers-store`.
2. **Keeping the advertised list honest.** Whatever is chosen, `OPENDAL_STORE_TYPES` must not name a
   type this build cannot construct. Either gate the table entries with the same `#[cfg]`s, or add a
   test that constructs every advertised type and fails when one is unsupported. The second is
   cheap and catches drift on an OpenDAL upgrade.

Note that OpenDAL construction is **offline** — verified: `create_store` for `s3` with a fake bucket
succeeds without network access, and an unreachable `ftp.invalid:21` endpoint constructs fine — so a
test that constructs every advertised type needs no credentials and no connection.

## The symptom is now reported correctly, which is not the same as fixed

`design/store-factories-in-core/` at first made this **worse**, and a review caught it. Its
`OpendalStoreFactory` marked a store type `Available` whenever `liquers-store`'s own `opendal`
feature was on — but that feature enables only `dep:opendal`, and OpenDAL's `default` compiles in
`services-memory` alone. So 20 of the 21 advertised types were reported as supported while
`Operator::via_iter` rejected them as disabled: the old code merely failed at construction, the new
metadata actively advertised them.

Fixed in that PR by asking OpenDAL rather than guessing — `opendal::Scheme::enabled()` reports the
services actually compiled in, so availability tracks whatever the dependency graph resolved:

```
Store type 's3' is not available in this build: OpenDAL is linked but its 'services-s3' feature is
not enabled, so the 's3' service is not compiled in
```

`availability01_declared_availability_matches_create` asserts the two APIs agree, in both feature
configurations.

**None of that makes the types work.** A user configuring `s3` still cannot get an S3 store; they
now get a message naming the reason instead of one that reads like a typo. This issue is still open
and still P0: the fix is to enable the service features, which is a decision about what the product
ships, not about how it reports.

## Also blocks deriving the argument descriptions

Found while implementing `design/store-factories-in-core/` Phase 4 Step 9. That design reports each
OpenDAL store type's configuration arguments by **deriving** them from the linked OpenDAL rather
than hand-writing them — sound because `Configurator` bounds `Serialize`, every service config
derives `Default`, and none carries `skip_serializing_if`, so `serde_json::to_value(C::default())`
yields every field name and default.

Deriving requires *naming* the config type, and `opendal::services::S3Config` is behind
`#[cfg(feature = "services-s3")]` (`opendal-0.55.0/src/services/mod.rs`). With no service features
enabled, `cargo check` fails with `cannot find type FsConfig in module opendal::services`, and the
only nameable config is `MemoryConfig` — not an `OPENDAL_STORE_TYPES` entry.

So this issue blocks two things beyond the store types themselves: the offline S3 tests, and the
derived argument descriptions. Both were deferred rather than worked around.

## Discovery

Found while writing OpenDAL configuration examples for
[`design/store-factories-in-core/`](../design/store-factories-in-core/phase3-examples.md) Phase 3, by
running `create_store` against each advertised type to check what a `StoreTypeInfo` should say about
availability. Independent of that design and not fixed by it: the manifest and the type table both
move or stay unchanged in behaviour.

The design does make the symptom clearer rather than fixing it — `StoreTypeAvailability` is intended
to distinguish "unknown type" from "known but unavailable in this build", which is the right shape
for reporting this. But reporting it well is not the same as the type working.

## Resolution

Fixed on `claude/store-opendal-services-not-enabled-q7zt7w`.

**What ships.** `liquers-store` now declares one Cargo feature per advertised type, named after
OpenDAL's own, and `default` enables `services-default`: `fs`, `s3`, `gcs`, `azblob`, `http`,
`https`, `webdav`, `ftp`, `github`, `webhdfs`, `dropbox`, `onedrive`, `gdrive`, `ipfs`. `sftp` is
enabled on Unix through a `[target.'cfg(unix)'.dependencies]` row. **15 of the 21 advertised types
are constructible in a default consumer build, against 1 before** — and that 1 only through the
dev-dependency leak.

Six are opt-in, each for a reason established by measurement rather than taste:

- `hdfs` — `hdfs-sys`'s build script calls `find_jvm()` unconditionally. A default build would
  fail outright on any machine without Java. It compiled during investigation only because the
  container happened to have a JVM.
- `redis`, `mongodb`, `postgresql`, `mysql`, `sqlite` — +104 crates between them (370 total
  against 242 for `services-default`, measured with `cargo tree -e normal --no-dedupe`).
- `sftp` is not opt-in but is not a plain default either: `openssh` uses `std::os::unix`
  unconditionally, so `services-sftp` in `default` would break every Windows build of
  `liquers-store` and `liquers-axum`.

The default set costs +102 crates (140 → 242), +126 with `sftp` on Unix.

**Keeping the list honest**, which the issue asked for separately:
`availability02_advertised_types_match_the_enabled_features` compares `OPENDAL_STORE_TYPES`
against a hand-written per-type `cfg!(feature = …)` table on one side and `Scheme::enabled()` on
the other. The two are arrived at independently — the manifest's claim and OpenDAL's — so the
assertion is not tautological, and it fails on drift in either direction: a type added with no
feature, a feature dropped from `services-default`, or an OpenDAL upgrade that renames a scheme.

**A second defect, found only because the services were turned on.** `https` is an advertised type
and `Scheme::from_str("https")` resolves to `Scheme::Http`, so availability reporting always said
it was fine — but `Operator::via_iter` matches the canonical scheme constant only, so
`via_iter("https", …)` failed however many features were on. It had never been reachable before,
because `https` was unavailable for the other reason and never got that far. `create` now resolves
to a `Scheme` and passes `into_static()`; `availability05_an_alias_scheme_builds` covers it,
including through the `opendal_https` escape hatch.

**The masking dev-dependency is gone.** `liquers-store`'s `opendal = { features = ["services-fs"] }`
dev-dependency, which put `services-fs` into the test binary while the shipped library had no
service at all, is deleted. Two existing tests that constructed `fs` under
`#[cfg(feature = "opendal")]` now carry `#[cfg(feature = "services-fs")]`, which is what they
always meant.

**Evidence.** `cargo test -p liquers-store` in five configurations (default; `async_store`;
`async_store,opendal`; `async_store,services-s3`; default + `services-sqlite`) — 24 tests in the
default build, all passing. `cargo test -p liquers-lib --lib --tests` (302 + 14 suites) and
`cargo test -p liquers-axum --lib --tests` green. `bash scripts/check-build-matrix.sh`: all 15
configurations OK, including a new `--no-default-features --features async_store,opendal` row for
the service-less state this issue was about.

**Both things this issue blocked are unblocked**, neither done here:
`STORE-OPENDAL-ARGUMENTS-NOT-DERIVED` (the config types are now nameable) and the offline S3
tests, of which `availability03_documented_s3_configuration_builds` is the first.

Filed in passing: `STORE-OPENDAL-WITHOUT-ASYNC-STORE-BROKEN`, a pre-existing feature-gating defect
confirmed against `35bba67`.
