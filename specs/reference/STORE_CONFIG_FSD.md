---
title: Store Configuration Functional Specification
kind: reference
audience: internal
area: [store/config]
reviewed: 2026-08-29
---
# Functional Specification Document (FSD): Store Configuration

## Overview

This document specifies how a store system is configured: a declarative document defines an
`AsyncStoreRouter` composed of several store backends.

**Where each part lives.** The configuration format, the factory seam and the builder are
`liquers-core`'s (`store_config.rs`, `store_factory.rs`); the OpenDAL backends are
`liquers-store`'s; browser backends are `liquers-web`'s. That split is deliberate — a type that
*describes* a store must be usable without a backend in the dependency graph, so core-side
configuration can embed one. See `specs/design/store-factories-in-core/`.

| Concern | Crate | Module |
|---|---|---|
| `StoreRouterConfig`, `StoreConfig`, `${VAR}` expansion | `liquers-core` | `store_config` |
| `StoreFactory`, chaining, `StoreRouterBuilder`, `memory`, `filesystem` | `liquers-core` | `store_factory` |
| OpenDAL backends and their factory | `liquers-store` | `store_factory`, `opendal_store` |
| `localstorage`, `js`, `http`/`https` via `fetch` | `liquers-web` | `store::builder` |

## Goals
- Enable declarative configuration of store backends and routing.
- Support multiple store types, including OpenDAL backends, memory, and filesystem stores.
- Allow flexible composition and routing of stores via `AsyncStoreRouter`.
- Provide extensibility for future store types and OpenDAL backends.
- Configuration should be serializable as a yaml, toml or json document.
- Configuration should support expansion of environment variables mainly to support secure access keys and passwords.

## Why this configuration exists alongside OpenDAL's own

OpenDAL has gained its own configuration surface since this document was written, so it is worth
stating what each layer does and why both are needed.

**What OpenDAL offers**, as of 0.55.0 (2025-11-11):

| Form | Since | Shape |
|---|---|---|
| `Operator::from_map` / `via_map` | early; **removed in 0.55** | scheme + `HashMap<String, String>` |
| `Operator::via_iter(scheme, iter)` | 0.48.0 (2024-07-26) | scheme + `(String, String)` pairs — what this implementation uses |
| `Operator::from_uri(uri)` | 0.55.0, for all services | a URI, e.g. `s3://bucket?region=eu-central-1`, resolved through OpenDAL's own scheme registry |

**What all three have in common: they configure exactly one backend.** None of them expresses a key
prefix, a routing order between several stores, environment-variable substitution, or a store type
OpenDAL does not implement — `memory` and `filesystem` from `liquers-core`, or the browser's
`localstorage`, `js` and `fetch` types.

`StoreRouterConfig` is therefore a **composition** format, not a competitor to a single-backend one.
The two layers meet at exactly one place: a `StoreConfig` entry's `config:` map, whose contents are
handed through to the backend nearly verbatim. Everything else in the document — `type`, `prefix`,
list order, `${VAR}` expansion — has no equivalent below.

A structural parallel worth knowing when reading both: OpenDAL 0.55's `from_uri` resolves a scheme
through an `OperatorRegistry` that maps a scheme name to a factory, registered per service behind a
feature gate. That is the same shape as this system's `StoreFactory` seam, one layer down.

## Key Concepts

### Key
A **Key** is the fundamental addressing unit in the store system. It is a sequence of path segments  used to identify resources. Key is not a filesystem path, but it plays a similar role. Examples:
- `data/images/photo.jpg`
- `cache/results/query1.json`
- `web/static/index.html`

Keys do not have leading or trailing slashes. They are parsed as slash-separated segments.

### Key Prefix
Each store in the router is assigned a **key prefix** - a Key that defines the namespace the store handles. The `AsyncStoreRouter` routes requests to the first store whose key prefix matches the beginning of the requested key.

For example:
- A store with prefix `data` handles keys like `data/images/photo.jpg`, `data/files/doc.txt`
- A store with prefix `cache` handles keys like `cache/results/query1.json`
- A store with empty prefix (`""`) matches all keys (useful as a fallback)

