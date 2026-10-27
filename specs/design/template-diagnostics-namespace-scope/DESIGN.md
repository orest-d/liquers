---
id: TEMPLATE-DIAGNOSTICS-NAMESPACE-SCOPE
kind: design
title: Template diagnostics look each command up in the namespaces the planner would use
form: compact
status: in_review
phase: implementation
readiness: ready
autofix: eligible
area: [core/query, lib/commands]
issues: [TEMPLATE-DIAGNOSTICS-RESOLVE-NAMESPACES-FROM-THE-WHOLE-QUERY]
created: 2026-10-26
---
# Template diagnostics look each command up in the namespaces the planner would use

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written; Phase 4
implemented on 2026-10-27 (automatic fix).

## Phase 1: High-Level Design

### Purpose

`validate_template` reports every action of a `{{$ query}}` placeholder that names no registered
command. It must answer what the planner would: an action is looked up in the namespaces of the
last `ns` before it *in its own transform segment*, then in the registry's defaults
(`RECIPES_AND_PLANS.md` §Planner instructions, "`ns`"). Today it uses one namespace list for the
whole query, so it reports valid queries and misses invalid ones.

### Problem Example

Verified at HEAD (`70b9005`) by reading `liquers-template/src/diagnostics.rs` and planning the
queries with `liquers-validate` against `specs/command_registry.yaml` (`head` is registered in
`pl` only, `fliph` in `img` only):

| Placeholder | Planner (`liquers-validate`) | Diagnostics today | Should be |
|---|---|---|---|
| `{{$ ns-pl/head-3/ns-img/fliph}}` | `Ok` | "unknown command 'head'" (`unknown_commands` uses `query.last_ns()` = `[img]` for every action) | nothing |
| `{{$ ns-pl/head-3/-/ns-img/fliph}}` | `Ok` | "unknown command 'head'" (the last segment's `ns` is applied to the first segment) | nothing |
| `{{$ ns-pl/head-{{n}}/-/head-2}}` | second `head` refused (no `ns` in its segment) | nothing (`template_commands` puts `pl` in front of every lookup) | "unknown command 'head'" at the second `head` |

### Scope and Acceptance Criteria

- **AC-1** A later `ns` replaces an earlier one
  WHEN a literal placeholder has `ns-pl/head-3/ns-img/fliph`
  THEN no diagnostic is reported
- **AC-2** `ns` stops at a segment boundary (literal query)
  WHEN the placeholder is `ns-pl/head-3/-/ns-img/fliph`
  THEN no diagnostic; and for `ns-img/fliph/-/head-3` exactly one, "unknown command 'head'", at
  the second segment's `head`
- **AC-3** The same rule for a placeholder body
  WHEN the placeholder body has placeholders, as `ns-pl/head-{{n}}/-/head-2`
  THEN exactly one diagnostic, at the second `head`; and `head-{{n}}/ns-pl/head-2` reports the
  first `head` only
- **AC-4** A placeholder namespace defers only its own scope
  WHEN the body is `ns-{{lib}}/cmd/-/missing`
  THEN `cmd` is not checked and `missing` is reported; `ns-{{lib}}/cmd` alone reports nothing, as
  today
- **AC-5** Existing results unchanged
  WHEN a query uses at most one leading `ns` in a single segment, or none
  THEN the diagnostics are the same as at HEAD

Non-goals: actions quoted by a later `q`, which the planner does not resolve but diagnostics still
check (filed: `TEMPLATE-DIAGNOSTICS-CHECK-COMMANDS-A-Q-QUOTES`); where instruction names come from
(`planner-instruction-names-public`); a public namespace helper in `liquers-core`.

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — bug fix restoring the documented planner rule in private
  functions of `liquers-template/src/diagnostics.rs`, tests in the same file; `validate_template`'s
  signature and `Diagnostic` are unchanged, no command, format or dependency changes (rules 1-6).
  Priority P3.
- **Leading issue:** None
- **Explanation:** the rule is local to one transform segment and its prefix, so it is computed
  while walking that segment's actions; nothing outside the file is needed.
- **Open questions:** None. Settled in Phase 2: the rule is written once in `diagnostics.rs` rather
  than exposing `plan.rs`'s private `namespaces_for_query` (exposing it is an interface change and
  needs a `Query` per prefix, which a placeholder body does not have).

### Design Dependencies

- `planner-instruction-names-public` — **overlaps** (weak, same file): deletes `is_instruction` and
  replaces the literal `"ns"` comparisons with a core constant. Independent changes in the same
  functions; whichever lands second rebases textually. E1 keeps them apart (that design is not
  eligible).
- `next-presets-follow-planner-rules` — **overlaps** (weak): the same rule applied in
  `liquers-lib`'s `find_next_presets`; different crate, no order.

### Consolidated Findings

- Both walkers change the same way: per transform segment, keep the current `ns` list (empty at the
  segment start), replace it at each `ns` action, and look each other action up in that list
  followed by `cmr.default_namespaces`. A `Link` parameter of `ns` is skipped, as today (the
  planner refuses it with `NotSupported`, which diagnostics leaves to planning).
- `template_commands`' `dynamic_namespace` becomes per segment and is cleared by a later literal
  `ns` in the same segment, so AC-4 narrows what a placeholder namespace hides.
- Links recurse with their own scope, as today (a link is its own query).

## Phase 2: Architecture

### Solution

In `liquers-template/src/diagnostics.rs`:

1. Remove `fn namespaces(query, cmr)`. Add a private helper:

   ```rust
   /// The namespaces an action is looked up in: the current segment's `ns`, then the defaults.
   fn lookup_namespaces(segment_ns: &[String], cmr: &CommandMetadataRegistry) -> Vec<String>;
   ```

