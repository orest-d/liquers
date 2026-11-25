# UI runner spike (S1, layout data): a free grid whose children carry their place

A spike run on 2026-11-25, true on that date. It extends spike S1 of the UI redesign
([plan](../design/ui-redesign/plan.md) §3; [stage 1](./2026-11-23-ui-runner-spike.md),
[stage 2](./2026-11-24-ui-runner-spike-web.md)) with the one thing §10.9 of the
[master design](../design/ui-redesign/master-design.md) marked "not tested": optional per-child
layout data that only the parent interprets, here for a free grid.

The code is throwaway and is not in the repository. It is kept in the project files under
`spikes/s1-ui-runner/`. This step changed the core crate `lui` by about 70 lines and added a grid
element, a `Grid` view node in the three interpreters and nine tests (seven in
`lui-demo/tests/layout.rs`, one under egui, one for the HTML form). All 25 native tests pass.
The nine browser tests of stage 2 were not run again; `lui-web` compiles for wasm32.

## What was built

- `Spec` (an element as data) has an optional `layout`: a flat map from a key to a number or a
  string. This is the value a command gets from a map-shaped variadic argument
  (`layout: BTreeMap<String, serde_json::Value> multiple`, PR #288), so
  `ns-lui/add-child-current-row-1-col-8-colspan-4` needs no code between the query and the tree.
  The spike does not link Liquers, so the tests build the map directly.
- **The runner keeps the layout data on the child's node.** The parent keeps nothing. It gets
  the layout data of its children together with the list of children when its view is built.
- `Runner::set_layout(handle, map, merge)` changes it later. It reports one new tree change,
  `Layout { handle, layout }`, and marks the parent as updated.
- The tree part of the runner protocol carries `layout` in `Added` (left out when empty) and has
  one new message, `Layout`.
- A portable view node `Grid { columns, cells }`, where a cell has a row, a column, two spans and
  a view. It is drawn in the terminal, in egui and as HTML (a CSS grid).
- A demo `grid` element. A child without a place takes the whole width of the next free row.

## Findings

1. **The proposal holds.** The document of §10.9 (a chart, a table and a log in twelve columns)
   is placed correctly in a terminal, in egui and as HTML. Removing the first child leaves the
   others where they were, which is the case a second array matched by index gets wrong. A child
   added later says where it goes, and a child that says nothing gets a default.
2. **The runner should hold the layout data, not the parent.** §10.9 said the parent keeps it per
   handle as part of its state and receives it with `ChildAdded`. Kept on the child's node
   instead, three things came for free: the parent has no state to keep in step, a snapshot of
   the tree contains it, and a client mirrors it with the tree. `ChildAdded` did not change. The
   saved form is the one of the UISpec example, `layout:` beside the child.
3. **A thick client needs it in the tree protocol; a thin one needs nothing.** With the grid part
   on the client, the client computes the same cells from the mirrored tree, and a change on the
   server moves the child on the client with one `Layout` frame. With no grid part on the client,
   the server sends the grid view and the client only draws it. Both are tested.
4. **A change redraws the parent only.** `set_layout` gives two changes, `Layout` for the child
   and `Updated` for the parent. The child's own view is not rebuilt.
5. **A free grid needs a row height.** A terminal cannot measure a cell before it draws it, and
   egui's immediate layout has the same problem for a row that spans. The spike uses a fixed row
   height in both (two lines, 48 points). The real grid needs a declared row height (`Fixed`,
   §10.8) or a measuring pass. CSS has no such problem.
6. **Wrong layout data is found late.** The grid reports a value that is not a number and a key
   it does not know, but only when its view is built, as a line under the grid. The plan cannot
   find it, because the plan does not know the parent. The command that adds a child should ask
   the parent to check the map and fail with its message. Not built.
7. **Removing a key needs a spelling.** `set_layout` merges: keys given replace or add. A query
   has no `null`, so `ns-lui/layout-current-colspan-` (an empty value) is the natural way to drop
   a key. The spike removes a key on a JSON `null` only.

## Not built

- A parent that needs layout data in `update`, not only in `view` (a dock that reacts to a new
  child). It would read it through its `Cx`, like the list of children.
- A client asking for a layout change (drag to resize in a dashboard): a `RequestLayout` beside
  `RequestAdd` and `RequestRemove`.
- The slot marker for a native retained parent (a DOM grid placing child nodes itself), still
  open as §28 S1-f. The HTML form uses `data-lq-slot` inside each grid cell, as for any child.
- Overlapping cells, and roles (`role-sidebar`), which use the same mechanism with other keys.
