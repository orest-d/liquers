---
title: Store Configuration Guide
kind: guide
audience: internal
area: [store/config, core/store]
reviewed: 2026-10-19
---
# Store Configuration Guide

This guide has two parts:
- **Part 1** covers writing a mount's `read_only` flag and `metadata` section: which entries exist,
  how to spell a spec, how to check one.
- **Part 2** covers adding a new entry: a handler, a storage model or a write strategy.

Related documents:
- The configuration format: [`reference/STORE_CONFIG_FSD.md`](../reference/STORE_CONFIG_FSD.md)
  §Metadata section. It owns the defaults table.
- What a configured store does: [`reference/STORE_SEMANTICS.md`](../reference/STORE_SEMANTICS.md),
  §11 (data-only access), §12 (read-only), §13 (the pipeline and storage models), §14 (stages),
  §15 (write strategies) and §16 (grouped writes).
- Why `store_config` is a realm of queries: [`reference/REALMS.md`](../reference/REALMS.md).

The code is in `liquers-core/src/store/`:
- `realm.rs`: the compiler and the types;
- `handlers.rs`: the built-in handlers;
- `storage.rs`: the storage models;
- `writes.rs`: the write strategies;
- `pipeline.rs`: the store that runs them.

The configuration types are in `liquers-core/src/store_config.rs` and the builder in
`liquers-core/src/store_factory.rs`.

---

# Part 1: Writing `metadata` sections

## 1. What a section does

```yaml
stores:
  - type: filesystem
    prefix: docs
    config: { path: ./docs }
    read_only: true
    metadata:
      storage: none
      on_read: media_type/file_facts/content_hash-on_change/front_matter
```

`StoreRouterBuilder` wraps each mount in this order (`build_router` in `store_factory.rs`):
1. the factory's store;
2. a `MetadataPipelineStore`, if the mount has a non-empty `metadata` section;
3. a `ReadOnlyStore`, if `read_only` is set.

A mount with **no section, or `metadata: {}`**, is not wrapped by a pipeline. It behaves exactly
as before this design existed, including writing default sidecars on read. The section changes
three things:
- **where** records are kept (`storage`);
- **how** a write is committed (`writes`);
- **what** a record contains, through four handler chains (`on_default`, `on_validate`,
  `on_write`, `on_read`).

**A section needs a backend with data-only access** (`AsyncStore::data_capabilities().data_access`).
`AsyncMemoryStore`, `AsyncFileStore`, `AsyncOpenDALStore` and a `JsStore` with the five data
methods have it. On any other store, a section fails the build. `read_only` alone works on any
store, but over a store without data access the builder prints a warning to stderr: the store's
own reads may still write default metadata.

## 2. The fields and their defaults

`MetadataConfig` (`store_config.rs`) has seven fields. It is `deny_unknown_fields`, so a misspelt
field, or the old free-form map, is refused when the document is parsed.

| Field | Kind | Default (`METADATA_DEFAULTS`) |
|---|---|---|
| `storage` | storage model spec, exactly one entry | `native` |
| `writes` | write strategy spec, exactly one entry | `direct` |
| `persist_defaults` | bool | `false` |
| `on_default` | handler chain | `defaults/media_type-fill/file_facts` |
| `on_validate` | handler chain | `""` |
| `on_write` | handler chain | `defaults/media_type-fill/file_facts` |
| `on_read` | handler chain | `media_type-fill/file_facts` |

The authoritative copy of this table is in `STORE_CONFIG_FSD.md`, where
`metadata_config_defaults_match_the_documented_table` checks it against the constant.

- **A field you set replaces its default whole.** Nothing is merged. `on_read: file_facts` drops
  the default `media_type-fill`. To add a step, write the default out and append to it.
- **`""` means no steps.** `on_write: ""` stores the caller's record unchanged.
- **`persist_defaults: true`** stores the record `on_default` built the first time an entry is
  read. A `read_only` mount never persists, whatever this says.

## 3. Writing a spec

A spec is a query of the realm `store_config`. Actions are separated by `/` and parameters by `-`:

```
defaults/media_type-fill/file_facts
```

- **The realm prefix is optional in configuration.** `-store_config/defaults/…` and
  `defaults/…` are the same. The compiler fills the realm into the parsed query
  (`realm::fill_realm`). A header naming any other realm (`-lui/…`) is refused.
