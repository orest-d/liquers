//! `keyabs12`–`keyabs14` — the absolute-key precondition, end to end.
//!
//! The store-level refusal is unit-tested in `liquers_core::store`. What these tests add is that
//! the refusal is actually *reached* from the two places a relative key can originate — an
//! evaluated query and a recipe's `cwd` — and that nothing which legitimately used `.` or `..`
//! before has stopped working.
//!
//! See `specs/design/store-key-guard/` and `specs/issues/STORE-FILESTORE-PATH-TRAVERSAL.md`.

use liquers_core::{
    context::{Environment, SimpleEnvironment},
    error::ErrorType,
    interpreter::evaluate,
    metadata::{Metadata, MetadataRecord, Status},
    parse::{parse_key, parse_query},
    query::Key,
    recipes::Recipe,
    store::{AsyncFileStore, AsyncMemoryStore, AsyncStore},
    value::Value,
};

fn unique_temp_dir(label: &str) -> std::path::PathBuf {
    let unique = format!(
        "liquers_{}_{}",
        label,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    );
    std::env::temp_dir().join(unique)
}

fn source_metadata(key: &Key) -> Metadata {
    let mut record = MetadataRecord::new();
    record
        .with_key(key.clone())
        .with_type_identifier("text".to_owned())
        .with_status(Status::Source);
    Metadata::MetadataRecord(record)
}

/// `keyabs12` — the reported vulnerability, reproduced through a real evaluation and through the
/// direct store call that the HTTP store API makes.
///
/// The two entry points do **not** behave alike, and the difference is worth pinning because it is
/// not what `STORE-FILESTORE-PATH-TRAVERSAL` describes:
///
/// - **Through `evaluate`, a *leading* `..` never reaches the store.** With no CWD in scope the
///   working-key cursor defaults to the logical root and resolves it there, so `-R/../SECRET.txt`
///   arrives as `SECRET.txt` — inside the root, harmless, and refused later as simply absent. That
///   path was never the exploitable one.
/// - **An *interior* `..` does reach the store**, because the cursor only resolves keys that
///   *start* with `.` or `..`; `data/../../SECRET.txt` needs no CWD, so nothing normalizes it.
///   This is the form that escaped, and it is the one an attacker writes.
/// - **The store API resolves nothing at all.** `liquers-axum`'s store handlers call `parse_key`
///   and hand the result straight to the store, so *both* forms arrive relative there. That is why
///   the rule belongs to the store rather than to query resolution.
///
/// `data/` is created deliberately: the kernel resolves `..` by walking real directories, so
/// without it an unguarded store would fail with `ENOENT` and this test would pass for the wrong
/// reason.
#[tokio::test]
async fn keyabs12_traversal_is_refused_and_reads_nothing() -> Result<(), Box<dyn std::error::Error>>
{
    let sandbox = unique_temp_dir("keyabs12");
    let root = sandbox.join("root");
    tokio::fs::create_dir_all(root.join("data")).await?;
    let secret = sandbox.join("SECRET.txt");
    let original = b"outside the store root".to_vec();
    tokio::fs::write(&secret, &original).await?;

    let store = AsyncFileStore::new(root.to_string_lossy().as_ref(), &Key::new());
    let inside = parse_key("data/ok.txt")?;
    store
        .set(&inside, b"inside the store root", &source_metadata(&inside))
        .await?;

    // The direct store call — what the HTTP store API does. Both forms are refused here.
    for text in ["../SECRET.txt", "data/../../SECRET.txt"] {
        let key = parse_key(text)?;
        let error = store
            .get_bytes(&key)
            .await
            .expect_err("the store must refuse a relative key");
        assert_eq!(error.error_type, ErrorType::KeyNotAbsolute, "{text}");
    }

    let mut env = SimpleEnvironment::<Value>::new();
    env.with_async_store(Box::new(store));
    let envref = env.to_ref();

    // The control: an ordinary query still works, so a failure below is the guard, not the setup.
    let state = evaluate(envref.clone(), "-R/data/ok.txt", None).await?;
    assert_eq!(state.try_into_string()?, "inside the store root");

    // The query is well-formed in both cases; `..` is still a legal `ResourceName` and the
    // language is deliberately unchanged.
    for text in ["-R/../SECRET.txt", "-R/data/../../SECRET.txt"] {
        parse_query(text).unwrap_or_else(|e| panic!("{text} must still parse: {}", e.message));
    }

    // Interior `..`: nothing resolves it, so the store is what refuses it.
    let error = evaluate(envref.clone(), "-R/data/../../SECRET.txt", None)
        .await
        .expect_err("an interior `..` must be refused");
    assert_eq!(
        error.error_type,
        ErrorType::KeyNotAbsolute,
        "interior `..`: {}",
        error.message
    );

    // Leading `..`: resolved against the defaulted root before the store sees it, so it addresses
    // `SECRET.txt` *inside* the root — which does not exist. It must not be `KeyNotAbsolute` (the
    // store never saw a relative key) and it must not return the file outside the root.
    match evaluate(envref.clone(), "-R/../SECRET.txt", None).await {
        Ok(state) => {
            let text = state.try_into_string().unwrap_or_default();
            assert_ne!(text, "outside the store root", "READ THE FILE OUTSIDE THE ROOT");
            panic!("expected a lookup failure inside the root, got {text:?}");
        }
        Err(e) => assert_ne!(
            e.error_type,
            ErrorType::KeyNotAbsolute,
            "a leading `..` is resolved against the root before the store sees it: {}",
            e.message
        ),
    }

    // Nothing outside the root was disturbed.
    assert_eq!(tokio::fs::read(&secret).await?, original);

    tokio::fs::remove_dir_all(&sandbox).await?;
    Ok(())
}

