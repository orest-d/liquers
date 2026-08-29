# Phase 1: High-Level Design - liquers-core No-Default-Features Build Decision

## Problem and Evidence

`cargo check -p liquers-core --no-default-features` fails because `context.rs`, `interpreter.rs`
and `store.rs` import `futures` or `async_trait` while those dependencies are gated behind
`async_store`, which is currently only a default feature.

## Expected Behaviour and Acceptance Criteria

The declared feature set must be truthful: either `liquers-core` builds without defaults, or the
manifest stops advertising an unsupported async-free core configuration. Validation is a focused
build-matrix command proving the chosen statement.

## Affected Systems

Build configuration, Store async traits, Context and Interpreter evaluation paths are affected.
No query syntax, metadata format, command namespace or store backend behaviour should change.

## Scope and Non-Goals

Scope is the smallest Cargo/source change that makes the supported feature contract honest. This
does not redesign sync store support, remove tokio from core, or alter downstream crate features.

## Compatibility, Assumptions and Questions

The key design choice is whether to support async-free core or make `futures` and `async-trait`
non-optional. Current architecture documents say async is the default, so the small repair should
prefer making the manifest honest unless Phase 2 finds a coherent gated surface.

## Documentation Assessment

If the feature contract changes, update build/store feature references such as
`specs/reference/STORE_CONFIG_FSD.md` and any build-matrix guide text. No new guide is expected.

## Review

The scope is explicit and testable, but the issue contains a real compatibility decision. Phase 2
must quantify the API effect before implementation approval.
