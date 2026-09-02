//! `C1`–`C5` — the conformance suite applied to `liquers-core`'s stores and to the trait defaults.
//!
//! This file is the **divergence census** the issue
//! `STORE-ASYNC-STORE-NO-BEHAVIOURAL-CONFORMANCE-SUITE` asked for: the first time these
//! implementations have been asked the same questions. Each suite prints its full report before
//! asserting, so a CI failure shows what disagreed and on which key rather than a bare rule ID.
//!
//! Every allowed failure names an open issue. `H5` fails the assertion when an allowed rule starts
//! passing, so a fixed issue forces its entry out rather than relying on anyone remembering.
//!
//! See `specs/design/store-conformance-suite/` and `specs/reference/STORE_SEMANTICS.md`.

#![cfg(feature = "store-conformance")]

use liquers_core::error::Error;
use liquers_core::metadata::{Metadata, MetadataRecord};
use liquers_core::parse::parse_key;
use liquers_core::query::Key;
use liquers_core::store::{AsyncFileStore, AsyncMemoryStore, AsyncStore, AsyncStoreRouter, NoAsyncStore};
use liquers_core::store_conformance::{
    run_all, AllowedFailure, GenericFixture, SafetyLevel, StoreCapabilities,
};

/// A unique temporary directory, as `store_key_absolute.rs` does it — nanosecond-stamped, because
/// `cargo test` runs these in parallel.
fn unique_temp_dir(label: &str) -> std::path::PathBuf {
    let unique = format!(
        "liquers_conf_{}_{}",
        label,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    );
    std::env::temp_dir().join(unique)
}

/// Capabilities of a store that keeps its own index and derives directories from its keys.
fn index_backed() -> StoreCapabilities {
    StoreCapabilities {
        write: true,
        remove: true,
        directories: true,
        derived_directories: true,
        explicit_directories: true,
        remove_directories: true,
        stored_metadata: true,
        enumerate_keys: true,
    }
}

/// Run a suite, print the report, then assert against the allowed failures.
///
/// The report is printed **before** the assertion and to stderr, so a failing CI job shows the
/// whole picture — skip reasons, residue and all — rather than one error line. Stdout belongs to a
/// binary's own output.
fn check(report: liquers_core::store_conformance::ConformanceReport, allowed: &[AllowedFailure]) {
    eprintln!("{report}");
    if let Err(e) = report.assert_conformant(allowed) {
        panic!("{}", e.message);
    }
}

/// `C1` — `AsyncMemoryStore`.
#[tokio::test]
async fn c1_async_memory_store() {
    let prefix = parse_key("mem").expect("prefix");
    let fixture = GenericFixture::new(
        "AsyncMemoryStore(prefix=mem)",
        Box::new(AsyncMemoryStore::new(&prefix)),
        prefix,
        index_backed(),
        SafetyLevel::Scratch,
    )
    // Its `is_supported` consults the prefix, so a key elsewhere is genuinely refused.
    .with_outside_prefix(parse_key("elsewhere/x.txt").expect("key"));

    check(
        run_all(&fixture).await,
        &[AllowedFailure {
            rule: "keys02",
            issue: "CORE-STORE-KEYS-MEANS-TWO-DIFFERENT-THINGS",
        }],
    );
}

/// `C2` — `AsyncFileStore` over a temporary directory.
///
/// **`derived_directories` is false**, and that is the point of the capability: a real filesystem
/// directory is an object in its own right and survives its last file, so `explicit02` must not be
/// asked of it.
#[tokio::test]
async fn c2_async_file_store() {
    let root = unique_temp_dir("c2");
    tokio::fs::create_dir_all(&root).await.expect("temp root");

    let mut capabilities = index_backed();
    capabilities.derived_directories = false;

    let fixture = GenericFixture::new(
        "AsyncFileStore(temp dir)",
        Box::new(AsyncFileStore::new(
            root.to_string_lossy().as_ref(),
            &Key::new(),
        )),
        Key::new(),
        capabilities,
        SafetyLevel::Scratch,
    )
    // The sidecar suffix makes this key's data path collide with `collide`'s metadata path.
    .with_metadata_collision(parse_key("collide.__metadata__").expect("key"));

    check(run_all(&fixture).await, &[]);

    let _ = tokio::fs::remove_dir_all(&root).await;
}

/// `C3` — `AsyncStoreRouter` over a memory store and a file store.
///
/// A router is not a store, but it implements the trait and a deployment addresses it as one — the
/// issue's Impact section is entirely about it answering the same question two ways depending on
/// which member a key lands in. The fixture keeps every request inside **one** member's prefix, so
/// a failure here is the router's own dispatch rather than a disagreement between its members.
#[tokio::test]
async fn c3_async_store_router() {
    let root = unique_temp_dir("c3");
    tokio::fs::create_dir_all(&root).await.expect("temp root");

    let mem_prefix = parse_key("mem").expect("prefix");
    let file_prefix = parse_key("files").expect("prefix");

    let mut router = AsyncStoreRouter::new();
    router.add_store(Box::new(AsyncMemoryStore::new(&mem_prefix)));
    router.add_store(Box::new(AsyncFileStore::new(
        root.to_string_lossy().as_ref(),
        &file_prefix,
    )));

    // The file store's prefix directory has to exist: `keys()` walks every member, and a member
    // whose prefix path is absent makes the whole enumeration fail rather than contribute nothing.
    // Filed as CORE-STORE-ROUTER-KEYS-FAILS-ON-AN-EMPTY-MEMBER.
    tokio::fs::create_dir_all(root.join("files"))
        .await
        .expect("member prefix directory");

    let fixture = GenericFixture::new(
        "AsyncStoreRouter(mem + file)",
        Box::new(router),
        // A router reports the *root* as its prefix, because it spans several — so that is the
        // ground truth `prefix01` and `keys01` are checked against …
        Key::new(),
        index_backed(),
        SafetyLevel::Scratch,
    )
    // … while keys must be generated under one member's prefix, or nothing routes.
    .with_key_base(mem_prefix);

    // No allowed failures. `keys02` was listed here at first, on the assumption the router would
    // inherit `AsyncMemoryStore`'s divergence; it does not — the router uses the trait default
    // `keys()`, which returns directories and the prefix as the contract requires. `H5` reported
    // the stale entry rather than letting it sit, which is what it is for.
    check(run_all(&fixture).await, &[]);

    let _ = tokio::fs::remove_dir_all(&root).await;
}

