---
id: WEB-MODULE-HAS-NO-VERSIONED-CDN-ADDRESS
kind: feature
title: The browser module has no npm package and no versioned address on a CDN
status: draft
priority: P2
complexity: M
area: [web, build]
design:
created: 2026-11-10
github:
---

## Problem

The only public copy of `liquers_web.js` and `liquers_web_bg.wasm` is the one the documentation
site publishes at `https://orest-d.github.io/liquers/pkg/liquers-web/`
(`scripts/build-docs.sh`, `PROJECT_SITE_GUIDE.md` §3.5). It is rebuilt from `main` on every
deployment, has no version in its address and sits on a documentation host. There is no npm
package, so no public CDN serves the module either.

## Impact

A page that imports the published copy can break when `main` changes. The Web user guide
(section 2.3) therefore tells readers to copy the files for anything they publish, which is manual
and leaves them without updates. The examples of the guide depend on the unversioned address.

## Expected behaviour

Each release of `liquers-web` is published as an npm package (the `--target web` output with a
`package.json`), so that the public CDNs serve it at a versioned, immutable address. The Web guide,
its examples and the runner's import pattern then use that address, and `pkg/liquers-web/` on the
site stays for a while for old links. Open points: the package name, whether the default package
includes the record library (about 4 MB more), and a release workflow like `liquers-py-release.yml`.

## Discovery

Requested by the owner when the Web user guide was written (2026-11-10): the goal is to host the
wasm on a CDN, with GitHub Pages in the meantime.
