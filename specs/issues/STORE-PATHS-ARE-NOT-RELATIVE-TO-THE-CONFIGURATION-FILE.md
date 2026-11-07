---
id: STORE-PATHS-ARE-NOT-RELATIVE-TO-THE-CONFIGURATION-FILE
kind: issue
title: "A relative filesystem path in a store configuration resolves against the working directory, not the configuration file"
status: draft
priority: P3
complexity: S
area: [core/store]
created: 2026-11-07
---
# Store paths are not relative to the configuration file

A `filesystem` mount's `config.path` is passed through `StoreConfig.config`, an opaque map
(`liquers-core/src/store_config.rs`), and the factory uses it as given, so `path: ./site` in
`/srv/app/liquers.yaml` means `./site` under whatever directory the process was started in. A
configuration file moved with its data, or started from another directory, then mounts the wrong
folder. The `configuration-document` design resolves only `command_registry` against the file's
folder (`ConfigDocument::base`). A fix would pass `base` to the store factories, so path-like
arguments of a store type resolve against it.
