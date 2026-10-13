//! Stack depth independent of recipe-chain length, on both asset managers
//! (`specs/design/dependency-resolved-once-per-evaluation/`, Phase 3, AC-9 to AC-14).
//!
//! Its own test binary: a stack overflow aborts the process, and would hide every other test in
//! the same binary. Deep tests run on a 1 MiB thread. The queued ones build a `current_thread`
//! runtime on that thread, because a multi-thread runtime would run their tasks on 2 MiB workers.

mod common;

use std::time::Duration;

use common::chain_fixture::{chain, count, env_with, environment, outcome, r, recipe_store, Counters, R};
use liquers_core::{
    assets::AssetManager,
    environment_builder::{AssetManagerKind, Inline, Queued},
    metadata::Status,
    parse::parse_key,
};

type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

const SMALL_STACK: usize = 1 << 20;

fn on_small_stack<F>(f: F) -> TestResult
where
    F: FnOnce() -> TestResult + Send + 'static,
{
    std::thread::Builder::new()
        .stack_size(SMALL_STACK)
        .spawn(f)?
        .join()
        .map_err(|_| "the test thread panicked")?
}

fn current_thread_runtime() -> Result<tokio::runtime::Runtime, std::io::Error> {
    tokio::runtime::Builder::new_current_thread().enable_all().build()
}

fn boxed(e: impl std::fmt::Display) -> Box<dyn std::error::Error + Send + Sync> {
    e.to_string().into()
}

/// AC-9: a cold 100-link chain on the queued manager, one link per task.
fn queued_cold_chain(capacity: Option<usize>) -> TestResult {
    on_small_stack(move || {
        current_thread_runtime()?.block_on(async move {
            let (envref, counters) = env_with::<Queued>(&chain("make_text", 100), capacity)
                .await
                .map_err(boxed)?;
            let text = tokio::time::timeout(Duration::from_secs(60), outcome(&envref, "-R/data/l100.txt"))
                .await?
                .map_err(boxed)?;
            assert_eq!(text, "TEXT");
            assert_eq!(count(&counters.upper), 100);
            assert_eq!(count(&counters.make_text), 1);
            Ok(())
        })
    })
}

#[test]
fn queued_cold_100_link_chain_on_1mib_stack() -> TestResult {
    queued_cold_chain(None)
}

#[test]
fn queued_capacity_one_cold_100_link_chain_on_1mib_stack() -> TestResult {
    queued_cold_chain(Some(1))
}

/// AC-1, AC-9: the failing 100-link chain on the queued manager runs its root once.
#[test]
fn queued_failing_root_of_100_link_chain_runs_once() -> TestResult {
    on_small_stack(|| {
        current_thread_runtime()?.block_on(async {
            let (envref, counters) = env_with::<Queued>(&chain("fail", 100), None).await.map_err(boxed)?;
            let result = tokio::time::timeout(Duration::from_secs(60), outcome(&envref, "-R/data/l100.txt")).await?;
            assert!(matches!(&result, Err(m) if m.contains("leaf failed")), "{result:?}");
            assert_eq!(count(&counters.fail), 1);
            Ok(())
        })
    })
}

/// AC-10: a cold 100-link chain on the inline manager, with no tokio runtime at all.
#[test]
fn immediate_cold_100_link_chain_without_runtime_on_1mib_stack() -> TestResult {
    on_small_stack(|| {
        futures::executor::block_on(async {
            let (envref, counters) = env_with::<Inline>(&chain("make_text", 100), None).await.map_err(boxed)?;
            let text = outcome(&envref, "-R/data/l100.txt").await.map_err(boxed)?;
            assert_eq!(text, "TEXT");
            assert_eq!(count(&counters.upper), 100);
            Ok(())
        })
    })
}

