//! Content changed outside Liquers (`specs/design/dependency-audit-and-expiry-provenance/`,
//! Part G; Phase 3 I2).
//!
//! A hand edit is simulated by rewriting the **bytes only** and keeping the old sidecar, which is
//! what a text editor does. Detection happens when the changed value is *read* by a process that
//! did not already hold it — so the tests "restart" first: a second environment over a replayed
//! snapshot of the first one's store ([`fixtures::StoreSnapshot`]).

mod common;
mod fixtures;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use liquers_core::{
    assets::{AssetManager, AuditMode, ExternalChangeAction, ExternalChangePolicy},
    context::{EnvRef, SimpleEnvironment},
    environment_builder::{AssetManagerOptions, EnvironmentBuilder, Queued, VersionVerification},
    error::Error,
    metadata::{
        DependencyKey, ExpiryCause, ExpiryReason, Metadata, MetadataRecord, Status, Version,
    },
    parse::parse_key,
    query::Key,
    recipes::DefaultRecipeProvider,
    store::{AsyncFileStore, AsyncMemoryStore, AsyncStore},
    value::Value,
};

use common::manager_scenarios::{
    provenance_evaluate_chain, provenance_store, provenance_text_metadata,
    register_provenance_commands, wait_until_stored,
};
use fixtures::{CountingStore, StoreSnapshot};

type TestEnv = SimpleEnvironment<Value>;
type TestResult = Result<(), Box<dyn std::error::Error>>;

const A_V1: &[u8] = b"hello";
const A_V2: &[u8] = b"hello, edited by hand";
const B_EDIT: &[u8] = b"EDITED BY HAND";

/// Fail fast instead of hanging if a call deadlocks.
async fn within<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(std::time::Duration::from_secs(20), future)
        .await
        .expect("call did not finish within 20 s")
}

fn key(name: &str) -> Key {
    parse_key(name).expect("key")
}

fn dep(name: &str) -> DependencyKey {
    DependencyKey::from(&key(name))
}

fn options(policy: ExternalChangePolicy) -> AssetManagerOptions {
    AssetManagerOptions::default().with_external_change(policy)
}

fn env_with(
    store: Arc<dyn AsyncStore>,
    options: AssetManagerOptions,
) -> Result<EnvRef<TestEnv>, Error> {
    let mut builder = EnvironmentBuilder::<Value, (), Queued>::new()
        .with_asset_manager_options(options)
        .with_async_store(store)
        .with_recipe_provider(Arc::new(DefaultRecipeProvider));
    register_provenance_commands(&mut builder.command_registry);
    builder.build()
}

fn log_of(metadata: &Metadata) -> Vec<String> {
    match metadata {
        Metadata::MetadataRecord(mr) => mr.log.iter().map(|e| e.message.clone()).collect(),
        Metadata::LegacyMetadata(_) => Vec::new(),
    }
}

fn chain_keys() -> Vec<Key> {
    [
        "data/recipes.yaml",
        "data/a.txt",
        "data/b.txt",
        "data/report.txt",
    ]
    .into_iter()
    .map(key)
    .collect()
}

/// Process one: `a.txt` is a `Source` written through the manager with content `a`;
/// `b.txt = upper(a.txt)` and `report.txt = summarize(b.txt)` are computed and stored. The
/// recipes file is re-seeded through `set_binary`, so it carries a content-hash version and a
/// sweep finds it verified (the raw seed has none: `verify_stored_versions_reports_an_unversioned_file`).
async fn computed_chain(a: &[u8]) -> Result<StoreSnapshot, Box<dyn std::error::Error>> {
    let seed = provenance_store(false).await?;
    let recipes = seed.get_bytes(&key("data/recipes.yaml")).await?;
    let envref = env_with(
        Arc::new(AsyncMemoryStore::new(&Key::new())),
        AssetManagerOptions::default(),
    )?;
    let am = envref.get_asset_manager();
    within(am.set_binary(
        &key("data/recipes.yaml"),
        &recipes,
        provenance_text_metadata(),
    ))
    .await?;
    within(am.set_binary(&key("data/a.txt"), a, provenance_text_metadata())).await?;
    within(provenance_evaluate_chain(&envref)).await?;
    Ok(StoreSnapshot::capture(&envref.get_async_store(), &chain_keys()).await?)
}

/// Rewrite the bytes of `name`, keeping its sidecar: an edit made outside Liquers.
async fn edit_outside_liquers(
    store: &dyn AsyncStore,
    name: &str,
    bytes: &[u8],
) -> Result<(), Error> {
    let k = key(name);
    let metadata = store.get_metadata(&k).await?;
    store.set(&k, bytes, &metadata).await
}

/// Replay `snapshot` into a fresh memory store and apply `edits` to it, as a restarted process
/// would find it.
async fn replayed(
    snapshot: &StoreSnapshot,
    edits: &[(&str, &[u8])],
) -> Result<AsyncMemoryStore, Error> {
    let store = AsyncMemoryStore::new(&Key::new());
    snapshot.replay_into(&store).await?;
    for (name, bytes) in edits {
        edit_outside_liquers(&store, name, bytes).await?;
    }
    Ok(store)
}

async fn restart(
    snapshot: &StoreSnapshot,
    edits: &[(&str, &[u8])],
    options: AssetManagerOptions,
) -> Result<EnvRef<TestEnv>, Box<dyn std::error::Error>> {
    Ok(env_with(
        Arc::new(replayed(snapshot, edits).await?),
        options,
    )?)
}

