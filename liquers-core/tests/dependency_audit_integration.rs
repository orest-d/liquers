//! Dependency audit across environments and restarts
//! (`specs/design/dependency-audit-and-expiry-provenance/`, Phase 3 I1).
//!
//! Step 4 of Phase 4 adds the first test here; the audit itself arrives with later steps.

mod common;
mod fixtures;

use liquers_core::{
    assets::AssetManager,
    context::{EnvRef, Environment, SimpleEnvironment},
    metadata::{DependencyKey, ExpiryCause, ExpiryReason, Status},
    parse::parse_key,
    query::Key,
    recipes::DefaultRecipeProvider,
    store::AsyncMemoryStore,
    value::Value,
};

use common::manager_scenarios::{
    provenance_evaluate_chain, provenance_store, register_provenance_commands,
};
use fixtures::StoreSnapshot;

type TestEnv = SimpleEnvironment<Value>;

fn env_over(store: AsyncMemoryStore) -> EnvRef<TestEnv> {
    let mut env = TestEnv::new();
    register_provenance_commands(&mut env.command_registry);
    env.with_async_store(Box::new(store));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    env.to_ref()
}

/// Corner case 5: a reason one environment persisted is what a second environment over the same
/// stored data reads — through the store and through `get_asset_info` — with nothing live in it.
#[tokio::test]
async fn two_envs_share_persisted_reason() -> Result<(), Box<dyn std::error::Error>> {
    let a = parse_key("data/a.txt")?;
    let b = parse_key("data/b.txt")?;
    let report = parse_key("data/report.txt")?;
    let keys: Vec<Key> = vec![parse_key("data/recipes.yaml")?, a.clone(), b.clone(), report.clone()];

    let snapshot = {
        let envref = env_over(provenance_store(true).await?);
        provenance_evaluate_chain(&envref).await?;
        envref.get_asset_manager().expire(&a).await?;
        StoreSnapshot::capture(&envref.get_async_store(), &keys).await?
    };

    let second_store = AsyncMemoryStore::new(&Key::new());
    snapshot.replay_into(&second_store).await?;
    let envref2 = env_over(second_store);
    let am2 = envref2.get_asset_manager();

    let root = DependencyKey::from(&a);
    let expected: [(&Key, ExpiryReason); 3] = [
        (
            &a,
            ExpiryReason::Direct {
                cause: ExpiryCause::Explicit,
            },
        ),
        (
            &b,
            ExpiryReason::Cascaded {
                cause: ExpiryCause::Explicit,
                root: root.clone(),
                via: root.clone(),
            },
        ),
        (
            &report,
            ExpiryReason::Cascaded {
                cause: ExpiryCause::Explicit,
                root,
                via: DependencyKey::from(&b),
            },
        ),
    ];
    for (key, reason) in expected {
        let stored = envref2.get_async_store().get_metadata(key).await?;
        assert_eq!(stored.status(), Status::Expired, "{key}");
        assert_eq!(stored.expiry_reason(), Some(reason.clone()), "{key}");
        let info = am2.get_asset_info(key).await?;
        assert_eq!(info.status, Status::Expired, "{key}");
        assert_eq!(info.expiry_reason, Some(reason), "{key}: AssetInfo projects the reason");
    }
    Ok(())
}
