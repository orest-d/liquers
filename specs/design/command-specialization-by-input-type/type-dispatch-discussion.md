---
id: COMMAND-SPECIALIZATION-TYPE-DISPATCH-DISCUSSION
kind: analysis
title: Type dispatch as a general mechanism — discussion of 2026-11-26 (viewers, editors, serializers)
workflow: liquers-project
status: draft
area: [core/commands, core/plan, lib/ui]
created: 2026-11-26
---
# Type dispatch as a general mechanism: discussion of 2026-11-26

A record of a discussion between Orest and Claude on 2026-11-26, during the UI redesign
(`specs/design/ui-redesign/`). It gives this design a second and larger use than `head`, and it
lists what the design would have to change for that use. **Nothing is decided.** Orest asked for
the discussion and its background to be recorded here while he thinks about it, and raised the
issue `COMMAND-SPECIALIZATION-BY-INPUT-TYPE-MISSING` to P1. Phases 1 to 4 of this design are
unchanged.

The longer analysis is
[`ui-redesign/presentation-extension.md`](../ui-redesign/presentation-extension.md); §7 there is
the part about this design.

## 1. Background: two problems of the UI

1. **Built-in UI elements cannot show a value they were not written for.** Many elements are in
   `liquers-lib` and are typed on its `Value`. A project with its own kinds of value and its own
   visual interface, such as [DPL](https://github.com/orest-d/dpl), has no systematic way to
   teach the built-in elements a new kind. DPL (at `d6ac474`) carries `dpl.Mesh` and `dpl.Field`
   in `ExtValue::Foreign` and has its own mesh view element; nothing links the type to the
   element, and the generic value view shows a foreign value as a label.
2. **The query and recipe editors have a fixed set of argument editors** (the closed enum
   `ArgumentGUIInfo`). A custom argument editor cannot be defined and used by the existing
   editors.

Orest asked for a holistic solution, one mechanism for several problems, and proposed to use what
Liquers has: configure viewers and editors through commands.

## 2. How the discussion went

1. **One question behind both problems.** "Given a thing and a purpose, what presents it?" is
   answered by fixed code in at least nine places: showing a value, editing a value, editing an
   argument, opening a file by media type, a table cell, a summary or thumbnail, a text form for
   an agent, showing an error or metadata, and what a client gets for a value it cannot hold.
2. **Claude's first proposal: presentation rules as data.** A table from a subject (a type
   identifier, an argument kind, a media type) and a role (`view`, `edit`, `summary`,
   `thumbnail`, `text`) to a presenter, which is a command, an element or a conversion. Commands
   do the work, as Orest proposed; the table chooses the command.
3. **Orest: aligned with a table keyed by type, but make it general.** Not a table for viewers
   and editors, but one mechanism for execution paths that depend on the type, for example a
   pipeline element that branches on the type. It would serve viewers and editors, and also
   serializers and deserializers, and it would make pipeline commands more flexible. His
   concerns: pipelines slowly become more and more powerful; the branch logic may cost
   performance; the plan and the interpreter become more complex.
4. **Claude: that mechanism is this design, placed in the command registry.** A generic command
   is a branch on the type. With it the table of point 2 needs no structure of its own: the role
   is the name of a generic command, the subject is what it selects on, the presenter is the
   variant, and the table is the command registry.
5. **Orest (2026-11-26): he will think about it.** Record it here; the issue becomes P1.

## 3. A generic command or a branch element in a pipeline

| | Generic command (this design) | Branch element in a pipeline |
|---|---|---|
| The pipeline | stays a straight line; the branch is inside one action | gets control flow |
| As a query | representable: it is a command name | not representable as text |
| An arm | a named command: documented, validated, tested, listed in the registry | anonymous elements inside one pipeline |
| An arm that is several steps | a pipeline command as the variant | written in place |
| Adding an arm from another crate (DPL adds `dpl.Mesh` to `view`) | register one more variant | edit the pipeline that holds the branch |
| The plan | one new step kind; an arm is one step | nested plans per arm |
| Pressure to grow | small: there is nothing to add but variants | large: a default arm, conditions on the value, loops |

Claude's recommendation is the generic command. A branch written in place, if it is wanted
later, can be sugar for an unnamed generic command and reuse `Step::Dispatch`.

Against Orest's three concerns:

