---
id: ENVIRONMENT-CANNOT-BE-SHAPED-BY-A-COMMAND-REGISTRY-FILE
kind: feature
title: "An environment's commands cannot be enriched, disabled, aliased or translated by a command registry file"
status: draft
priority: P2
complexity: M
area: [core/commands, py, web]
design: command-registry-file
created: 2026-11-07
---
# An environment cannot be shaped by a command registry file

Orest, 2026-11-07: a command metadata registry (CMR) file named by the configuration should be able
to enrich existing commands, disable commands by not listing them, add aliases, carry translated
labels and descriptions, and provide GUI information and hints; without a file the default CMR is
used, and a default file can be generated on demand. liquers-py's `import_metadata` only overlays
entries and liquers-web's `registerCommands` only adds definitions; nothing disables commands or
exports a starting file from a running environment (the `export-command-registry` binary covers
liquers-lib's built-ins only). Design: `command-registry-file`.
