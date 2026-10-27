---
id: TEMPLATE-RENDER-IN-ENV
kind: design
title: One Rust function renders a template at the top level inside an ad-hoc asset
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [lib/commands]
issues: [TEMPLATE-TOP-LEVEL-RENDER-HAS-NO-HOST]
created: 2026-10-26
---
# One Rust function renders a template at the top level inside an ad-hoc asset

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written, not
approved, nothing implemented. Readiness `needs-decision`: the function's public shape, and leaving
out two-frame top-level `render`, are proposed resolutions.

## Phase 1: High-Level Design

### Purpose

A language integration (Python first) offers `env.expand(template, data=…)` and
`env.render(template, state)` at the top level of a script (`LANGUAGE-INTEGRATION_GUIDE.md`
§TEMPLATE, tests `TEMPLATE04` and `TEMPLATE05`). The only route that keeps the asset lifecycle is to
apply the `ns-template/render` command ad hoc. Each integration would rebuild that detour by hand.
This design adds one function in `liquers-lib::template` that does it, so the detour is written and
tested once.

### Problem Example

Verified at HEAD (`cd6d5da`):

- `liquers_lib::template::ContextHost::new(context, cwd)` (`template/host.rs`) is the only
  `TemplateHost` that evaluates query tags, and it needs a command's `Context`. Nothing in
  `liquers-lib`, `liquers-py` or `liquers-web` renders a template outside a command. `liquers-py`
  registers the `ns-template` commands (`builtins.rs`) but exposes no `render`/`expand`.
- The route the guide documents is `AssetManager::apply(recipe, state, None)`, which evaluates
  before returning and is never cached or keyed (`assets.rs`, trait `AssetManager::apply`). The
  recipe is `ns-template/render-<template>--<errors>-<cwd>/<filename>`, where `<template>` is the
  text encoded as a token or a link `~X~…~E`. Both forms validate:
  `ns-template/render-Hello~.world--silent` and
  `ns-template/render-~X~-R/templates/greet.md~E--strict-templates/page.html` (`liquers-validate`,
  `Ok`).

Scenario: a script wants `"Ann has 42"` from `{{name}} has {{$ number-42}}` and `{name: "Ann"}`.
Today it must build and encode the query (`encode_token`: spaces become `~.`, `-` becomes `~_`),
call `apply`, read the state and search the log for warnings. It should be one call:
`render_in_env(&envref, ActionParameter::new_string(tpl), data_state, "strict", None, false)`
→ `("Ann has 42", [])`.

### Scope and Acceptance Criteria

- **AC-1** Top-level expand over data
  WHEN `render_in_env` is called with template text `{{name}} has {{$ number-42}}` and a state
  holding the map `{name: "Ann"}`
  THEN it returns `"Ann has 42"` with no warnings, and its query tag ran through an asset (the
  command was evaluated once, and nothing was stored under a key)
- **AC-2** Same text as the command
  WHEN the same template and state are given to `render_in_env` and to the query
  `…/ns-template/render-<template>` applied to that state
  THEN both produce the same text (`TEMPLATE05`)
- **AC-3** Text survives the query encoding
  WHEN the template text contains `-`, `/`, `~`, spaces, newlines, `{{#each}}` sections or
  non-ASCII characters
  THEN the rendered text is the same as rendering that text with the `render` command given the
  same text as a store resource
- **AC-4** Template by link
  WHEN the template argument is a link to `-R/templates/greet.md`
  THEN the stored template is rendered
- **AC-5** Recovered errors are returned
  WHEN the errors mode is `silent` or `expand` and a query tag fails
  THEN the text is returned, and the returned warnings hold one entry per recovered error, with its
  message
- **AC-6** Strict fails with the template position
  WHEN the mode is `strict` and a tag fails
  THEN the call returns `Err`, and the error carries the tag's position in the template
- **AC-7** HTML escaping is explicit
  WHEN `html_escape` is true
  THEN `{{name}}` with the value `<b>` renders `&lt;b&gt;`. When it is false, it renders `<b>`.
- **AC-8** Relative partials follow `cwd`
  WHEN `cwd` is `templates` and the template contains `{{> ./header.md}}`
  THEN `templates/header.md` is included. When `cwd` is `None`, relative keys resolve against the
  root.
- **AC-9** Bad arguments fail early
  WHEN the errors mode is not one `ErrorMode::parse` accepts
  THEN the call fails with its error, and no asset is created