- **Parameters are positional,** in the order of the catalogue below. Trailing parameters take
  their defaults. Surplus parameters are a plan error ("Too many parameters").
- **Options are checked when the spec is planned.** `content_hash-sometimes` fails with "Valid
  values: on_change, always, off".
- **Escape characters the grammar uses.** `/` is written `~I`, so `set-media_type-text~Imarkdown`
  sets `text/markdown`. `-` is written `~_`. `~.` is a *space*, not a dot: write a dot as it is
  (`grouped-__metadata__.json`). The full table is in `PROJECT_OVERVIEW.md` §1.
- **Only literals.** A link (`~X~…~E`), a placeholder or an injected parameter is refused.
- **Only actions.** A resource segment (`-R-key/…`, which could one day scope handlers to a key:
  [`STORE-CONFIG-SPEC-CANNOT-SCOPE-HANDLERS-TO-A-KEY`](../issues/STORE-CONFIG-SPEC-CANNOT-SCOPE-HANDLERS-TO-A-KEY.md)), an output filename, `SetCwd` or anything that
  needs the asset layer is refused. Planner messages (`Info`, `Warning`, `Error`) are kept and
  logged once per record.
- **A `multiple` parameter takes the rest of the action.** An empty list has a meaning for each
  entry that has one: `require` refuses it at build, while `front_matter` and `front_matter_sync`
  read it as `[title, description]`.
- **Watch the `/`.** `require-title-media_type` requires two fields.
  `require-title/media_type` requires `title` and then runs the `media_type` handler, which in
  `on_validate` checks the media type against the extension. Both are valid, but they mean
  different things.

## 4. Stages

| Stage (field) | Runs | A step's error |
|---|---|---|
| `on_default` | on a read of an entry that has data and no stored record, starting from an empty record | fails the read |
| `on_validate` | on `set` and `set_metadata`, before anything is written | refuses the write; nothing is written |
| `on_write` | after `on_validate`, on the record to be stored | fails the write |
| `on_read` | on every read, over the stored or default record | becomes `Metadata handler failed: …` in the record's log; the read succeeds |

Within a chain, steps run in order on one record, and **the later step wins** when two set one
field. Steps that detect change (`content_hash-on_change`, `front_matter` in `on_read`) compare
the backend's facts with the record **as stored** (`FactsIn::previous`), so their position in the
chain does not matter.

A chain reads the bytes only when one of its steps asks for them (`needs_bytes`). `get` always has
the bytes, because the caller asked for them. `get_metadata` and listings read them only for an
`on_read` or `on_write` step that needs them. A read whose `DataFacts` are unchanged is a cache hit
and runs no chain. Full semantics: `STORE_SEMANTICS.md` §14.

## 5. Catalogue: realm `store_config`

All entries are in namespace `""`. The registry declares them (`specs/command_registry.yaml`,
realm `store_config`), and each one's hint names its kind. `mode` is `overwrite` (replace the
field) or `fill` (set it only where it is empty). The **text fields** are `title`, `description`,
`media_type`, `data_format`, `type_identifier` and `unicode_icon` (`handlers::TEXT_FIELDS`).

### Handlers: `on_default`, `on_validate`, `on_write`, `on_read`