/// `keyabs13` — a recipe's `cwd` is the second source of a relative store key.
///
/// `Recipe.cwd` is an `Option<String>` that deserialization deliberately does not validate, and
/// `Recipe::store_to_key` joins the query filename onto whatever it parses to. A `recipes.yaml`
/// carrying `cwd: ../../etc` therefore produces a key with `..` elements that no CWD resolution
/// ever consumes.
///
/// Less severe than the query path — authoring `recipes.yaml` already requires store-write access
/// — but it is a second way in, and it is what the store-level rule catches that a query-level
/// check would not. Phase 1 of the design missed it; this test is why it cannot be missed again.
#[test]
fn keyabs13_recipe_with_relative_cwd_yields_a_refused_key() -> Result<(), Box<dyn std::error::Error>>
{
    // The derived filename comes from the *query*, not from the `filename:` field.
    let yaml = "filename: report.txt\nquery: hello/report.txt\ncwd: ../../etc\n";
    let recipe: Recipe = serde_yaml::from_str(yaml)?;

    // Deserialization accepts it — that is the documented behaviour, not a bug being asserted.
    let key = recipe
        .store_to_key()?
        .expect("a recipe with a filename and a cwd derives a key");
    assert!(
        key.is_relative(),
        "the recipe's cwd carries `..` into the derived key: {}",
        key.encode()
    );

    // …and the store is what refuses it.
    let error = key
        .as_absolute()
        .expect_err("a key derived from a relative cwd is not a store address");
    assert_eq!(error.error_type, ErrorType::KeyNotAbsolute);

    // A recipe with an ordinary cwd is unaffected.
    let ok: Recipe =
        serde_yaml::from_str("filename: report.txt\nquery: hello/report.txt\ncwd: data/reports\n")?;
    let ok_key = ok.store_to_key()?.expect("derives a key");
    assert_eq!(ok_key.encode(), "data/reports/report.txt");
    assert!(ok_key.as_absolute().is_ok());
    Ok(())
}

/// `keyabs14` — the regression surface: nothing that legitimately worked has stopped working.
///
/// The guard's real risk is not that it fails to refuse, but that it refuses something ordinary.
/// Dotted filenames are the ones most likely to be caught by a careless implementation: `.hidden`
/// begins with a dot and `a..b` contains two, and neither is a relative element.
#[tokio::test]
async fn keyabs14_ordinary_keys_and_dotted_names_still_work() -> Result<(), Box<dyn std::error::Error>>
{
    let store = AsyncMemoryStore::new(&Key::new());
    for text in [
        "data/report.txt",
        "data/.hidden",
        "data/a..b",
        "data/...",
        "data/..x",
        "archive/v1.2.3/notes.md",
    ] {
        let key = parse_key(text)?;
        assert!(store.is_supported(&key), "{text} must be routable");
        store
            .set(&key, text.as_bytes(), &source_metadata(&key))
            .await
            .unwrap_or_else(|e| panic!("{text} must be writable: {}", e.message));
        assert_eq!(store.get_bytes(&key).await?, text.as_bytes());
    }

    // The empty key is absolute — vacuously, no element is `.` or `..` — and must stay routable,
    // since it is a store's own prefix and listing it is the most ordinary call there is.
    let root = Key::new();
    assert!(!root.is_relative());
    assert!(root.as_absolute().is_ok());
    assert!(store.listdir(&root).await.is_ok());

    // CWD resolution still resolves; the store rule sits after it, not instead of it.
    let cwd = parse_key("data/reports")?;
    assert_eq!(parse_key("./x.txt")?.to_absolute(&cwd).encode(), "data/reports/x.txt");
    assert_eq!(parse_key("../x.txt")?.to_absolute(&cwd).encode(), "data/x.txt");
    assert!(parse_key("../x.txt")?.to_absolute(&cwd).as_absolute().is_ok());
    Ok(())
}
