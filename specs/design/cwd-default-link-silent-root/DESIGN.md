---
id: CWD-DEFAULT-LINK-SILENT-ROOT
kind: design
title: A default link that names the working directory resolves to the root without a warning
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/query, core/plan]
issues: [CWD-DEFAULT-LINK-WARNS-WITHOUT-WORKING-DIRECTORY]
created: 2026-10-17
---
# A default link that names the working directory resolves to the root without a warning

Produced under [`guides/autonomous_bulk_design.md`](../../guides/autonomous_bulk_design.md) by the
2026-10-17 backlog compaction: Phases 1-4, reviewed without phase approval. Not an approval and not
an implementation.

## Phase 1: High-Level Design

### Purpose

The guide's recommended argument `dir: Key = query "-R-key/."` freezes, with no CWD, to the root
and logs `RELATIVE_WITHOUT_CWD_WARNING`. The single source is `ParameterValue::freeze_cwd`
(`liquers-core/src/plan.rs`), which calls `cursor.absorb_diagnostics(&scoped)` for every link
variant, so `Plan::freeze_cwd` reports the root fallback and `finalize_plan_expanded`
(`interpreter.rs`) warns. After freezing, the link is absolute, so no later site warns.

### Problem Example

```
ns-template/render-~X~-R/templates/hello.md~E      (no -R-cwd/…)
```

Today: the result's log holds `Relative key/query has no CWD; using logical root '/'.`
(`liquers-lib/tests/template_commands.rs` `error_modes_log_warnings` works around it by filtering).
Should be: no such warning; `cwd` is the root key. `-R/./x.txt` with no CWD keeps warning.

### Scope and Acceptance Criteria

- **AC-1** Working-directory default is silent
  - WHEN a command's default link names the working directory itself (one resource segment whose
    key is exactly `.`, e.g. `-R-key/.`) and the query sets no CWD
  - THEN the link resolves to the root key and no `RELATIVE_WITHOUT_CWD_WARNING` is logged
- **AC-2** With a CWD it is unchanged
  - WHEN the same query starts with `-R-cwd/docs/`
  - THEN the link resolves to `docs`, as today
- **AC-3** Other relative operands still warn
  - WHEN a default link is `-R/./x.txt`, or a user writes a relative link parameter, with no CWD
  - THEN the warning is logged once, as today
- **AC-4** Freeze diagnostics stay correct
  - WHEN the silent default is frozen
  - THEN `consumed_cwd` is still reported (the freeze migration assertion keeps working)

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — changes observable warning behaviour on a decision still
  open (rule 5), and adds a `pub(crate)` cursor method across two modules.
- **Leading issue:** **Proposed resolution - what is silent:** only a *declared default* link
  (`ParameterValue::DefaultLink`) whose query is exactly the working directory (recommended).
- **Explanation:** The root is the intended value of "the working directory, or root" defaults, so
  the warning carries no information there; any other relative operand may be a mistake and keeps
  it. The change is one branch at the single emission source.
- **Open questions:**
  1. **Proposed resolution - what is silent:** recommended: `DefaultLink` whose query is one
     resource segment with key `.`. Alternatives: every `DefaultLink` (silences a command author's
     `-R/./x.txt`, which may be a mistake); also user-written `-R-key/.` links (the user asked for a
     relative key explicitly, so the warning is fair).

### Design Dependencies

- `overlaps` `registry-query-default-text-form` (weak: the same default, written differently).

### Consolidated Findings

`error_modes_log_warnings`'s comment and filter become stale; the test is tightened to assert the
warning is absent (AC-1 in `liquers-lib`). `COMMAND_REGISTRATION_GUIDE.md` "Passing the working
directory" gets one sentence: with no CWD the default is the root, silently.

## Phase 2: Architecture

### Solution

- `liquers-core/src/query.rs` `CwdCursor`: add
  `pub(crate) fn absorb_consumption(&mut self, child: &CwdCursor)` (merges `consumed_cwd` only).
- `liquers-core/src/plan.rs`: private `fn names_working_directory(query: &Query) -> bool` — exactly
  one segment, a `QuerySegment::Resource` whose `key` has one element with `is_cwd()`; any header.
- `ParameterValue::freeze_cwd`: split the `DefaultLink` arm from the other link arms; after
  `scoped.resolve_query_scoped(query)`, call `absorb_consumption` when the *unresolved* query
  `names_working_directory`, else `absorb_diagnostics` as today.

The context's CWD stays unset in the silent case, which is harmless: every operand is absolute
after freezing, and a later relative operand still installs the root with its warning.

Rejected: change the guide's recommended form (no form gives "cwd or root" today); suppress in
`finalize_plan_expanded` (loses the distinction between operands).

Known-issue preflight: none open touches `freeze_cwd`. Explicit `match` arms kept (CLAUDE.md).

### Changes

`query.rs` (one `pub(crate)` method), `plan.rs` (one helper, one arm split), tests, the guide
sentence (History row, `reviewed:`), `template_commands.rs` test tightened.

### Risks

| Risk | Handling |
|---|---|
| A test counts this warning | `grep RELATIVE_WITHOUT_CWD_WARNING`: core tests use explicit relative operands, not defaults; re-run both crates |
| Two crates touched (lib test only) | The behaviour change is in core; lib's test edit is a comment and assertion |

Recovery: revert. Certainty: high.

## Phase 3: Examples and Tests

### Tests

In `liquers-core/src/plan.rs` tests, with a command `here(dir: Key = query "-R-key/.")` built by
`ArgumentInfo::any_argument("dir").with_default_link(parse_query("-R-key/.")?)`:

- `cwd_default_link_freezes_to_root_silently` — `Plan::freeze_cwd(None)` returns `(_, false)` and
  the link's resource key is the root key — AC-1, AC-4 (and, through `freeze_cwd_with` on a test `CwdCursor::new(None)`, `take_consumed_cwd()` is `true`)
- `cwd_default_link_follows_explicit_cwd` — `freeze_cwd(Some(docs))` gives the key `docs` — AC-2
- `relative_default_link_still_warns` — default `-R/./x.txt`: `freeze_cwd(None)` returns `true`;
  same for a user `ParameterLink` `-R-key/.` — AC-3

In `liquers-lib/tests/template_commands.rs`: `error_modes_log_warnings` asserts no warning equals
`Relative key/query has no CWD; using logical root '/'.` — AC-1.

Commands: `cargo test -p liquers-core --lib cwd_default_link relative_default_link`,
`cargo test -p liquers-lib --test template_commands`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `query.rs` `CwdCursor::absorb_consumption` — `cargo check -p liquers-core`
- [ ] 2. `plan.rs` `names_working_directory`, `ParameterValue::freeze_cwd` arm split, three tests — `cargo test -p liquers-core --lib`
- [ ] 3. `template_commands.rs` `error_modes_log_warnings` — `cargo test -p liquers-lib --test template_commands`
- [ ] 4. `COMMAND_REGISTRATION_GUIDE.md` sentence, History row, `reviewed:`; close the issue — `python3 scripts/docs_index.py --check`

### Validation

`cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-lib --lib --tests`. Rollback:
revert the commit.
