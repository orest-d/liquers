---
id: WEB-THROWN-BACK-LIQUERSERROR-LOSES-CONTEXT
kind: issue
title: A LiquersError thrown back from JavaScript keeps only its type and message
status: draft
priority: P3
complexity: S
area: [web]
design: web-thrown-back-error-context
created: 2026-10-27
github:
---
## Problem

`liquers-web/src/error.rs` `js_error_to_liquers` begins with "A LiquersError thrown back at us
round-trips without degradation", and hands the value to `js_value_as_liquers_error`. That
function reads only `errorType` and `message` and returns `Error::from_error(error_type, message)`,
so the `query`, `key` and `position` the `LiquersError` exposes as getters are dropped. A
page-constructed error (`new LiquersError(type, message, query)`) thrown from a command or a store
delegate loses its query the same way.

The language-integration guide's `ERROR02` ("fields survive Rust-to-language-to-Rust"; its Python
blueprint compares message, position, query and key) is the contract. `liquers-web`'s `ERROR02`
test (`tests/objects_OBJECT.rs` `error02_fields_survive_rust_language_rust`) checks only type and
message, so the `ERROR` row of `liquers-web/README.md` says `complete` while the fields are lost,
and `specs/reference/JAVASCRIPT_INTEGRATION.md` §6 describes the loss ("its `query` and `key` are
not carried over").

## Problem Example

```rust
let mut err = Error::from_error(ErrorType::KeyNotFound, "no such key".to_string());
err.query = Some("-R/data/a.txt".to_string());
err.key = Some("data/a.txt".to_string());
err.position = Position::new(3, 1, 4);
let back = js_error_to_liquers(liquers_error_to_js(err), ErrorType::General);
// today:    back.query == None, back.key == None, back.position is unknown
// expected: all three as they were
```

## Impact

An error that crosses JavaScript (a command or store delegate that rethrows what it caught) no
longer says which query or key failed or where, so the asset's error log and the axum error body
lose that context. Low severity: type and message survive.

## Expected behaviour

`js_value_as_liquers_error` also copies `query` and `key` when they are strings and `position`
when it is an `{offset, line, column}` object, so the guide's `ERROR02` holds. The traceback is
part of the same round trip once `LiquersError` has a `traceback` getter, which
`LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT` adds.

## Discovery

Found during the 2026-10-27 bulk design of the Python backlog, comparing the error round trips of
the two bindings. Not covered by `LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT`, which is about a
JavaScript `Error` (the other branch of `js_error_to_liquers`) putting its class and stack in the
message; this is the `LiquersError` branch.