| Entry | Parameters | Stages | Effect |
|---|---|---|---|
| `defaults` | none | default, write | Default: the record stores have always synthesized: `updated` now, `key`, `query`, `filename` and `data_format` from the key, `is_dir` false, `file_size`, `status: Source` where it is `None`, and the warning "New metadata has been created. (*operation*)". Write: `updated` now, `key`, `filename` if unset, `file_size`, `status: Source` where `None` |
| `set` | `field`, `value`, `mode = overwrite` | default, write, read | Sets one text field; any other field name fails at build |
| `require` | `fields…` (multiple; empty fails at build) | validate | Refuses a record whose named text field is empty, naming the first |
| `media_type` | `mode = overwrite` | all four | Sets `media_type` from the key's extension (`file_extension_to_media_type`); an unknown extension leaves it alone. Validate: refuses a non-empty `media_type` that disagrees with a known extension |
| `file_facts` | `mode = overwrite` | default, write, read | Default/read: `file_size`, `created` and `etag` from the backend's `DataFacts`. Write: `file_size` from the bytes. The pipeline sets `created` and `etag` itself after the commit |
| `content_hash` | `when = on_change` (`on_change` · `always` · `off`) | default, write, read | Sets `version` from the bytes. `always` reads the bytes on every chain run. `on_change` reads them only when size, `created` or `etag` differ from the stored record, or it has no `version`. Whenever the bytes are at hand (`set`, `get`), it hashes. In `on_read`, it sets the mount's `version_verification` (`Off` for `off`, else `OnRead`) |
| `updated` | none | write | Sets `updated` to now, for a chain without `defaults` |
| `front_matter` *(liquers-lib, `records`)* | `mode = overwrite`, `fields…` (multiple; empty means `title`, `description`) | default, write, read | For a Markdown entry (by extension or `media_type`), copies the named text fields from its YAML or TOML front matter. Other entries: no effect. In `on_read` it reads the bytes only when the entry changed |
| `front_matter_sync` *(liquers-lib, `records`)* | `fields…` (multiple; empty means `title`, `description`) | write | In `set_metadata` only, rewrites a Markdown entry's YAML front matter from the record's fields (an empty field removes its key), so the metadata write becomes a data write too. In `set`, the caller's bytes are kept. TOML front matter is left alone, with a warning |
| `markdown_tiers` *(liquers-lib, `records`)* | `sections…` (multiple) | default, read | For a Markdown entry whose record has an empty `title` or `description`: fills `title` from the first heading and `description` from the first paragraph of the first listed section present, else the first paragraph after the front matter. Never overwrites, never writes. Put it after `front_matter`, as a fallback for documents without front matter |

`mode` comes first in `front_matter`, so its field list starts at the second parameter:
`front_matter-fill-title-description`. `front_matter-title` fails, because `title` is not a mode.

The two lib entries are registered by `liquers_lib::store_config::register_lib_store_config`,
which `register_records_commands!` (inside `register_all_commands!`) calls. A core-only
environment cannot name them.

### Storage models: `storage`

| Entry | Parameters | Records kept | Reserves |
|---|---|---|---|
| `native` | none | the backend's own metadata, through `get_stored_metadata` / `set_metadata` (the stores' existing sidecars) | what the backend reserves |
| `per_entry` | `layout = sidecar` (`sidecar` · `root` · `prefix` · `folder`) | one object per key: `{key}.__metadata__` · `__metadata__/{key}` · `{folder}/__metadata__{name}.json` · `{folder}/__metadata__/{name}.json` | the suffix, the `__metadata__` segment or the `__metadata__` prefix |
| `grouped` | `file = __metadata__.json` (a plain file name) | one JSON map, name → record, per folder | that file name in every folder |
| `none` | none | nothing; every record comes from `on_default` and `on_read` — the choice for a checkout edited by hand, since a hand edit is seen at the next read and no sidecar lands beside the files | nothing |
| `central` | `mount` | declared only: in a router-level section the builder resolves the mount (an unknown one, or a member section, is a configuration error), then the build fails with "declared but not implemented yet" ([`STORE-CENTRAL-STORAGE-MODEL-NOT-IMPLEMENTED`](../issues/STORE-CENTRAL-STORAGE-MODEL-NOT-IMPLEMENTED.md)) | — |
| `memory_central` | none | **test only** (`register_test_store_config`, under `cfg(test)` / feature `store-conformance`); an in-memory central store | nothing |

`per_entry` takes a layout *name* rather than a template, because a `/` in a template would have to
be escaped in every spec.

### Write strategies: `writes`

| Entry | Commit | Needs | Reserves |
|---|---|---|---|
| `direct` | each object in place, in order | `data_access` | nothing |
| `atomic` | each object atomically: in place where the backend writes atomically, else staged and renamed | `atomic_write` or `rename` | `.__staged__` |
| `transactional` | all objects staged, a journal, then the moves; an interrupted write is rolled forward or back on the next read or `sync` | `data_access` | `.__staged__`, `.__txn__` |

A client key using a reserved name is refused with `key_not_supported`, by `set` and by
`is_supported` alike. Recovery and crash behaviour: `STORE_SEMANTICS.md` §15.

## 6. Router-level sections and the two build calls

`read_only` and `metadata` may also sit at the top of the document, beside `stores`.
- A router-level `read_only` wraps the whole router.
- A router-level `metadata` section is **central storage**: one pipeline over the router, which
  is reached through its members' data-only methods. Under it, a member's own section may name
  only `storage`, which becomes that member's *backup* copy. A member section that also names
  handlers, `writes` or `persist_defaults` fails, naming the member. A member with no section
  keeps no backup.