/// Evaluate `name` and return its text. Loading a stored value registers its recorded edges.
async fn read(envref: &EnvRef<TestEnv>, name: &str) -> Result<String, Box<dyn std::error::Error>> {
    let asset = within(envref.get_asset_manager().get(&key(name))).await?;
    Ok(within(asset.get()).await?.try_into_string()?)
}

async fn status_of(
    envref: &EnvRef<TestEnv>,
    name: &str,
) -> Result<Status, Box<dyn std::error::Error>> {
    let asset = within(envref.get_asset_manager().get(&key(name))).await?;
    let _ = within(asset.get()).await?;
    Ok(asset.status().await)
}

async fn stored(envref: &EnvRef<TestEnv>, name: &str) -> Result<Metadata, Error> {
    envref.get_async_store().get_metadata(&key(name)).await
}

fn updated_in_store(actual: Version, root: &str, via: &str) -> Option<ExpiryReason> {
    Some(ExpiryReason::Cascaded {
        cause: ExpiryCause::UpdatedInStore { actual },
        root: dep(root),
        via: dep(via),
    })
}

/// Bytes whose legacy (unflagged) blake3 version has bit 127 clear, so that `kind()` cannot take
/// it for a content hash and only the legacy rule can verify it — about half of all byte strings.
fn legacy_bytes(stem: &str) -> Vec<u8> {
    (0..1000)
        .map(|i| format!("{stem} {i}").into_bytes())
        .find(|bytes| Version::from_bytes(bytes) != Version::from_content(bytes))
        .expect("a byte string with an unflagged legacy hash")
}

// ---------------------------------------------------------------------------------------------
// The read path
// ---------------------------------------------------------------------------------------------

/// Example 8: a `Source` edited by hand stays a `Source`, takes the new content hash as its
/// version, and expires its dependents with `UpdatedInStore`. It is not expired itself.
#[tokio::test]
async fn hand_edited_source_is_input_and_expires_dependents() -> TestResult {
    let snapshot = computed_chain(A_V1).await?;
    let envref = restart(
        &snapshot,
        &[("data/a.txt", A_V2)],
        AssetManagerOptions::default(),
    )
    .await?;
    let v2 = Version::from_content(A_V2);

    assert_eq!(read(&envref, "data/b.txt").await?, "HELLO"); // served; edge a -> b @V1 loaded
    assert_eq!(read(&envref, "data/a.txt").await?, "hello, edited by hand");

    assert_eq!(status_of(&envref, "data/a.txt").await?, Status::Source);
    assert_eq!(
        envref
            .get_asset_manager()
            .version(&key("data/a.txt"))
            .await?,
        v2
    );
    let a = stored(&envref, "data/a.txt").await?;
    assert_eq!(a.status(), Status::Source, "a Source stays a Source");
    assert_eq!(
        a.version(),
        Some(v2),
        "the version is bumped in the sidecar"
    );
    assert_eq!(a.expiry_reason(), None, "the root is input, not expired");
    assert!(log_of(&a)
        .iter()
        .any(|m| m == "content of data/a.txt changed outside Liquers; accepted as user input"));

    let b = stored(&envref, "data/b.txt").await?;
    assert_eq!(b.status(), Status::Expired);
    assert_eq!(
        b.expiry_reason(),
        updated_in_store(v2, "data/a.txt", "data/a.txt")
    );
    Ok(())
}

/// A recipe-backed value edited by hand: `user_input` keeps the edit as an `Override`,
/// `corrupted` deletes it so the recipe recomputes. Its dependent is expired with the same reason
/// either way.
#[tokio::test]
async fn recipe_backed_edit_follows_policy() -> TestResult {
    let actual = Version::from_content(B_EDIT);
    for policy in [
        ExternalChangePolicy::UserInput,
        ExternalChangePolicy::Corrupted,
    ] {
        let snapshot = computed_chain(A_V1).await?;
        let envref = restart(&snapshot, &[("data/b.txt", B_EDIT)], options(policy)).await?;
        assert_eq!(read(&envref, "data/report.txt").await?, "summary of HELLO"); // edge b -> report

        let served = read(&envref, "data/b.txt").await?;
        match policy {
            ExternalChangePolicy::UserInput => {
                assert_eq!(served, "EDITED BY HAND", "the edit is served");
                assert_eq!(status_of(&envref, "data/b.txt").await?, Status::Override);
                let b = stored(&envref, "data/b.txt").await?;
                assert_eq!(b.status(), Status::Override);
                assert_eq!(b.version(), Some(actual));
                assert!(log_of(&b).iter().any(|m| m
                    == "content of data/b.txt changed outside Liquers; accepted as user input"));
            }
            ExternalChangePolicy::Corrupted => {
                assert_eq!(served, "HELLO", "recomputed from the recipe");
                wait_until_stored(&envref, &key("data/b.txt"), Status::Ready).await?;
                let (bytes, b) = envref.get_async_store().get(&key("data/b.txt")).await?;
                assert_eq!(
                    bytes, b"HELLO",
                    "the stored copy was replaced by the recompute"
                );
                assert_eq!(b.version(), Some(Version::from_content(b"HELLO")));
            }
        }
        let report = stored(&envref, "data/report.txt").await?;
        assert_eq!(report.status(), Status::Expired, "{policy:?}");
        assert_eq!(
            report.expiry_reason(),
            updated_in_store(actual, "data/b.txt", "data/b.txt"),
            "{policy:?}"
        );
    }
    Ok(())
}

