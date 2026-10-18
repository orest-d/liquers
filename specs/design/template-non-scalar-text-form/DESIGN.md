---
id: TEMPLATE-NON-SCALAR-TEXT-FORM
kind: design
title: A data node can have a text form, so a Query or Key is indexable and still renders as text
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [lib/commands, core/value]
issues: [TEMPLATE-NON-SCALAR-HAS-NO-TEXT-FORM]
created: 2026-10-17
---
# A data node can have a text form, so a Query or Key is indexable and still renders as text

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-17 backlog compaction. Phases 1-4
are written but not approved, and nothing is implemented. **The maintainer deferred this issue on
2026-10-17 ("revisit once templates are in use")**; this design records the proposal from that
discussion so the revisit starts from a plan, and its first decision is whether to start at all.

## Phase 1: High-Level Design

### Purpose

To a template a Query or Key value is a text scalar: it renders, but cannot be walked. Every other
non-scalar (list, map, records) renders as its JSON, which is rarely what a page wants. A node with
a *text form* would render as that text and still be walkable.

### Problem Example

Data whose field `k` holds the key `data/2026/report.csv` (`SimpleValue::Key`), rendered by
`ns-template/render` with the template `{{k}} is in {{k[1]}}`:

- Today: `{{k}}` gives `data/2026/report.csv`; `{{k[1]}}` is an error (a bracket on a scalar),
  because `SimpleValue`'s `DataNode::kind` is `NodeKind::Text` for `Key` and `Query`
  (`liquers-lib/src/value/data_node.rs`; core's `Value` likewise, `liquers-core/src/data_node.rs`).
- Should be: `data/2026/report.csv is in 2026`.

### Scope and Acceptance Criteria

- **AC-1** Key and Query are lists
  - WHEN a template indexes a Key value (`{{k[0]}}`, `{{k[-1]}}`, `{{#k}}{{.}}{{/k}}`)
  - THEN it gets the key's names in order; a Query gives its segments, each as its encoded text
- **AC-2** A node with a text form renders as text
  - WHEN `{{k}}` names a Key or Query value
  - THEN the output is `encode()` of it, exactly as today, not its JSON list
- **AC-3** Other non-scalars unchanged
  - WHEN `{{x}}` names a list or map without a text form
  - THEN the output is its JSON, as today
- **AC-4** Hygienic slot
  - WHEN a whole P1 slot of a query tag holds a Query value
  - THEN the slot becomes a link parameter to that query (`~X~<query>~E`), not a parameter
    override; a Key in a P4 slot is refused as today (it contains `/`)

Non-goals: a text form for records, frames or folders; changing `try_into_string` (see decisions).

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — adds a `pub` default method to `DataNode` in `liquers-core`
  (rule 4) and spans `liquers-core`, `liquers-lib`, `liquers-template` (rule 6).
- **Leading issue:** whether to start now, against the maintainer's deferral.
- **Explanation:** Phases 1-4 describe a working solution under the recommended answers below; it
  changes what a template can observe about Query and Key values, which is a user-facing contract.
- **Open questions:**
  1. **Open design - timing:** implement now, or keep deferred until templates are in use?
     *Recommended:* keep deferred (the maintainer's call stands); this design is the plan for the
     revisit.
  2. **Open design - `try_into_string` encoding Query and Key** (part of the original proposal):
     it would change argument conversion for every command taking a `String` from a Query or Key
     value, far beyond templates. *Recommended:* exclude; file separately if wanted.
  3. **Proposed resolution - what a Query's items are:** its segments, encoded (not its actions or
     a flat path). *Recommended:* segments, since that is the unit `/-/` separates.
  4. **Proposed resolution - AC-4 link insertion** rather than inserting the text as an escaped
     string token. *Recommended:* link, so the slot means "the result of that query", as the
     proposal said.

### Design Dependencies

- `overlaps` `combined-value-shared-data-node` (weak: same `DataNode` impls in
  `liquers-lib/src/value/data_node.rs`; independent change sites, either order).

### Consolidated Findings

The renderer's `sync_text` / `any_text` and the instantiator's slot filling are the only two
consumers that decide scalar-versus-JSON, so the change is contained. The kind change from `Text`
to `List` is observable to truthiness (an empty key becomes falsey) and to bracket lookup, which is
the point; AC-2 keeps output text identical. Re-size: **M**, not S (three crates, a core trait).

## Phase 2: Architecture

### Solution

1. `liquers-core/src/data_node.rs` `DataNode`: new default method
   `fn text(&self) -> Option<Cow<'_, str>> { None }` — the text a node is written as when it is not
   a scalar. `AsyncDataNode` gets the same default (sync, since a text form is held, not read).
2. Core `Value::Query` / `Value::Key` and `SimpleValue::Query` / `Key`: `kind` is `List`, `scalar`
   is `None`, `len` / `items` / `index` give the names (`Key.0`, `ResourceName::encode`) or the
   query's `segments` encoded; `text` is `Some(encode())`; `to_json` stays the encoded string
   (parameter overrides and JSON output are unchanged).
3. `liquers-template/src/mustache/render.rs` `sync_text` / `any_text`: for a non-scalar, use
   `text()` when `Some`, else JSON (AC-2, AC-3).
4. `liquers-template/src/query/instantiate.rs`: before `Filled::Node`, a node whose `as_any`
   downcasts to a Query value (via a new host hook, below) fills as `Filled::Link(Query)` in a
   whole P1 slot; any other place takes `text()` as a scalar text and applies the P2-P5 rules.
   `liquers-template` cannot name liquers-lib's `Value`, so the host trait
   (`liquers-template/src/host.rs`) gets `fn query_of(&self, node: &AnyNode) -> Option<Query>`
   with default `None`; `ContextHost` (`liquers-lib/src/template/host.rs`) implements it by
   downcasting as `input_state` does.

Rejected: a `NodeKind::Path` kind (every exhaustive match over `NodeKind` grows a case for one
type); converting Query/Key to JSON arrays (loses the text form, AC-2).

### Changes

`DataNode::text`, `AsyncDataNode::text` (new defaults); `Filled::Link` (private);
`TemplateHost::query_of` (new default). `specs/reference/TEMPLATE_LANGUAGE.md` §2 (non-scalar
rule, link insertion) and §3 (node table), with a History row.

### Risks

A template relying on `{{#k}}` being a scalar section (truthy once) now iterates — documented
change, low use while templates are new. `DataNode` implementors outside the tree get a default.
Certainty: medium (AC-4's link insertion touches `Builder::query`, which is intricate).

## Phase 3: Examples and Tests

### Examples

The Problem Example; `{{q}}` over `-R/a/-/b` renders `-R/a/-/b`; `{{q[1]}}` renders `b`.

### Tests

- `liquers-core/src/data_node.rs` `key_and_query_are_lists_with_text` — AC-1, AC-2
- `liquers-lib/src/value/data_node.rs` `simple_key_and_query_are_lists_with_text` — AC-1, AC-2
- `liquers-template/tests/render.rs` `text_form_renders_as_text`, `list_without_text_renders_json`
  — AC-2, AC-3
- `liquers-lib/tests/template_commands.rs` `key_value_is_indexable` (the Problem Example),
  `query_value_in_p1_slot_is_a_link` — AC-1, AC-4

Run: `cargo test -p liquers-core --lib data_node`, `cargo test -p liquers-template`,
`cargo test -p liquers-lib --lib --tests`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. Decisions 1-4 answered; if 1 is "keep deferred", stop here.
- [ ] 2. `liquers-core/src/data_node.rs` `DataNode::text`, `AsyncDataNode::text`; core `Value` Query/Key as lists — `cargo test -p liquers-core --lib data_node`
- [ ] 3. `liquers-lib/src/value/data_node.rs` `SimpleValue` likewise — `cargo test -p liquers-lib --lib value::data_node`
- [ ] 4. `liquers-template/src/mustache/render.rs` `sync_text`, `any_text` use `text()` — `cargo test -p liquers-template`
- [ ] 5. `liquers-template/src/host.rs` `query_of`; `instantiate.rs` `Filled::Link`; `liquers-lib/src/template/host.rs` `ContextHost::query_of` — `cargo test -p liquers-lib --test template_commands`
- [ ] 6. `TEMPLATE_LANGUAGE.md` §2-§3 and History; close the issue — `python3 scripts/docs_index.py --check`

### Validation

`cargo test -p liquers-lib --lib --tests`; `bash scripts/check-build-matrix.sh` (the `templates`
feature off must still build). Rollback: revert; no data format changes.
