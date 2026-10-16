//! `C1`–`C5`, `C8` — the conformance suite applied to `liquers-core`'s stores and trait defaults;
//! `A1`–`A7` — the same suite applied to adapted stores built from configuration
//! (`specs/design/store-adapters/`).
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
use liquers_core::metadata::{AssetInfo, Metadata, MetadataRecord};
use liquers_core::parse::parse_key;
use liquers_core::query::Key;
use liquers_core::store::{
    AsyncFileStore, AsyncMemoryStore, AsyncStore, AsyncStoreRouter, NoAsyncStore,
};
use liquers_core::store_conformance::{
    rules, run_all, AllowedFailure, ConformanceReport, GenericFixture, RuleOutcome, SafetyLevel,
    StoreCapabilities,
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
    // No allowed failures. `keys02` was listed here until step 10 made `keys()` return the
    // directories and the prefix as §9 requires; `H5` then reported the entry as stale, which is
    // how a fixed issue forces its own bookkeeping out.
    check(c1_report().await, C1_ALLOWED);
}

/// Known divergences of `C1`, shared with the [`status_table`] generator.
const C1_ALLOWED: &[AllowedFailure] = &[];

async fn c1_report() -> ConformanceReport {
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
    run_all(&fixture).await
}

/// `C2` — `AsyncFileStore` over a temporary directory.
///
/// **`derived_directories` is false**, and that is the point of the capability: a real filesystem
/// directory is an object in its own right and survives its last file, so `explicit02` must not be
/// asked of it.
#[tokio::test]
async fn c2_async_file_store() {
    // No allowed failures. `sidecar03` was listed here until the path builders learned to refuse a
    // reserved key; `H5` then reported the entry as stale, which is how a fixed issue forces its
    // own bookkeeping out. See CORE-FILE-STORE-WRITES-METADATA-COLLIDING-KEYS.
    let report = c2_report().await;

    // `assert_conformant` inspects failures and stale allowed-failures; a rule that declined its
    // precondition is invisible to it, and the report only reaches stderr, which `cargo test`
    // swallows on success. So a `with_unsupported_shape` that was dropped or mistyped would leave
    // these two "not run" and this test would still pass — which is exactly how they came to have
    // never run against a file store in the first place. Assert the outcome, not the absence of a
    // failure.
    for rule in ["prefix03", "sibling05"] {
        let entry = report
            .entries
            .iter()
            .find(|entry| entry.id == rule)
            .unwrap_or_else(|| panic!("{rule} is not in the report at all"));
        assert!(
            matches!(entry.outcome, RuleOutcome::Passed),
            "{rule} must run and pass for C2, not decline its precondition: {:?}",
            entry.outcome
        );
    }

    check(report, C2_ALLOWED);
}

/// Known divergences of `C2`, shared with the [`status_table`] generator.
const C2_ALLOWED: &[AllowedFailure] = &[];

async fn c2_report() -> ConformanceReport {
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
    .with_metadata_collision(parse_key("collide.__metadata__").expect("key"))
    // The same key, declared as a shape the store cannot address. Until this was declared,
    // `prefix03` and `sibling05` had never run against a file store: the report said "not run"
    // and nothing failed, which is the quiet way for coverage to be missing.
    .with_unsupported_shape(parse_key("collide.__metadata__").expect("key"));

    let report = run_all(&fixture).await;
    let _ = tokio::fs::remove_dir_all(&root).await;
    report
}

/// `C3` — `AsyncStoreRouter` over a memory store and a file store.
///
/// A router is not a store, but it implements the trait and a deployment addresses it as one — the
/// issue's Impact section is entirely about it answering the same question two ways depending on
/// which member a key lands in. The fixture keeps every request inside **one** member's prefix, so
/// a failure here is the router's own dispatch rather than a disagreement between its members.
#[tokio::test]
async fn c3_async_store_router() {
    // No allowed failures. `keys02` was listed here at first, on the assumption the router would
    // inherit `AsyncMemoryStore`'s divergence; it does not — the router uses the trait default
    // `keys()`, which returns directories and the prefix as the contract requires. `H5` reported
    // the stale entry rather than letting it sit, which is what it is for.
    check(c3_report().await, C3_ALLOWED);
}

