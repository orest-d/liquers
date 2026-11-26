---
id: UI-REDESIGN-PRESENTATION-EXTENSION
kind: analysis
title: UI redesign — how an application adds viewers and editors for its own values and arguments
workflow: liquers-project
status: draft
area: [lib/ui, core/commands, web]
created: 2026-11-26
---
# UI redesign: viewers and editors an application can add

Written 2026-11-26 at Orest's request. It belongs to the [master design](./master-design.md) and
proposes a change to its §15. Nothing here is implemented or tested.

## 1. The problem

Orest (2026-11-26) named two problems and asked for solutions that fit the architecture and
solve several problems with one mechanism, not a special case each.

**P1. Built-in elements cannot show a value they were not written for.** Many elements are
defined in `liquers-lib` and are typed on its `Value`. That is fine for `liquers-py`, which adds
no variants. It is a problem for a project that has its own kinds of value and needs its own
visual interface, such as [DPL](https://github.com/orest-d/dpl). There is no systematic way to
teach the built-in elements a new kind of value.

**P2. The query and recipe editors have a fixed set of argument editors.** The set is the closed
enum `ArgumentGUIInfo`. There is no easy way to define a custom argument editor and have the
existing editors use it.

Orest's own proposal: use what Liquers already has, and configure viewers and editors through
commands.

### What DPL does today

Checked in `orest-d/dpl` at `d6ac474`. DPL uses `liquers_lib::value::Value` unchanged, because
Liquers' UI is typed on it. A mesh is `dpl.Mesh`, carried in `ExtValue::Foreign`, and so is
`dpl.Field`. DPL has its own element for a mesh (`MeshViewElement`). What is missing is the link
between the two: the generic value view of Liquers shows a foreign value as a label
(`liquers-lib/src/egui/mod.rs`, `liquers-lib/src/ui/web/html.rs`), and nothing says "a
`dpl.Mesh` is shown by the mesh view". Every place that may meet a mesh has to know about it.

So P1 is not only about the Rust type. Even with one shared `Value`, the choice of what shows a
value is written into the elements.

## 2. The same problem in other places

P1 and P2 are two cases of one question: **given a thing and a purpose, what presents it?** The
same question is asked, and answered by fixed code, in these places:

| # | The thing | The purpose | Today |
|---|---|---|---|
| 1 | a value of some type | show it | a `match` over `ExtValue` in each backend |
| 2 | a value of some type | edit it | only text; `TEXT-EDITOR-EDITS-ARE-NEVER-SAVED` |
| 3 | a command argument | edit it | the closed enum `ArgumentGUIInfo` |
| 4 | a file in a store, by media type or extension | open it | fixed in the store browser |
| 5 | a column of a table | show a cell | text |
| 6 | a value | a one-line summary, a thumbnail, an icon | type name only |
| 7 | a value | a text form for an agent or a terminal | `registry-query-default-text-form` (in review) |
| 8 | an error, a status, metadata | show it | fixed |
| 9 | a value a client cannot hold | what the client gets instead | §15.1 of the master design |

A solution for P1 and P2 should be the solution for the whole table.

## 3. Candidate solutions

### O1. A projector trait (the master design §15 as it stands)

An application implements `Projector<V>` for its value type. It turns a value into a display form
from a closed set (text, table page, image, chart, element).

- For: typed, fast, no query runs; display forms travel to any client.
- Against: Rust only, so a Python or JavaScript application cannot add one. It answers row 1 and
  parts of 6 and 9, and nothing else. The set of forms is closed, so a mesh ends as "element"
  anyway. Configuration needs code.

### O2. A registry of elements by type and purpose, in Rust

A map from (type identifier, purpose) to an element factory. The value view looks the type up
and mounts what it finds.

- For: simple, fast, covers rows 1, 2, 4, 6 and 8.
- Against: a second registry beside the command registry, with its own publication, its own
  bindings for Python and JavaScript, and its own validation. Not configurable without code.

### O3. Conversion only

The new type declares a conversion to a type that can already be shown
(`value-conversion-registry`, `ui-display-conversion`, both in review). A mesh converts to an
image, a field to a table.

- For: no UI code at all, works in every frontend and for agents.
- Against: no interaction and no custom look. It is the right first step for a type, and not
  enough for DPL's viewport. It says nothing about arguments.

### O4. Viewers and editors are commands (Orest's proposal)

The view of a value is the result of a command applied to it, which returns an element (elements
are values, master design §14) or a value that can be shown. An argument editor is an element
made by a command.

```
-R/scene/net.obj/-/ns-dpl/mesh_view          the mesh view of a stored mesh
```

- For: nothing new to register, publish or bind. A command can be written in Rust, Python or
  JavaScript, is described by metadata, is checked by `liquers-validate`, is cached, and runs
  where the value is. "Every view is a query" (VISION) becomes literally true. An application
  with its own `Value` registers its own commands in its own environment.
- Against, if it is used alone: something must still choose the command. Choosing by the
  specialization of one generic command (`command-specialization-by-input-type`, in review) needs
  the type at plan time and gives one global answer per type, which cannot differ per project,
  per user or per frontend. Running a query per table cell or per frame is too slow. An argument
  has no value to apply a command to.

### O5. Presentation rules as data, whose entries are queries

One table answers the question of §2. A rule has a subject, a role, and a presenter:

```yaml
presentation:
  - {type: dpl.Mesh,           role: view,      action: ns-dpl/mesh_view}
  - {type: dpl.Mesh,           role: thumbnail, action: ns-dpl/render-128-128}
  - {type: dpl.Field,          role: view,      convert: RecordView}
  - {type: polars.DataFrame,   role: view,      element: table_view}
  - {argument: color,          role: edit,      element: color_editor}
  - {argument: column,         role: edit,      query: ns-pl/column_picker}
  - {media: model/obj,         role: view,      action: ns-dpl/mesh_view}
```

- A **subject** is a value type identifier, a named kind of argument, a media type, or a record
  field type. All are identifiers that already exist and already travel in metadata.
- A **role** is the purpose: `view`, `edit`, `summary`, `thumbnail`, `icon`, `text`.
- A **presenter** is one of: an action appended to the subject's query (O4), a query that makes
  an element, a registered element type, or a conversion to another type (O3), after which the
  rules are asked again.

The table is data. It is filled from four layers, later ones winning: the defaults of the UI
crate, what a crate registers (`liquers-lib` for its `ExtValue` variants, DPL for its own), a
configuration document of the application, and commands at run time (a session or a user
overriding a viewer).

- For: one mechanism for the whole table of §2. The work is done by commands (O4), so everything
  said for O4 holds. The choice is separate from the work, so it can differ per project, user and
  frontend, and it can be listed, checked and documented.
- Against: one more concept to learn. A rule is found at run time, so a missing rule is a
  fallback on the screen, not a compile error.

## 4. Recommendation

**O5 with O4 inside it: Orest's proposal, plus an explicit table that chooses the command.** O1
and O3 stay, as the two cheapest kinds of presenter, not as separate extension points.

It gives an application a ladder, and each step uses the same table:

| Step | The application writes | It gets | Works in |
|---|---|---|---|
| 0 | nothing | type name, metadata, download | everywhere |
| 1 | a conversion to a known type (O3) | the built-in viewer of that type | everywhere, agents too |
| 2 | a command that returns a display form or a value that can be shown | a custom static view | everywhere |
| 3 | a command that returns a portable element (Rust, Python or JavaScript) | an interactive view | every frontend, thin clients too |
| 4 | a native implementation of that element for one frontend | the frontend's full power (DPL's GPU viewport) | that frontend; the others use step 3 or lower |

