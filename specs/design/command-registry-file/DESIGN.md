---
id: COMMAND-REGISTRY-FILE
kind: design
title: "A command metadata registry file that shapes an environment's commands"
form: compact
workflow: liquers-project
status: in_review
phase: high-level
readiness: needs-decision
autofix: not-eligible
area: [core/commands, py, web]
issues: [ENVIRONMENT-CANNOT-BE-SHAPED-BY-A-COMMAND-REGISTRY-FILE]
affects_docs: [specs/reference/ENVIRONMENT_CONFIG.md, specs/reference/COMMAND_ALIASES.md, specs/guides/COMMAND_REGISTRATION_GUIDE.md, specs/guides/PYTHON_GUIDE.md, specs/README.md]
created: 2026-11-07
---
# A command metadata registry file that shapes an environment's commands

What the `commands.metadata` entry of a [configuration document](../configuration-document/DESIGN.md)
does. Orest, 2026-11-07, gave examples of what a command metadata registry (CMR) file can do
("not so much rules, more examples"): specify or enrich existing commands; disable existing commands
by not listing them; add aliases; make a language mutation of command labels and descriptions;
provide or configure GUI information and additional hints. Without a file, the default CMR is used,
and a default CMR file can be generated on demand. Compact form (`M`); Phase 1 only so far.

## Phase 1: High-Level Design

### Purpose

An application, a deployment or a translator should be able to change how an environment's
commands are presented and which of them are offered, without changing the code that registers
them. The format already exists: `CommandMetadataRegistry` (`specs/command_registry.yaml`) carries
labels, docs, hints, argument `gui_info`, presets and definitions (`Alias`, `Pipeline`). What is
missing is a way to apply such a file to an environment and to write one out. Today liquers-py's
`import_metadata` overlays entries and refuses unknown commands, and liquers-web's
`registerCommands` takes definitions; neither disables commands or exports a starting file.

### Problem Example

```yaml
# commands.de.yaml — generated with env.export_command_registry(), then edited
commands:
  - name: greet
    label: Begrüßen
    doc: Begrüßt eine Person.
    arguments:
      - name: name
        label: Name
        gui_info: !TextField 20
  - name: hallo                    # an alias added in the file
    label: Hallo
    arguments:
      - {name: name, argument_type: string}
    definition: !Alias
      command: {realm: '', namespace: '', name: greet}
      head_parameters: []
# `report` is registered in code but not listed: it is not offered
```

**Today** the file cannot be applied: `import_metadata` adds the entries but `report` stays, and
there is no way to produce the starting file. **After** `greet` shows its German label and text
field, `hallo` runs `greet`, and `report` is unknown to queries.

### Scope and Acceptance Criteria

- **AC-1** Enrich a registered command
  WHEN the file lists a registered command with a label, doc, hints and argument GUI information
  THEN the environment reports them, and the command's interface and implementation are unchanged
- **AC-2** Disable by omission
  WHEN a registered command is not listed in the file
  THEN a query naming it fails as an unknown command, and `validate()` lists it as disabled
- **AC-3** Aliases and pipeline commands from the file
  WHEN the file defines an alias or a pipeline command whose targets are listed
  THEN it runs like one registered in code
- **AC-4** A language mutation
  WHEN a second file translates labels and descriptions and the configuration names it
  THEN the environment reports the translations
- **AC-5** The default CMR
  WHEN no file is configured
  THEN the registry is exactly what code registered (today's behaviour)
- **AC-6** Generate the default CMR
  WHEN `env.export_command_registry()` (Python) or its Rust counterpart runs
  THEN it returns a YAML document of the registry as code registered it, which applied as a file
  changes nothing
- **AC-7** Mistakes are reported
  WHEN an entry names a command that is not registered and defines none, or changes a command's
  arguments
  THEN `validate()` reports an error naming the entry, and the next build fails as today

Non-goals: per-user or per-request registries (`access-policy-at-entry-points`); a translation
format other than a CMR file; the GUI editor that would write such files.

### Documentation Intent

- Reference: `ENVIRONMENT_CONFIG.md` §Command metadata file; `COMMAND_ALIASES.md` (aliases from a
  file).
- Guide: `COMMAND_REGISTRATION_GUIDE.md` "Shape commands with a registry file"; `PYTHON_GUIDE.md`.
- API docs: the core module that applies the file (Phase 2 names it) and liquers-py's
  `environment.rs`.

### Open Questions

1. **Proposed: what the file may change.** The file owns presentation (label, doc, hints, argument
   labels, descriptions and `gui_info`, enum alternative labels, presets, `next`, `filename`) and
   may add definitions (aliases, pipeline commands). Code owns the interface (argument names,
   types, order, `injected`, `multiple`, state argument, async, volatility, implementation
   version); a file entry that differs there is AC-7's error. Generated fields such as
   `metadata_version` and `impl_version` in the file are ignored.
2. **Proposed: what omission disables.** Every command code registers, built-ins included, is
   disabled when a file is configured and does not list it; that includes a Python command
   decorated after the environment was created. `validate()` lists the disabled commands, so a
   stale file is visible. A configuration that wants only enrichment and no disabling sets
   `commands: {metadata: …, unlisted: keep}` (default `disable`).
3. **Proposed: several files.** `commands.metadata` is a list applied in order: a base file, then
   a translation that only overlays (an entry not in the base is an error; omission in a later
   file disables nothing).
4. **Proposed: where it applies.** In core, after code registration and before the build
   validates the registry, so every host (Rust, Python, liquers-web from text) gets the same
   behaviour; liquers-py's `import_metadata` becomes one call of it.

### Design Dependencies

- `configuration-document`: **required-by** (its `commands` section).
- `access-policy-at-entry-points` (in review): **overlaps**; disabling by omission is not access
  control.
