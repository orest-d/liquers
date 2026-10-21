---
id: ISSUE-REPORT-PLAN-AND-METADATA-LOGGING
kind: feature
title: Reuse structured issue reports for plan validation and metadata logging
status: draft
priority: P3
complexity: M
area: [core/validate, core/plan, core/commands]
design: diagnostics-carry-their-origin
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

## Design

Origin design: `variadic-metadata-tail-check` (complete). Still open at HEAD 18732dd: `Issue`
(`liquers-core/src/issue_report.rs`) has only `Generic` and `CommandRegistry`, and plan
diagnostics reach callers as `Plan::error` plus strings (`validate::collect_warnings`). Designed
with `LOG-ENTRY-HAS-NO-SOURCE` in `specs/design/diagnostics-carry-their-origin/` (bulk design
2026-10-20): `Issue::Plan`, `Plan::issue_report`, and `Issue::to_log_entry` as the "metadata
logging" half (report to log; the reverse is a non-goal).

**Problem example.** A plan with `Plan::error` set and an init warning can be inspected only as an
`Error` and strings; `EnvironmentBuilder::validation_report()` cannot `append` it. Should:
`report.append(plan.issue_report())` counts the plan's error.
