---
id: WEB-DESCRIBE-COMMAND-SHAPE-DIFFERS-FROM-DECLARATION
kind: issue
title: describeCommand returns raw CommandMetadata JSON, not the LiquersCommandInfo shape the .d.ts declares
status: draft
priority: P2
complexity: S
area: [web]
design:
created: 2026-10-26
github:
---
## Problem

`liquers-web/src/typescript.rs` declares `describeCommand` as returning `LiquersCommandInfo`:
`{ name, namespace, realm, label, doc, arguments: LiquersArgument[], argumentsInferred, module,
volatile }`, with each argument `{ name, type?, default? }`. What `describe_metadata`
(`liquers-web/src/environment.rs`) returns is the `serde` form of `CommandMetadata` with
`arguments` forced to an array and `argumentsInferred` added. Measured on a wasm build under Node
(2026-10-26), for `{ name: "repeat", state: "text", arguments: [{ name: "count", type: "int",
default: 2 }] }`:

```json
{"name":"repeat","label":"repeat","module":"javascript","state_argument":{…},
 "arguments":[{"name":"count","label":"count","default":{"Value":2},"argument_type":"int","gui_info":{"TextField":40}}],
 "volatile":false,"expires":"never","definition":"Registered","argumentsInferred":false}
```

`namespace`, `realm` and `doc` are absent when empty (skipped by `serde`), an argument's type is
`argument_type` rather than `type`, and a default is the tagged `{"Value": 2}` rather than `2`.
TypeScript therefore accepts `info.arguments[0].type` and `info.namespace`, which are `undefined` at
runtime.

## Impact

A page that reads command metadata through the declared shape reads `undefined`; the type checker
endorses the wrong code, which is the failure `STUBS` exists to prevent. Workaround: read
`argument_type` and unwrap `default.Value`, ignoring the declaration.

## Expected behaviour

The declaration and the runtime agree: either normalize the result to the declared shape at the
boundary (always-present `namespace`/`realm`/`doc`, `type`, a bare default), or declare the shape
actually returned. A test should compare a described command against the declared keys.

## Discovery

Writing `specs/guides/JAVASCRIPT_GUIDE.md` and checking its examples against a build (2026-10-26).
