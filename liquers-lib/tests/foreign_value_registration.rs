//! `fvt7` — a value type known only to an integration can be registered and stored.
//!
//! Regression suite for `specs/issues/FOREIGN-VALUE-TYPES-NOT-REGISTERED.md`. Since
//! `value-type-system` step 6 the write path refuses any identifier the `TypeRegistry` does not
//! contain, and a foreign value — `ExtValue::Foreign`, holding a JavaScript, Python or Starlark
//! handle — supplies its identifier at *runtime*, so it could never appear in the **static**
//! `ValueInterface::type_descriptions()` the registry is seeded from. It therefore could not be
//! stored at all.
//!
//! The fix is that an integration extends the base registry and hands the finished registry to an
//! environment constructor. These tests exercise that natively with a mock `ForeignValue`, which
//! is what makes the guarantee checkable without a `wasm32` toolchain — `liquers-web`'s `JsOpaque`
//! is the real implementation and is covered by that crate's own suite.
//!
//! See `specs/design/foreign-value-type-registration/`.

use std::borrow::Cow;
use std::sync::Arc;

use liquers_core::assets::AssetManager;
use liquers_core::context::Environment;
use liquers_core::error::ErrorType;
use liquers_core::metadata::{Metadata, MetadataRecord, Status};
use liquers_core::parse::parse_key;
use liquers_core::query::Key;
use liquers_core::state::State;
use liquers_core::store::{AsyncMemoryStore, AsyncStore};
use liquers_core::type_system::{TypeInfo, TypeRegistry};
use liquers_core::value::ValueInterface;
use liquers_lib::environment::DefaultEnvironment;
use liquers_lib::value::foreign::ForeignValue;
use liquers_lib::value::{ExtValue, Value};

/// Deliberately **not** `js.Value`: that identifier belongs to `liquers-web`, and a test in this
/// crate must not depend on a name another crate owns. A provider prefix keeps it inside the
/// naming rule — a bare name would assert that Liquers owns the concept.
const MOCK_TYPE_IDENTIFIER: &str = "mock.Value";

/// A language handle with no byte form — the ordinary shape, and what `JsOpaque` is.
#[derive(Debug)]
struct MockForeign;

impl ForeignValue for MockForeign {
    fn origin(&self) -> &'static str {
        "mock"
    }
    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
    fn identifier(&self) -> Cow<'static, str> {
        MOCK_TYPE_IDENTIFIER.into()
    }
    fn type_name(&self) -> Cow<'static, str> {
        "MockObject".into()
    }
    fn default_extension(&self) -> Cow<'static, str> {
        "json".into()
    }
    fn default_filename(&self) -> Cow<'static, str> {
        "value.json".into()
    }
    fn default_media_type(&self) -> Cow<'static, str> {
        "application/json".into()
    }
    // `type_info()` is not implemented: the default derives it from the six methods above and
    // declares no data formats, which is exactly right for a handle `as_bytes` refuses to encode.
}

fn foreign_value() -> Value {
    Value::new_extended(ExtValue::Foreign {
        value: Arc::new(MockForeign),
    })
}

/// A state carrying the foreign value, with metadata naming its type.
fn foreign_state(key: &Key) -> State<Value> {
    let value = foreign_value();
    let mut record = MetadataRecord::new();
    record
        .with_key(key.clone())
        .with_type_identifier(value.identifier().to_string())
        .with_type_name(value.type_name().to_string())
        .with_status(Status::Ready);
    State::new()
        .with_data(value)
        .with_metadata(Metadata::MetadataRecord(record))
}

fn store() -> Box<dyn AsyncStore> {
    Box::new(AsyncMemoryStore::new(&Key::new()))
}

/// An environment that has never heard of the mock type — the state of the world before the fix.
fn environment_without_the_mock() -> DefaultEnvironment<Value> {
    let mut env = DefaultEnvironment::<Value>::new();
    env.with_async_store(store());
    env
}

/// An environment whose registry was extended with the mock type at construction.
fn environment_knowing_the_mock() -> Result<DefaultEnvironment<Value>, Box<dyn std::error::Error>> {
    let mut types = TypeRegistry::from_value_type::<Value>();
    types.register(MockForeign.type_info())?;

    let mut env = DefaultEnvironment::<Value>::new_with_type_registry(types);
    env.with_async_store(store());
    Ok(env)
}

/// `fvt7.1` — an unregistered foreign value is still refused.
///
/// This records a **decision**, not the fix: the design deliberately keeps the hard refusal rather
/// than restoring the pre-`value-type-system` degrade-to-metadata behaviour. That behaviour hid
/// exactly the mistake the fix makes correctable, and an integration that forgets to register its
/// type should hear about it at once rather than discover months of assets carrying an identifier
/// nothing can resolve. So this test passes before and after — it exists to stop the refusal
/// being softened later without a decision.
#[tokio::test]
async fn an_unregistered_foreign_value_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let envref = environment_without_the_mock().to_ref();
    let key = parse_key("test/value.json")?;

    let error = envref
        .get_asset_manager()
        .set_state(&key, foreign_state(&key))
        .await
        .expect_err("an identifier the registry does not contain must be refused");

    assert_eq!(error.error_type, ErrorType::General);
    Ok(())
}

