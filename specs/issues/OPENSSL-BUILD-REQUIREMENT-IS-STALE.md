---
id: OPENSSL-BUILD-REQUIREMENT-IS-STALE
kind: issue
title: The build documents a system OpenSSL requirement that nothing in the workspace links
status: draft
priority: P3
complexity: S
area: [build, docs]
design: 
created: 2026-11-01
github:
---
## Problem

`CLAUDE.md` §"Applied measures" item 2 says `liquers-lib`'s `openssl` dependency links the system
library and "requires OpenSSL development headers (`libssl-dev`)", and both
`.github/workflows/build-matrix.yml` and `liquers-py-tests.yml` install `libssl-dev`. But
`liquers-lib/Cargo.toml` has the `openssl` dependency commented out (`#openssl = …`,
`#polars = ["dep:polars", "dep:reqwest", "dep:openssl"]`), and
`cargo tree -i openssl-sys --workspace --target all -e normal` prints nothing (2026-11-01): no
library or binary links OpenSSL. (Without `-e normal` it does, through `liquers-axum`'s
dev-dependency `reqwest` with default TLS, so only that crate's tests need it.) TLS reaches the graph through `rustls` with `aws-lc-rs` and `ring`
(opendal, polars' `object_store`). `openssl-sys` is in the local lockfile through that dev-dependency. `LANGUAGE-INTEGRATION_GUIDE.md` §PACKAGE
cites the same choice as this repository's example of not vendoring.

## Impact

Contributors and CI install a package nothing uses, and the guide's PACKAGE example is wrong. The
real native build requirement (a C compiler for `aws-lc-sys` and `ring`) is undocumented, which
matters for the wheel builds of `liquers-py-package`.

## Expected behaviour

`CLAUDE.md`, the guide's §PACKAGE sentence and the two workflows state the actual requirement
(no OpenSSL; a C toolchain for `aws-lc-sys`/`ring`), and the `libssl-dev` steps stay only where `liquers-axum`'s tests
are built, once a CI run confirms where they are needed.

## Discovery

Found 2026-11-01 while writing Phase 1 of `liquers-py-package`, whose plan item "vendored OpenSSL
for the wheels" turned out to have nothing to vendor.

Triage: overlaps `liquers-py-package` (T3, a shared change site: the guide's §PACKAGE OpenSSL
sentence and the CI workflows), kept separate by E1, since this fix is eligible for automatic
fixing (`P3`, `S`, documentation and CI only, no interface change) and the design is not. Spin-off:
to be fixed in its own branch and pull request, not in the M5 design PR; that PR owns the guide's
OpenSSL sentence. Not started yet: this session works on one designated branch, so the second
branch waits for the owner's go-ahead.

