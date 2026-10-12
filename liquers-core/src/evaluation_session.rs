//! The evaluation session: one top-level evaluation, in which each dependency resolves to one
//! asset (`specs/design/dependency-resolved-once-per-evaluation/`).
//!
//! A session begins when an asset's run starts without belonging to one, and ends when the last
//! run holding it finishes. Every run of an asset scheduled into the session belongs to it. The
//! first resolution of a dependency in a session fixes the asset that every later request for the
//! same dependency in that session receives, whatever its status. A dependency that failed is
//! therefore not run again within the session, and a volatile dependency has one value. A *later*
//! session resolves afresh, so a failure is not remembered across evaluations.
//!
//! Running [`Context`](crate::context::Context)s hold a session strongly; assets scheduled into it
//! hold it only weakly, so a pinned asset never keeps its own session alive.
//!
//! Not to be confused with a *user* session ([`Environment::SessionType`](crate::context::Environment)).

use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex};

use tokio::sync::OnceCell;

use crate::assets::AssetRef;
use crate::context::Environment;
use crate::error::Error;
use crate::metadata::DependencyKey;

/// First-wins resolution of values by key: the first resolver of a key runs, and every other
/// request for that key, concurrent or later, receives its value.
///
/// No lock is held across a resolver's `await`; concurrent requests for one key wait on that key's
/// cell, and requests for other keys are not blocked.
struct PinMap<T: Clone> {
    cells: Mutex<HashMap<DependencyKey, Arc<OnceCell<T>>>>,
}

impl<T: Clone> PinMap<T> {
    fn new() -> Self {
        PinMap {
            cells: Mutex::new(HashMap::new()),
        }
    }

    fn cell(&self, id: DependencyKey) -> Arc<OnceCell<T>> {
        let mut cells = self.cells.lock().unwrap_or_else(|e| e.into_inner());
        cells.entry(id).or_insert_with(|| Arc::new(OnceCell::new())).clone()
    }

    /// The value for `id`, resolving it with `resolve` if this is the first request. An error
    /// pins nothing: the next request resolves again.
    async fn resolve<F, Fut>(&self, id: DependencyKey, resolve: F) -> Result<T, Error>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, Error>>,
    {
        let cell = self.cell(id);
        cell.get_or_try_init(resolve).await.cloned()
    }

    /// Pins `value` for `id` unless a value is already pinned; returns the pinned value.
    async fn pin(&self, id: DependencyKey, value: T) -> T {
        let cell = self.cell(id);
        cell.get_or_init(|| async move { value }).await.clone()
    }

    #[cfg(test)]
    fn get(&self, id: &DependencyKey) -> Option<T> {
        let cells = self.cells.lock().unwrap_or_else(|e| e.into_inner());
        cells.get(id).and_then(|cell| cell.get().cloned())
    }
}

/// One top-level evaluation: each dependency identity resolves to one asset.
pub(crate) struct EvaluationSession<E: Environment> {
    resolved: PinMap<AssetRef<E>>,
}

/// A session, shared by every run in it.
pub(crate) type EvaluationSessionRef<E> = Arc<EvaluationSession<E>>;

impl<E: Environment> EvaluationSession<E> {
    pub(crate) fn new() -> EvaluationSessionRef<E> {
        Arc::new(EvaluationSession {
            resolved: PinMap::new(),
        })
    }

    /// The asset of dependency `id` in this session. The first call runs `resolve`, and every
    /// other call, concurrent or later, gets its asset. An `Err` from `resolve` pins nothing.
    pub(crate) async fn resolve<F, Fut>(&self, id: DependencyKey, resolve: F) -> Result<AssetRef<E>, Error>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<AssetRef<E>, Error>>,
    {
        self.resolved.resolve(id, resolve).await
    }

    /// Pins an asset resolved outside [`Self::resolve`] (the inline manager's priming); returns
    /// the asset already pinned for `id`, if there is one.
    pub(crate) async fn pin(&self, id: DependencyKey, asset: AssetRef<E>) -> AssetRef<E> {
        self.resolved.pin(id, asset).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn key(name: &str) -> DependencyKey {
        DependencyKey::new(name)
    }

    #[tokio::test]
    async fn resolve_runs_the_resolver_once() {
        let pins: Arc<PinMap<usize>> = Arc::new(PinMap::new());
        let runs = Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::new();
        for _ in 0..8 {
            let (pins, runs) = (pins.clone(), runs.clone());
            handles.push(tokio::spawn(async move {
                pins.resolve(key("-R/data/x.txt"), || async {
                    tokio::task::yield_now().await;
                    Ok(runs.fetch_add(1, Ordering::SeqCst) + 100)
                })
                .await
            }));
        }
        for handle in handles {
            assert_eq!(handle.await.expect("join").expect("resolved"), 100);
        }
        assert_eq!(runs.load(Ordering::SeqCst), 1);
        // A later request gets the pinned value without resolving.
        let later = pins
            .resolve(key("-R/data/x.txt"), || async { Ok(7) })
            .await
            .expect("resolved");
        assert_eq!(later, 100);
    }

    #[tokio::test]
    async fn resolve_error_pins_nothing() {
        let pins: PinMap<usize> = PinMap::new();
        let failed = pins
            .resolve(key("k"), || async { Err(Error::general_error("lookup failed".to_string())) })
            .await;
        assert!(failed.is_err());
        assert!(pins.get(&key("k")).is_none());
        let resolved = pins.resolve(key("k"), || async { Ok(3) }).await.expect("resolved");
        assert_eq!(resolved, 3);
    }

    #[tokio::test]
    async fn pin_keeps_the_first_asset() {
        let pins: PinMap<usize> = PinMap::new();
        assert_eq!(pins.pin(key("k"), 1).await, 1);
        assert_eq!(pins.pin(key("k"), 2).await, 1);
        let resolved = pins.resolve(key("k"), || async { Ok(3) }).await.expect("resolved");
        assert_eq!(resolved, 1);
    }
}
