---
id: WEB-ENVIRON-VALIDATION-REPORT-NOT-EXPOSED
kind: issue
title: liquers-web exposes no builder validation report, so ENVIRON08 is unimplemented
status: closed
priority: P2
complexity: M
area: [web]
design: web-environ-validation-report
created: 2026-10-17
github:
---

## Problem

`LANGUAGE-INTEGRATION_GUIDE.md` added `ENVIRON08` on 2026-09-05 (from
`specs/design/variadic-metadata-tail-check/`): invalid imported command metadata must expose a
complete language-visible validation report (severity, message, command identity) before
initialization returns its compact error. `liquers-core` has the report
(`EnvironmentBuilder::validate` / `validation_report`,
`liquers-core/src/environment_builder.rs:441-460`), but `liquers-web` never reaches it: it builds
with `DefaultEnvironment::new_with_type_registry` (`liquers-web/src/environment.rs:94`) and
registers JavaScript commands through `JsCommandSpec::parse` / `register_js_command`, both of which
return a single compact `Error`. No `EnvironmentBuilder`, `IssueReport` or `validation_report` use
exists in `liquers-web/src/`, and `tests/environment_ENVIRON.rs` covers only `ENVIRON01`-`07`.

## Impact

A page registering a command with invalid metadata gets one flattened error message and has no way
to enumerate the individual findings, their severity, or which command they belong to. The
`ENVIRON` feature — Essential in every profile — cannot be claimed `CONFORMANT` against the current
guide.

## Expected behaviour

A language-visible validation report on the registration/initialization path (for example, a
JavaScript array of `{severity, message, command}` objects available from the thrown error or a
`validationReport()` accessor), plus an `environ08_*` test. Whether liquers-web should route
environment assembly through `EnvironmentBuilder` to get this, or surface the report from its own
rebuild path, is a design decision.

## Discovery

Bidirectional conformance audit of `liquers-web` against `LANGUAGE-INTEGRATION_GUIDE.md`
(2026-10-17); the guide's History dates `ENVIRON08` after the liquers-web design closed.

## Resolution (2026-10-18)

`liquers-web` exports `validateCommands(input, impls?)` and `registerCommands(input, impls?)`. A
batch is a declaration, an array of them, or a JSON/YAML registry document with functions bound by
name. The report holds every intake finding plus `CommandMetadataRegistry::check()` of the batch,
in the shape `{ issues, errorCount, warningCount, hasErrors, summary }`. Registration is all or
nothing, and a refused batch throws a `parameter_error` carrying the report as `report`.
`registerCommand` is a batch of one. Core gained `CommandDeclaration::check`. Tests:
`environ08_*` in `liquers-web/tests/environment_ENVIRON.rs`, `check01`-`check03` in
`liquers-core/src/command_declaration.rs`. Design: `specs/design/web-environ-validation-report/`.
