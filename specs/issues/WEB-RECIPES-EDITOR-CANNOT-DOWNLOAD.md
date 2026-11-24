---
id: WEB-RECIPES-EDITOR-CANNOT-DOWNLOAD
kind: feature
title: The browser recipes panel cannot save or export a value to a local file
status: draft
priority: P3
complexity: M
area: [lib/ui]
design:
created: 2026-11-24
github:
---
## Problem

The recipes panel's Save as and Export (`design/recipes-editor/`, AC-19) write a local file through
`AppMessage::SaveAs`, which the runner serves with `tokio::fs` on native targets only. In a browser
(`ui::web::recipes`) both menu items are drawn disabled with "Saving files is not available here",
and a `SaveAs` message sent there is refused with a notice.

## What would fix it

A browser download: the runner serializes the value as it does natively, and the web driver hands
the bytes to the page (a `Blob` and an `<a download>` click), named by the file and the format.
That needs a way for the runner to pass bytes to the web driver, which `AppMessage` has no reply
for today.
