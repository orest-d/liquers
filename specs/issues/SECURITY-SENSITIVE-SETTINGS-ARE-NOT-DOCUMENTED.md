---
id: SECURITY-SENSITIVE-SETTINGS-ARE-NOT-DOCUMENTED
kind: issue
title: No guideline or user documentation says which Liquers settings are security-sensitive
status: draft
priority: P2
complexity: L
area: [docs, axum, py, web]
design:
created: 2026-11-06
github:
---

## Problem

Several Liquers settings decide what an untrusted party can do, and none is documented as such:

- the HTTP API (`liquers-axum`) has no authentication and no Host or Origin check, and its store
  and assets APIs accept writes (`StoreApiBuilder` has no read-only mode);
- the MCP server limits its query tool by a command allow-list
  (`MCP-SERVER-APPLIES-NO-ACCESS-POLICY`);
- the pickle codec of liquers-py (`Environment(pickle=True)`) decodes stored bytes into Python
  objects, which runs code;
- a store mount is writable unless `read_only`;
- the planned `liquers-py-modules` search path (deferred, PR #265) executes code read from the
  store, and the planned `liquers-py-web-server` starts a server with writes enabled by default;
- the planned web route of `axum-server-assembly` serves `web/` pages on the same origin as the
  write APIs, so a page stored by any client that can write runs with access to them (stored
  cross-site scripting), and an anonymous `GET` of a recipe-backed page starts a computation;
- a query `GET` can run a command that writes, whatever the server's `read_only` says.

There is no `specs/` guideline that lists them, and none of the user guides
(`docs/guides/{rust,python,web}/`) has a section on security.

## Impact

A user can expose a store, or code execution, to any local process or, through DNS rebinding, to a
web page, without being told. Combining two features (a writable server and a module path over the
same store) is code execution, and nothing says so.

## Expected behaviour

A security guideline in `specs/guides/` naming every security-sensitive setting, its default, the
risk and the safe configuration, kept current by every design that adds one; and a "Security"
chapter (or sections) in each user guide, written from it. Owner's decision of 2026-11-06 (thread
"Design M7 web server, modules and UI"): defaults stay "batteries included", and the risks are
documented rather than defaulted away. On the web route (2026-11-06): the security section must warn
explicitly that a page stored under `web/` runs with access to the write APIs, and show how to
configure the `web` mount `read_only`; a later version may make `web` read-only by default. Today a
store is made read-only per mount or for the whole router (`STORE_CONFIG_FSD.md`), not per key
(`STORE-CONFIG-SPEC-CANNOT-SCOPE-HANDLERS-TO-A-KEY`).

## Done so far

The `server` settings that exist at HEAD (2026-11-09, `axum-server-assembly` and
`liquers-py-web-server`) are documented where they are used, pending the guideline:

| Setting | Default | Risk | Documented in |
|---|---|---|---|
| `server.host` | `127.0.0.1` | `0.0.0.0` exposes every API to the network; locally, any process or a DNS-rebinding page reaches it | `WEB_API_SPECIFICATION.md` §9, `PYTHON_INTEGRATION.md` §14, chapter 12 of every user guide (`docs/guides/_shared/serving.md`) |
| `server.read_only` | `false` | any client that reaches the server writes and deletes stored data | the same |
| `server.web` | `true` | a page stored under `web/` runs with the server's origin (stored XSS); the read-only `web` mount is shown | the same |
| `python.pickle` / `pickle=` | `false` | reading pickle runs code | `PYTHON_GUIDE.md` §4, `docs/guides/python/08-environment.md` |

Still missing: the guideline in `specs/guides/`, the MCP allow-list, the modules search path (when
it lands), and the command-writes-on-`GET` case.

## Discovery

`liquers-py-web-server` Phase 1 gate, 2026-11-06: Orest chose writes enabled by default and asked
for a security guideline and user documentation of security-sensitive settings.
