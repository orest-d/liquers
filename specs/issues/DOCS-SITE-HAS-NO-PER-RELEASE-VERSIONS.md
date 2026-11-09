---
id: DOCS-SITE-HAS-NO-PER-RELEASE-VERSIONS
kind: feature
title: The documentation site publishes only the latest main, with no documentation per release
status: draft
priority: P3
complexity: M
area: [docs, build]
design: 
created: 2026-11-09
github:
---
## Problem

`docs-site.yml` deploys one site, built from the latest `main`, as a Pages artifact. Each
deployment replaces the whole site, so there is nowhere to keep the guides and API references of an
earlier release. Once the Python wheels are published, a user of version N reads the documentation
of whatever `main` is today.

## Impact

Low while nothing is released: there is one version and it is current. It grows with the first
releases, when an API reference can describe a function the installed wheel does not have. The
workaround is building the docs from a release tag locally (`scripts/build-docs.sh`).

## Expected behaviour

The site keeps the documentation of each release under its own path (for example `v0.3/`), with
the latest `main` as the default and a version switcher on the landing page and in the guides. This
needs a design: Pages artifact deployments cannot keep older folders, so either the build gathers
earlier versions (from release assets or a `gh-pages`-style branch) or deployment moves to a
branch.

## Discovery

Noted while writing `specs/guides/PROJECT_SITE_GUIDE.md` §4.4, alongside the landing page (PR #266).
