---
title: Command library
status: ready
sources: [command_registry.yaml]
reviewed: 2026-11-14
---
# B. Command library

PURPOSE: Every command of the standard library, by namespace, generated from the command registry.

These are the commands that come with `liquers-lib`. An environment has the groups that you
register (section 2.4), and a group exists when its feature is on (section 2.3). A command in a
namespace is called with `ns-<namespace>` before it: `ns-rec/head-10`. Commands in the first
group have no namespace.

The same information is available at run time. The query `commands_doc` returns it as Markdown,
`ns-search/commands` as a table that can be searched (section 13.7), and
`env.command_metadata_snapshot()` gives the descriptions themselves, a
`CommandMetadataRegistry`, from which this appendix is generated.

{{$ -R/specs/command_registry.yaml/-/ns-docs/command_table}}
