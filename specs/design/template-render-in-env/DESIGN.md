---
id: TEMPLATE-RENDER-IN-ENV
kind: design
title: One Rust function renders a template with frames, inside a command or at the top level
form: compact
workflow: liquers-project
status: approved
phase: implementation
readiness: ready
autofix: not-eligible
area: [lib/commands]
issues: [TEMPLATE-TOP-LEVEL-RENDER-HAS-NO-HOST]
affects_docs: [specs/reference/TEMPLATE_LANGUAGE.md, specs/guides/TEMPLATE_GUIDE.md]
created: 2026-10-26
---
# One Rust function renders a template with frames, inside a command or at the top level

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written for an
ad-hoc `apply` of `ns-template/render`. **Returned to Phase 1 on 2026-11-04** and rewritten the same
day on the closure asset of [`ad-hoc-closure-asset`](../ad-hoc-closure-asset/DESIGN.md) (Scope
Changes). Approved under the M6 pre-approval (Orest: "proceed all" at the Phase 1 gate of
[`liquers-py-templates`](../liquers-py-templates/DESIGN.md), whose Decision 1 asks for this
route). A core prerequisite of M6: lands in its own PR, after `ad-hoc-closure-asset` and before
`liquers-py-templates`.

## Phase 1: High-Level Design

### Purpose

A language integration offers `render` and `expand` at the top level of a script and inside its
commands (`LANGUAGE-INTEGRATION_GUIDE.md` §TEMPLATE, `TEMPLATE04`, `TEMPLATE05`). The code that
renders lives in the `ns-template` commands and takes its frames from command arguments. This
design moves it into one public function that takes the frames directly and runs in any
`Context`, and a top-level helper that runs it in a closure asset, so every integration and the
commands share one render.

### Problem Example

Verified at HEAD (`liquers-lib/src/template/commands.rs` `render`, `expand`, `render_with`;
`host.rs` `ContextHost`): `ContextHost::new(context, cwd)` is the only host that evaluates query
tags, and only the commands build it. Nothing renders outside a command. A script wanting
`"Ann has 42"` from `{{name}} has {{$ number-42}}` over `{name: "Ann"}` must encode the text into
`ns-template/render-<encoded>` and apply it ad hoc, which carries one frame only, and loses each
warning's position (`ContextHost::warn` logs `error.to_string()`).

After: `render_top_level(&envref, request)?` returns a `TopLevelRender`; the caller keeps
`asset()` to cancel and awaits `run()`, which gives `Rendered { text: "Ann has 42", warnings: [] }`.
Inside a command, `render_in_context(context, request)` gives the same result.

### Scope and Acceptance Criteria

- **AC-1** Top-level expand over data
  WHEN `render_top_level` runs `{{name}} has {{$ number-42}}` over a `Data` frame `{name: "Ann"}`
  THEN it gives `"Ann has 42"` with no warnings, and the query tag ran in its closure asset, which ends `Ready` with key `None`
- **AC-2** Same text as the command
  WHEN the same template and state are given to `render_top_level` and to `…/ns-template/render-<template>` applied to that state
  THEN both give the same text
- **AC-3** Text never travels in a query
  WHEN the text contains `-`, `/`, `~`, spaces, newlines, `{{#each}}` sections and non-ASCII characters
  THEN it renders as the same text stored and rendered by the command
- **AC-4** Template by link
  WHEN the template is the link `-R/templates/greet.md`
  THEN the stored template is rendered and recorded as a dependency of the asset
- **AC-5** Recovered errors are returned with positions
  WHEN the mode is `silent` or `expand` and a query tag fails
  THEN the text is returned, with one warning per recovered error carrying its message and position, and the log holds the same entries
- **AC-6** Strict fails with the template position
  WHEN the mode is `strict` and a tag fails
  THEN `run` returns `Err` carrying the tag's position
- **AC-7** HTML escaping is explicit
  WHEN `html_escape` is true
  THEN `{{name}}` with `<b>` renders `&lt;b&gt;`; when false, `<b>`
- **AC-8** Relative partials follow `cwd`
  WHEN `cwd` is `templates` and the template contains `{{> ./header.md}}`
  THEN `templates/header.md` is included; with the root as `cwd`, relative keys resolve against the root
- **AC-9** Bad text fails early
  WHEN the template text does not parse
  THEN `render_top_level` returns the parse error and no asset is created
- **AC-10** Two frames at the top level
  WHEN `{{a}}{{b}}` is rendered over a `Data` state frame `{a: 1}` and a `Names` node frame `{b: 2}`
  THEN the text is `12`
- **AC-11** A top-level render can be cancelled
  WHEN `asset().cancel()` is called while a query tag waits
  THEN `run` returns `Ok(None)` or the cancellation error, and the asset is `Cancelled`
