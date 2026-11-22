---
id: UI-DOCUMENT-CANNOT-NAME-ITS-THEME
kind: feature
title: A saved UI document cannot name the theme, colour mode or density it opens with
status: draft
priority: P3
complexity: S
area: [lib/ui]
design: 
created: 2026-11-21
github:
---
## Problem

The `query-editor-ergonomics` design takes the theme, colour mode and density from the host's `ui`
configuration section (its AC-16). A UI document saved from `DirectAppState` carries none of them,
so a dashboard designed for the light theme reopens in whatever the host was started with.
`DirectAppState` serializes through a hand-written snapshot (`DirectAppStateSnapshot`,
`liquers-lib/src/ui/app_state.rs`), so a field cannot simply be added with `#[serde(default)]`.

## Impact

Per-document styling is impossible; users must start the host with a matching configuration.

## Expected behaviour

`DirectAppState` (snapshot and both serde impls) keeps an optional `UiConfig`; the runner reads it
when a document is loaded and overrides the host section field by field; an `AppState` default
method (`ui_settings`) exposes it without changing other implementations.

## Discovery

Deferred from `query-editor-ergonomics` Phase 2 (2026-11-21), where review found the snapshot
serializer; the Phase 1 recommendation had kept a theme reference in `AppState`.
