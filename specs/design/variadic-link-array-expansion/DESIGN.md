---
id: VARIADIC-LINK-ARRAY-EXPANSION
kind: design
title: A link inside a variadic argument supplies one element per array entry
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/plan, core/commands]
issues: [LINK-IN-VARIADIC-DOES-NOT-EXPAND]
created: 2026-10-20
---
# A link inside a variadic argument supplies one element per array entry

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-20: Phases 1-4 written and
reviewed without phase approval. Nothing is implemented. The origin design,
`design/variadic-arguments-declaration/` (complete), deliberately left this case out.

## Phase 1: High-Level Design

### Purpose

A variadic argument is a list. The natural way to give it a list held elsewhere is a link, such as
a stored `cols.json` or a record column. Today a link is always exactly one element, so an
array-valued link gives the command one element that holds the whole array. Decide how such a link
should behave, and make the query spelling and the recipe `links:` spelling behave the same.

### Problem Example

Verified at HEAD (18732dd). `-R/config/cols.json` holds `["date", "amount"]`. The query below
validates, and the plan holds **one** `ParameterLink` element for `columns`
(`liquers-validate -- 'ns-pl/select_columns-~X~-R/config/cols.json~E'`):

```
ns-pl/select_columns-~X~-R/config/cols.json~E
```

At run time, `interpreter.rs` `materialize_nested_parameter` turns that link into one
`ParameterValue` holding the JSON array. Then `CommandArguments::get_multiple::<String>` →
`convert_multiple_element` → `String::from_parameter_value` fails, because an array is not a
string. A recipe `links: {columns: -R/config/cols.json}` takes the same path: `plan.rs`
`ResolvedParameterValues::override_link` wraps the link as the only element (the comment there
names this issue).

What should happen (recommended answer, question 1): `select_columns` receives `["date",
"amount"]`. With `select_columns-id-~X~-R/config/cols.json~E-status`, it receives `["id", "date",
"amount", "status"]`.

### Scope and Acceptance Criteria

- **AC-1** An array link expands
  WHEN a variadic argument's element is a link whose value is a JSON array
  THEN the command receives one element per array entry, in array order
- **AC-2** Order with literals
  WHEN literals and an array link are mixed (`join_all-a-~X~letters~E-z`, `letters` → `["b","c"]`)
  THEN the command receives `a, b, c, z`
- **AC-3** A scalar link stays one element
  WHEN the link's value is not an array (a string, number, object or one-cell record view)
  THEN it is one element, as today
- **AC-4** Empty array
  WHEN the link's value is `[]`
  THEN it contributes no element, and the command may receive an empty list
- **AC-5** Recipe override
  WHEN a recipe's `links:` entry names a variadic argument and resolves to an array
  THEN the command receives the entries, exactly as for AC-1
- **AC-6** One level only
  WHEN an entry of the array is itself an array (`[["a","b"],"c"]`)
  THEN that entry stays one element, and a `Vec<String>` command fails converting it with an error
  that carries the link's position
- **AC-7** Scalar arguments unchanged
  WHEN an array link fills a non-variadic argument
  THEN the argument receives the whole array, as today

Non-goals: expansion at plan time (the plan keeps one link element; only execution sees the
entries); expanding objects into key-value pairs (`COMMAND-COMPOSITE-VARIADIC-ARGUMENTS`); type
checking link entries at plan time.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible. It changes query semantics by a design choice instead of
  restoring documented behaviour (rule 2), and the readiness is `needs-decision` (rule 5).
- **Leading issue:** **Proposed resolution - expand or convert:** recommended *expand at
  materialisation* (option 1 of the issue).
- **Explanation:** the change sits in one interpreter function and needs no new type or API. The
  plan stays static. Only the element count seen by the command depends on the linked data.
- **Open questions:**
  1. **Proposed resolution - expand or convert.** Expand: an array-valued link element becomes one
     element per entry, at the point where the link is materialised. Convert (issue option 2):
     keep one element and flatten it inside `get_multiple`. Recommended: **expand**. `get_multiple`
     cannot tell a link-derived array from any other one-element array, so it would have to
     flatten every array element. Expanding also keeps one meaning for "a list value supplies a
     list", which `FromParameterValue<Vec<V>>` (`commands.rs`) already gives a non-variadic
     `Vec<Value>` argument. Consequence: a `Vec<Value>` variadic command that wanted one array
     element must now link to `[[…]]`. No in-repo command does this.
  2. **Proposed resolution - depth.** Recommended: expand one level only (AC-6). Recursive
     flattening would make a nested value impossible to pass at all.