- **AC-12** The commands share the function
  WHEN the `ns-template/render` and `ns-template/expand` tests of `liquers-lib` run after the commands are rebased on `render_in_context`
  THEN they pass unchanged

Non-goals: the Python and JavaScript methods (their own designs); `liquers-web`
(`WEB-HAS-NO-TEMPLATE-COMMANDS`); validation, which needs no asset (`validate_template`).

### Documentation Intent

- Reference: `TEMPLATE_LANGUAGE.md`, a short §Rendering from Rust (the two functions, frames, the
  warnings).
- Guide: `TEMPLATE_GUIDE.md`, where it lists the entry points.
- API docs: the module docs of `liquers-lib/src/template/top_level.rs` (new),
  `template/commands.rs` and `template/host.rs`.

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** not-eligible: new `pub` items in `liquers-lib`.
- **Leading issue:** None.
- **Explanation:** the render code exists (`render_with`); the closure asset is
  `ad-hoc-closure-asset`. The work is a move, a request type and a helper.
- **Open questions:** None. The former questions are settled: frames are given directly (Scope
  Change); the warnings are `LogEntry` values with positions (former question 3, now kept by
  `ContextHost` itself).

### Design Dependencies

- `ad-hoc-closure-asset` / `AD-HOC-ASSET-CANNOT-RUN-A-CLOSURE`: **requires** (lands first).
- `liquers-py-templates`: **required-by**; `liquers-py-data-nodes`: **overlaps** (its adapter nodes
  are `Node` frames).

### Scope Changes

- **2026-11-04** — re-based on `AD-HOC-ASSET-CANNOT-RUN-A-CLOSURE`. Orest, at the Phase 1 gate of
  `liquers-py-templates`: "why would template work only in commands - if the environment is
  available, it should be possible outside the commands too?" Two frames at the top level are
  required, so the ad-hoc `apply` of `ns-template/render` (one frame, the data converted to a
  `Value`, the text encoded into a query) no longer fits. The function runs `ContextHost` and
  `liquers_template::render` inside a closure asset, with the frames given directly. AC-1 to AC-9
  keep their meaning, re-read for the new route (AC-3: no encoding; AC-9: a parse error instead of
  an unknown mode, now a typed `ErrorMode`); AC-10 to AC-12 are new. The former
  `render_in_env(envref, template, state, errors, cwd, html_escape)` signature is gone. Size stays
  `M`. Rewritten in full the same day (Phases 2-4).

## Phase 2: Architecture

### Solution

