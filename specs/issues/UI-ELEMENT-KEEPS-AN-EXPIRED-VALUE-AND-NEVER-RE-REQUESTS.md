---
id: UI-ELEMENT-KEEPS-AN-EXPIRED-VALUE-AND-NEVER-RE-REQUESTS
kind: issue
title: A UI element keeps an expired asset's value and never requests the query again
status: draft
priority: P3
complexity: S
area: [lib/ui]
design:
created: 2026-11-15
github:
---
## Problem

When the asset behind a UI element expires, the element keeps showing the last value and nothing
requests the query again. `QueryConsoleElement::update` (`liquers-lib/src/ui/widgets/query_console_element.rs`)
deliberately keeps `previous_value` when a snapshot arrives with `Status::Expired` and no value,
which avoids flicker. `AssetViewElement` (`liquers-lib/src/ui/element.rs`) ignores
`AssetNotificationMessage::Expired` and `Removed` (the empty arm near the end of its notification
`match`). Neither element, nor `AppRunner`, submits the query again.

## Impact

A view of an expiring value (an `eod` recipe, a value an override replaced) silently becomes stale
until the user re-runs it. It matters more for `settable-value-accessor`: an invalidated projection
accessor expires its asset precisely so that its holders re-evaluate its query, and an element that
holds it would instead keep a dead accessor. The workaround is to submit the query again by hand.

## Expected behaviour

An element showing a query's asset can declare what happens on expiry: keep the old value until a
new one arrives (today's behaviour, without flicker) **and** re-request the query, or re-request
only on demand. For an element bound to an accessor, the default is to re-request.

## Discovery

Found while writing Phase 2 of `specs/design/settable-value-accessor/` (2026-11-15), whose
invalidation model (Phase 1 AC-15) relies on holders re-evaluating an expired accessor's query.