/// An `Override` is the user's by definition: even under `corrupted` it is accepted, never
/// deleted.
#[tokio::test]
async fn override_is_never_deleted() -> TestResult {
    let snapshot = computed_chain(A_V1).await?;
    let store = replayed(&snapshot, &[]).await?;
    let mut b = store.get_metadata(&key("data/b.txt")).await?;
    b.set_status(Status::Override)?;
    store.set_metadata(&key("data/b.txt"), &b).await?;
    edit_outside_liquers(&store, "data/b.txt", B_EDIT).await?;
    let envref = env_with(Arc::new(store), options(ExternalChangePolicy::Corrupted))?;

    assert_eq!(read(&envref, "data/b.txt").await?, "EDITED BY HAND");
    let b = stored(&envref, "data/b.txt").await?;
    assert_eq!(b.status(), Status::Override);
    assert_eq!(b.version(), Some(Version::from_content(B_EDIT)));
    Ok(())
}

/// The sidecar a file store synthesizes for a bare file: status `Source`, no version.
fn no_metadata_sidecar() -> Metadata {
    Metadata::MetadataRecord(
        MetadataRecord::new()
            .with_type_identifier("Text".to_owned())
            .with_status(Status::Source)
            .clone(),
    )
}

/// A file dropped in with no metadata and no recipe: recorded version 0, so a mismatch; it stays
/// `Source`, its version is the hash, and — the owner's decision — **nothing is written**: the
/// stored metadata still records no version. (Not "no sidecar exists": the memory store always
/// holds metadata, and the file store writes its own on a bare file's first read.)
#[tokio::test]
async fn file_with_no_metadata_and_no_recipe_becomes_source_with_hash_version() -> TestResult {
    let bytes: &[u8] = b"x,y\n1,2\n";
    let store = AsyncMemoryStore::new(&Key::new());
    store
        .set(&key("data/notes.txt"), bytes, &no_metadata_sidecar())
        .await?;
    let envref = env_with(Arc::new(store), AssetManagerOptions::default())?;

    assert_eq!(read(&envref, "data/notes.txt").await?, "x,y\n1,2\n");
    assert_eq!(status_of(&envref, "data/notes.txt").await?, Status::Source);
    assert_eq!(
        envref
            .get_asset_manager()
            .version(&key("data/notes.txt"))
            .await?,
        Version::from_content(bytes),
        "the version is the content hash"
    );
    let notes = stored(&envref, "data/notes.txt").await?;
    assert_eq!(notes.version(), None, "the manager wrote nothing");
    assert_eq!(notes.status(), Status::Source);
    assert!(
        log_of(&notes).is_empty(),
        "no sidecar was written, so no log line: {:?}",
        log_of(&notes)
    );
    Ok(())
}

/// A file with no metadata where a recipe exists: `user_input` makes it an `Override` and writes
/// the sidecar (the status really changes); `corrupted` deletes it and the recipe recomputes.
#[tokio::test]
async fn file_with_no_metadata_under_recipe_follows_policy() -> TestResult {
    for policy in [
        ExternalChangePolicy::UserInput,
        ExternalChangePolicy::Corrupted,
    ] {
        let snapshot = computed_chain(A_V1).await?;
        let store = replayed(&snapshot, &[]).await?;
        store
            .set(&key("data/b.txt"), B_EDIT, &no_metadata_sidecar())
            .await?;
        let envref = env_with(Arc::new(store), options(policy))?;

        let served = read(&envref, "data/b.txt").await?;
        let b = match policy {
            ExternalChangePolicy::UserInput => {
                assert_eq!(served, "EDITED BY HAND");
                stored(&envref, "data/b.txt").await?
            }
            ExternalChangePolicy::Corrupted => {
                assert_eq!(served, "HELLO", "recomputed");
                wait_until_stored(&envref, &key("data/b.txt"), Status::Ready).await?;
                stored(&envref, "data/b.txt").await?
            }
        };
        match policy {
            ExternalChangePolicy::UserInput => {
                assert_eq!(b.status(), Status::Override, "a sidecar is written");
                assert_eq!(b.version(), Some(Version::from_content(B_EDIT)));
            }
            ExternalChangePolicy::Corrupted => {
                assert_eq!(b.status(), Status::Ready);
                assert_eq!(b.version(), Some(Version::from_content(b"HELLO")));
            }
        }
    }
    Ok(())
}

/// A value whose recorded version is a timestamp (not a hash) holds bytes: the bytes cannot be
/// what that version fingerprinted, so they are adopted, with the "no content hash" wording.
#[tokio::test]
async fn timestamp_versioned_value_with_bytes_is_adopted() -> TestResult {
    let recorded = Version::from_time_now();
    let mut record = provenance_text_metadata();
    record.status = Status::Source;
    record.version = Some(recorded);
    let store = AsyncMemoryStore::new(&Key::new());
    store
        .set(&key("data/a.txt"), A_V1, &Metadata::MetadataRecord(record))
        .await?;
    let envref = env_with(Arc::new(store), AssetManagerOptions::default())?;

    assert_eq!(read(&envref, "data/a.txt").await?, "hello");
    let a = stored(&envref, "data/a.txt").await?;
    assert_eq!(a.version(), Some(Version::from_content(A_V1)));
    assert_eq!(a.status(), Status::Source);
    assert!(log_of(&a)
        .iter()
        .any(|m| m
            == "no content hash was recorded for data/a.txt; adopting its content as user input"));
    Ok(())
}

