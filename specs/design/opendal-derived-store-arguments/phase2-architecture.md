# Phase 2: Solution and Architecture - Derived OpenDAL Store Arguments

## Chosen Solution

All in `liquers-store/src/store_factory.rs`.

### 1. Derive one config's arguments

```rust
/// Every field of an OpenDAL service config, with its default, as `StoreArgumentInfo::derived`.
/// Sorted by name (serde_json's map order). Empty if the default cannot be serialized, which no
/// OpenDAL config does; the caller then falls back to the hand-written list.
#[cfg(feature = "opendal")]
fn derived_arguments<C: opendal::Configurator + Default>() -> Vec<StoreArgumentInfo> {
    match serde_json::to_value(C::default()) {
        Ok(serde_json::Value::Object(fields)) => fields
            .into_iter()
            .map(|(name, default)| StoreArgumentInfo::derived(&name, default))
            .collect(),
        Ok(_) | Err(_) => Vec::new(),
    }
}
```

`Option<_>` fields default to `null`, so they are typed `Any` with no default; `bool` fields are
`Boolean` with their default. That is the honest extent of what a default can say.

### 2. Map store types to configs, gated per service

```rust
/// The derived arguments of `store_type`'s OpenDAL config, or `None` when its service is not
/// compiled into this build. The only hand-maintained part: one arm per advertised type.
fn service_arguments(store_type: &str) -> Option<Vec<StoreArgumentInfo>> {
    match store_type {
        #[cfg(feature = "services-fs")]
        "fs" => Some(derived_arguments::<opendal::services::FsConfig>()),
        #[cfg(feature = "services-s3")]
        "s3" => Some(derived_arguments::<opendal::services::S3Config>()),
        #[cfg(feature = "services-http")]
        "http" | "https" => Some(derived_arguments::<opendal::services::HttpConfig>()),
        #[cfg(all(feature = "opendal", any(unix, feature = "services-sftp")))]
        "sftp" => Some(derived_arguments::<opendal::services::SftpConfig>()),
        // … one arm per OPENDAL_STORE_TYPES entry: gcs, azblob, ftp, webdav, github, webhdfs,
        // dropbox, onedrive, gdrive, ipfs, hdfs, redis, mongodb, postgresql, mysql, sqlite —
        // each `#[cfg(feature = "services-<name>")]`, config `<Name>Config`.
        _ => None,
    }
}
```

`sftp` is gated on `any(unix, feature = "services-sftp")` because on Unix the `opendal`
dependency enables `services-sftp` itself (`liquers-store/Cargo.toml`, target row), without
`liquers-store`'s feature. The `_ => None` arm is correct here, not a forbidden default arm: the
match is over strings, and a type whose service is compiled out must fall through.

### 3. Merge hand-written documentation

`common_arguments(store_type)` keeps its hand-written list, renamed `hand_written_arguments`.
`type_info` calls a new `arguments(store_type)`:

```rust
fn arguments(store_type: &str) -> Vec<StoreArgumentInfo> {
    let hand = Self::hand_written_arguments(store_type);
    let Some(derived) = service_arguments(store_type) else {
        return hand; // service not compiled in: criterion 3
    };
    let mut result = Vec::new();
    for h in &hand {                      // hand-written names first, in their order
        if let Some(d) = derived.iter().find(|d| d.name == h.name) {
            result.push(merge(d, h));     // criterion 1
        }                                 // else dropped: criterion 2
    }
    for d in derived {                    // then the rest, alphabetically
        if !hand.iter().any(|h| h.name == d.name) {
            result.push(d);
        }
    }
    result
}
```

`merge(derived, hand)`: name and default from `derived`; `doc`, `label` and `required` from `hand`;
type from `hand` when `derived` says `Any`, otherwise from `derived`. `type_info` keeps
`.partial(OPENDAL_DOCS)` (criterion 4). The doc comment of `hand_written_arguments` loses its
"for now" section and states the merge rule.

### 4. Offline S3 tests (`mod tests`, gated `#[cfg(feature = "services-s3")]`)

From `design/store-factories-in-core/phase3-examples.md` ≈492-527, with one correction found on
2026-10-05: OpenDAL's S3 builder reads `AWS_REGION` and the AWS profile unless
`disable_config_load` is set (`services/s3/backend.rs` ≈777), so `s3_02` as specified there passes
or fails depending on the machine. It must set `disable_config_load: true`.

## Rejected Alternatives

- **One derived list for all of `OPENDAL_STORE_TYPES` without gates** — does not compile when a
  service is compiled out; its config type does not exist.
- **`ArgumentCoverage::Complete` once derived** — a default cannot say what is required.
- **Assert an exhaustive field list** — fails on every OpenDAL release that adds a field, which is
  the maintenance burden derivation removes.

## Errors, Ownership, Sync/Async

Synchronous, startup-time, no I/O. No new error; the serialization failure path degrades to the
hand-written list. No `unwrap`.

## Risk Table

| Aspect | Assessment |
|---|---|
| Likely files | `liquers-store/src/store_factory.rs` |
| Feature matrix | every arm is gated; `check-build-matrix.sh` covers `--no-default-features`, single services and wasm32 (where `liquers-store` is not built) |
| Existing tests | `availability01/02`, `coverage02` unaffected; a test asserting the old hand-written list (if any) must change to presence assertions |
| Recovery | `type_info` calls the hand-written list again |
| Certainty | high |