/// Known divergences of `C3`, shared with the [`status_table`] generator.
const C3_ALLOWED: &[AllowedFailure] = &[];

async fn c3_report() -> ConformanceReport {
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

    // `files/` deliberately does not exist. An absent member namespace must contribute an empty
    // listing, so it cannot abort `keys()` for the router. This is the regression case for
    // CORE-STORE-ROUTER-KEYS-FAILS-ON-AN-EMPTY-MEMBER.

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

    let report = run_all(&fixture).await;
    let _ = tokio::fs::remove_dir_all(&root).await;
    report
}

/// `C8` — `AsyncStoreRouter` with a member that describes its own entries and a member mounted
/// inside it. (Named `C6` until 2026-10-16; `C6`–`C7` are `liquers-store`'s OpenDAL suites.)
///
/// The router composes every directory answer here instead of forwarding it: the fixture's
/// namespace `mem` has two contributors. `dir07` (directory `children` are the listing) is the rule
/// this exercises hardest — before `design/store-router-forwards-asset-info/` the owner alone filled
/// `children` and omitted the nested mount. The decorating member stands in for the front-matter
/// adapter of `design/store-adapters/`, whose `get_asset_info` override the router must not bypass.
#[tokio::test]
async fn c8_router_with_a_decorated_member_and_a_nested_mount() {
    check(c8_report().await, C8_ALLOWED);
}

/// Known divergences of `C8`.
const C8_ALLOWED: &[AllowedFailure] = &[];

async fn c8_report() -> ConformanceReport {
    let mem_prefix = parse_key("mem").expect("prefix");
    let mut router = AsyncStoreRouter::new();
    // First match wins, so the nested mount goes first.
    router.add_store(Box::new(AsyncMemoryStore::new(
        &parse_key("mem/nested").expect("prefix"),
    )));
    router.add_store(Box::new(DecoratingStore(AsyncMemoryStore::new(
        &mem_prefix,
    ))));
    let fixture = GenericFixture::new(
        "AsyncStoreRouter(decorated mem + nested mem)",
        Box::new(router),
        Key::new(),
        index_backed(),
        SafetyLevel::Scratch,
    )
    .with_key_base(mem_prefix);
    run_all(&fixture).await
}

/// An `AsyncMemoryStore` with every method forwarded, whose `get_asset_info` adds a title.
struct DecoratingStore(AsyncMemoryStore);

