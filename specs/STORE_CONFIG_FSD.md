# Functional Specification Document (FSD): Store Configuration for `liquers-store`

## Overview
This document specifies the requirements and design for configuring a store system in the `liquers-store` crate. The configuration system will allow users to define and instantiate an `AsyncStoreRouter` composed of multiple store backends, including OpenDAL-based stores, a built-in memory store, and a built-in filesystem store.

## Goals
- Enable declarative configuration of store backends and routing.
- Support multiple store types, including OpenDAL backends, memory, and filesystem stores.
- Allow flexible composition and routing of stores via `AsyncStoreRouter`.
- Provide extensibility for future store types and OpenDAL backends.
- Configuration should be serializable as a yaml, toml or json document.
- Configuration should support expansion of environment variables mainly to support secure access keys and passwords.

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
OpenDAL does not provide a built-in way to create operators from text configuration. The implementation must:

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

---
**End of FSD**
