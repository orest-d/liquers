---
id: REGISTRY-QUERY-DEFAULT-NOT-WRITABLE-AS-TEXT
kind: issue
title: A default link argument cannot be written as query text in a registry file
status: draft
priority: P3
complexity: S
area: [core/commands, core/validate]
design:
created: 2026-10-17
github:
---

## Problem

`CommandParameterValue::Query` holds a `Query`, and `Query` derives `Serialize` and `Deserialize`
(`liquers-core/src/query.rs`). A registry file therefore has to give a default link as the
serialized AST. Writing it as text fails:

```yaml
- {name: cwd, label: cwd, default: !Query "-R-key/.", argument_type: any}
# invalid type: string "-R-key/.", expected struct Query
```

`CommandParameterValue`'s deserializer already accepts a bare scalar as shorthand for `Value`, but
it has no text form for `Query`. No command in `specs/command_registry.yaml` has a link default
yet, so the exported form has never been exercised either.

## Impact

A design that proposes a command with the `-R-key/.` cwd default
(`specs/guides/COMMAND_REGISTRATION_GUIDE.md`, "Passing the working directory") cannot write that
default in a `liquers-validate --registry-file` overlay. Found in `design/liquers-mustache` Phase 3,
where `template/render` was validated with `!Value ''` standing in for the link. The stand-in is
enough to check parsing and arity, but it hides what the default resolves to. The first exported
command with a link default (`template/render`) will write a verbose AST into the generated file.

## Expected behaviour

`!Query "-R-key/."` (and the exporter's output) is the encoded query text, parsed on read with
`parse_query`. Serializing writes `Query::encode()`. The derived AST form may still be accepted on
read for compatibility.
