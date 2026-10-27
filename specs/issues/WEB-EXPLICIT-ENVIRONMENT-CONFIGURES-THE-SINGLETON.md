---
id: WEB-EXPLICIT-ENVIRONMENT-CONFIGURES-THE-SINGLETON
kind: issue
title: configureStore and registerStoreObject on a new Environment() configure the singleton instead
status: draft
priority: P2
complexity: M
area: [web]
design:
created: 2026-10-26
github:
---
## Problem

`LiquersEnvironment::configure_store` and `register_store_object` (`liquers-web/src/environment.rs`)
call `configure_store_on` / `register_store_object_on`, which act on the global singleton, whatever
handle they are called on. On a `new Environment()` the call therefore changes the page's
environment and leaves the instance with no store, while `instance.store()` reports the instance's
own (empty) store. `registerCommand` on an instance is refused with `not_supported`
(`POST-INIT-COMMAND-REGISTRATION`), so an instance can be neither given commands nor a store.

The maintainer's rule for `liquers-web` is that nothing resolves through the global singleton when
the environment is known; a method on an instance handle is the clearest case of a known
environment.

## Impact

A test that builds an isolated environment and configures a store on it silently configures the
shared one instead, and the instance's `-R/` queries fail with `asset_not_found`. Workaround: use
the singleton and `shutdown()` between tests.

## Expected behaviour

Either a method on an instance acts on that instance (which needs the rebuild machinery the
singleton has, as for commands), or it is refused with `not_supported` like `registerCommand`,
rather than acting elsewhere.

## Discovery

Writing `specs/reference/JAVASCRIPT_INTEGRATION.md` (2026-10-26).
