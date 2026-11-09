---
title: The environment
status: draft
sources: [guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md, reference/ENVIRONMENT_CONFIG.md]
reviewed:
---
# 8. The environment

PURPOSE: The reader can build an environment with the commands, stores and recipes an application needs.

TBD: 8.1 EnvironmentBuilder: commands first, then stores and recipes; `build`.
SOURCE: guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md

## 8.2 Configuration documents

One file configures a program built on Liquers: the store, the recipes, the asset manager, a
command metadata file, and sections of the parts you use, such as the web server's `server`.

```yaml
# liquers.yaml
store:
  stores:
    - {type: filesystem, prefix: data, config: {path: ${LIQUERS_DATA}}}
recipes: default
assets:
  job_capacity: 8
command_registry: commands.yaml
server:
  port: 8000
```

Read it as a `ConfigDocument`. Each part of your program takes its section, and `finish()` returns
the environment's configuration once nothing is left over:

```rust
use std::path::Path;
use liquers_axum::ServerConfig;
use liquers_core::environment_config::ConfigDocument;

let mut doc = ConfigDocument::load(Path::new("liquers.yaml"))?;
let server = ServerConfig::from_document(&mut doc)?;      // the `server` section
let metadata_file = doc.command_registry_path();          // next to liquers.yaml
let environment = doc.finish()?;                          // `sever:` would fail here
```

`load` reads `.yaml`, `.yml` and `.json` files, and `.toml` with the `toml` feature of
`liquers-core`; `from_yaml` and `from_json` read text. A section nobody took is an error that
names it and lists the known sections, so a typo never leaves a setting at its default without
notice. A section of your own is any type that implements `serde::Deserialize`:
`doc.take_section::<MySettings>("my_app")?` returns `None` when the file has no such section.

Give the environment's configuration to the builder with `with_config`, register your commands,
and build:

```rust
let mut builder = EnvironmentBuilder::<Value>::new()
    .with_config(environment, Box::new(default_store_factory()));
register_my_commands(&mut builder.command_registry)?;
let envref = builder.build()?;
```

Commands are always registered in code. The command metadata file, when the document names one,
adds labels, documentation and argument hints to them: read it as a `CommandMetadataRegistry` and
add its commands to `builder.command_registry.command_metadata_registry` before `build()`. An entry
naming no registered command fails the build.

A variable in the store section, such as the `LIQUERS_DATA` above, takes the environment
variable's value when the store is built, and an unset variable fails `build()`.

TBD: 8.3 Execution models and manager options: inline and queued, caching strategies.
SOURCE: guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md §Choosing an execution model

The same in Python: [8. The environment](../python/08-environment.md).
