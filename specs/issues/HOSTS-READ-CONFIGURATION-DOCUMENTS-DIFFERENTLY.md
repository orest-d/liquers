---
id: HOSTS-READ-CONFIGURATION-DOCUMENTS-DIFFERENTLY
kind: issue
title: "Each host parses its configuration document on its own and ignores sections it does not know"
status: draft
priority: P3
complexity: M
area: [core/context, mcp, web, py]
created: 2026-11-07
---
# Hosts read configuration documents differently

`liquers-agent` (`AgentConfig::from_yaml` takes `memory` out by hand), `liquers-web` and liquers-py
each turn a document into an `EnvironmentConfig` their own way, and `EnvironmentConfig` ignores
unknown keys (`an_unknown_key_is_currently_ignored` in `liquers-core/src/environment_config.rs`), so
a misspelt or unsupported section (`server:` in a browser) passes silently.

The design `liquers-py-web-server` (Phase 2, §Configuration Document, Orest 2026-11-07) adds a
portable `ConfigDocument` in core with `take_section` and `finish`, used first by liquers-py and the
`liquers-axum` server. Still to do after it lands: move `AgentConfig` and liquers-web onto it, so
every host refuses a section it does not support.
