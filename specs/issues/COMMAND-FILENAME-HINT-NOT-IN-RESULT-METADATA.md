---
id: COMMAND-FILENAME-HINT-NOT-IN-RESULT-METADATA
kind: issue
title: A command declared with a filename does not give its result that file name
status: draft
priority: P3
complexity: S
area: [core/commands]
design:
created: 2026-11-10
github:
---

## Problem

`register_command!` takes `filename: "text.txt"`, and a JavaScript declaration takes `filename`.
The value is kept in the command's metadata, but the result of a query that ends with the command
does not carry it: the asset's `filename` and `data_format` stay empty unless the query itself ends
in a file name.

```js
liquers.registerCommand({ name: "report", filename: "report.txt", run: () => "x" });
const m = (await (await liquers.getAsset("report")).get()).metadata();
// m.filename === null, m.data_format === null
```

## Impact

A command cannot say what format its result naturally has, so a query without a file name is
served as `application/octet-stream`, and each caller must know to add one. The user guides tell
readers to end a query with a file name and do not document the statement.

## Expected behaviour

Either the last command's `filename` is the default file name of the result when the query names
none, or the statement is documented as a hint for user interfaces only.

## Discovery

Writing the Rust and Web user guides (2026-11-10); the JavaScript observation above is from a
probe against the `records` build of `liquers-web`.