/// `fvt7.2` — a registered foreign value can be stored. **This is the fix.**
///
/// Before the change this test could not be written: `new_with_type_registry` did not exist, so
/// there was no way to tell a build about a type its value type cannot describe.
#[tokio::test]
async fn a_registered_foreign_value_can_be_stored() -> Result<(), Box<dyn std::error::Error>> {
    let envref = environment_knowing_the_mock()?.to_ref();
    let key = parse_key("test/value.json")?;

    envref
        .get_asset_manager()
        .set_state(&key, foreign_state(&key))
        .await?;
    Ok(())
}

/// `fvt7.3` — and it persists as metadata only.
///
/// The issue claims a foreign value should degrade to metadata-only persistence rather than fail,
/// and nothing verified that claim. It is checked here rather than assumed: the bytes are empty
/// because `as_bytes` refuses, while the metadata retains the identifier, which is what lets a
/// later read report *which* type it cannot materialize instead of reporting nothing.
#[tokio::test]
async fn a_registered_foreign_value_persists_as_metadata_only(
) -> Result<(), Box<dyn std::error::Error>> {
    let env = environment_knowing_the_mock()?;
    let store = env.get_async_store();
    let envref = env.to_ref();
    let key = parse_key("test/value.json")?;

    envref
        .get_asset_manager()
        .set_state(&key, foreign_state(&key))
        .await?;

    let metadata = store.get_metadata(&key).await?;
    assert_eq!(
        metadata.type_identifier()?,
        MOCK_TYPE_IDENTIFIER,
        "the stored metadata names the type, so a reader can say what it cannot materialize"
    );

    let (data, _) = store.get(&key).await?;
    assert!(
        data.is_empty(),
        "a value whose as_bytes refuses stores no bytes, got {} of them",
        data.len()
    );
    Ok(())
}

/// `fvt7.4` — the refusal names the identifier.
///
/// The diagnostic is the whole value of refusing rather than degrading: a message that does not
/// name the type leaves an integration author with nothing to act on.
#[tokio::test]
async fn the_refusal_names_the_identifier() -> Result<(), Box<dyn std::error::Error>> {
    let envref = environment_without_the_mock().to_ref();
    let key = parse_key("test/value.json")?;

    let error = envref
        .get_asset_manager()
        .set_state(&key, foreign_state(&key))
        .await
        .expect_err("an unregistered identifier must be refused");

    assert!(
        error.message.contains(MOCK_TYPE_IDENTIFIER),
        "the message names the identifier: {error}"
    );
    assert!(
        error.message.contains("not registered"),
        "and says what is wrong with it: {error}"
    );
    Ok(())
}

/// `fvt7.5` — starting from an empty registry loses every type the build already had.
///
/// The pitfall the constructor's doc comment warns about. `TypeRegistry::new()` is empty;
/// `from_value_type` is what adds the value type's own descriptions. Build on the wrong one and
/// the failure is delayed and confusing: the foreign type works, and then storing an *ordinary*
/// value — text, an image, anything the build has always supported — is refused.
#[tokio::test]
async fn an_empty_base_registry_loses_the_ordinary_types(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut types = TypeRegistry::new();
    types.register(MockForeign.type_info())?;
    assert!(
        !types.contains("Text"),
        "the premise: an empty base describes nothing but what was just added"
    );

    let mut env = DefaultEnvironment::<Value>::new_with_type_registry(types);
    env.with_async_store(store());
    let envref = env.to_ref();
    let key = parse_key("test/notes.txt")?;

    let state = State::<Value>::new().with_data(Value::new("hello"));
    let error = envref
        .get_asset_manager()
        .set_state(&key, state)
        .await
        .expect_err("an ordinary text value needs its type registered too");
    assert!(
        error.message.contains("'Text' is not registered"),
        "the message names the missing type: {error}"
    );
    Ok(())
}

/// An errored state is storable, and is typed by the value it holds rather than by its failure.
///
/// There is no `error` type: the type axis says what a value *is*, and an errored state holds
/// none. The failure is recorded in the metadata, and the asset persists as metadata with no
/// bytes — the same shape a foreign value gets, and for the same reason.
#[tokio::test]
async fn an_errored_state_is_stored_as_metadata_typed_none(
) -> Result<(), Box<dyn std::error::Error>> {
    let env = environment_knowing_the_mock()?;
    let store = env.get_async_store();
    let envref = env.to_ref();
    let key = parse_key("test/failed.json")?;

    let state = State::<Value>::from_error(liquers_core::error::Error::general_error(
        "something went wrong".to_owned(),
    ));
    envref.get_asset_manager().set_state(&key, state).await?;

    let metadata = store.get_metadata(&key).await?;
    assert_eq!(
        metadata.type_identifier()?,
        "None",
        "the type axis reports what is available, not what was intended"
    );
    assert!(metadata.is_error()?, "and the metadata carries the failure");

    let (data, _) = store.get(&key).await?;
    assert!(data.is_empty(), "with no bytes, since there is no value");
    Ok(())
}

/// The registry a real integration builds is the base plus its own type — not a replacement.
///
/// `fvt7.5` shows what the wrong shape costs; this shows the right one holds both.
#[test]
fn the_extended_registry_holds_both_the_base_and_the_addition(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut types = TypeRegistry::from_value_type::<Value>();
    types.register(MockForeign.type_info())?;

    assert!(types.contains(MOCK_TYPE_IDENTIFIER), "the integration's type");
    assert!(types.contains("Text"), "the base value type's types");
    assert!(types.contains("Image"), "the extension's types");
    assert!(
        !types.contains("error"),
        "and no error type — a failure is metadata, not a type a value can have"
    );
    Ok(())
}
