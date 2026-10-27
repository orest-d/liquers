---
id: NEXT-PRESETS-FOLLOW-PLANNER-RULES
kind: design
title: Next-command presets resolve the last command as the planner does
form: compact
status: complete
gh_pr: [212]
readiness: ready
autofix: eligible
area: [lib/commands]
issues: [NEXT-PRESETS-MISREAD-PLANNER-INSTRUCTIONS]
created: 2026-10-26
---
# Next-command presets resolve the last command as the planner does

> **Complete on 2026-10-27**: implemented by automatic fix in #212.

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written; Phase 4
implemented on 2026-10-27 (automatic fix).

## Phase 1: High-Level Design

### Purpose

The query console offers "next command" presets: the commands that a query's last command declares
in its `next` metadata. `find_next_presets` (`liquers-lib/src/utils.rs`) finds that last command
with its own reading of the planner instructions. It disagrees with the planner in four ways, so
presets go missing or are looked up in the wrong place. It should resolve the last command exactly
as `PlanBuilder` would (`RECIPES_AND_PLANS.md` §Planner instructions).

### Problem Example

Verified by reading `find_next_presets` at HEAD (cd6d5da). The registry has `uppercase` (realm `""`,
namespace `""`, preset `lowercase`), `head` in namespace `pl` (preset `tail`) and `cmd` in realm
`r` (preset `next`).

| Query | Planner resolves | `find_next_presets` today | Should be |
|---|---|---|---|
| `text-Hello/uppercase/v` | `uppercase` (`v` is an instruction) | takes `v` as the last command: no presets | the `lowercase` preset, `text-Hello/uppercase/v/lowercase` |
| `ns-pl-img/head` | `head` in `[pl, img, defaults]` | flattens all `ns` parameters and keeps the last one: looks in `[img, defaults]`, no presets | the `tail` preset |
| `ns-pl/head/ns-img` | `head` in `[pl, …]` (only an `ns` *before* the action applies) | the last `ns` of the segment: `[img, …]`, no presets | the `tail` preset |
| `-r/cmd` | `cmd` in realm `r` | realm `""` always: no presets | the `next` preset |

### Scope and Acceptance Criteria

- **AC-1** A trailing `v` is skipped
  WHEN presets are asked for `text-Hello/uppercase/v`
  THEN the result holds the `lowercase` preset with query `text-Hello/uppercase/v/lowercase`
- **AC-2** Every namespace of the last `ns` applies
  WHEN presets are asked for `ns-pl-img/head`, `head` living in `pl`
  THEN the `tail` preset is returned
- **AC-3** Only an `ns` before the command applies to it
  WHEN presets are asked for `ns-pl/head/ns-img`
  THEN `head` is looked up in `pl` and the `tail` preset is returned
- **AC-4** The segment's realm applies
  WHEN presets are asked for `-r/cmd`, `cmd` registered in realm `r`
  THEN the `next` preset is returned
- **AC-5** Namespace injection follows the appended position
  WHEN a preset is appended after `ns-pl/head/ns-img` and the preset command lives in `img`
  THEN no `ns-` action is injected (the trailing `ns-img` already applies to the appended action)
- **AC-6** Existing behaviour stays
  WHEN the six existing `utils.rs` tests run
  THEN they pass unchanged

Non-goals: making the instruction names or the namespace rule public in `liquers-core`
(`planner-instruction-names-public`); changing `CommandPreset` or `append_action`; presets from
any source other than `CommandMetadata::next`.

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — a bug fix in `liquers-lib/src/utils.rs` only, restoring the
  documented planner rule; private helpers replace `active_namespace` / `build_namespace_list`. No
  `pub` item, command or format changes (rules 1-6 hold).
- **Leading issue:** None
- **Explanation:** the planner's rule is documented and its building blocks (`ActionRequest::is_ns`
  / `is_q`, `TransformQuerySegment::last_ns`, `TransformQuerySegment::name`) are already public. The
  fix calls them on the segment prefix that ends at the last command.
- **Open questions:** None. Settled in Phase 2: until `planner-instruction-names-public` lands,
  `v` is recognised by a private helper in `utils.rs`. That design replaces it.

### Design Dependencies

- `planner-instruction-names-public` — **required-by** (soft): it replaces this design's private
  `v` test with the public core predicate. Either order works, and this one should go first because
  it is eligible for automatic fixing (overlap exclusion E1 kept the two apart).
- `reserved-instruction-command-names` — **overlaps** (finished Phase 4, not edited): adds a
  crate-private `is_v` that `liquers-lib` cannot call. No order.

### Consolidated Findings

- The source issue named two disagreements (`v`, the namespace union). There are four: the
  flattening also loses every namespace but the last of `ns-a-b`, and the realm is ignored. The
  issue has been widened accordingly.
- Two namespace lists are needed. The **last command** is looked up with the `ns` that precedes it.
  The **appended preset** is placed after everything, so the whole segment's last `ns` applies to
  it, which is what `plan::append_action` already computes for the injection decision. The
  existing `ns_hint` lookup must use the second list.