Only one storage model can serve central storage today: `memory_central`, which is test-only.

| Call | Returns | Router-level sections |
|---|---|---|
| `StoreRouterBuilder::build` / `build_without_env_expansion` | `AsyncStoreRouter` | **refused**, with a message naming `build_store` |
| `StoreRouterBuilder::build_store` / `build_store_without_env_expansion` | `Arc<dyn AsyncStore>` | applied |

`EnvironmentBuilder::build` uses `build_store`. It passes the environment's own
`command_metadata_registry` and `store_config` functions, so an entry a library registered is
nameable in the environment's store configuration. A bare `StoreRouterBuilder` uses
`StoreConfigFunctions::builtin()` unless it is given a pair with `with_store_config(registry,
functions)`. This is the case in `liquers-store`'s `create_router*` and `liquers-web`'s builder.

## 7. Validating a spec

The build checks everything, but `liquers-validate` is quicker. **Give it the filled form**, because
the tool does not fill the realm:

```bash
cargo run -p liquers-core --features cli --bin liquers-validate -- --detail summary -- \
  '-store_config/defaults/media_type-fill/file_facts'
```

Without `-store_config/`, the actions plan in realm `""`, and the tool reports "Action 'defaults'
not registered". The tool checks names, parameter counts and options against
`specs/command_registry.yaml`. It does **not** check:
- the stage rules (`require` in `on_read`);
- the field names `set` accepts;
- the kind of a component (a write strategy written in `storage`);
- refused steps (a resource segment plans fine).

The build checks all of these.

## 8. Error messages

Every build error is a `configuration_error`. A mount's errors start with `Mount '<prefix>' (type
<type>):`, and the router's section's errors start with `Router metadata:`. A spec's errors quote
the field and the **filled** spec:

| Message (after the prefix) | Cause |
|---|---|
| `Store configuration 'storage: -store_config/sidecars' does not plan: Action 'sidecars' not registered …` | unknown entry |
| `… does not plan: Can't convert 'sometimes' to when: … Valid values: …` | bad option |
| `… does not plan: Too many parameters for command 'updated' …` | surplus parameter |
| `Store configuration 'on_read: …': 'require' cannot be used in on_read; it belongs in on_validate (at …)` | stage refusal |
| `… 'transactional' is a write strategy, not a storage model` / `… is a handler, not …` / `… is a storage model, written in 'storage', not a handler` | entry in the wrong field |
| `… names 2 entries; it must name exactly one` | component spec with two actions |
| `… contains a resource segment, which is not supported in store configuration yet` | `-R…` in a spec |
| `… parameter 'x' is a link; only literals are allowed` | link parameter |
| `Store configuration names realm 'lui'; only 'store_config' is allowed` | foreign header |
| `Store '<name>' does not offer data-only access, so its metadata cannot be configured` | section on a store without data access |
| `Write strategy 'atomic' needs a backend that writes atomically or renames atomically; this one does neither` | `atomic` on such a backend |
| `under central storage a member's metadata section may name only its storage model …` | member section names handlers |
| `… router-level read_only or metadata section, … build it with build_store` | router section given to `build` |
| `Failed to parse YAML configuration: … unknown field …` (a `parse_error` from `StoreRouterConfig::from_yaml`, before any build) | misspelt field, or the old free-form map |

At run time, a validate refusal is a `General` error such as "Metadata of 'docs/a.md' is refused:
field 'title' is required", and nothing is written.

## 9. Two complete mounts

A read-only Markdown checkout. Nothing is written to it, and titles come from front matter:

```yaml
  - type: filesystem
    prefix: docs
    config: { path: ./docs }
    read_only: true
    metadata:
      storage: none
      on_read: media_type/file_facts/content_hash-on_change/front_matter
```

A writable store with one record file per folder and all-or-nothing writes:

```yaml
  - type: filesystem
    prefix: memory
    config: { path: ./memory }
    metadata:
      storage: grouped-__metadata__.json
      writes: transactional
      on_validate: require-title/media_type
      on_write: defaults/media_type-fill/file_facts/content_hash
```

---

# Part 2: Defining a new entry

