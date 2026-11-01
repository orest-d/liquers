---
title: Commands
status: outline
sources: [guides/COMMAND_REGISTRATION_GUIDE.md, guides/COMMAND_DESIGN_GUIDE.md, reference/COMMAND_DECLARATION.md, reference/REGISTER_COMMAND_FSD.md]
reviewed:
---
# 5. Commands

PURPOSE: The reader can turn a Rust function into a Liquers command, choose its namespace and defaults, and call it from a query.

TBD: 5.1 A function becomes a command: `register_command!` and its DSL: state, arguments, result.
SOURCE: guides/COMMAND_REGISTRATION_GUIDE.md
TBD: 5.2 Arguments and defaults: types, defaults, links (`query "…"`), `multiple`, `injected`.
SOURCE: reference/COMMAND_DECLARATION.md
TBD: 5.3 Namespaces and realms: `namespace:`, `realm:`, the `ns` instruction.
SOURCE: reference/REALMS.md
TBD: 5.4 The context: logging, progress, evaluating other queries, waiting for dependencies.
SOURCE: guides/COMMAND_REGISTRATION_GUIDE.md
TBD: 5.5 Designing a good command: naming, arguments, what to cache.
SOURCE: guides/COMMAND_DESIGN_GUIDE.md
EDITOR: keep the sections of 5 parallel across the three guides; the defaulting rules may be included from reference/COMMAND_DECLARATION.md once it has a link-free section for them.