/// A store built from the `AsyncStore` **trait defaults** over a backing map.
///
/// Only `get` and `set_metadata` have no default, so everything else here is the trait's own code.
/// Giving it a real backing map — rather than a store that refuses everything — is what lets the
/// defaults be exercised at `Scratch` instead of trivially skipped: `removedir`, `makedir`, the
/// `contains`→`is_dir` fallback and `keys()` are all defaults, and four of the eleven divergences
/// the issue enumerated live in them.
#[derive(Default)]
struct DefaultsStore {
    data: std::sync::Mutex<Vec<(Key, Vec<u8>, Metadata)>>,
}

#[async_trait::async_trait]
impl AsyncStore for DefaultsStore {
    async fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), Error> {
        key.as_absolute()?;
        let data = self
            .data
            .lock()
            .map_err(|_| Error::general_error("poisoned".to_owned()))?;
        data.iter()
            .find(|(k, _, _)| k == key)
            .map(|(_, bytes, metadata)| (bytes.clone(), metadata.clone()))
            .ok_or_else(|| Error::key_not_found(key))
    }

    async fn set(&self, key: &Key, bytes: &[u8], metadata: &Metadata) -> Result<(), Error> {
        key.as_absolute()?;
        let mut data = self
            .data
            .lock()
            .map_err(|_| Error::general_error("poisoned".to_owned()))?;
        data.retain(|(k, _, _)| k != key);
        data.push((key.clone(), bytes.to_vec(), metadata.clone()));
        Ok(())
    }

    async fn set_metadata(&self, key: &Key, metadata: &Metadata) -> Result<(), Error> {
        key.as_absolute()?;
        let mut data = self
            .data
            .lock()
            .map_err(|_| Error::general_error("poisoned".to_owned()))?;
        match data.iter_mut().find(|(k, _, _)| k == key) {
            Some(entry) => {
                entry.2 = metadata.clone();
                Ok(())
            }
            None => {
                data.push((key.clone(), Vec::new(), metadata.clone()));
                Ok(())
            }
        }
    }

    fn is_supported(&self, key: &Key) -> bool {
        !key.is_relative()
    }
}

/// `C4` — the `AsyncStore` trait defaults.
#[tokio::test]
async fn c4_trait_defaults() {
    // Declared honestly, which is the whole point of the capability model. A store built from the
    // defaults alone has **no directory support at all** — `is_dir` returns `Ok(false)`, `listdir`
    // has nothing to list — and cannot enumerate, because `keys()` is built on `listdir_keys_deep`
    // and there is nothing under it. Claiming otherwise would report those rules as store defects
    // when they are the absence of an implementation.
    let capabilities = StoreCapabilities {
        write: true,
        remove: false,
        directories: false,
        derived_directories: false,
        explicit_directories: false,
        remove_directories: false,
        stored_metadata: true,
        enumerate_keys: false,
    };

    let fixture = GenericFixture::new(
        "AsyncStore trait defaults",
        Box::new(DefaultsStore::default()),
        Key::new(),
        capabilities,
        SafetyLevel::Scratch,
    );

    let report = run_all(&fixture).await;
    eprintln!("{report}");
    // Not asserted conformant: the defaults are what several of the issue's rows are *about*, and
    // this suite exists to record which of them still disagree. What is asserted is that every rule
    // reached a decided outcome and none errored unexpectedly.
    let counts = report.counts();
    assert_eq!(counts.errored, 0, "a trait default errored:\n{report}");
}

/// `C5` — `NoAsyncStore`, the environment's default.
///
/// An eighth in-tree implementation that nobody had counted: it is `pub`, it is what an
/// `Environment` holds until a store is configured, and it refuses everything by design. It
/// declines every precondition that would require it to accept a key, which is the honest answer
/// and leaves a report of skips rather than a wall of failures.
#[tokio::test]
async fn c5_no_async_store() {
    let capabilities = StoreCapabilities {
        write: false,
        remove: false,
        directories: false,
        derived_directories: false,
        explicit_directories: false,
        remove_directories: false,
        stored_metadata: false,
        enumerate_keys: false,
    };
    let fixture = GenericFixture::new(
        "NoAsyncStore",
        Box::new(NoAsyncStore),
        Key::new(),
        capabilities,
        SafetyLevel::ReadOnly,
    )
    .without_supported("NoAsyncStore accepts no key at all — that is what it is for");

    let report = run_all(&fixture).await;
    eprintln!("{report}");
    // Fully conformant, which is worth stating: a store that does nothing still has to say no
    // correctly. Absence is Ok(false), reads are KeyNotFound, and a relative key is refused as
    // KeyNotAbsolute rather than as "not found".
    check(report, &[]);
}

/// The metadata a seeded key carries, kept next to the suites that use it.
#[allow(dead_code)]
fn seed_metadata(key: &Key) -> Metadata {
    let mut record = MetadataRecord::new();
    record.with_key(key.clone());
    Metadata::MetadataRecord(record)
}