### Design Dependencies

- `composite-variadic-arguments` — **required-by** (weak): if composite elements group parameters
  by arity, grouping must count elements after expansion. That design's runtime arity check is
  specified to run on the expanded list. Land this one first, or that one must not assume static
  element counts.
- `UI-VARIADIC-ARGUMENT-LIST-EDITOR` (issue, origin design complete) — `overlaps`: an editor showing
  a link element shows one row. Expansion happens at run time, so the editor does not change.

### Consolidated Findings

- The expansion point is `interpreter.rs` `materialize_nested_parameter`, `MultipleParameters`
  branch. The plan (`plan.rs` `pop_value`, `override_link`) does not change. So plan display,
  validation and dependency recording keep one link per element.
- An expanded entry takes the link's kind: `ParameterLink` / `EnumLink` entries become
  `ParameterValue(name, entry, link position)`, and `OverrideLink` entries become
  `OverrideValue(name, entry)`. Conversion errors then point at the link (AC-6).
- `record_scalar_reading.rs` `record_cell_binds_inside_a_multiple_parameter_through_a_link` uses
  scalar-valued links and must stay green (AC-3). A multi-row record view now expands. That is
  intended, and it is a reason to prefer expansion.
- The `override_link` comment in `plan.rs` and the `get_multiple` / `convert_multiple_element`
  doc comments in `commands.rs` describe today's one-element behaviour, so they change with the fix.

## Phase 2: Architecture

### Solution

In `liquers-core/src/interpreter.rs` `materialize_nested_parameter`, the
`ParameterValue::MultipleParameters(name, values)` branch stops materialising each element 1:1.
It calls a new private helper per element:

```rust
/// Materialises one element of a variadic argument: a link whose value is a JSON array
/// yields one element per entry (one level); anything else yields one element.
async fn materialize_variadic_element<E: Environment>(
    context: &Context<E>,
    element: &ParameterValue,
    out: &mut Vec<ParameterValue>,
) -> Result<(), Error>;
```

The helper matches every `ParameterValue` variant explicitly (no `_ =>`):

- `ParameterLink(n, q, pos)` and `EnumLink(n, q, pos)`: `materialize_link_json(context, q,
  Some(pos))`. For `Array(items)`, push `ParameterValue(n, item, pos)` for each item. Otherwise
  push one `ParameterValue(n, json, pos)`, as today.
- `OverrideLink(n, q)`: the same, with `OverrideValue(n, item)`.
- `DefaultLink(n, q)`: the same, with `DefaultValue(n, item)`. A variadic argument has no default,
  so this only keeps the rule uniform.
- Value variants are pushed unchanged. `MultipleParameters`, `Injected`, `Placeholder` and `None`
  are pushed unchanged too, so `convert_multiple_element` keeps reporting them as today.

The scalar branches of `materialize_nested_parameter` do not change (AC-7). The top-level link
fast path in `apply_plan`'s `Step::Action` arm handles non-variadic links and does not change.

Rejected: flattening in `CommandArguments::get_multiple` (cannot tell where an array came from;
open question 1); expanding in `pop_value` (the plan cannot know the linked value); recursive
flattening (open question 2).

### Changes

- `liquers-core/src/interpreter.rs`: `materialize_nested_parameter` (`MultipleParameters` branch),
  new private `materialize_variadic_element`, unit tests.
- `liquers-core/src/plan.rs` `ResolvedParameterValues::override_link`: comment only.
- `liquers-core/src/commands.rs` `get_multiple`, `convert_multiple_element`: doc comments only.
- Documents: `guides/COMMAND_REGISTRATION_GUIDE.md` §"Accepting a variable number of parameters"
  (new paragraph: a link supplies zero or more elements), `reference/REGISTER_COMMAND_FSD.md`
  §Variadic Parameters (one rule line), `reference/RECIPES_AND_PLANS.md` (the variadic
  override paragraph near line 104). Each gets a History row and a `reviewed:` bump.

No `pub` signature, command, registry or serialized form changes. Async as today: the helper awaits
`materialize_link_json`. It holds no lock.

### Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `interpreter.rs` (code), `plan.rs` and `commands.rs` (comments), three documents |
| Existing tests | `multiple_parameter_links_materialize_in_order` and `multiple_parameter_non_json_link_reports_positioned_conversion_error` (scalar links) stay green. `record_cell_binds_inside_a_multiple_parameter_through_a_link` stays green |
| Compatibility | observable change only for array-valued links in variadic slots, which fail today for every scalar element type |
| Performance | one `match` per element. The arrays were already materialised |
| Recovery | revert one function |
| Certainty | high for the mechanism. The semantics wait on question 1 |

## Phase 3: Examples and Tests

### Examples

```rust
register_command!(cr, fn letters() -> result)?;                    // Value::Array(["b","c"])
register_command!(cr, fn join_all(items: Vec<String> multiple) -> result)?;  // items.join(",")
envref.evaluate("join_all-a-~X~letters~E-z").await?;               // "a,b,c,z"  (AC-2)
```

Validated: `join_all-a-~X~letters~E-z` (with `--command join_all --command letters`) and
`ns-pl/select_columns-id-~X~-R/config/cols.json~E-status` plan as `ParameterValue`,
`ParameterLink`, `ParameterValue` elements.

### Tests

Unit, `liquers-core/src/interpreter.rs` `tests`, with the plan built by hand as
`multiple_parameter_links_materialize_in_order` does:

- `variadic_array_link_expands_in_order` (AC-1, AC-2): elements `a`, link → `["b","c"]`, `z`;
  rendered `[P:a|P:b|P:c|P:z]`.
- `variadic_scalar_link_stays_one_element` (AC-3): link → `"b"` gives one element.
- `variadic_empty_array_link_contributes_nothing` (AC-4): link → `[]` gives `[P:a]`.
- `variadic_override_link_expands_as_override_values` (AC-5): `OverrideLink` → `["b","c"]` gives
  `[O:b|O:c]`.
- `variadic_nested_array_entry_is_not_flattened` (AC-6): link → `[["a","b"],"c"]` gives two
  elements. `get_multiple::<String>` then errs with the link's position.

Integration, new `liquers-core/tests/variadic_link_expansion.rs`, `ImmediateEnvironment`,
`register_command!`:

- `query_link_supplies_variadic_elements` (AC-1, AC-2): `join_all-a-~X~letters~E-z` → `"a,b,c,z"`.
- `array_link_into_scalar_argument_is_unchanged` (AC-7): `count_items-~X~letters~E`, where
  `count_items` is registered by hand with one `ArgumentInfo::any_argument("items")` and reads
  `arguments.get_value(0, "items")`: it receives the whole 2-element array.

Run: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib variadic_` and
`cargo test -p liquers-core --test variadic_link_expansion`; `cargo test -p liquers-lib --test
record_scalar_reading` (AC-3 regression).

## Phase 4: Implementation Plan

### Steps

- [ ] 1. Record the answers to questions 1 and 2 in Phase 1 (or revise this design if the answer
  is "convert") — review only
- [ ] 2. `liquers-core/src/interpreter.rs` — add `materialize_variadic_element`; use it in the
  `MultipleParameters` branch of `materialize_nested_parameter` — `cargo test -p liquers-core --lib interpreter::`
- [ ] 3. `interpreter.rs` `tests` — the five unit tests above — `cargo test -p liquers-core --lib variadic_`
- [ ] 4. `liquers-core/tests/variadic_link_expansion.rs` — the two integration tests —
  `cargo test -p liquers-core --test variadic_link_expansion`
- [ ] 5. Comments in `plan.rs` `override_link` and `commands.rs` `get_multiple` /
  `convert_multiple_element` — `cargo doc -p liquers-core --no-deps`
- [ ] 6. Documents (COMMAND_REGISTRATION_GUIDE, REGISTER_COMMAND_FSD, RECIPES_AND_PLANS) with History rows;
  close `LINK-IN-VARIADIC-DOES-NOT-EXPAND`; this design to `complete` —
  `python3 scripts/docs_index.py` then `--check`

### Validation

`cargo fmt --check -p liquers-core`; `CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib --tests`;
`cargo test -p liquers-lib --test record_scalar_reading`. `specs/command_registry.yaml` does not
change. Diff review: only `liquers-core` and the three documents.