### Routing Logic
The router iterates through stores in order and selects the first store where:
1. The requested key has the store's key prefix as its prefix (segment-wise comparison)
2. The store's `is_supported()` method returns true for the key

## Store Router Configuration

The configuration describes an `AsyncStoreRouter` as a list of store definitions. Each store definition specifies:
- **type**: Store type (e.g., `memory`, `filesystem`, `s3`) Store type imples an implementation, e.g. s3 is implemented as an OpenDAL store 
- **prefix**: Key prefix for routing (string, optional - empty string matches all keys)
- **config**: (not required for some store types, e.g. memory) Store-specific configuration parameters
- **metadata**: (optional) Reserved. In he future this will decide how metadata are stored. For now, a fixed convention is used.

### Example Configuration (YAML)
```yaml
stores:
  - type: opendal_fs
    prefix: data
    config:
      root: /var/liquers/data

  - type: s3
    prefix: remote
    config:
      bucket: my-liquers-bucket
      region: us-east-1
      access_key_id: ${AWS_ACCESS_KEY_ID}
      secret_access_key: ${AWS_SECRET_ACCESS_KEY}

  - type: ftp
    prefix: ftp
    config:
      endpoint: ftp.example.com:21
      user: ${FTP_USER}
      password: ${FTP_PASSWORD}

  - type: memory
    prefix: temp

  - type: filesystem
    prefix: local
    config:
      path: ./localdata

  # Fallback store - empty prefix matches anything not matched above
  - type: memory
    prefix: ""
```

## Supported Store Types

### 1. OpenDAL Store
- **Type:** `opendal`
- **Parameters:**
  - `prefix`: Key prefix for routing (string, optional)
  - `type`: identifies a backend scheme (string, required) - e.g., `fs`, `s3`, `ftp`, `memory`, `gcs`, `azblob`
  - `config`: Backend-specific configuration (object, required)

OpenDAL does not natively support text-based configuration. The implementation must map the YAML/JSON configuration to OpenDAL's builder API or use `Operator::via_iter()` with the scheme and config key-value pairs.

#### OpenDAL Filesystem Backend (`fs`)
Configuration options:
- `root`: Absolute path to the root directory (string, required)
- `atomic_write_dir`: Temporary directory for atomic writes (string, optional)

```yaml
- type: fs
  prefix: data
  config:
    root: /var/liquers/data
```

#### OpenDAL FTP Backend (`ftp`)
Configuration options:
- `endpoint`: FTP server endpoint, e.g., `ftp.example.com:21` (string, required)
- `user`: FTP username (string, optional)
- `password`: FTP password (string, optional)

```yaml
- type: ftp
  prefix: ftp
  config:
    endpoint: ftp.example.com:21
    user: myuser
    password: mypassword
```

#### OpenDAL S3 Backend (`s3`)
Configuration options:
- `bucket`: S3 bucket name (string, required)
- `region`: AWS region (string, optional)
- `endpoint`: Custom S3 endpoint URL for S3-compatible services (string, optional)
- `access_key_id`: AWS access key (string, optional - can use environment/IAM)
- `secret_access_key`: AWS secret key (string, optional)
- `session_token`: Temporary session token (string, optional)
- `role_arn`: IAM role ARN for role assumption (string, optional)
- `enable_virtual_host_style`: Use virtual-hosted style URLs (boolean, optional)
- `default_storage_class`: Storage class, e.g., `STANDARD`, `GLACIER` (string, optional)
- `server_side_encryption`: Encryption algorithm, e.g., `AES256`, `aws:kms` (string, optional)
- `server_side_encryption_aws_kms_key_id`: KMS key ID (string, optional)

```yaml
- type: opendal
  prefix: s3data
  backend: s3
  config:
    bucket: my-bucket
    region: us-east-1
    access_key_id: ${AWS_ACCESS_KEY_ID}
    secret_access_key: ${AWS_SECRET_ACCESS_KEY}
```