/// AC-14: a cold failing 100-link chain on the inline manager fails at once, running its root once.
#[test]
fn immediate_failing_root_of_100_link_chain_runs_once() -> TestResult {
    on_small_stack(|| {
        futures::executor::block_on(async {
            let (envref, counters) = env_with::<Inline>(&chain("fail", 100), None).await.map_err(boxed)?;
            let result = outcome(&envref, "-R/data/l100.txt").await;
            assert!(matches!(&result, Err(m) if m.contains("leaf failed")), "{result:?}");
            assert_eq!(count(&counters.fail), 1);
            assert_eq!(count(&counters.upper), 0);
            if let Some(l1) = envref.get_asset_manager().lookup_key_asset(&parse_key("data/l1.txt").map_err(boxed)?) {
                let status = l1.status().await;
                assert!(!matches!(status, Status::Ready | Status::Error), "l1 is {status:?}");
            }
            Ok(())
        })
    })
}

/// AC-11: with `l0..l99` persisted, a fresh inline environment evaluates only `l100`.
#[test]
fn immediate_warm_upstream_is_not_reloaded() -> TestResult {
    on_small_stack(|| {
        futures::executor::block_on(async {
            let store = recipe_store(&chain("make_text", 100)).await.map_err(boxed)?;
            {
                // Persist l0..l99 link by link, so this setup never nests.
                let warm = environment::<Inline>(store.clone(), None, &Counters::default()).map_err(boxed)?;
                for i in 0..100 {
                    outcome(&warm, &format!("-R/data/l{i}.txt")).await.map_err(boxed)?;
                }
            }
            let counters = Counters::default();
            let envref = environment::<Inline>(store, None, &counters).map_err(boxed)?;
            let text = outcome(&envref, "-R/data/l100.txt").await.map_err(boxed)?;
            assert_eq!(text, "TEXT");
            assert_eq!(count(&counters.upper), 1, "only l100 ran");
            let manager = envref.get_asset_manager();
            for i in 0..=98 {
                let key = parse_key(&format!("data/l{i}.txt")).map_err(boxed)?;
                assert!(manager.lookup_key_asset(&key).is_none(), "l{i} became a live asset");
            }
            Ok(())
        })
    })
}

fn cycle_recipes() -> Vec<R> {
    let mut recipes = vec![r("-R/data/l40.txt/-/upper/l0.txt")];
    for i in 1..=40 {
        recipes.push(r(format!("-R/data/l{}.txt/-/upper/l{i}.txt", i - 1)));
    }
    recipes
}

async fn cycle_scenario<K: AssetManagerKind>() -> TestResult {
    let (envref, _counters) = env_with::<K>(&cycle_recipes(), None).await.map_err(boxed)?;
    let result = tokio::time::timeout(Duration::from_secs(20), outcome(&envref, "-R/data/l5.txt")).await?;
    assert!(
        matches!(&result, Err(m) if m.contains("Circular dependency detected")),
        "{result:?}"
    );
    Ok(())
}

/// AC-12: a recipe cycle is still reported by both managers, without hanging or overflowing.
#[tokio::test(flavor = "multi_thread")]
async fn recipe_cycle_is_reported_by_both_managers() -> TestResult {
    cycle_scenario::<Queued>().await?;
    cycle_scenario::<Inline>().await
}

/// AC-13: at capacity 1, cancelling a dependent lets its locally queued dependency finish, without
/// running the dependency's command a second time.
#[tokio::test(flavor = "multi_thread")]
async fn queued_cancelled_dependent_lets_dependency_finish() -> TestResult {
    let (envref, counters) = env_with::<Queued>(&chain("gated_text", 2), Some(1)).await.map_err(boxed)?;
    let l2 = envref.evaluate("-R/data/l2.txt").await?;
    let entered = async {
        while count(&counters.gated) == 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(20), entered).await?;
    l2.cancel().await?;
    counters.latch.open();

    let manager = envref.get_asset_manager();
    let l1 = manager.lookup_key_asset(&parse_key("data/l1.txt")?).ok_or("l1 is not live")?;
    let state = tokio::time::timeout(Duration::from_secs(20), l1.get()).await??;
    assert_eq!(state.try_into_string()?, "TEXT");
    assert_eq!(l1.status().await, Status::Ready);
    assert_eq!(l2.status().await, Status::Cancelled);
    assert_eq!(count(&counters.gated), 1, "the gated command entered once");
    Ok(())
}