2. `unknown_commands`: inside the `QuerySegment::Transform` arm, `let mut segment_ns: Vec<String>
   = Vec::new();`. For each action: when its name is `ns`, set `segment_ns` to its string
   parameters and skip the lookup; otherwise look it up with `lookup_namespaces(&segment_ns, cmr)`
   unless `is_instruction`. Parameter-link recursion unchanged.
3. `template_commands`: drop the first pass over all segments. Inside the transform arm keep
   `segment_ns: Vec<String>` and `dynamic: bool`, both reset per segment. A literal `ns` action
   sets `segment_ns` to its literal parameters and `dynamic` to whether any parameter is not
   literal (`pieces_literal` is `None`). Other literal action names are checked when `!dynamic`.
   Link recursion unchanged.

Rejected: making `plan.rs` `namespaces_for_query` public and calling it on each action's prefix
(a `pub` change in `liquers-core`, and a placeholder body has no `Query` until rendered); keeping
one list but per segment (still wrong for AC-1).

### Changes

- `liquers-template/src/diagnostics.rs`: `unknown_commands`, `template_commands`, new private
  `lookup_namespaces`, removed private `namespaces`; tests. Sync; no error type changes.
- No reference document describes the namespace rule of template diagnostics;
  `TEMPLATE_LANGUAGE.md` is unaffected. No `liquers-lib` change (`template/commands.rs` only calls
  `validate_template`).

### Risks

| Risk | Handling | Certainty |
|---|---|---|
| Likely files | `liquers-template/src/diagnostics.rs` only | high |
| Existing tests | `validate_reports_all` (no `ns`) and `dynamic_namespace_defers_command_check` (one segment) keep their results (AC-5) | high |
| Template editor or `ns-template` commands showing different diagnostics | only for multi-`ns` / multi-segment queries, where today's result is wrong | high |
| Conflict with `planner-instruction-names-public` | textual; both keep the same control flow | high |
| Recovery | revert the file | high |

## Phase 3: Examples and Tests

### Examples

The Problem Example table is the example set. Registry for the tests:
`CommandMetadataRegistry::new()` with `CommandMetadata::new("head")` in namespace `pl`,
`CommandMetadata::new("fliph")` in namespace `img`, and `CommandMetadata::new("known")` in the
default namespace (`with_namespace` on the metadata, then `add_command`).

### Tests

In `liquers-template/src/diagnostics.rs` `tests`:

- `later_ns_replaces_earlier_ns` — `{{$ ns-pl/head-3/ns-img/fliph}}` gives no diagnostic. Covers
  AC-1.
- `ns_does_not_cross_a_segment_boundary` — `{{$ ns-pl/head-3/-/ns-img/fliph}}` none;
  `{{$ ns-img/fliph/-/head-3}}` exactly one, message contains `head`, position at the second
  segment. Covers AC-2.
- `placeholder_body_ns_is_scoped_per_segment` — `{{$ ns-pl/head-{{n}}/-/head-2}}` exactly one
  diagnostic (second `head`); `{{$ head-{{n}}/ns-pl/head-2}}` exactly one (first `head`). Covers
  AC-3.
- `placeholder_namespace_defers_only_its_segment` — `{{$ ns-{{lib}}/cmd/-/missing}}` exactly one,
  naming `missing`. Covers AC-4.
- `single_leading_ns_unchanged` — `{{$ ns-pl/head-3}}` and `{{$ known/head-3}}` give none and one
  respectively; existing `validate_reports_all` and `dynamic_namespace_defers_command_check`
  unchanged. Covers AC-5.

Queries checked with `liquers-validate` against the committed registry:
`ns-pl/head-3/ns-img/fliph` `Ok`, `ns-pl/head-3/-/ns-img/fliph` `Ok`,
`ns-img/fliph/-/head-3` `Action 'head' not registered`.

Run: `CARGO_INCREMENTAL=0 cargo test -p liquers-template --lib diagnostics`.

## Phase 4: Implementation Plan

### Steps

- [x] 1. `diagnostics.rs` tests — add the five tests; AC-1, AC-2, AC-3 and AC-4 tests fail at
  HEAD — `cargo test -p liquers-template --lib diagnostics`
- [x] 2. `diagnostics.rs` `unknown_commands` with `lookup_namespaces`; remove `namespaces` —
  AC-1/AC-2 tests pass: `cargo test -p liquers-template --lib diagnostics`
- [x] 3. `diagnostics.rs` `template_commands` per-segment scope and `dynamic` — all tests pass:
  `cargo test -p liquers-template --lib diagnostics`
- [x] 4. Issue `status: closed` with a resolution note — `python3 scripts/docs_index.py --check`

### Validation

`cargo test -p liquers-template --lib --tests`; `cargo test -p liquers-lib --no-default-features
--features templates --lib --tests` (the `ns-template` commands report these diagnostics). Diff
review: no `println!`, no `unwrap` outside tests, no `_ =>` arm added.

### Implementation notes (2026-10-27)

- The Problem Example's multi-segment queries (`ns-pl/head-3/-/ns-img/fliph`, …) parse with the part before `/-/` as a **resource** key (`-R/ns-pl/head-3/-/…`), so they never exercised two transform segments. The tests use a leading `-/` (`-/ns-pl/head-3/-/ns-img/fliph`), which makes the first segment a transform; `liquers-validate` confirms the planner's answers for these forms (`Ok`, and `Action 'head' not registered` for `-/ns-img/fliph/-/head-3`).