#### OpenDAL Memory Backend (`memory`)
No configuration options required. Useful for testing.

NOTE: OpenDAL memory store should not be used, since it has limitations, mainly does not properly support directories.

#### OpenDAL Google Cloud Storage Backend (`gcs`)
Configuration options:
- `bucket`: GCS bucket name (string, required)
- `root`: Working directory for operations (string, optional)
- `endpoint`: Custom endpoint URL (string, optional)
- `credential`: Base64-encoded Service Account JSON (string, optional)
- `credential_path`: Path to Service Account JSON file (string, optional)
- `service_account`: Service Account name for VM metadata (string, optional)
- `scope`: GCS service scope (string, optional, default: `https://www.googleapis.com/auth/devstorage.read_write`)
- `predefined_acl`: ACL setting - `authenticatedRead`, `bucketOwnerFullControl`, `bucketOwnerRead`, `private`, `projectPrivate`, `publicRead` (string, optional)
- `default_storage_class`: Storage class - `STANDARD`, `NEARLINE`, `COLDLINE`, `ARCHIVE` (string, optional)
- `disable_vm_metadata`: Disable GCE metadata credential loading (boolean, optional)
- `disable_config_load`: Disable environment config loading (boolean, optional)
- `allow_anonymous`: Enable anonymous requests for public buckets (boolean, optional)

```yaml
- type: gcs
  prefix: gcloud
  config:
    bucket: my-gcs-bucket
    credential_path: /path/to/service-account.json
```

#### OpenDAL Azure Blob Storage Backend (`azblob`)
Configuration options:
- `container`: Azure container name (string, required)
- `endpoint`: Azure Blob endpoint URL (string, required)
- `root`: Working directory (string, optional)
- `account_name`: Azure storage account name (string, optional - can use environment)
- `account_key`: Azure storage account key (string, optional - can use environment)
- `sas_token`: Shared Access Signature token (string, optional)
- `encryption_key`: Base64-encoded encryption key for server-side encryption (string, optional)
- `encryption_key_sha256`: Base64-encoded SHA256 of encryption key (string, optional)
- `encryption_algorithm`: Encryption algorithm, e.g., `AES256` (string, optional)
- `batch_max_operations`: Maximum batch operations (integer, optional)

```yaml
- type: azblob
  prefix: azure
  config:
    container: my-container
    endpoint: https://myaccount.blob.core.windows.net
    account_name: ${AZURE_ACCOUNT_NAME}
    account_key: ${AZURE_ACCOUNT_KEY}
```

#### OpenDAL SFTP Backend (`sftp`)
Configuration options:
- `endpoint`: SSH endpoint in OpenSSH format: `[user@]hostname` or `ssh://[user@]hostname[:port]` (string, required)
- `root`: Working directory (string, optional)
- `user`: SSH username (string, optional - can be in endpoint)
- `key`: Path to private key file (string, required for auth - password auth not supported)
- `known_hosts_strategy`: Host verification strategy - `Strict` (default), `Accept`, `Add` (string, optional)
- `enable_copy`: Enable remote copy extension (boolean, optional)

NOTE: SFTP backend only works on Unix systems.

```yaml
- type: sftp
  prefix: secure
  config:
    endpoint: user@sftp.example.com:22
    key: ~/.ssh/id_rsa
    known_hosts_strategy: Strict
```

#### OpenDAL WebDAV Backend (`webdav`)
Configuration options:
- `endpoint`: WebDAV server URL (string, required)
- `root`: Root path on server (string, optional)
- `username`: Authentication username (string, optional)
- `password`: Authentication password (string, optional)
- `token`: Bearer token for authentication (string, optional)
- `enable_user_metadata`: Enable metadata via PROPPATCH (boolean, optional, default: false)
- `user_metadata_prefix`: XML namespace prefix for metadata (string, optional, default: `opendal`)
- `user_metadata_uri`: XML namespace URI for metadata (string, optional)

```yaml
- type: webdav
  prefix: dav
  config:
    endpoint: https://webdav.example.com
    username: ${WEBDAV_USER}
    password: ${WEBDAV_PASSWORD}
```