Non-goals: rendering with two frames (a state *and* a `data` map) at the top level (see open
question 2); passing native, unconverted structures in place, which `FOREIGN-VALUE-HAS-NO-DATA-NODE`
provides for Python values carried as `ExtValue::Foreign`; the Python and JavaScript methods
themselves (bindings call this function in their own designs); `liquers-web`
(`WEB-HAS-NO-TEMPLATE-COMMANDS`).

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible. It adds a `pub` function to `liquers-lib` (rule 4).
- **Leading issue:** **Proposed resolution - entry point shape:** one function,
  `render_in_env(envref, template: ActionParameter, state, errors: &str, cwd: Option<&Key>,
  html_escape: bool) -> Result<(String, Vec<LogEntry>), Error>`. It reuses existing types: a string
  or link parameter for the template, the log entries for the warnings. This takes the place of the
  issue's `frames: Vec<(AnyNode, FrameKind)>` signature. That signature cannot be built without a
  core change: frames are live `AnyNode`s, and an ad-hoc asset receives only a recipe, a `State` and
  the environment's payload type. Alternative: options and result structs (two more `pub` types).
- **Explanation:** everything the function needs exists: `encode_token`, `Recipe::new`,
  `AssetManager::apply`, the asset's state and its metadata log. The work is assembly and tests in
  one crate. The issue's complexity is corrected from `S` to `M` (a new module plus integration
  tests and two documents).
- **Open questions:**
  1. **Proposed resolution - entry point shape:** as the leading issue.
  2. **Open design - two frames at the top level:** the guide's `render(template, state, data)`
     with both frames cannot pass `data` through this route. `data` is a `Value` argument and could
     only travel as text or as a link. Recommended: leave it out here. `expand(template, data)` is
     `render_in_env` with the data as the state, which is the layout the guide already documents
     (`_meta` aside). A later core primitive that runs a closure as an ad-hoc asset would lift the
     limit and accept frames directly; file it only when an integration needs it.
  3. **Proposed resolution - warnings:** return the ad-hoc asset's `LogEntryKind::Warning` entries
     as `LogEntry` values, not strings, so the position and query survive. Bindings convert them.

### Design Dependencies

- `liquers-mustache` (complete): **requires**. It provides the `ns-template` commands and
  `ContextHost`, which this design uses unchanged.
- `FOREIGN-VALUE-HAS-NO-DATA-NODE` (issue, no design): **overlaps**. Until it is fixed, a Python
  object passed as a foreign state renders as an opaque node. The two can land in either order.
- No binding design yet offers top-level `render`/`expand` (`liquers-py-async`, PR #181, does
  not touch templates). A future one is **required-by** and calls this function.

## Phase 2: Architecture

### Solution

A new module, `liquers-lib/src/template/top_level.rs` (feature `templates`, like the rest of
`template/`), with one public function, re-exported from `template/mod.rs`:

```rust
/// Renders a template outside any command, inside an ad-hoc asset of `envref`, by applying
/// `ns-template/render` to `state`. Returns the text and the recovered errors (warnings) of that
/// asset's log. Nothing is cached, keyed or stored.
pub async fn render_in_env<E: Environment<Value = Value>>(
    envref: &EnvRef<E>,
    template: ActionParameter,      // ActionParameter::new_string(text) or ::new_link(query)
    state: State<Value>,            // the bottom frame and `.`; for expand semantics, the data map
    errors: &str,                   // strict | expand | expand_html | expand_md | silent
    cwd: Option<&Key>,              // None: the command's default link applies (root)
    html_escape: bool,              // true: filename `page.html`; false: `text.txt`
) -> Result<(String, Vec<LogEntry>), Error>;
```

Steps:
1. `ErrorMode::parse(errors)?`, which fails before any asset exists (AC-9).
2. Build the query text:
   `format!("ns-template/render-{}--{}{}/{}", template.encode(), encode_token(errors), cwd_part,
   filename)`. Here `cwd_part` is `format!("-{}", encode_token(cwd.encode()))` when `cwd` is given and
   empty otherwise. `ActionParameter::encode` escapes text and wraps links (AC-3, AC-4). The
   filename picks HTML escaping, as `render_with` reads it from the result's extension (AC-7).
3. `let recipe = Recipe::new(query, "template".into(), String::new())?;` and
   `let asset = envref.get_asset_manager().apply(recipe, state, None).await?;`. This evaluates
   before returning, takes no job-queue slot, and returns the evaluation error in `strict` mode with
   the error's position (AC-1, AC-6).
4. Read the state with `asset.get().await?` and convert it with `try_into_string()`. Read the
   warnings from `asset.get_metadata().await?`: the `MetadataRecord` `log` entries whose `kind` is
   `LogEntryKind::Warning` (AC-5). Legacy metadata gives an empty list.

Rejected alternatives:
- **A host that calls the store or executor directly.** It bypasses caching, dependencies,
  volatility, cancellation and the log, which `EVAL` and the guide forbid.
- **Building the `Context` of a temporary asset** (`AssetData::new_temporary` plus the public
  `Context::new`). This runs outside `run`/`run_inline`, so it has no status transitions, no
  cancellation token and no evaluation session. It is the same bypass in another form.
- **A core primitive that runs a closure as an ad-hoc asset.** This would allow frames as
  `AnyNode`. It changes `liquers-core`'s `AssetManager`, which makes the work L, and no integration
  needs it yet (open question 2).

### Changes