Every entry is **a `CommandMetadata` plus a builder, registered in code**. There is no
`register_command!` form: the entry is not a command, and its builder's signature is not a command
function's.

## 10. A handler

A handler's builder is a `StepBuilder`, a plain `fn` pointer:

```rust
pub type StepBuilder = fn(Stage, &ResolvedParameterValues) -> Result<CompiledStep, Error>;
```

It is called **once per chain, at store build**, with the stage the chain is for. It must:
1. **Refuse stages it has no meaning in**, with `handlers::check_stage(name, stage, &[…])`. The
   compiler adds the field and the position to the error.
2. **Read and check its parameters now**, with `realm::string_parameter` /
   `string_list_parameter`, `handlers::Mode::from_parameters` and `check_text_field`. A bad value
   is a build error, never a read error.
3. **Return a `CompiledStep`**, which holds the closure. The closure captures what it needs by
   `move`.

```rust
pub type CompiledFn =   // wasm32: the same, without Send + Sync
    Box<dyn Fn(&mut MetadataRecord, &FactsIn<'_>, &mut StepOut) -> Result<(), Error> + Send + Sync>;
pub type NeedsBytesFn = Box<dyn Fn(&FactsIn<'_>) -> bool + Send + Sync>;
```

- The closure is `Fn`, not `FnMut`: the store calls it from many tasks at once through `&self`.
  It must not need a lock or a registry lookup.
- It shapes the record **in place**. `FactsIn` gives it `key`, `facts` (`DataFacts`), `bytes`
  (when present), `previous` (the record as stored) and `operation` (`get`, `get_metadata`,
  `listdir`, `set`, `set_metadata`).
- It may replace the data object by setting `StepOut::data`. Only a write-stage step should, and
  `front_matter_sync` shows why: it does so in `set_metadata` only.
- In `on_read`, an error becomes a warning and the chain goes on. **Check before you mutate**, so
  that a failing step leaves the record as it found it.
- `CompiledStep::new(run)` never needs the bytes. `CompiledStep::with_needs_bytes(run, predicate)`
  asks for them when `predicate(facts)` holds. The chain's predicate is the `or` of its steps', and
  the pipeline evaluates it before running the chain. Return `false` when `facts.bytes` is already
  `Some`, and compare with `previous` to avoid reading on every listing (see
  `handlers::changed_since_stored`).

The metadata:

```rust
let mut metadata = CommandMetadata::new("title_prefix");
metadata
    .with_namespace("")
    .with_label("Title prefix")
    .with_doc("Prefixes the title. on_default, on_write, on_read.")
    .with_hint("store_config", serde_json::json!("handler"));
metadata.with_argument(ArgumentInfo::string_argument("prefix"));
functions.register_step(cmr, metadata, build_title_prefix)?;
```

`StoreConfigFunctions::register_step` moves the metadata into realm `store_config`, fills an empty
label from the name, adds it to `cmr` and keeps the builder. Use `handlers::mode_argument()` for a
`mode` parameter, an `EnumArgument` for a closed option set (the planner then checks it), and
`.set_multiple()` for a field list. A `multiple` argument takes no default, so say in `doc` what an
empty list means.

## 11. A storage model or a write strategy

A component's builder is a `ComponentBuilder`:

```rust
pub type ComponentBuilder =
    fn(&ResolvedParameterValues, &ComponentContext<'_>) -> Result<StoreComponent, Error>;
```

`ComponentContext` is what the builder can see of the store being built. For a router-level
(central) section it holds the router's members by mount prefix (`mount(&prefix)`,
`has_mounts()`); for a member's own section it is empty. A storage model that keeps its records in
another mount resolves it here and returns that member from `MetadataStorage::object_store`, which
is where the pipeline then writes and reads records. Most builders ignore it.

It returns `StoreComponent::Storage(Arc<dyn MetadataStorage>)` or
`StoreComponent::Writes(Arc<dyn WriteStrategy>)`. Register it with
`functions.register_component(cmr, metadata, ComponentKind::Storage | ComponentKind::Writes,
builder)`. The kind is what lets the compiler refuse a component written in the wrong field.

- **`MetadataStorage`** (`storage.rs`): `name`, `reserved_names`, `get` (returns `Option<Metadata>`,
  so a `LegacyMetadata` record survives) and `plan` (the objects to write for a record, or for its
  removal). It also has defaulted methods:
  - `get_folder`, for one read per folder;
  - `record_names`, so a key with a record and no data is listed;
  - `group_of`, the lock and cache unit;
  - `object_keys`, for recovery;
  - `object_store`, when the records live outside the backend.

  `NativeStorage`, `PerEntryStorage` and `GroupedStorage` are the worked examples.