`render_with` and the frame assembly of `commands::render` / `expand` move into
`render_in_context`, which takes a `RenderRequest`. The commands build a request from their
arguments (HTML escaping from the result's filename, as now) and call it. `render_top_level` parses
the text first (AC-9), makes a closure asset (`AssetManager::closure_asset("template render")`),
and returns it with the request; its `run` calls `ClosureAsset::run` with
`|context| render_in_context(context, request)`, the asset's value being the text and the caller
result the `Rendered`. `ContextHost` keeps the warnings it logs.

Rejected: the ad-hoc `apply` of `ns-template/render` (the former design: one frame, text encoded);
a host on a temporary asset (no run harness); a collecting wrapper around `ContextHost` (it must
forward every `TemplateHost` method, for one field).

### Changes

`liquers-lib/src/template/top_level.rs` (new, feature `templates`), re-exported from
`template/mod.rs`:

```rust
pub enum TemplateSource { Text(String), Link(Query) }   // a link is read as a dependency
pub enum FrameData { State(State<Value>), Node(AnyNode) } // a state goes through `ContextHost::node_of`
pub struct RenderRequest {
    pub template: TemplateSource,
    pub frames: Vec<(FrameData, FrameKind)>,  // bottom first
    pub errors: ErrorMode,
    pub cwd: Key,                              // `Key::new()`: the root
    pub html_escape: bool,
}
pub struct Rendered { pub text: String, pub warnings: Vec<LogEntry> }

/// Renders in `context`, a command's or a closure asset's. Query tags and a template link are
/// dependencies of that context's asset; recovered errors go to its log and to `warnings`.
pub async fn render_in_context<E: Environment<Value = Value>>(context: Context<E>, request: RenderRequest)
    -> Result<Rendered, Error>;

/// Prepares a top-level render: parses text now (a parse error fails here, with no asset).
pub fn render_top_level<E: Environment<Value = Value>>(envref: &EnvRef<E>, request: RenderRequest)
    -> Result<TopLevelRender<E>, Error>;

pub struct TopLevelRender<E: Environment> { closure: ClosureAsset<E>, request: RenderRequest }
impl<E: Environment<Value = Value>> TopLevelRender<E> {
    pub fn asset(&self) -> &AssetRef<E>;
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn run(self) -> Result<Option<Rendered>, Error>;
    pub async fn run_inline(self) -> Result<Option<Rendered>, Error>;
}
```

- `host.rs` `ContextHost`: field `warnings: std::sync::Mutex<Vec<LogEntry>>`; `warn` builds a
  `LogEntry` of kind `Warning` with the error's message and `position`, adds it to the context's
  log (`Context::add_log_entry`) and keeps it; `pub fn take_warnings(&self) -> Vec<LogEntry>`.
- `commands.rs`: `render` and `expand` build a `RenderRequest` (`Text`, their frames as now,
  `ErrorMode::parse(errors)?`, `cwd`, escaping from `context.get_metadata()`) and return
  `Value::from(render_in_context(context, request).await?.text)`. Registrations unchanged, so
  `specs/command_registry.yaml` is unchanged.
- Sync and async: `render_top_level` is sync (parsing and `closure_asset` only); the rest is async.
- Errors: a parse error (`parse_template`); a link whose state is not text
  (`State::try_into_string`); `ErrorMode::parse` in the commands; the render's own errors. No new
  constructor.

### Risks

| Assessment | Finding |
|---|---|
| Files | `template/top_level.rs` (new), `template/mod.rs`, `commands.rs`, `host.rs`; `tests/template_top_level.rs` (new); the two documents |
| Existing tests | the `ns-template` tests, which pin the commands' text (AC-12); a test reading a warning's log message keeps the same message |
| Compatibility | additive `pub` items; warnings gain positions in the log |
| Recovery | revert the PR |
| Certainty | High |

## Phase 3: Examples and Tests

### Examples

```rust
let request = RenderRequest {
    template: TemplateSource::Text("{{a}}{{b}}".into()),
    frames: vec![(FrameData::State(State::new().with_data(a_map)), FrameKind::Data),
                 (FrameData::Node(AnyNode::from_json(json!({"b": 2}))), FrameKind::Names)],
    errors: ErrorMode::Strict, cwd: Key::new(), html_escape: false,
};
let render = render_top_level(&envref, request)?;
let asset = render.asset().clone();
assert_eq!(render.run().await?.map(|r| r.text), Some("12".into()));
```

### Tests

`liquers-lib/tests/template_top_level.rs` (`#![cfg(feature = "templates")]`), on an environment
with a memory store holding `templates/greet.md`, `templates/header.md`, and a test command
`number` returning its argument:

- `top01_expand_over_data` — AC-1
- `top02_matches_the_command` — AC-2, AC-3
- `top03_template_by_link_is_a_dependency` — AC-4
- `top04_recovered_errors_carry_positions` — AC-5
- `top05_strict_fails_with_position` — AC-6
- `top06_html_escape_is_explicit` — AC-7
- `top07_partials_follow_cwd` — AC-8
- `top08_parse_error_creates_no_asset` — AC-9
- `top09_two_frames` — AC-10
- `top10_cancel_during_a_query_tag` — AC-11 (a test command that waits on a `Notify`)
- the existing `ns-template` tests — AC-12

Run: `cargo test -p liquers-lib --test template_top_level`, then
`cargo test -p liquers-lib --lib --tests`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `template/host.rs` `ContextHost` — `warnings`, `warn`, `take_warnings` —
  `cargo test -p liquers-lib --lib template`
- [ ] 2. `template/top_level.rs` — the types, `render_in_context` (moved from `render_with`) —
  `cargo test -p liquers-lib --lib template`
- [ ] 3. `template/commands.rs` — `render` and `expand` on `render_in_context` —
  `cargo test -p liquers-lib --lib --tests` (AC-12), `cargo test -p liquers-lib --test registry_export`
- [ ] 4. `render_top_level`, `TopLevelRender`; `tests/template_top_level.rs` —
  `cargo test -p liquers-lib --test template_top_level`
- [ ] 5. Module docs; `TEMPLATE_LANGUAGE.md`, `TEMPLATE_GUIDE.md` (History rows, `reviewed:`) —
  `python3 scripts/check_api_docs.py liquers-lib/src/template/top_level.rs`
- [ ] 6. Feature matrix — `bash scripts/check-build-matrix.sh` (`templates` rows, wasm32)

Rollback: revert the PR; `liquers-py-templates` has not landed yet.

### Validation

The build matrix; `cargo test -p liquers-lib --no-default-features --features templates --lib
--tests`; `check_api_docs.py` on the three touched modules; `python3 scripts/docs_index.py
--check`. The issue `TEMPLATE-TOP-LEVEL-RENDER-HAS-NO-HOST` is closed in Phase 5.