Five refinements that make the proposal fit the architecture:

1. **Choose by type identifier, from metadata.** The type of a result is in its metadata before
   the value is loaded, and it is known to a client that cannot hold the value. So the rule is
   found without touching the value, and a rule is found the same way on the server and in a
   client.
2. **The presenter gets the query, not only the value.** A view command returns an element as
   data (`ElementSpec`) whose state names the source query. The element then subscribes to the
   query like any element, so it follows progress, expiry and re-evaluation (R7), and saving the
   UI saves a query, not a mesh.
3. **No query on a hot path.** A rule is resolved once per element or per table column, never
   per cell or per frame. The built-in rules name registered elements and run no command at all.
4. **Fallbacks are part of the table.** If no rule matches the type, the rules are asked for what
   the type converts to, then for the generic JSON, text and bytes viewers, and last for the
   "unknown value" element. A value always shows something (AC-4 of the master design).
5. **In split mode the rule is resolved on the server and checked against the client profile**
   (§15.1). If the client has the element's client part it gets the element; if not, it gets the
   portable view or a display form. A custom native web widget still has to be in the client, or
   be registered in the page from JavaScript.

### Argument editors (P2)

An argument editor is an element with a small fixed contract, the same for built-in and custom
ones:

- **In**: the `ArgumentInfo`, the current value as the query parameters it stands for (one text,
  or the list of texts of a variadic argument; PR #288 defines the shapes), and the query up to
  this action. The last one is what lets an editor offer the columns of the table that flows
  into the command.
- **Out**: the new parameters, and whether they are valid.

In the metadata, `ArgumentGUIInfo` gets one open variant that names a kind of editor and carries
properties, for example `gui: editor "color"`. The existing variants (text field, slider, radio)
become the default rules for their kinds, so the built-in editors go through the same table as a
custom one. A rule may also apply to an argument type or an element type, so every `Key` argument
gets the store browser and every `Color` gets the color editor without each command saying so.
Because the kind is a name in published metadata, the web and Python editors see it too, and a
frontend that has no editor of that kind falls back to the text field.

### What it solves, with one mechanism

| Problem | How |
|---|---|
| P1: own value kinds in built-in elements | a rule for the type; built-in elements ask the table and contain no `match` over value variants |
| P1: built-in elements typed on `liquers-lib`'s `Value` | the generic elements move to the UI crate and need only metadata and rules; `liquers-lib` registers rules for its variants exactly as DPL does |
| P2: custom argument editors | a rule for the argument kind |
| Viewers by file type in the store browser | rules with a `media` subject |
| Cell renderers, summaries, thumbnails, icons | roles |
| Text form of a value for agents and terminals | the `text` role; joins `registry-query-default-text-form` |
| Python and JavaScript viewers (R10) | a command in that language; no new binding |
| Per project, per user, per frontend choice | layers of the table |
| What a thin client gets for a value it cannot hold | the rule resolved against the client profile |
| Documentation and checks | the table is data: it can be listed, and its queries pass `liquers-validate` |

### What changes in the master design

- §15: `Projector` is no longer the extension point. Display forms stay as what travels to a
  client; a projector is how a built-in presenter is implemented.
- §14: "a query that returns any other value mounts an `asset_view`" becomes "mounts what the
  `view` rule of its type gives".
- §10.6 and §16 are unchanged: a native widget and a client part are still how step 4 is built.
- Core: `ArgumentGUIInfo` gets the open variant. This is a change to published command metadata,
  so it has a language-integration side (Python and JavaScript editors and declarations).
- Plan: a new design, between D2 (elements) and the editors, owns the table, the roles and the
  editor contract.

## 5. Risks and open points

1. **Speed of the first view.** A view that is a command is shown one evaluation later than a
   built-in one. Rules that name an element avoid it; a command presenter should be marked
   `cached: false` when it is cheap.
2. **Run-time errors.** A rule that names a missing command fails when used. The table should be
   checked when the environment is built, with the same code as `liquers-validate`.
3. **Too many layers.** Four layers may be more than needed. Defaults, crate, and one
   configuration document may be enough at first; run-time commands can come later.
4. **Editing a value (row 2).** An `edit` role needs a way to write the value back. This rests on
   accessors (`VALUE-ACCESSOR-CANNOT-LOCK`) and is not solved here.
5. **Security.** A rule set at run time decides which command runs when a value is shown. In a
   multi-user server the run-time layer must be per session and limited to commands the session
   may run anyway.

## 6. Questions for Orest

1. Is the explicit table (O5) acceptable on top of your proposal, or do you want the choice made
   by commands alone (O4, through specialization by input type)?
2. Should rules be keyed by the type identifier only, or also by `type_name` (the per-instance
   name), so that two kinds of JSON object can have different viewers?
3. For DPL: is the own `Value` type still wanted once the built-in elements no longer depend on
   `liquers-lib`'s `Value`, or is `ExtValue::Foreign` enough when its viewers can be registered?

## 7. A general mechanism: execution that depends on the type (2026-11-26)

Orest agreed with a table keyed by type and proposed to make it general: not a table for viewers
and editors, but one mechanism for execution paths that depend on the type, for example a
pipeline element that branches on the type. It would serve viewers and editors, and also
serializers and deserializers, and make pipeline commands more flexible. His concerns: pipelines
slowly become more and more powerful, the branch costs performance, and the plan and the
interpreter become more complex.

### 7.1 The mechanism already has a design, placed elsewhere

`command-specialization-by-input-type` (in review, not implemented) is a branch on the type. It
puts the branch in the command registry, not in the pipeline: a **generic command** lists
variants, each an existing command, and the variant is chosen by the type identifier of the
input. It is chosen when the plan is built if the type is known then, and otherwise when the step
runs (`Step::Dispatch`).

With it, the table of §3 O5 needs no structure of its own:

| Table of O5 | With generic commands |
|---|---|
| role (`view`, `edit`, `thumbnail`, `summary`, `text`) | the name of a generic command |
| subject (a type identifier) | what the generic command selects on |
| presenter | the variant: a command, an alias or a pipeline command |
| the table | the command registry, exported and published as it is today |

```
-R/scene/net.obj/-/ns-lui/present-view       what §3 needed a table for
-R/scene/net.obj/-/view                      the same as a generic command
```

### 7.2 A generic command or a branch element in a pipeline

| | Generic command | Branch element in a pipeline |
|---|---|---|
| The pipeline | stays a straight line; the branch is inside one action | gets control flow |
| As a query | representable: it is a command name | not representable as text |
| An arm | a named command: documented, validated, tested, listed in the registry | anonymous elements inside one pipeline |
| An arm that is several steps | a pipeline command as the variant | written in place |
| Adding an arm from another crate (DPL adds `dpl.Mesh` to `view`) | register one more variant | edit the pipeline that holds the branch |
| The plan | one new step kind; an arm is one step | nested plans per arm |
| Pressure to grow | small: there is nothing to add but variants | large: a default arm, conditions on the value, loops |

**Recommendation: the generic command.** It gives every use named in the request, and a pipeline
command as a variant gives "an arm that is a pipeline" with two existing features and no new
element. If a branch written in place is wanted later, it can be sugar for an unnamed generic
command and reuse the same step, so nothing is lost by starting here.

### 7.3 The three concerns

1. **Pipelines growing more powerful.** The line to hold: selection is by type identifier only,
   which is known from metadata. It is never by the content of a value. That is dispatch, as in a
   trait or a method table, and not a conditional. A generic command cannot express `if`, and no
   later step can turn it into one.
2. **Performance.** When the type is declared the variant is chosen in the plan and running
   costs nothing. Otherwise it is one map lookup per step. The real cost is in planning: the
   design in review plans every variant in full inside `Step::Dispatch`. For `head` with two
   variants that is fine. For `view` with thirty types, some of them pipeline commands, it is
   not. See 7.4, point 3.
3. **Plan and interpreter.** One new step kind. Pipeline frames, placeholders and realms are not
   touched.

### 7.4 What the design in review needs for this use

1. **Open registration.** It recommends one place that lists all variants. For viewers the
   opposite is needed: DPL adds a variant to a generic command that Liquers owns. So
   `register_variant(generic, type, command)`, with a rule for two variants claiming one type
   (the later registration wins and the registry says so), and variants from a configuration
   document, as aliases and pipeline commands are data already.
2. **Asking without running.** The UI needs "which variant would `view` choose for
   `dpl.Mesh`?" as a registry lookup, with no plan and no value. This is what keeps a query off
   the hot paths (a table column resolves its renderer once), and it works before the value is
   loaded and in a client that cannot hold it.
3. **Lazy arms.** For a generic command with many variants the plan should name the generic and
   its candidates, and plan the chosen variant when the type is known. The price is that the
   dependencies of such a step are not known before it runs. That has to be accepted or bounded
   (a variant of a lazy generic may not read other assets), and it is the main open technical
   point.
4. **A default variant.** The fallback of §4 (convert, then the generic JSON, text and bytes
   viewers) is the undeclared variant the design already has. Conversion during selection is
   excluded there and would have to be allowed for this.

### 7.5 Where it fits less well

- **Argument editors.** There is no input state to select on. Two cases cover it. When the
  argument has a value type (a key, a color, a query), its editor is `edit` for that type, which
  is the same generic command that edits a value anywhere; rows 2 and 3 of §2 become one. When
  the editor depends on something else (the columns of the table flowing in), the argument names
  the editor command directly, `gui: editor "column_picker"`, and nothing is selected.
- **Serializers and deserializers.** Selection by type and format matches what `TypeInfo`
  already declares. But deserializing has no typed input (it selects on the type identifier and
  format in metadata, not on a value), the store path is hot, and reading the configuration that
  defines commands needs a deserializer first. Proposal: the built-in ones stay native and are
  not routed through the interpreter; a generic `serialize` and `deserialize` exists for types
  that have no native one, which is what lets a Python or JavaScript type bring its own format.
- **Per user or per frontend choice.** The table of O5 had layers. A registry has one answer per
  environment. Per frontend is not needed, because a variant returns an element and each frontend
  has its implementation. Per user would need a session-level overlay; I would leave it out until
  it is asked for.

### 7.6 Revised recommendation

Replace the table of O5 by generic commands: one type-dispatch mechanism in the command
registry, used by the UI for `view`, `edit`, `summary`, `thumbnail` and `text`, usable by any
command library (`head`), and available to serialization for foreign types. The ladder of §4 and
its refinements 1 to 5 are unchanged. No branch element is added to pipelines.

This makes `command-specialization-by-input-type` and `command-io-type-declarations` (both in
review) prerequisites of the UI redesign, with the four changes of 7.4.

Orest (2026-11-26) will think about it. The discussion is recorded in
[`command-specialization-by-input-type/type-dispatch-discussion.md`](../command-specialization-by-input-type/type-dispatch-discussion.md),
and the issue `COMMAND-SPECIALIZATION-BY-INPUT-TYPE-MISSING` is P1.