#[async_trait::async_trait]
impl AsyncStore for DecoratingStore {
    fn store_name(&self) -> String {
        self.0.store_name()
    }
    fn key_prefix(&self) -> Key {
        self.0.key_prefix()
    }
    fn default_metadata(&self, key: &Key, is_dir: bool) -> MetadataRecord {
        self.0.default_metadata(key, is_dir)
    }
    async fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), Error> {
        self.0.get(key).await
    }
    async fn get_bytes(&self, key: &Key) -> Result<Vec<u8>, Error> {
        self.0.get_bytes(key).await
    }
    async fn get_metadata(&self, key: &Key) -> Result<Metadata, Error> {
        self.0.get_metadata(key).await
    }
    async fn get_asset_info(&self, key: &Key) -> Result<AssetInfo, Error> {
        let mut info = self.0.get_asset_info(key).await?;
        info.title = format!("Decorated {}", key.encode());
        Ok(info)
    }
    async fn set(&self, key: &Key, data: &[u8], metadata: &Metadata) -> Result<(), Error> {
        self.0.set(key, data, metadata).await
    }
    async fn set_metadata(&self, key: &Key, metadata: &Metadata) -> Result<(), Error> {
        self.0.set_metadata(key, metadata).await
    }
    async fn remove(&self, key: &Key) -> Result<(), Error> {
        self.0.remove(key).await
    }
    async fn removedir(&self, key: &Key) -> Result<(), Error> {
        self.0.removedir(key).await
    }
    async fn contains(&self, key: &Key) -> Result<bool, Error> {
        self.0.contains(key).await
    }
    async fn is_dir(&self, key: &Key) -> Result<bool, Error> {
        self.0.is_dir(key).await
    }
    async fn keys(&self) -> Result<Vec<Key>, Error> {
        self.0.keys().await
    }
    async fn listdir(&self, key: &Key) -> Result<Vec<String>, Error> {
        self.0.listdir(key).await
    }
    async fn listdir_keys(&self, key: &Key) -> Result<Vec<Key>, Error> {
        self.0.listdir_keys(key).await
    }
    async fn listdir_keys_deep(&self, key: &Key) -> Result<Vec<Key>, Error> {
        self.0.listdir_keys_deep(key).await
    }
    async fn makedir(&self, key: &Key) -> Result<(), Error> {
        self.0.makedir(key).await
    }
    fn is_supported(&self, key: &Key) -> bool {
        self.0.is_supported(key)
    }
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
    /// `None` bytes is a metadata-only entry, which has no data object (`sidecar05`).
    data: std::sync::Mutex<Vec<(Key, Option<Vec<u8>>, Metadata)>>,
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
            .find_map(|(k, bytes, metadata)| match bytes {
                Some(bytes) if k == key => Some((bytes.clone(), metadata.clone())),
                Some(_) | None => None,
            })
            .ok_or_else(|| Error::key_not_found(key))
    }

    async fn set(&self, key: &Key, bytes: &[u8], metadata: &Metadata) -> Result<(), Error> {
        key.as_absolute()?;
        let mut data = self
            .data
            .lock()
            .map_err(|_| Error::general_error("poisoned".to_owned()))?;
        data.retain(|(k, _, _)| k != key);
        // `set` is this store's own, not a default, so it seeds as every store must (§10).
        let mut metadata = metadata.clone();
        liquers_core::store::seed_filename_from_key(&mut metadata, key);
        data.push((key.clone(), Some(bytes.to_vec()), metadata));
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
                data.push((key.clone(), None, metadata.clone()));
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
    let report = c4_report().await;
    eprintln!("{report}");
    // Not asserted conformant: the defaults are what several of the issue's rows are *about*, and
    // this suite exists to record which of them still disagree. What is asserted is that every rule
    // reached a decided outcome and none errored unexpectedly.
    let counts = report.counts();
    assert_eq!(counts.errored, 0, "a trait default errored:\n{report}");
}

async fn c4_report() -> ConformanceReport {
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
    run_all(&fixture).await
}

/// `C5` — `NoAsyncStore`, the environment's default.
///
/// An eighth in-tree implementation that nobody had counted: it is `pub`, it is what an
/// `Environment` holds until a store is configured, and it refuses everything by design. It
/// declines every precondition that would require it to accept a key, which is the honest answer
/// and leaves a report of skips rather than a wall of failures.
#[tokio::test]
async fn c5_no_async_store() {
    // Fully conformant, which is worth stating: a store that does nothing still has to say no
    // correctly. Absence is Ok(false), reads are KeyNotFound, and a relative key is refused as
    // KeyNotAbsolute rather than as "not found".
    check(c5_report().await, C5_ALLOWED);
}

/// Known divergences of `C5`, shared with the [`status_table`] generator.
const C5_ALLOWED: &[AllowedFailure] = &[];

async fn c5_report() -> ConformanceReport {
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
    run_all(&fixture).await
}

// --- A1–A7: adapted stores (`specs/design/store-adapters/`, AC-7) ---

/// The `store_config` entries the adapted suites compile against: the built-ins, `memory_central`
/// and the front-matter stand-in.
fn adapter_entries() -> (
    liquers_core::command_metadata::CommandMetadataRegistry,
    liquers_core::store::realm::StoreConfigFunctions,
) {
    use liquers_core::store::realm::{register_test_store_config, StoreConfigFunctions};
    let (mut cmr, mut functions) = StoreConfigFunctions::builtin();
    register_test_store_config(&mut cmr, &mut functions).expect("test entries register");
    liquers_core::store::test_support::register_front_matter_stand_in(&mut cmr, &mut functions)
        .expect("stand-in registers");
    (cmr, functions)
}

