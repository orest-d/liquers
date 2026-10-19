---
id: WEB-RECORD-SOURCE-RESOLVES-THROUGH-SINGLETON
kind: issue
title: A RecordSource from an explicit Environment streams through the singleton
status: draft
priority: P3
complexity: M
area: [web, records]
created: 2026-10-18
github:
---

## Problem

`RecordSource.stream()` and `materialize()` in `liquers-web/src/records.rs` build their chunk
resolver from `environment::shared_env()`, the global singleton, whatever environment produced the
source. For a source returned by `new Environment().evaluate(…)`:

- if the singleton was never initialized, both reject, even for an `InMemorySource`, whose stream
  never uses the resolver;
- otherwise, manifest chunks are evaluated against the singleton's command registry and store, not
  the instance's.

## Impact

Only explicit `Environment` instances are affected; the singleton, which is the documented and
usual path, is correct. Explicit instances already have other limits (`registerCommand` is refused
on them, and `configureStore` targets the singleton).

## Expected behaviour

A source handle resolves chunks through the environment whose evaluation produced it.

## Notes

The handle is created in `to_js_custom`, which receives no environment, and `value_to_js` is called
from several generic sites (`eval.rs`, `asset.rs`, the command adapter). Carrying the origin needs
either a conversion context through `JsValueBridge`, or the environment recorded where an explicit
instance's evaluation converts its result. A cheaper partial step is to build the resolver lazily,
so that sources which never resolve (in-memory) do not need the singleton.

## Discovery

Codex review of PR #157 (`liquers-web-records-bridge`), 2026-10-18.