#### OpenDAL GitHub Backend (`github`)
Access GitHub repositories via the GitHub Contents API.

Configuration options:
- `owner`: GitHub repository owner (string, required)
- `repo`: GitHub repository name (string, required)
- `token`: GitHub personal access token (string, optional - required for private repos, optional for public)
- `root`: Root path within repository (string, optional)

NOTE: Supports read, write, delete, list operations. Does not support directories creation or copy/rename.

```yaml
- type: github
  prefix: gh
  config:
    owner: myorg
    repo: myrepo
    token: ${GITHUB_TOKEN}
    root: data
```

#### OpenDAL HDFS Backend (`hdfs`)
Hadoop Distributed File System support. Requires Java and Hadoop installation.

Configuration options:
- `name_node`: HDFS namenode address, e.g., `default` or `hdfs://127.0.0.1:9000` (string, required)
- `root`: Working directory - must be absolute path (string, optional)
- `user`: HDFS user (string, optional)
- `kerberos_ticket_cache_path`: Kerberos ticket cache path from `klist` after `kinit` (string, optional)
- `enable_append`: Enable append operations (boolean, optional, default: false)
- `atomic_write_dir`: Temp directory for atomic writes (string, optional)

NOTE: Requires `JAVA_HOME` and `HADOOP_HOME` environment variables. May need `LD_LIBRARY_PATH` for Java libs.

```yaml
- type: hdfs
  prefix: hadoop
  config:
    name_node: hdfs://namenode.example.com:9000
    root: /user/liquers
    user: hdfs_user
```

#### OpenDAL WebHDFS Backend (`webhdfs`)
HDFS access via REST API. No Java/Hadoop installation required.

Configuration options:
- `endpoint`: WebHDFS namenode endpoint (string, optional, default: `http://127.0.0.1:9870`)
- `root`: Working directory (string, optional)
- `delegation_token`: Authentication token (string, optional)
- `atomic_write_dir`: Temp directory for multi-write operations (string, optional)

```yaml
- type: webhdfs
  prefix: hdfs
  config:
    endpoint: http://namenode.example.com:9870
    root: /user/liquers
    delegation_token: ${HDFS_TOKEN}
```