/// A store built from configuration, the way an environment builds it.
fn configured(yaml: &str) -> Box<dyn AsyncStore> {
    let (cmr, functions) = adapter_entries();
    let store = liquers_core::store_factory::StoreRouterBuilder::from_yaml(
        yaml,
        Box::new(liquers_core::store_factory::core_store_factory()),
    )
    .expect("configuration parses")
    .with_store_config(cmr, functions)
    .build_store()
    .expect("store builds");
    Box::new(liquers_core::store::SharedStore(store))
}

/// A memory mount at `mem` with `metadata` as its section, run through the generic fixture.
async fn adapted_memory_report(
    label: &str,
    metadata: &str,
    capabilities: StoreCapabilities,
) -> ConformanceReport {
    let yaml = format!("stores:\n  - type: memory\n    prefix: mem\n    metadata: {metadata}\n");
    let fixture = GenericFixture::new(
        label,
        configured(&yaml),
        Key::new(),
        capabilities,
        SafetyLevel::Scratch,
    )
    .with_key_base(parse_key("mem").expect("prefix"));
    run_all(&fixture).await
}

/// `A1` — a read-only file store, over a directory seeded with one file.
#[tokio::test]
async fn a1_read_only_file_store() {
    check(a1_report().await, A1_ALLOWED);
}

const A1_ALLOWED: &[AllowedFailure] = &[];

async fn a1_report() -> ConformanceReport {
    let root = unique_temp_dir("a1");
    tokio::fs::create_dir_all(root.join("dir"))
        .await
        .expect("temp root");
    tokio::fs::write(root.join("dir/existing.txt"), b"hello")
        .await
        .expect("seed file");
    let yaml = format!(
        "stores:\n  - type: filesystem\n    prefix: \"\"\n    config: {{ path: \"{}\" }}\n    read_only: true\n",
        root.to_string_lossy()
    );
    let capabilities = StoreCapabilities {
        write: false,
        remove: false,
        directories: true,
        derived_directories: false,
        explicit_directories: false,
        remove_directories: false,
        stored_metadata: false,
        enumerate_keys: true,
    };
    let fixture = GenericFixture::new(
        "ReadOnlyStore(AsyncFileStore)",
        configured(&yaml),
        Key::new(),
        capabilities,
        SafetyLevel::ReadOnly,
    )
    .with_existing(parse_key("dir/existing.txt").expect("key"))
    .with_existing_directory(parse_key("dir").expect("key"));
    let report = run_all(&fixture).await;
    let _ = tokio::fs::remove_dir_all(&root).await;
    report
}

/// `A2` — `storage: none`: records are synthesized, never stored.
#[tokio::test]
async fn a2_storage_none() {
    check(a2_report().await, A2_ALLOWED);
}

const A2_ALLOWED: &[AllowedFailure] = &[];

async fn a2_report() -> ConformanceReport {
    let mut capabilities = index_backed();
    capabilities.stored_metadata = false;
    adapted_memory_report(
        "MetadataPipelineStore(memory, storage: none)",
        "{ storage: none }",
        capabilities,
    )
    .await
}

const PER_ENTRY_LAYOUTS: [&str; 4] = ["sidecar", "root", "prefix", "folder"];

/// `A3` — `per_entry`, once for each layout.
#[tokio::test]
async fn a3_per_entry_each_layout() {
    for layout in PER_ENTRY_LAYOUTS {
        check(a3_report(layout).await, A3_ALLOWED);
    }
}

const A3_ALLOWED: &[AllowedFailure] = &[];

async fn a3_report(layout: &str) -> ConformanceReport {
    adapted_memory_report(
        &format!("MetadataPipelineStore(memory, per_entry-{layout})"),
        &format!("{{ storage: per_entry-{layout} }}"),
        index_backed(),
    )
    .await
}

