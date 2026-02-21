# EGUI-ASSET-MANAGER-INTEGRATION

Status: Draft

## Summary
Replace temporary/implicit asset-manager coupling in egui widgets with a stable integration interface.

## Problem
Current widget integration includes temporary paths and direct assumptions about asset-manager access, making behavior brittle and harder to test.

## Goals
1. Define a stable adapter boundary between widgets and asset manager.
2. Decouple widget logic from transient integration details.
3. Improve testability of update/poll/render workflow.

## Proposed Scope
1. Introduce adapter trait(s) for asset fetch/poll/status operations required by widgets.
2. Refactor widget code to depend on adapter, not direct manager internals.
3. Add integration tests for update cycle and state transitions.

## Acceptance Criteria
1. Widget code no longer relies on temporary integration path.
2. Adapter-backed integration passes existing and new tests.
3. Poll/update/render loop remains functionally equivalent or improved.
