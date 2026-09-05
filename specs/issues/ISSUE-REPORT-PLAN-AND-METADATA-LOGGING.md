---
id: ISSUE-REPORT-PLAN-AND-METADATA-LOGGING
kind: feature
title: Reuse structured issue reports for plan validation and metadata logging
status: draft
priority: P3
complexity: M
area: [core/validate, core/plan, core/commands]
design: variadic-metadata-tail-check
created: 2026-09-05
github:
---
## Problem

`IssueReport` initially collects command-metadata validation diagnostics during environment
construction. Plan building and metadata logging have separate error/reporting paths, so callers
cannot yet obtain one composable report spanning those components.

## Expected behaviour

Plan validation and selected metadata logging can add typed `Issue` variants with a severity and
message to the same report type. An application can render, filter, summarize, and route the
combined report without parsing error strings.

## Scope

Do not add plan or metadata-log variants in `VARIADIC-ARGUMENT-STARVES-LATER-ARGUMENTS`. This
follow-up must design their ownership, positions/context, reporting boundary, and logging policy.

## Discovery

Filed from `design/variadic-metadata-tail-check` while creating the reusable issue-report module.