/// `A4` — `grouped`: one record file per folder.
#[tokio::test]
async fn a4_grouped() {
    check(a4_report().await, A4_ALLOWED);
}

const A4_ALLOWED: &[AllowedFailure] = &[];

async fn a4_report() -> ConformanceReport {
    adapted_memory_report(
        "MetadataPipelineStore(memory, grouped)",
        "{ storage: grouped }",
        index_backed(),
    )
    .await
}

/// `A5` — central storage on a router, with a member keeping a backup.
#[tokio::test]
async fn a5_memory_central() {
    check(a5_report().await, A5_ALLOWED);
}

const A5_ALLOWED: &[AllowedFailure] = &[];

async fn a5_report() -> ConformanceReport {
    let yaml = "metadata: { storage: memory_central }\nstores:\n  - type: memory\n    prefix: mem\n    metadata: { storage: per_entry }\n";
    let fixture = GenericFixture::new(
        "MetadataPipelineStore(router, memory_central)",
        configured(yaml),
        Key::new(),
        index_backed(),
        SafetyLevel::Scratch,
    )
    .with_key_base(parse_key("mem").expect("prefix"));
    run_all(&fixture).await
}

const WRITE_STRATEGIES: [&str; 3] = ["direct", "atomic", "transactional"];

/// `A6` — each write strategy, over `per_entry` storage.
#[tokio::test]
async fn a6_each_write_strategy() {
    for writes in WRITE_STRATEGIES {
        check(a6_report(writes).await, A6_ALLOWED);
    }
}

const A6_ALLOWED: &[AllowedFailure] = &[];

async fn a6_report(writes: &str) -> ConformanceReport {
    adapted_memory_report(
        &format!("MetadataPipelineStore(memory, per_entry, {writes})"),
        &format!("{{ storage: per_entry, writes: {writes} }}"),
        index_backed(),
    )
    .await
}

/// `A7` — a front-matter handler in the read chain (the core stand-in for liquers-lib's).
#[tokio::test]
async fn a7_front_matter() {
    check(a7_report().await, A7_ALLOWED);
}

const A7_ALLOWED: &[AllowedFailure] = &[];

async fn a7_report() -> ConformanceReport {
    adapted_memory_report(
        "MetadataPipelineStore(memory, test_front_matter on read)",
        "{ on_read: media_type-fill/file_facts/test_front_matter }",
        index_backed(),
    )
    .await
}

/// The `STORE_IMPLEMENTATION_GUIDE.md` §9 row of one report: rules that ran, and a status that is
/// `CONFORMANT` only when nothing failed or errored.
///
/// `allowed` is the list the suite passes to `assert_conformant`: a failure it names is a known
/// divergence, so it makes the row `BLOCKED` rather than `NONCONFORMANT`, as does a `Blocked`
/// outcome.
fn status_row(
    store: &str,
    report: &ConformanceReport,
    allowed: &[AllowedFailure],
    notes: &str,
) -> String {
    let counts = report.counts();
    let allowed_failures = report
        .failures()
        .filter(|entry| allowed.iter().any(|a| a.rule == entry.id))
        .count();
    let unexpected = counts.failed + counts.errored - allowed_failures;
    let blocked = counts.blocked + allowed_failures;
    let status = if unexpected > 0 {
        format!(
            "`NONCONFORMANT` (failed {}, errored {})",
            counts.failed, counts.errored
        )
    } else if blocked > 0 {
        format!("`BLOCKED` ({blocked} rules)")
    } else {
        "`CONFORMANT`".to_string()
    };
    format!("| {store} | {} | {status} | {notes} |", counts.ran())
}