- **Pipelines growing more powerful.** Selection is by type identifier only, which is known from
  metadata, and never by the content of a value. That is dispatch, not a conditional; a generic
  command cannot express `if`.
- **Performance.** No cost when the type is declared (the variant is in the plan), one map
  lookup otherwise. The cost is in planning; see §4, point 3.
- **Plan and interpreter.** One new step kind. Pipeline frames, placeholders and realms are not
  touched.

## 4. What this design would have to change for the UI use

1. **Open registration of variants.** Phase 1 question 1 recommends one place that lists all
   variants (`register_generic`) over each variant declaring what it specializes. For viewers the
   opposite is needed: another crate adds a variant to a generic command that Liquers owns
   (`register_variant(generic, type, command)`), and a configuration document can add one. It
   needs a rule for two variants claiming one type; proposed: the later registration wins and
   the registry records it. This reopens question 1.
2. **Asking without running.** The UI needs "which variant would `view` choose for `dpl.Mesh`?"
   as a registry lookup, with no plan and no value. It keeps a query off the hot paths (a table
   column resolves its renderer once), and it works before the value is loaded and in a client
   that cannot hold the value.
3. **Lazy arms.** Phase 1 question 2 plans every variant in full inside `Step::Dispatch`. That
   suits `head` with two variants. For `view` with some thirty types, some of them pipeline
   commands, the plan should name the generic and its candidates and plan the chosen variant when
   the type is known. The price: the dependencies of such a step are not known before it runs.
   That has to be accepted or bounded (for example, a variant of a lazily planned generic may not
   read other assets). **This is the main open technical point.**
4. **A default variant and conversion.** A value must always show something, so `view` needs a
   fallback chain: what the type converts to, then the generic JSON, text and bytes viewers. The
   undeclared variant of question 3 is the last step of it. Conversion during selection is
   excluded by question 3 and would have to be allowed here.
5. **Variants share the argument shape** (question 5). This holds for `view` and `edit` and is no
   obstacle.

## 5. Where the mechanism fits less well

- **Argument editors.** An argument editor has no input state to select on. Two cases cover it.
  When the argument has a value type (a key, a color, a query), its editor is the generic `edit`
  for that type, the same command that edits a value anywhere. When the editor depends on
  something else (the columns of the table that flows into the command), the argument names its
  editor command directly, for example `gui: editor "column_picker"`, and nothing is selected.
  Either way `ArgumentGUIInfo` needs one open variant, which is a change to published command
  metadata.
- **Serializers and deserializers.** Selection by type and format matches what `TypeInfo`
  declares. But deserializing has no typed input (it selects on the type identifier and format in
  metadata, not on a value), the store path is hot, and reading the configuration that defines
  commands needs a deserializer first. Proposal: the built-in ones stay native and do not go
  through the interpreter; a generic `serialize` and `deserialize` serves types that have no
  native one, which is what lets a Python or JavaScript type bring its own format.
- **Per user or per frontend choice.** A registry has one answer per environment. Per frontend
  is not needed, because a variant returns an element and each frontend has its implementation
  of it. Per user would need a session-level overlay and is left out until it is asked for.

## 6. Generic commands the UI would use

Not designed; the names are placeholders. Each returns an element as data, or a value that can
itself be shown.

| Generic command | Purpose |
|---|---|
| `view` | the main view of a value |
| `edit` | an editor of a value, also used for an argument of that type |
| `summary` | one line, for a table cell or a list |
| `thumbnail` | a small image |
| `text` | a text form for an agent or a terminal (`registry-query-default-text-form`) |

```
-R/scene/net.obj/-/view
```

parses and plans with a `view` command declared (`liquers-validate --command view`); `view` does
not exist.

## 7. Consequences if this direction is taken

- This design and `command-io-type-declarations` become prerequisites of the UI redesign, where
  today they serve the command libraries only.
- §15 of the UI master design changes: the `Projector` trait is no longer the extension point.
- The reference generic of question 4 (`head`) is joined by the UI's generics, which live in the
  UI crates, so the mechanism must not depend on `liquers-lib`.

## 8. Open, for Orest

1. Generic commands as the direction, or is there a case that needs a branch written inside a
   pipeline?
2. Open registration of variants (§4, point 1): accepted?
3. Lazy arms and their unknown dependencies (§4, point 3): accept, bound, or plan all arms?
4. Serializers: native built-ins plus a generic for foreign types, or everything through one
   path?