- **`WriteStrategy`** (`writes.rs`): `name`, `check(&DataCapabilities)` (refuse an unsuitable
  backend at build), `reserved_names` and `commit`. The defaulted `recover` and `sync` matter for
  any strategy that can leave a write half done. `DirectWrite`, `AtomicWrite` and `JournalWrite`
  are the worked examples.

Every name a component's objects occupy must be in its `reserved_names`. The pipeline refuses
client keys that use them, and it relies on that to keep the objects out of listings
(`STORE_SEMANTICS.md` §8).

## 12. Where to register

| Entry lives in | Register from | Reaches the exported registry by |
|---|---|---|
| `liquers-core` | `register_builtin_store_config` (`realm.rs`), which `CommandRegistry::new()` and `StoreConfigFunctions::builtin()` call | automatically |
| `liquers-lib` | `register_lib_store_config` (`liquers-lib/src/store_config.rs`) | the exporter's `store_config` group |
| tests only | a separate function, such as `register_test_store_config`, which `register_builtin_store_config` never calls | never: the export must not depend on a test feature |

Register **before the store is built**. `EnvironmentBuilder::build` clones the registry and the
functions when it builds the store, so a later registration is not seen. Then regenerate
`specs/command_registry.yaml` and add a CHANGELOG line (`CLAUDE.md` §Maintaining
`specs/command_registry.yaml`). `cargo test -p liquers-lib --test registry_export` fails until you
do.

## 13. Tests

- **Compile and run the chain directly.** Get the registry and functions from
  `StoreConfigFunctions::builtin()` plus your registration, then call `functions.compile(spec,
  stage, &cmr)?`. Build a `FactsIn::new(&key)`, set `bytes`, `facts` and `previous` as the case
  needs, and call `chain.run(&mut record, &facts, &mut StepOut::default())`. Assert on
  `chain.needs_bytes(&facts)` for the cases where the bytes must not be read.
- **Refusals:** a stage you do not allow, an empty list, a bad option, a field outside the text
  fields. Each must fail at `compile`.
- **Through the store:** build with `StoreRouterBuilder::from_yaml(…).with_store_config(cmr,
  functions).build_store()` and use the store (`liquers-core/tests/store_adapters.rs`). For a
  storage model or a write strategy, add a conformance suite beside `A1`–`A7` in
  `liquers-core/tests/store_conformance_CONF.rs` (`STORE_IMPLEMENTATION_GUIDE.md` §5).

Worked examples:
- `liquers-lib/src/store_config.rs`: `front_matter` / `front_matter_sync` and their tests. Unit
  tests by direct compile, `needs_bytes`, `set` vs `set_metadata`, and registration making the
  name known.
- `register_front_matter_stand_in` (`liquers-core/src/store/test_support.rs`): a minimal pair of
  handlers registered from outside the built-ins. `front_matter_write_back_through_the_pipeline`
  (`tests/store_adapters.rs`) and conformance suite `A7` use it.
- `registered_entry_is_named_like_a_builtin` (`store/realm.rs`): a test-registered handler and
  storage model.

Language-defined entries (Python, JavaScript) are not possible yet:
[`LANGUAGE-STORE-CONFIG-FUNCTION-NOT-DEFINABLE`](../issues/LANGUAGE-STORE-CONFIG-FUNCTION-NOT-DEFINABLE.md).

## History

| Date | Change | Source |
|---|---|---|
| 2026-10-19 | §5: the `markdown_tiers` handler; `storage: none` recommended for checkouts edited by hand. | `design/agent-memory-mvp/` Phase 5 |
| 2026-10-16 | §11: `ComponentBuilder` takes a `ComponentContext` holding the router's mounts for a router-level section, so a storage model can keep records in another mount; `central` resolves its mount. | `design/store-adapters/` phase-5 review |
| 2026-10-15 | Created by store-adapters: writing `metadata` sections (fields and defaults, spec syntax and escaping, stages, the `store_config` catalogue, router-level sections, validation, error messages) and defining a new handler, storage model or write strategy. | `design/store-adapters/` phase-5 |