/// `verify_versions: off` restores the old behaviour: nothing is hashed, the edit is served under
/// its old version, nothing is written or expired, and a sweep reports nothing.
#[tokio::test]
async fn verification_off_changes_nothing() -> TestResult {
    let snapshot = computed_chain(A_V1).await?;
    let envref = restart(
        &snapshot,
        &[("data/a.txt", A_V2)],
        AssetManagerOptions::default().with_verify_versions(VersionVerification::Off),
    )
    .await?;
    let v1 = Version::from_content(A_V1);
    assert_eq!(read(&envref, "data/b.txt").await?, "HELLO");
    assert_eq!(read(&envref, "data/a.txt").await?, "hello, edited by hand");
    assert_eq!(
        envref
            .get_asset_manager()
            .version(&key("data/a.txt"))
            .await?,
        v1
    );
    assert_eq!(stored(&envref, "data/a.txt").await?.version(), Some(v1));
    assert_eq!(stored(&envref, "data/b.txt").await?.status(), Status::Ready);

    let am = envref.get_asset_manager();
    let any = within(am.get_binary_any_status(&key("data/a.txt"))).await?;
    assert!(any.is_some());
    let report = within(am.verify_stored_versions(&key("data"), true, AuditMode::Expire)).await?;
    assert!(
        report.changed.is_empty() && report.verified.is_empty(),
        "{report:?}"
    );
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// The sweep
// ---------------------------------------------------------------------------------------------

/// `ReportOnly` says what would be done and changes nothing.
#[tokio::test]
async fn verify_stored_versions_report_only_changes_nothing() -> TestResult {
    let snapshot = computed_chain(A_V1).await?;
    let envref = restart(
        &snapshot,
        &[("data/a.txt", A_V2)],
        AssetManagerOptions::default(),
    )
    .await?;
    let am = envref.get_asset_manager();
    let v1 = Version::from_content(A_V1);

    let report =
        within(am.verify_stored_versions(&key("data"), true, AuditMode::ReportOnly)).await?;
    assert_eq!(
        report.changed,
        vec![(
            key("data/a.txt"),
            ExternalChangeAction::AcceptAsInput {
                actual: Version::from_content(A_V2)
            }
        )]
    );
    for name in ["data/recipes.yaml", "data/b.txt", "data/report.txt"] {
        assert!(report.verified.contains(&key(name)), "{name}: {report:?}");
    }
    assert!(report.skipped.is_empty(), "{report:?}");

    let a = stored(&envref, "data/a.txt").await?;
    assert_eq!(a.version(), Some(v1), "nothing written");
    assert_eq!(
        am.version(&key("data/a.txt")).await?,
        v1,
        "nothing registered"
    );
    assert_eq!(stored(&envref, "data/b.txt").await?.status(), Status::Ready);
    Ok(())
}

/// In `Expire` mode the sweep applies the policy to every changed value, read or not.
#[tokio::test]
async fn verify_stored_versions_applies_policy() -> TestResult {
    let va = Version::from_content(A_V2);
    let vb = Version::from_content(B_EDIT);
    for policy in [
        ExternalChangePolicy::UserInput,
        ExternalChangePolicy::Corrupted,
    ] {
        let snapshot = computed_chain(A_V1).await?;
        let envref = restart(
            &snapshot,
            &[("data/a.txt", A_V2), ("data/b.txt", B_EDIT)],
            options(policy),
        )
        .await?;
        assert_eq!(read(&envref, "data/report.txt").await?, "summary of HELLO"); // edge b -> report
        let am = envref.get_asset_manager();

        let report =
            within(am.verify_stored_versions(&key("data"), true, AuditMode::Expire)).await?;
        let b_action = match policy {
            ExternalChangePolicy::UserInput => {
                ExternalChangeAction::ConvertToOverride { actual: vb }
            }
            ExternalChangePolicy::Corrupted => ExternalChangeAction::Delete,
        };
        let mut changed = report.changed.clone();
        changed.sort_by(|x, y| x.0.cmp(&y.0));
        assert_eq!(
            changed,
            vec![
                (
                    key("data/a.txt"),
                    ExternalChangeAction::AcceptAsInput { actual: va }
                ),
                (key("data/b.txt"), b_action),
            ],
            "{policy:?}"
        );
        assert_eq!(stored(&envref, "data/a.txt").await?.version(), Some(va));
        match policy {
            ExternalChangePolicy::UserInput => {
                let b = stored(&envref, "data/b.txt").await?;
                assert_eq!(b.status(), Status::Override);
                assert_eq!(b.version(), Some(vb));
            }
            ExternalChangePolicy::Corrupted => {
                assert!(
                    !envref
                        .get_async_store()
                        .contains(&key("data/b.txt"))
                        .await?,
                    "the stored copy is deleted"
                );
            }
        }
        let report_meta = stored(&envref, "data/report.txt").await?;
        assert_eq!(
            report_meta.expiry_reason(),
            updated_in_store(vb, "data/b.txt", "data/b.txt"),
            "{policy:?}"
        );
    }
    Ok(())
}

/// A file seeded by a raw `store.set` with empty metadata has no recorded version: the sweep
/// reports it changed (accepted as input), and applying that writes nothing.
#[tokio::test]
async fn verify_stored_versions_reports_an_unversioned_file() -> TestResult {
    let store = provenance_store(false).await?; // data/recipes.yaml, Metadata::new()
    let yaml = store.get_bytes(&key("data/recipes.yaml")).await?;
    let envref = env_with(Arc::new(store), AssetManagerOptions::default())?;
    let am = envref.get_asset_manager();
    let expected = vec![(
        key("data/recipes.yaml"),
        ExternalChangeAction::AcceptAsInput {
            actual: Version::from_content(&yaml),
        },
    )];

    let report =
        within(am.verify_stored_versions(&key("data"), true, AuditMode::ReportOnly)).await?;
    assert_eq!(report.changed, expected);
    let report = within(am.verify_stored_versions(&key("data"), true, AuditMode::Expire)).await?;
    assert_eq!(report.changed, expected);
    assert_eq!(
        stored(&envref, "data/recipes.yaml").await?.version(),
        None,
        "no sidecar is written for a file with no metadata and no recipe"
    );
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Legacy (unflagged) hashes
// ---------------------------------------------------------------------------------------------

/// A store whose `a.txt` carries a legacy version, as written before content hashes were flagged,
/// with `b.txt`'s record of it rewritten to match.
async fn legacy_chain(a: &[u8]) -> Result<AsyncMemoryStore, Box<dyn std::error::Error>> {
    let snapshot = computed_chain(a).await?;
    let store = replayed(&snapshot, &[]).await?;
    let legacy = Version::from_bytes(a);
    let mut a_meta = store.get_metadata(&key("data/a.txt")).await?;
    a_meta.set_version(Some(legacy))?;
    store.set_metadata(&key("data/a.txt"), &a_meta).await?;
    let mut b_meta = store.get_metadata(&key("data/b.txt")).await?;
    if let Metadata::MetadataRecord(mr) = &mut b_meta {
        for record in mr.dependencies.iter_mut() {
            if record.key == dep("data/a.txt") {
                record.version = legacy;
            }
        }
    }
    store.set_metadata(&key("data/b.txt"), &b_meta).await?;
    Ok(store)
}

#[tokio::test]
async fn legacy_unchanged_value_still_verifies() -> TestResult {
    let a = legacy_bytes("legacy unchanged");
    let legacy = Version::from_bytes(&a);
    let envref = env_with(
        Arc::new(legacy_chain(&a).await?),
        AssetManagerOptions::default(),
    )?;

    let _ = read(&envref, "data/b.txt").await?;
    let _ = read(&envref, "data/a.txt").await?;
    assert_eq!(
        envref
            .get_asset_manager()
            .version(&key("data/a.txt"))
            .await?,
        legacy
    );
    assert_eq!(
        stored(&envref, "data/a.txt").await?.version(),
        Some(legacy),
        "not rewritten"
    );
    assert_eq!(stored(&envref, "data/b.txt").await?.status(), Status::Ready);
    let report = within(envref.get_asset_manager().verify_stored_versions(
        &key("data/a.txt"),
        false,
        AuditMode::ReportOnly,
    ))
    .await?;
    assert_eq!(report.verified, vec![key("data/a.txt")]);
    Ok(())
}

#[tokio::test]
async fn legacy_changed_value_is_a_mismatch() -> TestResult {
    let a = legacy_bytes("legacy changed");
    let store = legacy_chain(&a).await?;
    edit_outside_liquers(&store, "data/a.txt", A_V2).await?;
    let envref = env_with(Arc::new(store), AssetManagerOptions::default())?;
    let actual = Version::from_content(A_V2);

    let report = within(envref.get_asset_manager().verify_stored_versions(
        &key("data/a.txt"),
        false,
        AuditMode::ReportOnly,
    ))
    .await?;
    assert_eq!(
        report.changed,
        vec![(
            key("data/a.txt"),
            ExternalChangeAction::AcceptAsInput { actual }
        )]
    );
    let _ = read(&envref, "data/b.txt").await?;
    assert_eq!(read(&envref, "data/a.txt").await?, "hello, edited by hand");
    assert_eq!(
        envref
            .get_asset_manager()
            .version(&key("data/a.txt"))
            .await?,
        actual
    );
    assert_eq!(
        stored(&envref, "data/b.txt").await?.expiry_reason(),
        updated_in_store(actual, "data/a.txt", "data/a.txt")
    );
    Ok(())
}

/// Re-storing a legacy value through Liquers gives it a flagged version: its dependents are
/// expired once, and only once.
#[tokio::test]
async fn restoring_a_legacy_value_costs_one_cascade() -> TestResult {
    let a = legacy_bytes("legacy restored");
    let envref = env_with(
        Arc::new(legacy_chain(&a).await?),
        AssetManagerOptions::default(),
    )?;
    let am = envref.get_asset_manager();
    let text = String::from_utf8(a.clone())?.to_uppercase();
    assert_eq!(read(&envref, "data/b.txt").await?, text);
    let _ = read(&envref, "data/a.txt").await?; // verified: nothing expires
    assert_eq!(stored(&envref, "data/b.txt").await?.status(), Status::Ready);

    within(am.set_binary(&key("data/a.txt"), &a, provenance_text_metadata())).await?;
    let b = stored(&envref, "data/b.txt").await?;
    assert_eq!(b.status(), Status::Expired, "the one cascade");
    assert_eq!(
        stored(&envref, "data/a.txt").await?.version(),
        Some(Version::from_content(&a))
    );

    assert_eq!(read(&envref, "data/b.txt").await?, text); // recomputed against the new version
    wait_until_stored(&envref, &key("data/b.txt"), Status::Ready).await?;
    within(am.set_binary(&key("data/a.txt"), &a, provenance_text_metadata())).await?;
    assert_eq!(
        stored(&envref, "data/b.txt").await?.status(),
        Status::Ready,
        "the same bytes again cost nothing"
    );
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Pitfalls and corner cases
// ---------------------------------------------------------------------------------------------

/// Pitfall 3: the Part B audit compares recorded versions only, so it cannot see an edit nobody
/// has read. Reading (or sweeping) is what makes the metadata truthful.
#[tokio::test]
async fn audit_alone_does_not_see_unread_hand_edit() -> TestResult {
    let snapshot = computed_chain(A_V1).await?;
    let envref = restart(
        &snapshot,
        &[("data/a.txt", A_V2)],
        AssetManagerOptions::default(),
    )
    .await?;
    let am = envref.get_asset_manager();
    let _ = read(&envref, "data/b.txt").await?;

    let audit =
        within(am.trigger_dependency_audit_all_registered_with(AuditMode::ReportOnly)).await?;
    assert!(audit.findings.is_empty(), "{audit:?}");
    let audit = within(am.trigger_dependency_audit_all_registered()).await?;
    assert!(audit.expired.is_empty(), "{audit:?}");
    assert_eq!(stored(&envref, "data/b.txt").await?.status(), Status::Ready);

    let _ = read(&envref, "data/a.txt").await?;
    assert_eq!(
        stored(&envref, "data/b.txt").await?.status(),
        Status::Expired
    );
    Ok(())
}

/// A store that refuses every write.
struct ReadOnlyStore {
    inner: AsyncMemoryStore,
    refused_writes: AtomicUsize,
}

impl ReadOnlyStore {
    fn refuse(&self, key: &Key) -> Result<(), Error> {
        self.refused_writes.fetch_add(1, Ordering::SeqCst);
        Err(Error::key_not_supported(key, "read-only store"))
    }
}

#[async_trait]
impl AsyncStore for ReadOnlyStore {
    async fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), Error> {
        self.inner.get(key).await
    }
    async fn get_bytes(&self, key: &Key) -> Result<Vec<u8>, Error> {
        self.inner.get_bytes(key).await
    }
    async fn get_metadata(&self, key: &Key) -> Result<Metadata, Error> {
        self.inner.get_metadata(key).await
    }
    async fn set(&self, key: &Key, _data: &[u8], _metadata: &Metadata) -> Result<(), Error> {
        self.refuse(key)
    }
    async fn set_metadata(&self, key: &Key, _metadata: &Metadata) -> Result<(), Error> {
        self.refuse(key)
    }
    async fn remove(&self, key: &Key) -> Result<(), Error> {
        self.refuse(key)
    }
    async fn contains(&self, key: &Key) -> Result<bool, Error> {
        self.inner.contains(key).await
    }
    async fn is_dir(&self, key: &Key) -> Result<bool, Error> {
        self.inner.is_dir(key).await
    }
    async fn listdir(&self, key: &Key) -> Result<Vec<String>, Error> {
        self.inner.listdir(key).await
    }
}

/// Pitfall 7: against a read-only store the change is accepted in memory, the failed write is
/// logged, and the read succeeds.
#[tokio::test]
async fn read_only_store_accepts_in_memory_and_does_not_fail_the_read() -> TestResult {
    let snapshot = computed_chain(A_V1).await?;
    let store = Arc::new(ReadOnlyStore {
        inner: replayed(&snapshot, &[("data/a.txt", A_V2)]).await?,
        refused_writes: AtomicUsize::new(0),
    });
    let envref = env_with(store.clone(), AssetManagerOptions::default())?;
    let v2 = Version::from_content(A_V2);

    let b = within(envref.get_asset_manager().get(&key("data/b.txt"))).await?;
    let _ = within(b.get()).await?;
    assert_eq!(read(&envref, "data/a.txt").await?, "hello, edited by hand");
    assert_eq!(
        envref
            .get_asset_manager()
            .version(&key("data/a.txt"))
            .await?,
        v2
    );
    assert!(
        store.refused_writes.load(Ordering::SeqCst) > 0,
        "the write was attempted"
    );
    assert_eq!(
        stored(&envref, "data/a.txt").await?.version(),
        Some(Version::from_content(A_V1)),
        "the store is unchanged"
    );
    assert_eq!(
        b.status().await,
        Status::Expired,
        "the cascade happened in memory"
    );
    // A second read neither fails nor applies the change again.
    let before = store.refused_writes.load(Ordering::SeqCst);
    assert!(within(
        envref
            .get_asset_manager()
            .get_any_status(&key("data/a.txt"))
    )
    .await?
    .is_some());
    assert_eq!(read(&envref, "data/a.txt").await?, "hello, edited by hand");
    assert_eq!(store.refused_writes.load(Ordering::SeqCst), before);
    Ok(())
}

/// Corner case 1: verification hashes the bytes the manager already read; the value is read from
/// the store once, verified or changed.
#[tokio::test]
async fn verify_reads_the_store_once() -> TestResult {
    for (bytes, recorded) in [
        (A_V1, Version::from_content(A_V1)), // verified
        (A_V2, Version::from_content(A_V1)), // changed outside Liquers
    ] {
        let mut record = provenance_text_metadata();
        record.status = Status::Source;
        record.version = Some(recorded);
        let inner = AsyncMemoryStore::new(&Key::new());
        inner
            .set(&key("data/a.txt"), bytes, &Metadata::MetadataRecord(record))
            .await?;
        let store = CountingStore::new(inner);
        let envref = env_with(Arc::new(store.clone()), AssetManagerOptions::default())?;

        let before = store.byte_reads();
        let _ = read(&envref, "data/a.txt").await?;
        assert_eq!(store.byte_reads() - before, 1, "one value read on load");

        let before = store.byte_reads();
        let _ = within(envref.get_asset_manager().verify_stored_versions(
            &key("data/a.txt"),
            false,
            AuditMode::ReportOnly,
        ))
        .await?;
        assert_eq!(
            store.byte_reads() - before,
            1,
            "one value read per swept key"
        );
    }
    Ok(())
}

fn temp_root(tag: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    std::env::temp_dir().join(format!("lq-external-change-{tag}-{nanos}"))
}

/// Corner case 3: metadata kept, data object deleted (the exploratory workflow) — nothing to hash,
/// so the key is skipped, not reported changed, and nothing is touched.
#[tokio::test]
async fn missing_bytes_are_skipped() -> TestResult {
    let root = temp_root("missing");
    tokio::fs::create_dir_all(&root).await?;
    let outcome = async {
        let store = AsyncFileStore::new(root.to_string_lossy().as_ref(), &Key::new());
        for (name, bytes) in [
            ("data/ghost.txt", b"gone".as_slice()),
            ("data/kept.txt", b"kept"),
        ] {
            let mut record = provenance_text_metadata();
            record.status = Status::Source;
            record.version = Some(Version::from_content(bytes));
            store
                .set(&key(name), bytes, &Metadata::MetadataRecord(record))
                .await?;
        }
        tokio::fs::remove_file(store.key_to_path(&key("data/ghost.txt"))?).await?;
        let store: Arc<dyn AsyncStore> = Arc::new(store);
        let envref = env_with(store.clone(), AssetManagerOptions::default())?;
        let am = envref.get_asset_manager();

        let report =
            within(am.verify_stored_versions(&key("data/ghost.txt"), false, AuditMode::Expire))
                .await?;
        assert_eq!(report.skipped, vec![key("data/ghost.txt")]);
        assert!(
            report.changed.is_empty() && report.verified.is_empty(),
            "{report:?}"
        );

        let report =
            within(am.verify_stored_versions(&key("data"), true, AuditMode::Expire)).await?;
        assert!(report.changed.is_empty(), "{report:?}");
        assert!(
            report.verified.contains(&key("data/kept.txt")),
            "{report:?}"
        );
        assert!(
            !report.verified.contains(&key("data/ghost.txt")),
            "{report:?}"
        );
        assert!(
            store.contains(&key("data/ghost.txt")).await?,
            "the metadata is kept"
        );
        assert_eq!(
            store.get_metadata(&key("data/ghost.txt")).await?.version(),
            Some(Version::from_content(b"gone"))
        );
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    let _ = tokio::fs::remove_dir_all(&root).await;
    outcome?;

    // A metadata-only memory entry has no data object either (`sidecar05`), so it is skipped
    // whatever version it records — here a content hash, which the former empty-bytes heuristic
    // would have reported as changed outside Liquers.
    let store = AsyncMemoryStore::new(&Key::new());
    let mut record = provenance_text_metadata();
    record.status = Status::Ready;
    record.version = Some(Version::from_content(b"never stored"));
    store
        .set_metadata(
            &key("data/unserializable.txt"),
            &Metadata::MetadataRecord(record),
        )
        .await?;
    let envref = env_with(Arc::new(store), AssetManagerOptions::default())?;
    let report = within(envref.get_asset_manager().verify_stored_versions(
        &key("data"),
        true,
        AuditMode::Expire,
    ))
    .await?;
    assert_eq!(
        report.skipped,
        vec![key("data/unserializable.txt")],
        "{report:?}"
    );
    assert!(
        within(
            envref
                .get_asset_manager()
                .get_binary_any_status(&key("data/unserializable.txt"))
        )
        .await?
        .is_none(),
        "there are no bytes to recover, and the read says so rather than failing"
    );
    assert_eq!(
        stored(&envref, "data/unserializable.txt").await?.status(),
        Status::Ready
    );
    Ok(())
}

/// An empty data object is bytes: it is checked, not skipped.
#[tokio::test]
async fn empty_data_object_is_checked_not_skipped() -> TestResult {
    let envref = env_with(
        Arc::new(AsyncMemoryStore::new(&Key::new())),
        AssetManagerOptions::default(),
    )?;
    let am = envref.get_asset_manager();
    within(am.set_binary(&key("data/empty.txt"), b"", provenance_text_metadata())).await?;
    let report =
        within(am.verify_stored_versions(&key("data/empty.txt"), false, AuditMode::ReportOnly))
            .await?;
    assert_eq!(report.verified, vec![key("data/empty.txt")], "{report:?}");

    // Emptied outside Liquers: the empty object no longer matches the recorded version.
    within(am.set_binary(
        &key("data/full.txt"),
        b"content",
        provenance_text_metadata(),
    ))
    .await?;
    edit_outside_liquers(&*envref.get_async_store(), "data/full.txt", b"").await?;
    let report =
        within(am.verify_stored_versions(&key("data/full.txt"), false, AuditMode::ReportOnly))
            .await?;
    assert_eq!(
        report.changed,
        vec![(
            key("data/full.txt"),
            ExternalChangeAction::AcceptAsInput {
                actual: Version::from_content(b"")
            }
        )]
    );
    assert!(report.skipped.is_empty(), "{report:?}");

    // An empty data object under a timestamp version is content too. The former heuristic took
    // it for a metadata-only entry and skipped it; now only a missing data object is skipped.
    let mut record = provenance_text_metadata();
    record.status = Status::Ready;
    record.version = Some(Version::new_unique());
    envref
        .get_async_store()
        .set(&key("data/stamped.txt"), b"", &Metadata::MetadataRecord(record))
        .await?;
    let report = within(am.verify_stored_versions(
        &key("data/stamped.txt"),
        false,
        AuditMode::ReportOnly,
    ))
    .await?;
    assert!(report.skipped.is_empty(), "{report:?}");
    Ok(())
}

/// Delegates to a memory store; the first `READERS` reads of `data/a.txt` wait for each other
/// after reading, so every reader holds the old sidecar before any of them applies the change.
struct GatedStore {
    inner: AsyncMemoryStore,
    gate: tokio::sync::Barrier,
    arrivals: AtomicUsize,
}

const READERS: usize = 6;

impl GatedStore {
    async fn pass_gate(&self, k: &Key) {
        if *k == key("data/a.txt") && self.arrivals.fetch_add(1, Ordering::SeqCst) < READERS {
            self.gate.wait().await;
        }
    }
}

#[async_trait]
impl AsyncStore for GatedStore {
    async fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), Error> {
        let result = self.inner.get(key).await;
        self.pass_gate(key).await;
        result
    }
    async fn get_bytes(&self, key: &Key) -> Result<Vec<u8>, Error> {
        self.inner.get_bytes(key).await
    }
    async fn get_metadata(&self, key: &Key) -> Result<Metadata, Error> {
        self.inner.get_metadata(key).await
    }
    async fn set(&self, key: &Key, data: &[u8], metadata: &Metadata) -> Result<(), Error> {
        self.inner.set(key, data, metadata).await
    }
    async fn set_metadata(&self, key: &Key, metadata: &Metadata) -> Result<(), Error> {
        self.inner.set_metadata(key, metadata).await
    }
    async fn remove(&self, key: &Key) -> Result<(), Error> {
        self.inner.remove(key).await
    }
    async fn contains(&self, key: &Key) -> Result<bool, Error> {
        self.inner.contains(key).await
    }
    async fn is_dir(&self, key: &Key) -> Result<bool, Error> {
        self.inner.is_dir(key).await
    }
    async fn listdir(&self, key: &Key) -> Result<Vec<String>, Error> {
        self.inner.listdir(key).await
    }
}

/// Corner case 2: many readers of one hand-edited key apply the change once — one log line, one
/// cascade — because `apply_external_change` re-reads the stored version under
/// `key_mutation_lock`. The gate makes the race certain: every reader has read the old sidecar
/// and decided to apply before the first one does.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_reads_apply_external_change_once() -> TestResult {
    let snapshot = computed_chain(A_V1).await?;
    let store = Arc::new(GatedStore {
        inner: replayed(&snapshot, &[("data/a.txt", A_V2)]).await?,
        gate: tokio::sync::Barrier::new(READERS),
        arrivals: AtomicUsize::new(0),
    });
    let envref = env_with(store, AssetManagerOptions::default())?;
    let _ = read(&envref, "data/b.txt").await?; // edge a -> b @V1
    let v2 = Version::from_content(A_V2);

    // The store branches of the recovery reads hold no asset lock, so each reader reaches
    // `apply_external_change` on its own.
    let mut tasks = Vec::new();
    for i in 0..READERS {
        let envref = envref.clone();
        tasks.push(tokio::spawn(async move {
            let am = envref.get_asset_manager();
            let a = key("data/a.txt");
            if i % 2 == 0 {
                am.get_binary_any_status(&a)
                    .await
                    .map(|found| found.is_some())
            } else {
                am.get_any_status(&a).await.map(|found| found.is_some())
            }
        }));
    }
    for task in tasks {
        assert!(within(task).await??, "every read returns the value");
    }
    // A later load through the fast track finds the change applied and applies nothing.
    assert_eq!(read(&envref, "data/a.txt").await?, "hello, edited by hand");

    let a = stored(&envref, "data/a.txt").await?;
    assert_eq!(a.version(), Some(v2));
    let lines = log_of(&a)
        .into_iter()
        .filter(|m| m.contains("changed outside Liquers"))
        .count();
    assert_eq!(lines, 1, "applied once: {:?}", log_of(&a));
    let b = stored(&envref, "data/b.txt").await?;
    assert_eq!(
        b.expiry_reason(),
        updated_in_store(v2, "data/a.txt", "data/a.txt")
    );
    let expiry_lines = log_of(&b)
        .into_iter()
        .filter(|m| m.starts_with("data/b.txt expired:"))
        .count();
    assert_eq!(expiry_lines, 1, "one cascade: {:?}", log_of(&b));
    Ok(())
}
