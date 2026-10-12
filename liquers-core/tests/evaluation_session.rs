//! The evaluation session: within one evaluation each dependency resolves to one asset and runs
//! at most once (`specs/design/dependency-resolved-once-per-evaluation/`, Phase 3).

mod common;

use std::time::Duration;

use common::chain_fixture::{chain, count, env_with, lattice, outcome, r, volatile, R};
use liquers_core::{
    assets::AssetManager,
    environment_builder::{Inline, Queued},
    error::ErrorType,
    parse::{parse_key, parse_query},
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn failed_with_leaf(result: &Result<String, String>) -> bool {
    matches!(result, Err(message) if message.contains("leaf failed"))
}

/// AC-1: a failing root runs once, at the default queue capacity.
#[tokio::test(flavor = "multi_thread")]
async fn failing_chain_runs_root_once_at_default_capacity() -> TestResult {
    let (envref, counters) = env_with::<Queued>(&chain("fail", 10), None).await?;
    let result = tokio::time::timeout(Duration::from_secs(20), outcome(&envref, "-R/data/l10.txt")).await?;
    assert!(failed_with_leaf(&result), "{result:?}");
    assert_eq!(count(&counters.fail), 1);
    Ok(())
}

/// AC-1: the same with a single job slot, where every dependency is drained locally.
#[tokio::test(flavor = "multi_thread")]
async fn failing_chain_runs_root_once_at_capacity_one() -> TestResult {
    let (envref, counters) = env_with::<Queued>(&chain("fail", 10), Some(1)).await?;
    let result = tokio::time::timeout(Duration::from_secs(20), outcome(&envref, "-R/data/l10.txt")).await?;
    assert!(failed_with_leaf(&result), "{result:?}");
    assert_eq!(count(&counters.fail), 1);
    Ok(())
}

/// AC-2: a root shared by every node of a lattice runs once.
#[tokio::test(flavor = "multi_thread")]
async fn shared_failing_root_runs_once_in_lattice() -> TestResult {
    let (envref, counters) = env_with::<Queued>(&lattice(10), None).await?;
    let result = tokio::time::timeout(Duration::from_secs(20), outcome(&envref, "-R/data/a10.txt")).await?;
    assert!(failed_with_leaf(&result), "{result:?}");
    assert_eq!(count(&counters.fail), 1);
    Ok(())
}

/// AC-3: a 20-link failing chain finishes promptly.
#[tokio::test(flavor = "multi_thread")]
async fn long_failing_chain_finishes_promptly() -> TestResult {
    let (envref, counters) = env_with::<Queued>(&chain("fail", 20), None).await?;
    let result = tokio::time::timeout(Duration::from_secs(10), outcome(&envref, "-R/data/l20.txt")).await?;
    assert!(failed_with_leaf(&result), "{result:?}");
    assert_eq!(count(&counters.fail), 1);
    Ok(())
}

/// AC-5: a failure is not remembered: the next evaluation runs the failing root again.
#[tokio::test(flavor = "multi_thread")]
async fn failure_reruns_in_a_later_evaluation() -> TestResult {
    let (envref, counters) = env_with::<Queued>(&chain("fail", 3), None).await?;
    assert!(failed_with_leaf(&outcome(&envref, "-R/data/l3.txt").await));
    assert_eq!(count(&counters.fail), 1);
    assert!(failed_with_leaf(&outcome(&envref, "-R/data/l3.txt").await));
    assert_eq!(count(&counters.fail), 2);
    Ok(())
}

/// AC-6: a command requesting one dependency twice gets the same asset, and its error, twice.
#[tokio::test(flavor = "multi_thread")]
async fn command_reading_dependency_twice_gets_one_asset() -> TestResult {
    let recipes = vec![r("fail/l0.txt"), r("read_twice/t.txt")];
    let (envref, counters) = env_with::<Queued>(&recipes, None).await?;
    let text = tokio::time::timeout(Duration::from_secs(20), outcome(&envref, "-R/data/t.txt"))
        .await?
        .map_err(|e| format!("t failed: {e}"))?;
    let parts: Vec<&str> = text.split('|').collect();
    assert_eq!(parts.len(), 2, "{text}");
    assert!(parts.iter().all(|p| p.starts_with("err:") && p.contains("leaf failed")), "{text}");
    assert_eq!(count(&counters.fail), 1);
    Ok(())
}

fn volatile_recipes() -> Vec<R> {
    vec![
        volatile("tick/v.txt"),
        r("-R/data/v.txt/-/upper/a.txt"),
        r("-R/data/v.txt/-/upper/b.txt"),
        r("-R/data/a.txt/-/join-~X~-R/data/b.txt~E/c.txt"),
    ]
}

/// AC-7 (both managers): `a` and `b` read one value of the volatile `v`; a later evaluation that
/// needs `v` again runs it again.
async fn volatile_scenario<K: liquers_core::environment_builder::AssetManagerKind>() -> TestResult {
    let (envref, counters) = env_with::<K>(&volatile_recipes(), None).await?;
    let text = outcome(&envref, "-R/data/c.txt").await.map_err(|e| format!("c failed: {e}"))?;
    assert_eq!(count(&counters.tick), 1, "tick ran once");
    assert_eq!(count(&counters.upper), 2, "a and b ran once each");
    assert_eq!(count(&counters.join), 1);
    let (left, right) = text.split_once('+').ok_or("join output")?;
    assert_eq!(left, right, "a and b saw the same value of v: {text}");

    let manager = envref.get_asset_manager();
    for key in ["data/c.txt", "data/a.txt", "data/b.txt"] {
        manager.remove(&parse_key(key)?).await?;
    }
    outcome(&envref, "-R/data/c.txt").await.map_err(|e| format!("c failed: {e}"))?;
    assert_eq!(count(&counters.tick), 2, "a later evaluation runs v again");
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn volatile_dependency_has_one_value_per_evaluation() -> TestResult {
    tokio::time::timeout(Duration::from_secs(20), volatile_scenario::<Queued>()).await?
}

#[tokio::test(flavor = "multi_thread")]
async fn volatile_dependency_has_one_value_per_evaluation_inline() -> TestResult {
    volatile_scenario::<Inline>().await
}

/// AC-8: a second evaluation started while the first is running joins it.
#[tokio::test(flavor = "multi_thread")]
async fn concurrent_evaluations_join_the_first() -> TestResult {
    let (envref, counters) = env_with::<Queued>(&chain("gated_fail", 5), None).await?;
    let first = envref.evaluate("-R/data/l5.txt").await?;
    let second = envref.evaluate("-R/data/l5.txt").await?;
    let wait = async {
        while count(&counters.gated) == 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(20), wait).await?;
    counters.latch.open();
    for asset in [first, second] {
        let state = tokio::time::timeout(Duration::from_secs(20), asset.get()).await??;
        let error = state.value_state().err().ok_or("expected a failure")?;
        assert!(error.to_string().contains("leaf failed"), "{error}");
    }
    assert_eq!(count(&counters.fail), 1);
    Ok(())
}

/// AC-15: a dependency that has already failed stops the rest of the plan (inline manager).
#[tokio::test]
async fn failed_dependency_stops_the_rest_of_the_plan_inline() -> TestResult {
    let recipes = vec![r("fail/l0.txt"), r("y_cmd/y.txt")];
    let (envref, counters) = env_with::<Inline>(&recipes, None).await?;
    let result = outcome(&envref, "-R/data/l0.txt/-/join-~X~-R/data/y.txt~E").await;
    assert!(failed_with_leaf(&result), "{result:?}");
    assert_eq!(count(&counters.y), 0, "y was not evaluated");
    Ok(())
}

/// AC-16 (both managers): a command requesting its own key is reported as a cycle, not a hang.
async fn dynamic_cycle_scenario<K: liquers_core::environment_builder::AssetManagerKind>() -> TestResult {
    let (envref, _counters) = env_with::<K>(&[r("loop_cmd/k.txt")], None).await?;
    let evaluation = async {
        let asset = envref.get_asset_manager().get_asset(&parse_query("-R/data/k.txt")?).await;
        match asset {
            Err(error) => Ok::<_, Box<dyn std::error::Error>>(error),
            Ok(asset) => Ok(asset.get().await?.value_state().err().ok_or("expected a failure")?),
        }
    };
    let error = tokio::time::timeout(Duration::from_secs(10), evaluation).await??;
    assert_eq!(error.error_type, ErrorType::DependencyCycle, "{error}");
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn dynamic_cycle_is_reported_without_hanging() -> TestResult {
    dynamic_cycle_scenario::<Queued>().await?;
    dynamic_cycle_scenario::<Inline>().await
}