#### Other OpenDAL Backends
OpenDAL supports 80+ backends including: `redis`, `mongodb`, `postgresql`, `mysql`, `sqlite`, `dropbox`, `onedrive`, `gdrive` (Google Drive), `ipfs`, and many more. Refer to [OpenDAL services documentation](https://opendal.apache.org/docs/rust/opendal/services/index.html) for backend-specific configuration options.

### 2. Memory Store (Built-in)
- **Type:** `memory`
- **Parameters:**
  - `prefix`: Key prefix for routing (string, optional)

A simple in-memory store. Data is lost when the process exits.

```yaml
- type: memory
  prefix: cache
```

Currently memory store can be implemented via AsyncStoreWrapper.
A proper AsyncMemoryStore should be implemented.

### 3. Filesystem Store (Built-in)
- **Type:** `filesystem`
- **Parameters:**
  - `prefix`: Key prefix for routing (string, optional)
  - `path`: Path to the root directory (string, required)

Uses the built-in `FileStore` implementation from `liquers-core`.
A proper AsyncFileStore should be implemented.

```yaml
- type: filesystem
  prefix: local
  path: ./data
```

## Implementation Notes


### OpenDAL Operator Creation

*(Corrected 2026-08-29: this section previously stated that OpenDAL provides no way to create an
operator from text configuration. That was true when this document was written and is no longer —
see "Why this configuration exists alongside OpenDAL's own" above.)*

To build an operator from a `StoreConfig` entry, the implementation must:

1. Parse the `type` (backend) field to determine the backend and eventually the OpenDAL scheme
2. Convert the `config` object to key-value pairs
3. Use `Operator::via_iter(scheme, config_pairs)` for dynamic dispatch, or
4. Use backend-specific builders with `Operator::new(builder)` for static dispatch

Example implementation pattern:
```rust
fn create_opendal_operator(store_type: &str, config: &HashMap<String, String>) -> Result<Operator> {
    let config_pairs: Vec<(String, String)> = config.iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    Operator::via_iter(store_type, config_pairs)
}
```

### Configuration values and the OpenDAL string boundary

**Every OpenDAL configuration parameter is a string at the boundary.** `Operator::via_iter` takes
`(String, String)` pairs, and OpenDAL parses each value back into the field's real type using its own
text conventions (`opendal/src/raw/serde_util.rs`, `ConfigDeserializer`):

| Field type | Text OpenDAL accepts |
|---|---|
| `bool` | `true` / `on` / `false` / `off`, case-insensitive |
| integers | decimal digits, via `parse::<T>()` — no decimal point |
| sequences (`Vec<String>`) | **comma-separated**, elements trimmed; empty string means empty list |

A `config:` value in a Liquers document, however, is a `serde_json::Value`, and
`StoreConfig::config_as_string_map` flattens it before handing it over. The two encodings must agree,
and they do not agree everywhere:

| Document value | Flattened to | OpenDAL reads it as | Agrees? |
|---|---|---|---|
| `"eu-central-1"` (string) | `eu-central-1` | the string | **yes** — passed through verbatim |
| `true` (boolean) | `true` | `true` | **yes** |
| `1000` (integer) | `1000` | `1000` | **yes** |
| `1000.0` (float) | `1000.0` | rejected by an integer field | **no** |
| `[a, b]` (list) | `["a","b"]` — **JSON text** | splits on commas, keeping brackets and quotes | **no** |
| `null` | `null` | the four-character string `null` | **no** |

**The rule that follows: write OpenDAL parameters as scalars, and write a list-valued OpenDAL option
as a comma-separated string.** Quoting every value is always safe, because a JSON string is passed
through unchanged and OpenDAL's conventions then apply directly:

```yaml
config:
  endpoints: "127.0.0.1:2379,127.0.0.1:2380"   # correct
  # endpoints: [ "127.0.0.1:2379", "127.0.0.1:2380" ]   # WRONG — see below
  enable_virtual_host_style: true              # fine unquoted: booleans round-trip
  batch_max_operations: 1000                   # fine unquoted: whole numbers round-trip
```

Booleans and whole numbers need no quoting — their flattened text is exactly what OpenDAL expects —
so requiring quotes everywhere would cost ergonomics without buying correctness.

The last three rows of the table are a **known defect**, not a designed behaviour:
[`STORE-OPENDAL-LIST-OPTION-MISPARSED`](../issues/STORE-OPENDAL-LIST-OPTION-MISPARSED.md). Its reach
today is narrow — `endpoints` on the `tikv` service is the only non-scalar field across all of
OpenDAL 0.55's service configs, and `tikv` is reachable only through the `opendal_tikv` escape hatch
— but the flattening rule is general, so a future OpenDAL release that adds a list field to a common
service inherits it silently.

This applies **only to OpenDAL-backed types.** The built-in and browser store types read their
`config:` values as `serde_json::Value` directly and never pass through this flattening, which is
why the browser `http` store's `keys` is written as a genuine YAML list.

### Configuration Loading
- The configuration should be loadable from YAML, TOML or JSON.
- Provide Rust structs for deserialization (using serde).
- Support environment variable substitution for secrets using `${VAR_NAME}` syntax.

### Store Instantiation
The system should:
1. Parse and validate the configuration
2. Instantiate the appropriate store objects based on type
3. Compose them into an `AsyncStoreRouter` with stores added in configuration order

NOTE: `AsyncStoreRouter`is implemented in liquers_core::store.

## Routing and Prefixes
- Each store is assigned a key prefix.
- The `AsyncStoreRouter` routes requests to the first store whose prefix matches the key.
- Prefixes should generally be non-overlapping for unambiguous routing.
- An empty prefix (`""`) matches all keys - useful for fallback stores (should be last).
- Stores are evaluated in the order they appear in the configuration.

## Validation and Error Handling
- The configuration loader must validate required fields for each store type and backend.
- Invalid or missing parameters should result in clear error messages.
- Backend-specific validation should check required fields (e.g., `bucket` for S3, `root` for filesystem).
- Configuration errors should use `liquers_core::error::Error` as the result type for consistency with the rest of the codebase. Use appropriate error types such as `ErrorType::General` for configuration errors or `ErrorType::ParseError` for parsing failures.

## Security Considerations
- Sensitive information (e.g., S3 credentials, FTP passwords) should be handled securely and not logged.
- Support environment variable substitution (`${VAR_NAME}`) for secrets.
- Consider supporting external secret providers in future versions.

## UI
Though UI is put of scope, the UI should be optionally supported (as a feature) via egui_struct.

## Out of Scope
- UI for editing configuration
- Dynamic reloading of configuration at runtime
- Secret management integration (AWS Secrets Manager, HashiCorp Vault, etc.)

## References

### OpenDAL Documentation
- [OpenDAL Services Documentation](https://opendal.apache.org/docs/rust/opendal/services/index.html)
- [OpenDAL Operator](https://opendal.apache.org/docs/rust/opendal/struct.Operator.html)
- [OpenDAL S3 Backend](https://opendal.apache.org/docs/rust/opendal/services/struct.S3.html)
- [OpenDAL Fs Backend](https://opendal.apache.org/docs/rust/opendal/services/struct.Fs.html)
- [OpenDAL FTP Backend](https://nightlies.apache.org/opendal/opendal-docs-stable/docs/rust/opendal/services/struct.FtpConfig.html)
- [OpenDAL GCS Backend](https://opendal.apache.org/docs/rust/opendal/services/struct.Gcs.html)
- [OpenDAL Azure Blob Backend](https://opendal.apache.org/docs/rust/opendal/services/struct.Azblob.html)
- [OpenDAL SFTP Backend](https://opendal.apache.org/docs/rust/opendal/services/struct.Sftp.html)
- [OpenDAL WebDAV Backend](https://opendal.apache.org/docs/rust/opendal/services/struct.Webdav.html)
- [OpenDAL HDFS Backend](https://opendal.apache.org/docs/rust/opendal/services/struct.Hdfs.html)
- [OpenDAL WebHDFS Backend](https://opendal.apache.org/docs/rust/opendal/services/struct.Webhdfs.html)
- [OpenDAL GitHub Repository](https://github.com/apache/opendal)

### UI Support
- [egui_struct crate](https://crates.io/crates/egui_struct) - Derive macro for generating egui UIs from structs

### liquers-core
- `liquers_core::store` - Store traits (`AsyncStore`, `Store`) and `AsyncStoreRouter` implementation
- `liquers_core::error::Error` - Error type to be used for configuration errors
- `liquers_core::query::Key` - Key type used for store addressing

### Related
- [serde](https://serde.rs/) - Serialization framework for Rust

## Building stores: the factory model

A [`StoreFactory`] declares the store types it can build, resolves a configuration entry to one of
them, and builds it. Three rules govern the whole model.

### 1. A factory declares what it can build

`store_types()` returns a `StoreTypeInfo` per type: its name, documentation, configuration
arguments, whether this build can construct it, and whether the argument list is exhaustive.

That list is not decoration. It is what the error for an unrecognised type prints, so the message
is accurate for the build in hand rather than describing a type set that may not be compiled in.

### 2. The store type is resolved, not merely matched

`resolve(&StoreConfig) -> Option<String>` answers "which of my types is this entry?". The default
is an exact match on `type`, and every factory in the tree uses it today. A factory **may** override
it to *infer* the type from the entry — the store type is the resolved identity of an entry, and
what identifies it (a `type` field, or something else) is input to that.

Two rules keep inference from becoming magic in a routing decision:

- a factory may only resolve to a store type it declares; and
- inference should key on something whose purpose is identification, never on the incidental
  presence of an argument — otherwise adding that argument elsewhere silently reroutes a document.

`create` is then called with `store_type` set to whatever `resolve` returned, so an implementation
never has to re-derive it.

### 3. Factories chain, and the first to resolve wins

`ChainedStoreFactory` consults its members in order. A chain is assembled **bottom-up** —
`liquers-core`, then `liquers-store`, then a library, then the integration — so a core store type
means the same thing everywhere by default.

**Overriding is available to anyone who needs it**: compose a chain with your factory first.
First-wins fixes the *default* ordering, not the only possible one.

There is **no built-in fallback**. `StoreRouterBuilder` has no store types of its own; everything it
builds comes from the factory it was given, which is why the factory is a required constructor
argument. Each crate offers a `default_store_factory()` as the convenience:

| Crate | Its own factory claims | `default_store_factory()` |
|---|---|---|
| `liquers-core` | `memory`, `filesystem` | core only |
| `liquers-store` | the OpenDAL types | core, then OpenDAL |
| `liquers-web` | `localstorage`, `js`, `http`, `https` | core, then browser — **not** OpenDAL, which is not in its graph |

```rust
use liquers_core::store_factory::StoreRouterBuilder;
use liquers_store::store_factory::default_store_factory;

let router = StoreRouterBuilder::from_yaml(yaml, Box::new(default_store_factory()))?.build()?;
```

### Unrecognised and unavailable types

An entry no factory resolves is an `ErrorType::NotSupported` naming the type and listing what the
chain supports:

```
Unknown store type 'postgress'. Supported store types: filesystem, memory.
Known but unavailable in this build: fs, s3, ftp (requires the 'opendal' feature of liquers-store).
```

The second sentence is the point. A type that is real and documented but compiled out — a Cargo
feature is off, or the target does not support it — is reported as **unavailable with the reason**,
never as unknown. Reporting it as unknown sends the reader hunting for a typo in something that
exists. `filesystem` on wasm32 and every OpenDAL type without the `opendal` feature are the live
cases.

### How complete is an argument list?

`ArgumentCoverage` distinguishes two situations, and the distinction is load-bearing:

- **`Complete`** — Liquers owns the store type, so the argument list *is* the specification.
  `memory`, `filesystem` and the browser types.
- **`Partial { authority }`** — the type's arguments are defined by another project, so the list is
  guidance, unlisted keys are passed to the backend, and `authority` says where the real
  documentation lives. Every OpenDAL type.

An externally-owned surface can only ever be described incompletely, because it changes on someone
else's release schedule. `Partial` makes that a stated fact rather than an omission: an upstream
release adding a field makes the description *less complete*, never *wrong*, and nobody has to
notice for it to stay honest.

Argument types are JSON's — `string`, `number`, `boolean`, `array`, `object`, `any` — because a
configuration document is JSON or YAML. Scalars are strongly preferred; see the string-boundary
rules above for why a list-valued OpenDAL option is written as a comma-separated string.

## Optional backends and extension

### The `opendal` feature

OpenDAL is an **optional** dependency of `liquers-store`, enabled by default. Building with
`--no-default-features --features async_store` gives the crate without it.

The original reason for the option no longer applies and is worth correcting rather than leaving:
it let a `wasm32` consumer take this crate for its configuration types and builder without OpenDAL.
Those live in `liquers-core` now, so `liquers-web` does not depend on `liquers-store` at all. The
feature earns its place for a different reason — non-OpenDAL backends are expected here too, so an
OpenDAL-free configuration of the crate is still meaningful.

With the feature off, the OpenDAL types are **still declared**, each marked unavailable and naming
the feature responsible. A configuration naming one is refused with that reason rather than as an
unknown type: a real, documented store type must not look like a typo. `filesystem` behaves the
same way on `wasm32`, where `AsyncFileStore` cannot exist because it uses `tokio::fs`.

### The `StoreFactory` trait

```rust
pub trait StoreFactory {
    fn store_types(&self) -> Vec<StoreTypeInfo>;
    fn resolve(&self, config: &StoreConfig) -> Option<String>;   // default: exact type match
    fn create(&self, config: &StoreConfig) -> Result<Box<dyn AsyncStore>, Error>;
}
```

Contributing store types means implementing it and chaining your factory:

```rust
ChainedStoreFactory::new()
    .chain(Box::new(core_store_factory()))
    .chain(Box::new(my_factory))
```

`StoreTypeMap` is the alternative for a set of types that do not need a bespoke `resolve`: build it
from `StoreTypeInfo` values and creation closures rather than implementing the trait.

The trait deliberately carries no `Send`/`Sync` bound. A factory is transient — consumed while the
router is built — and only the `AsyncStore` it produces has thread requirements, which `AsyncStore`
already states. A bound no call site needs would exclude a browser factory holding JavaScript
handles.

**A note on how the browser's `http` wins.** Earlier versions of this document said factories were
consulted before the built-in types, and that preceding them was what let a browser override `http`.
That is no longer how it works, and the difference matters to anyone composing a chain: there are no
built-ins, order in the chain decides, and `liquers-web`'s `http` wins because `liquers-store` is
not one of its dependencies — the OpenDAL factory that would claim `http` is never in the browser's
chain. Adding it would put both in one chain, and the earlier one would win.

### Browser store types

`liquers-web` contributes three types through that seam. They appear in the same document as any
other store, and the routing rules above are unchanged.

| `type` | Backend | Writes | Configuration |
|---|---|---|---|
| `localstorage` | `localStorage` | yes | `namespace` (no `/`, default `liquers`), `quota_bytes` (omit for unlimited) |
| `http` / `https` | `fetch` | no | `url_prefix`, `keys` |
| `js` | a page object | depends on the object | `object` — a name registered with `registerStoreObject` |

Each is declared with its arguments in `WebStoreFactory::store_types`, at
`ArgumentCoverage::Complete`: Liquers owns these types, so that list is their specification.

```yaml
stores:
  - type: localstorage
    prefix: local
    config: { namespace: myapp, quota_bytes: 4000000 }
  - type: http
    prefix: data
    config:
      url_prefix: https://example.org/reference/
      keys: [ input.csv, sub/report.json ]
```

Two differences from a native deployment are worth knowing:

- **`${VAR}` is not expanded in a browser**, because there is no environment. The builder leaves
  the text verbatim and warns; the syntax is reserved for page-supplied variables later.
- **`http` has no directory listing.** HTTP does not provide one, so the store is told its `keys`
  and derives `contains`, `is_dir` and `listdir` from that set — which is what keeps them
  consistent with what a `get` will actually fetch.

Full design: `specs/design/liquers-web-store/`.

---
**End of FSD**

## History

| Date | Change | Source |
|---|---|---|
| 2026-03-02 | Present at repository import; content unchanged since. Not reviewed against the implementation. | migration |
| 2026-08-09 | Documented the optional `opendal` feature, the `StoreFactory` extension seam, and the three browser store types (`localstorage`, `http`/`https` via `fetch`, `js`). | `design/liquers-web-store/` |
| 2026-08-29 | Reviewed against the implementation at HEAD. Added "Why this configuration exists alongside OpenDAL's own", correcting the claim that OpenDAL offers no text configuration — `via_iter` (0.48) and `from_uri` (0.55) do, but configure one backend each. Added "Configuration values and the OpenDAL string boundary", recording which document value types survive the flattening into OpenDAL's string map and which do not. No change to the configuration format itself. | `design/store-factories-in-core/` Phase 3 |
| 2026-08-29 | Rescoped: the configuration format, the factory seam and the builder are `liquers-core`'s; `liquers-store` keeps the OpenDAL backends. Added "Building stores: the factory model" — declaration, resolution, first-wins chaining, the absence of a built-in fallback, the unrecognised/unavailable distinction, and `ArgumentCoverage`. Rewrote the `StoreFactory` section for the new trait and **corrected the claim that factories precede built-in types**: there are no built-ins, chain order decides, and the browser's `http` wins because `liquers-store` is not one of its dependencies. Corrected the `opendal` feature's stated rationale. | `design/store-factories-in-core/` Phase 5 |
