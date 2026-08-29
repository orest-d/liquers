---
id: STORE-OPENDAL-SERVICES-NOT-ENABLED
kind: issue
title: No OpenDAL service features are enabled, so every advertised OpenDAL store type fails to build
status: draft
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

`liquers-axum` — the HTTP server, the most consumer-like crate in the workspace — gets
`services-memory` and nothing else. Not even `fs`.

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

## Discovery

Found while writing OpenDAL configuration examples for
[`design/store-factories-in-core/`](../design/store-factories-in-core/phase3-examples.md) Phase 3, by
running `create_store` against each advertised type to check what a `StoreTypeInfo` should say about
availability. Independent of that design and not fixed by it: the manifest and the type table both
move or stay unchanged in behaviour.

The design does make the symptom clearer rather than fixing it — `StoreTypeAvailability` is intended
to distinguish "unknown type" from "known but unavailable in this build", which is the right shape
for reporting this. But reporting it well is not the same as the type working.
