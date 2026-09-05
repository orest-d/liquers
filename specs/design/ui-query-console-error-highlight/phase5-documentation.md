# Phase 5: Documentation - Query Console Highlights Positioned Errors

## Current Behaviour

The egui query console now passes the latest asset error's known `Position` to its layout helper.
That helper preserves syntax-only rendering when no position is available and underlines the
matching query token when one is present. Browser HTML rendering remains out of scope.

## Documentation Decision

No reference or guide currently describes query-console error presentation, so no current-state
documentation needed revision. The design and source issue record the delivered behaviour,
validation, and native-egui build limitation.

## Maintenance

The source issue is closed. Future browser-side highlighting should be tracked as a separate issue
rather than extending this egui-only change.
