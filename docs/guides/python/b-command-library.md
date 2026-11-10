---
title: Command library
status: ready
sources: [command_registry.yaml]
reviewed: 2026-11-09
---
# B. Command library

PURPOSE: Every command of the standard library, by namespace, generated from the command registry.

These are the commands that come with Liquers. Every environment has them. A command in a
namespace is called with `ns-<namespace>` before it: `ns-rec/head-10`. Commands in the first
group have no namespace.

In Python you can get the same information at run time: `lq.evaluate("commands_doc")` returns it
as Markdown, `lq.evaluate("ns-search/commands")` as a table that can be searched (chapter 13),
and `env.command_metadata(name, namespace)` describes one command.

{{$ -R/specs/command_registry.yaml/-/ns-docs/command_table}}
