---
id: UI-INTERFACE-FSD-DESCRIBES-UNBUILT-API
kind: issue
title: UI_INTERFACE_FSD describes a UI API that was never built
status: draft
priority: P2
complexity: M
area: [docs, lib/ui]
design:
created: 2026-10-27
github:
---
## Problem

`specs/reference/UI_INTERFACE_FSD.md` (reviewed 2026-03-02, "content unchanged since" import)
describes string handles, `UiScope`, `AppStateProvider`, `AssetAppStateProvider`,
`DirectAppStateProvider`, `ElementType`/`LayoutKind`, an `ExtValue::UiWidget` and
`liquers-lib/src/ui/traits.rs` / `navigation.rs` / `asset_provider.rs`. None of these exist. The
implemented model is `UIHandle(u64)`, the `AppState` trait with `DirectAppState`, the `UIElement`
trait (typetag), `UIContext`, `UIPayload` / `SimpleUIPayload`, `AppMessage`, `AppRunner`,
`UiAction`, `Invalidation` and the `lui` commands (`liquers-lib/src/ui/`). `reference/` documents
must be true at HEAD.

## Impact

Anyone designing against the reference (the `web-ui-use` and `web-ui-define` designs, the open
`ui-events` and `console-mounts-nested-element` designs) has to read the code instead. A reader
who trusts it designs against types that do not exist.

## Expected behaviour

Rewrite the reference from `liquers-lib/src/ui/` as implemented, keeping the open questions that
are still open, with a `## History` row.

## Discovery

Found while designing `web-ui-use` (2026-10-27): every type the FSD names under "AppState Access
Design" is absent from `liquers-lib/src/ui/`.