- New `liquers-lib/src/template/top_level.rs`; `template/mod.rs` gets `mod top_level; pub use
  top_level::render_in_env;`.
- No command, registry or core change. `specs/command_registry.yaml` is unchanged.
- Errors: those of `ErrorMode::parse`, `Recipe::new`, `apply` and `try_into_string`. No new
  `Error` constructors.
- Async; usable from any tokio context. `apply` takes no job-queue slot (its doc comment), so the
  function does not deadlock inside a command either. A command should still use the guide's
  `ContextHost` path, which records the template's queries as dependencies of the calling asset.
- Documents: `specs/reference/TEMPLATE_LANGUAGE.md` §4 gets a paragraph, "Rendering from Rust outside
  a command" (History row, `reviewed:`). `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §TEMPLATE
  changes "A direct helper … is filed as `TEMPLATE-TOP-LEVEL-RENDER-HAS-NO-HOST`" to name
  `render_in_env` and its two-frame limit (History row, `reviewed:`). `specs/guides/TEMPLATE_GUIDE.md`
  gets a short Rust example.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| A long template makes a long recipe query | It is never cached or keyed, so its length is only memory; AC-3 tests multi-line text | high |
| Encoding round trip of unusual text | `encode_token` is the codec of the parser (`escape/tests.rs`); AC-3 covers newline, `~`, `-`, non-ASCII | high |
| `cwd` omitted: the default link `-R-key/.` evaluated in a keyless asset | AC-8 tests the `None` case resolving against the root | medium |
| The environment lacks the `ns-template` commands | The planner's `ActionNotRegistered` error is returned unchanged; documented | high |

Recovery: additive; revert the module.

## Phase 3: Examples and Tests

### Examples

```rust
use liquers_core::query::ActionParameter;
use liquers_lib::template::render_in_env;

let data = State::new().with_data(Value::try_from_json_value(&serde_json::json!({"name": "Ann"}))?);
let (text, warnings) = render_in_env(
    &envref, ActionParameter::new_string("{{name}} has {{$ number-42}}".into()),
    data, "strict", None, false).await?;
assert_eq!(text, "Ann has 42");
assert!(warnings.is_empty());
```

The query built internally for a link template, with `cwd = templates` and HTML escaping, is
`ns-template/render-~X~-R/templates/greet.md~E--strict-templates/page.html` (validated `Ok`).

### Tests

New integration file `liquers-lib/tests/template_top_level.rs` (`#![cfg(feature = "templates")]`).
It reuses the fixture pattern of `tests/template_commands.rs`: a memory store with
`templates/greet.md` and `templates/header.md`, plus the `number`, `count` and `fail` commands.

- `top_level_expand_over_a_map`: AC-1. `count` is evaluated once (`count_of`), and
  `store.contains` is false for every key except the fixtures.
- `top_level_matches_the_command_query`: AC-2. Compares the result with `eval` of
  `-R/templates/greet.md/-/ns-template/expand` over the same data and with the `render` query.
- `top_level_text_round_trips_through_encoding`: AC-3. Several texts, each compared with rendering
  the same text stored as `templates/t.md` through a link.
- `top_level_link_template`: AC-4
- `top_level_recovered_errors_are_returned`: AC-5 (`silent` and `expand`, with the `fail` command)
- `top_level_strict_error_has_template_position`: AC-6
- `top_level_html_escape_flag`: AC-7
- `top_level_cwd_resolves_relative_partial`: AC-8 (`Some(templates)` and `None`)
- `top_level_bad_error_mode_creates_no_asset`: AC-9

Run: `cargo test -p liquers-lib --test template_top_level`, and
`cargo test -p liquers-lib --no-default-features --features templates --lib --tests`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-lib/src/template/top_level.rs` `render_in_env`; `template/mod.rs` re-export. Proof: `cargo check -p liquers-lib --no-default-features --features templates`
- [ ] 2. `liquers-lib/tests/template_top_level.rs`, the nine tests. Proof: `cargo test -p liquers-lib --test template_top_level` (AC-1 to AC-9)
- [ ] 3. Feature matrix: the module is behind `templates` only. Proof: `bash scripts/check-build-matrix.sh` (the `templates` and wasm32 `webui,templates` rows)
- [ ] 4. Documents: `TEMPLATE_LANGUAGE.md` §4 paragraph, `LANGUAGE-INTEGRATION_GUIDE.md` §TEMPLATE sentence, `TEMPLATE_GUIDE.md` example, each with a History row and `reviewed:`. Proof: `python3 scripts/docs_index.py --check`
- [ ] 5. `specs/issues/TEMPLATE-TOP-LEVEL-RENDER-HAS-NO-HOST.md` `status: closed` with a resolution naming the two-frame limit; `python3 scripts/docs_index.py`. Proof: `python3 scripts/docs_index.py --check`

### Validation

`cargo test -p liquers-lib --lib --tests`; `cargo test -p liquers-lib --test registry_export`
(the registry is unchanged). Review the diff for scope.
