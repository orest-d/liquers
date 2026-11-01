---
title: Commands
status: outline
sources: [guides/PYTHON_GUIDE.md, guides/COMMAND_REGISTRATION_GUIDE.md, reference/COMMAND_DECLARATION.md]
reviewed:
---
# 5. Commands

PURPOSE: The reader can turn a Python function into a Liquers command, choose its namespace and defaults, and call it from a query.

TBD: 5.1 A function becomes a command: `@command`, the state argument, what introspection fills in.
SOURCE: guides/PYTHON_GUIDE.md §5
TBD: 5.2 Arguments and defaults: types, defaults, links; only what differs from Rust.
SOURCE: reference/COMMAND_DECLARATION.md
TBD: 5.3 Namespaces and realms: the decorator's options.
SOURCE: guides/PYTHON_GUIDE.md §5
TBD: 5.4 The context: logging, progress, evaluating other queries.
SOURCE: guides/PYTHON_GUIDE.md §5
EDITOR: keep the sections of 5 parallel across the three guides; the defaulting rules may be included from reference/COMMAND_DECLARATION.md once it has a link-free section for them.
