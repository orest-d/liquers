//! `C6`–`C7` — the conformance suite applied to `AsyncOpenDALStore`.
//!
//! Two backends, because they differ in exactly the way that matters: the memory service has **no
//! directory objects**, which is the object-store shape, while the filesystem service has real
//! ones. A store verified on only one proves nothing about the other — that asymmetry is what
//! produced `STORE-OPENDAL-SLASH-HANDLING`, including the `removedir` that deleted a sibling.
//!
//! This is also the only in-tree store whose fixture can supply two preconditions no other can: a
//! key shape it refuses (the `.__metadata__` suffix, which would collide with another key's
//! sidecar path) and therefore a real subject for `sibling05`, `prefix03` and `sidecar01`.
//!
//! See `specs/design/store-conformance-suite/` and `specs/reference/STORE_SEMANTICS.md`.

#![cfg(all(feature = "store-conformance", feature = "opendal"))]

use liquers_core::parse::parse_key;
use liquers_core::query::Key;
use liquers_core::store_conformance::{
    run_all, ConformanceReport, GenericFixture, SafetyLevel, StoreCapabilities,
};
use liquers_store::opendal_store::AsyncOpenDALStore;
use opendal::Operator;

/// Capabilities of an OpenDAL-backed store.
///
/// `derived_directories` differs by *service*, which is the point of the capability. On the memory
/// service a directory exists only because keys sit under it, so it retires with its last child. On
/// the filesystem service the directory is a real object that outlives its contents — `explicit02`
/// caught the difference the first time both were run.
fn opendal_capabilities() -> StoreCapabilities {
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

fn fixture(
    label: &str,
    store: AsyncOpenDALStore,
    prefix: Key,
    capabilities: StoreCapabilities,
) -> GenericFixture {
    GenericFixture::new(
        label,
        Box::new(store),
        prefix,
        capabilities,
        SafetyLevel::Scratch,
    )
    // The suffix that makes a key's data path identical to another key's metadata path. Refused by
    // `is_supported`, which is what `prefix03`, `sibling05` and `sidecar01` are about.
    .with_unsupported_shape(parse_key("collide.__metadata__").expect("key"))
    .with_metadata_collision(parse_key("collide.__metadata__").expect("key"))
}

fn check(report: ConformanceReport) {
    eprintln!("{report}");
    if let Err(e) = report.assert_conformant(&[]) {
        panic!("{}", e.message);
    }
}

/// `C6` — the memory service: a flat key space with no directory objects.
async fn c6_report() -> ConformanceReport {
    let op = Operator::new(opendal::services::Memory::default())
        .expect("memory operator")
        .finish();
    let capabilities = opendal_capabilities();
    run_all(&fixture(
        "AsyncOpenDALStore(memory)",
        AsyncOpenDALStore::new(op, Key::new()),
        Key::new(),
        capabilities,
    ))
    .await
}

#[tokio::test]
async fn c6_opendal_memory() {
    check(c6_report().await);
}

/// `C7` — the filesystem service, which does have directory objects.
#[cfg(feature = "services-fs")]
async fn c7_report() -> ConformanceReport {
    let root = std::env::temp_dir().join(format!(
        "liquers_conf_opendal_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("temp root");

    let mut builder = opendal::services::Fs::default();
    builder = builder.root(root.to_string_lossy().as_ref());
    let op = Operator::new(builder).expect("fs operator").finish();

    let mut capabilities = opendal_capabilities();
    // A real directory outlives its last file, so `explicit02` must not be asked of this service.
    capabilities.derived_directories = false;
    let report = run_all(&fixture(
        "AsyncOpenDALStore(fs)",
        AsyncOpenDALStore::new(op, Key::new()),
        Key::new(),
        capabilities,
    ))
    .await;

    let _ = std::fs::remove_dir_all(&root);
    report
}

#[cfg(feature = "services-fs")]
#[tokio::test]
async fn c7_opendal_fs() {
    check(c7_report().await);
}

/// One row of the `STORE_IMPLEMENTATION_GUIDE.md` §9 status table.
fn status_row(store: &str, report: &ConformanceReport, notes: &str) -> String {
    let counts = report.counts();
    let status = if counts.failed == 0 && counts.errored == 0 {
        "`CONFORMANT`".to_string()
    } else {
        format!(
            "`NONCONFORMANT` (failed {}, errored {})",
            counts.failed, counts.errored
        )
    };
    format!("| {store} | {} | {status} | {notes} |", counts.ran())
}

/// Generator for the OpenDAL rows of `STORE_IMPLEMENTATION_GUIDE.md` §9, printed to **stderr**:
///
/// ```text
/// cargo test -p liquers-store --features store-conformance --test store_conformance_CONF \
///     -- --ignored --nocapture status_table
/// ```
///
/// The native rows come from the matching generator in `liquers-core`.
#[tokio::test]
#[ignore = "generator: prints the STORE_IMPLEMENTATION_GUIDE §9 OpenDAL rows to stderr"]
async fn status_table() {
    eprintln!(
        "{}",
        status_row(
            "`AsyncOpenDALStore` (memory)",
            &c6_report().await,
            "the widest coverage in tree"
        )
    );
    #[cfg(feature = "services-fs")]
    eprintln!(
        "{}",
        status_row(
            "`AsyncOpenDALStore` (fs)",
            &c7_report().await,
            "`derived_directories: false`"
        )
    );
}
