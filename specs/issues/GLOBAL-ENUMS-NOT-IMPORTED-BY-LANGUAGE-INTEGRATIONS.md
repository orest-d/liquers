---
id: GLOBAL-ENUMS-NOT-IMPORTED-BY-LANGUAGE-INTEGRATIONS
kind: issue
title: JavaScript and Python cannot supply the global enums their commands refer to
status: draft
priority: P3
complexity: M
area: [web, py]
design:
created: 2026-10-20
github:
---

## Problem

A command declared from JavaScript or Python may name a global enum (`{"GlobalEnum": "colours"}`
is accepted by `liquers-core/src/command_declaration.rs`), but neither integration can put the enum
itself into the registry:

- `liquers-web/src/command/import.rs` `read_registry` deserializes `global_enums` and only warns
  *"The command metadata defines global enums, which are not imported; arguments referring to them
  will not resolve"*.
- `liquers-py` `import_metadata` (`liquers-py/src/environment.rs`) reads `commands` only; there is
  no decorator or method for an enum.

So such a command's argument fails at plan time with *Global enum colours not defined*.

## Problem Example

```javascript
liquers.registerCommands(`
global_enums: { colours: { name: colours, values: [{name: red}, {name: green}] } }
commands: ...   # a command `paint` whose argument `colour` is {GlobalEnum: colours}
`, { paint: (colour) => `painted ${colour}` });   // warns: "...global enums, which are not imported..."
await liquers.evaluate("paint-red");   // rejects: Global enum colours not defined
```

Re-checked at HEAD (2026-10-26): unchanged in both integrations. The core registration method
(`design/global-enum-registration/`, `in_review`) is not implemented yet.

## Impact

Global enums are usable from Rust only. Low today: no in-repo command uses one. Workaround: an
inline enum on each argument.

## Expected behaviour

Both integrations import `global_enums` from a registry document and offer a way to add one, going
through the core registration method proposed in `design/global-enum-registration/`
(`CommandMetadataRegistry::add_global_enum`), so the same validation applies.

## Discovery

Found during the 2026-10-20 bulk design of `COMMAND-METADATA-ENHANCEMENTS`, while listing who can
register a global enum.