- A non-string `ns` parameter makes the planner fail (`NotSupported`). Presets then return empty,
  as for an unparseable query.

## Phase 2: Architecture

### Solution

In `liquers-lib/src/utils.rs`:

1. Private `fn is_planner_instruction(action: &ActionRequest) -> bool` — `action.is_ns() ||
   action.is_q() || action.name == "v"`. (Replaced by the core predicate in
   `planner-instruction-names-public`.)
2. Find the last command by index:
   `let Some(i) = tqs.query.iter().rposition(|a| !is_planner_instruction(a)) else { return vec![] }`.
3. Private `fn namespaces_from(ns: Option<Vec<ActionParameter>>, registry) -> Option<Vec<String>>`:
   each parameter must be `ActionParameter::String(s, _)` (else `None`), then `registry.default_namespaces`
   appended, in order, as `plan.rs` `namespaces_for_query` does.
4. Command namespaces: a clone of `tqs` with `query` truncated to `..=i`, then `.last_ns()`, then
   `namespaces_from`. Preset namespaces: `tqs.last_ns()` (the whole segment), then `namespaces_from`.
   `None` from either returns `vec![]`.
5. Realm: `let realm = tqs.name();` passed to both `find_command_in_namespaces` calls instead of `""`.
6. Remove `active_namespace` and `build_namespace_list`.

`append_action` is unchanged; it already uses the whole query's last `ns` and its last realm.

Rejected: calling `PlanBuilder` on the query and reading the last `Step::Action` (fails on any
unresolvable earlier command and on placeholders, so presets would vanish while typing); a public
core helper now (that is the other design's scope, and would make this one ineligible).

### Changes

- `liquers-lib/src/utils.rs`: `find_next_presets`, two private helpers, two removed, tests.
- No documents describe `find_next_presets` beyond its rustdoc, which gains one sentence ("the last
  command is resolved as the planner does: instructions skipped, the `ns` before it, the segment's
  realm").

Sync. No `pub` change.

### Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `liquers-lib/src/utils.rs` |
| Existing tests | the six `utils.rs` tests keep their queries and expectations |
| Callers | `ui/widgets/query_console_element.rs` only; the signature is unchanged |
| Compatibility | more presets appear where they were missing; none disappear for a query the planner resolves |
| Recovery | revert |
| Certainty | high |

## Phase 3: Examples and Tests

### Examples

The Problem Example table. Validated with `liquers-validate --command uppercase --command text
--command head --command cmd`: `text-Hello/uppercase/v` (`v` emits no step), `ns-pl-img/head` and
`ns-pl/head/ns-img` are `Ok` and encode unchanged. `-r/cmd` is `ActionNotRegistered` there
("command keys tried: 'r--cmd'"), because `--command` registers in realm `""`. That error confirms
the planner looks `cmd` up in realm `r`, which is the behaviour AC-4 asks presets to match.

### Tests

In `liquers-lib/src/utils.rs` `mod tests`:

- `test_trailing_v_is_skipped` (AC-1).
- `test_all_namespaces_of_ns_apply` (AC-2): `head` in namespace `pl` with a `tail` preset;
  `ns-pl-img/head` returns one preset.
- `test_only_ns_before_the_command_applies` (AC-3): `ns-pl/head/ns-img` returns the `tail` preset.
- `test_segment_realm_applies` (AC-4): `cmd` with `realm = "r"`; `-r/cmd` returns its preset.
- `test_preset_after_trailing_ns_is_not_injected` (AC-5): `head` (in `pl`) has preset `show` with
  `ns: "img"`, `show` lives in `img`; for `ns-pl/head/ns-img` the query is `ns-pl/head/ns-img/show`.
- Existing six tests (AC-6).

Run: `CARGO_INCREMENTAL=0 cargo test -p liquers-lib --lib utils::tests`.

## Phase 4: Implementation Plan

### Steps

- [x] 1. `utils.rs` tests — add the five tests; AC-1 to AC-4 fail at HEAD — `cargo test -p liquers-lib --lib utils::tests`
- [x] 2. `utils.rs` — `is_planner_instruction`, `namespaces_from`, rewrite `find_next_presets` (index of last command, two namespace lists, realm), remove `active_namespace` / `build_namespace_list`, rustdoc sentence — depends on 1 — all `utils::tests` pass. Rollback: revert.
- [x] 3. Close `NEXT-PRESETS-MISREAD-PLANNER-INSTRUCTIONS` with a resolution note; this design to `complete` — `python3 scripts/docs_index.py` then `--check`

### Validation

`cargo fmt --check -p liquers-lib`; `CARGO_INCREMENTAL=0 cargo test -p liquers-lib --lib --tests`;
`cargo test -p liquers-lib --no-default-features --features webui --lib` (the console's feature
set). Diff review: `utils.rs` only, plus the issue and design records.