/// Generator for the native rows of `STORE_IMPLEMENTATION_GUIDE.md` §9, printed to **stderr**:
///
/// ```text
/// cargo test -p liquers-core --features store-conformance --test store_conformance_CONF \
///     -- --ignored --nocapture status_table
/// ```
///
/// The OpenDAL rows come from the matching generator in `liquers-store`; the browser rows
/// (`FetchStore`, `JsStore`, `LocalStorageStore`) from the wasm suites, merged by hand. The notes
/// are kept here so the rows are reproducible.
#[tokio::test]
#[ignore = "generator: prints the STORE_IMPLEMENTATION_GUIDE §9 table to stderr"]
async fn status_table() {
    eprintln!(
        "As of {}, from the suites above: {} rules are registered.\n",
        chrono::Utc::now().format("%Y-%m-%d"),
        rules().len()
    );
    eprintln!("| Store | Rules run | Status | Notes |");
    eprintln!("|---|---|---|---|");
    eprintln!(
        "{}",
        status_row("`AsyncMemoryStore`", &c1_report().await, C1_ALLOWED, "")
    );
    eprintln!(
        "{}",
        status_row(
            "`AsyncFileStore`",
            &c2_report().await,
            C2_ALLOWED,
            "`derived_directories: false` — real directories persist",
        )
    );
    eprintln!(
        "{}",
        status_row(
            "`AsyncStoreRouter`",
            &c3_report().await,
            C3_ALLOWED,
            "needs each member's prefix to exist (`CORE-STORE-ROUTER-KEYS-FAILS-ON-AN-EMPTY-MEMBER`); \
             a directory above its members answers directory metadata, checked by router unit \
             tests because no generic fixture requests such a key",
        )
    );
    eprintln!(
        "{}",
        status_row(
            "`AsyncStoreRouter` (nested)",
            &c8_report().await,
            C8_ALLOWED,
            "a decorating member with a member mounted inside it: every directory answer is \
             composed by the router (`design/store-router-forwards-asset-info/`)",
        )
    );
    eprintln!(
        "{}",
        status_row(
            "Trait defaults",
            &c4_report().await,
            &[],
            "no directory support, no enumeration",
        )
    );
    eprintln!(
        "{}",
        status_row(
            "`NoAsyncStore`",
            &c5_report().await,
            C5_ALLOWED,
            "accepts no key, and says so correctly",
        )
    );
    eprintln!(
        "{}",
        status_row(
            "`ReadOnlyStore` (file store)",
            &a1_report().await,
            A1_ALLOWED,
            "`read_only: true` from configuration; write and remove refused",
        )
    );
    eprintln!(
        "{}",
        status_row(
            "Pipeline, `storage: none`",
            &a2_report().await,
            A2_ALLOWED,
            "records synthesized, never stored",
        )
    );
    for layout in PER_ENTRY_LAYOUTS {
        eprintln!(
            "{}",
            status_row(
                &format!("Pipeline, `per_entry-{layout}`"),
                &a3_report(layout).await,
                A3_ALLOWED,
                "",
            )
        );
    }
    eprintln!(
        "{}",
        status_row(
            "Pipeline, `grouped`",
            &a4_report().await,
            A4_ALLOWED,
            "one record file per folder",
        )
    );
    eprintln!(
        "{}",
        status_row(
            "Pipeline, central (`memory_central`)",
            &a5_report().await,
            A5_ALLOWED,
            "router-level section; the member keeps a `per_entry` backup",
        )
    );
    for writes in WRITE_STRATEGIES {
        eprintln!(
            "{}",
            status_row(
                &format!("Pipeline, `writes: {writes}`"),
                &a6_report(writes).await,
                A6_ALLOWED,
                "over `per_entry`",
            )
        );
    }
    eprintln!(
        "{}",
        status_row(
            "Pipeline, front matter on read",
            &a7_report().await,
            A7_ALLOWED,
            "core stand-in for liquers-lib's `front_matter`",
        )
    );
}

/// The metadata a seeded key carries, kept next to the suites that use it.
#[allow(dead_code)]
fn seed_metadata(key: &Key) -> Metadata {
    let mut record = MetadataRecord::new();
    record.with_key(key.clone());
    Metadata::MetadataRecord(record)
}
