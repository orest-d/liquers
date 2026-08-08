# Phase 3: Examples & Use-cases - dependency-management

## Example Type

**User choice:** Runnable prototypes (`#[tokio::test]`)

---

## Overview Table

| # | Type | Name | Purpose |
|---|------|------|---------|
| 1 | Example | Basic Cascade Expiration | Linear chain A→B→C: expire A cascades to all dependents |
| 2 | Example | MetadataRecord Persistence | JSON round-trip + load_from_records reconstructs graph |
| 3 | Example | Cycle Detection & Version Mismatch | Self-loop, chain cycle, stale-version rejection |
| 4 | Unit Tests | DependencyManager Unit Tests | 25 focused tests covering all API methods |
| 5 | Integration | Full Pipeline Integration | DefaultAssetManager + expiration + concurrency |
| - | Corner Cases | Memory / Concurrency / Errors / Serialization | Non-code analysis of edge conditions |

---

## Example 1: Basic Cascade Expiration

**Scenario:** A dependency chain A → B → C where expiring A cascades expiration to B and C.

**Context:** When an asset's base dependency is updated, `DependencyManager::expire()` must
transitively expire all dependents. This is the primary everyday use case.

```rust
// Example 1: Basic cascade expiration — linear chain A→B→C

#[cfg(test)]
mod example_1 {
    use super::*;
    use liquers_core::{
        context::SimpleEnvironment,
        dependencies::{DependencyKey, DependencyManager, Version},
        value::Value,
    };
    use std::sync::Arc;

    type TestEnv = SimpleEnvironment<Value>;

    #[tokio::test]
    async fn cascade_expiration_linear_chain() -> Result<(), Box<dyn std::error::Error>> {
        let dm = Arc::new(DependencyManager::<TestEnv>::new());

        let key_a = DependencyKey::new("-R/data/a");
        let key_b = DependencyKey::new("-R/data/b");
        let key_c = DependencyKey::new("-R/data/c");

        // Step 1: Register versions for A, B, C
        let ver_a = Version::new(100);
        let ver_b = Version::new(200);
        let ver_c = Version::new(300);

        dm.register_version(&key_a, ver_a).await;
        dm.register_version(&key_b, ver_b).await;
        dm.register_version(&key_c, ver_c).await;

        // Step 2: Register dependency chain: B depends on A, C depends on B
        dm.add_dependency(&key_b, &key_a, ver_a).await?;
        dm.add_dependency(&key_c, &key_b, ver_b).await?;

        // Step 3: Expire A (the root)
        let expired = dm.expire(&key_a).await;

        // Step 4: Assert transitive cascade: A, B, and C are all expired
        assert!(expired.keys.contains(&key_a));
        assert!(expired.keys.contains(&key_b));
        assert!(expired.keys.contains(&key_c));
        assert_eq!(expired.keys.len(), 3);
        assert!(expired.assets.is_empty()); // no untracked dependents in this example

        // Step 5: All three no longer tracked (DM only holds valid deps)
        assert_eq!(dm.get_version(&key_a).await, None);
        assert_eq!(dm.get_version(&key_b).await, None);
        assert_eq!(dm.get_version(&key_c).await, None);

        Ok(())
    }

    /// Diamond topology: A and C both depend on B; D is independent.
    /// Expiring B cascades to A and C but leaves D untouched.
    #[tokio::test]
    async fn cascade_expiration_diamond_topology() -> Result<(), Box<dyn std::error::Error>> {
        let dm = Arc::new(DependencyManager::<TestEnv>::new());

        let key_a = DependencyKey::new("-R/data/a");
        let key_b = DependencyKey::new("-R/data/b");
        let key_c = DependencyKey::new("-R/data/c");
        let key_d = DependencyKey::new("-R/data/d"); // unrelated

        let ver_a = Version::new(100);
        let ver_b = Version::new(200);
        let ver_c = Version::new(300);
        let ver_d = Version::new(400);

        dm.register_version(&key_a, ver_a).await;
        dm.register_version(&key_b, ver_b).await;
        dm.register_version(&key_c, ver_c).await;
        dm.register_version(&key_d, ver_d).await;

        // A depends on B, C depends on B; D is independent
        dm.add_dependency(&key_a, &key_b, ver_b).await?;
        dm.add_dependency(&key_c, &key_b, ver_b).await?;

        let expired = dm.expire(&key_b).await;

        // B, A, C all expired
        assert_eq!(expired.keys.len(), 3);
        assert!(expired.keys.contains(&key_b));
        assert!(expired.keys.contains(&key_a));
        assert!(expired.keys.contains(&key_c));

        // D unaffected
        assert_eq!(dm.get_version(&key_d).await, Some(ver_d));

        Ok(())
    }
}
```

---

## Example 2: MetadataRecord Persistence Round-Trip

**Scenario:** Demonstrate that `DependencyRecord` serializes/deserializes cleanly (Version hex
format survives serde), and that `load_from_records` correctly reconstructs the dependency graph
for cascade expiration after a restart.

**Context:** After a server restart, `DependencyManager` is empty. As assets load from the store,
their `MetadataRecord.dependencies` fields are fed to `load_from_records`, progressively rebuilding
the in-memory dependency graph.

```rust
// Example 2: MetadataRecord persistence — JSON round-trip + load_from_records cascade

#[cfg(test)]
mod example_2 {
    use super::*;
    use liquers_core::{
        context::SimpleEnvironment,
        dependencies::{DependencyKey, DependencyManager, DependencyRecord, Version},
        value::Value,
    };
    use std::sync::Arc;

    type TestEnv = SimpleEnvironment<Value>;

    /// Serialize a Vec<DependencyRecord> to JSON and back; verify round-trip equality.
    /// Version(u128) serializes as a 32-char lowercase hex string per Phase 2 spec.
    #[test]
    fn dependency_record_json_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let dep_a = DependencyKey::new("-R/data/config.yaml");
        let dep_b = DependencyKey::new("ns-dep/command_metadata--root-greet");
        let ver_a = Version::new(0x000000000000000a_000000000000004b_u128);
        let ver_b = Version::new(u128::MAX); // max hex = "ffffffffffffffffffffffffffffffff"

        let records = vec![
            DependencyRecord { key: dep_a.clone(), version: ver_a },
            DependencyRecord { key: dep_b.clone(), version: ver_b },
        ];

        let json = serde_json::to_string(&records)?;

        // Version must appear as 32-char hex string, not as a decimal integer
        // (u128 would overflow JavaScript's Number.MAX_SAFE_INTEGER)
        let json_val: serde_json::Value = serde_json::from_str(&json)?;
        let first_version = json_val[0]["version"].as_str()
            .ok_or("Version must serialize as string")?;
        assert_eq!(first_version.len(), 32, "Version hex must be exactly 32 chars");
        assert!(first_version.chars().all(|c| c.is_ascii_hexdigit()),
            "Version hex must be lowercase hex");

        // Full round-trip equality
        let deserialized: Vec<DependencyRecord> = serde_json::from_str(&json)?;
        assert_eq!(records, deserialized);

        Ok(())
    }

    /// load_from_records reconstructs dependency edges; cascade expiration works
    /// after a simulated restart.
    #[tokio::test]
    async fn load_from_records_and_cascade() -> Result<(), Box<dyn std::error::Error>> {
        // Simulate a fresh DependencyManager after restart
        let dm = Arc::new(DependencyManager::<TestEnv>::new());

        let dep_a = DependencyKey::new("-R/data/source.csv");
        let asset_b = DependencyKey::new("-R/results/summary");
        let ver_a = Version::new(12345);

        // Step 1: asset_b's MetadataRecord was persisted with dep_a as a dependency.
        //         Simulate what was stored in MetadataRecord.dependencies:
        let persisted = vec![DependencyRecord { key: dep_a.clone(), version: ver_a }];

        // Step 2: After restart, dep_a's version is registered first (e.g., loaded from store)
        dm.register_version(&dep_a, ver_a).await;

        // Step 3: load_from_records for asset_b — only registers edges for known versions
        dm.register_version(&asset_b, Version::new(99999)).await;
        dm.load_from_records(&asset_b, &persisted).await;

        // Step 4: Expire dep_a — cascade should reach asset_b via the reconstructed edge
        let expired = dm.expire(&dep_a).await;

        assert!(expired.keys.contains(&dep_a));
        assert!(expired.keys.contains(&asset_b),
            "asset_b should cascade-expire via reconstructed edge");

        Ok(())
    }

    /// load_from_records silently skips unknown dependency keys (progressive reconstruction).
    #[tokio::test]
    async fn load_from_records_skips_unknown_keys() -> Result<(), Box<dyn std::error::Error>> {
        let dm = Arc::new(DependencyManager::<TestEnv>::new());

        let known_dep = DependencyKey::new("-R/data/known");
        let unknown_dep = DependencyKey::new("-R/data/not-yet-loaded");
        let dependent = DependencyKey::new("-R/results/output");

        let ver_known = Version::new(100);
        let ver_unknown = Version::new(200);

        dm.register_version(&known_dep, ver_known).await;
        // unknown_dep is NOT registered yet
        dm.register_version(&dependent, Version::new(300)).await;

        let records = vec![
            DependencyRecord { key: known_dep.clone(), version: ver_known },
            DependencyRecord { key: unknown_dep.clone(), version: ver_unknown },
        ];

        // load_from_records must not panic on unknown_dep
        dm.load_from_records(&dependent, &records).await;

        // known_dep edge is established; unknown_dep is silently skipped
        let expired = dm.expire(&known_dep).await;
        assert!(expired.keys.contains(&dependent));

        // unknown_dep remains untracked
        assert_eq!(dm.get_version(&unknown_dep).await, None);

        Ok(())
    }
}
```

---

## Example 3: Cycle Detection & Version Mismatch

**Scenario:** Verify that `add_dependency` rejects edges that would create cycles, and rejects
stale version references. Covers self-loop, chain cycles, and version inconsistency.

**Context:** Both cycle detection and version validation are correctness guarantees — without them,
expiration could loop infinitely or use stale cached results.

```rust
// Example 3: Cycle detection and version mismatch prevention

#[cfg(test)]
mod example_3 {
    use super::*;
    use liquers_core::{
        context::SimpleEnvironment,
        dependencies::{DependencyKey, DependencyManager, Version},
        error::ErrorType,
        value::Value,
    };
    use std::sync::Arc;

    type TestEnv = SimpleEnvironment<Value>;

    /// 3a: Self-dependency is immediately rejected as a cycle.
    #[tokio::test]
    async fn self_dependency_rejected() -> Result<(), Box<dyn std::error::Error>> {
        let dm = DependencyManager::<TestEnv>::new();
        let key_a = DependencyKey::new("-R/self-ref");
        let ver_a = Version::new(1);

        dm.register_version(&key_a, ver_a).await;

        // would_create_cycle must detect the self-loop
        assert!(dm.would_create_cycle(&key_a, &key_a).await);

        let result = dm.add_dependency(&key_a, &key_a, ver_a).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.error_type, ErrorType::DependencyCycle);

        Ok(())
    }

    /// 3b: Chain cycle: B→A, C→B; adding A→C closes the loop.
    #[tokio::test]
    async fn chain_cycle_detected() -> Result<(), Box<dyn std::error::Error>> {
        let dm = DependencyManager::<TestEnv>::new();
        let key_a = DependencyKey::new("-R/a");
        let key_b = DependencyKey::new("-R/b");
        let key_c = DependencyKey::new("-R/c");
        let ver_a = Version::new(1);
        let ver_b = Version::new(2);
        let ver_c = Version::new(3);

        dm.register_version(&key_a, ver_a).await;
        dm.register_version(&key_b, ver_b).await;
        dm.register_version(&key_c, ver_c).await;

        // Build chain: B depends on A, C depends on B
        dm.add_dependency(&key_b, &key_a, ver_a).await?;
        dm.add_dependency(&key_c, &key_b, ver_b).await?;

        // Adding A→C would close A→C→B→A (cycle)
        assert!(dm.would_create_cycle(&key_a, &key_c).await);
        let result = dm.add_dependency(&key_a, &key_c, ver_c).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().error_type, ErrorType::DependencyCycle);

        // C→A is not a cycle (C already depends transitively on A; adding another edge in the
        // same direction is fine from BFS perspective)
        // Note: C depends on B depends on A; adding C→A adds a shortcut, not a cycle
        assert!(!dm.would_create_cycle(&key_c, &key_a).await);

        Ok(())
    }

    /// 3c: Diamond structure cycle detection — BFS must visit all branches.
    #[tokio::test]
    async fn diamond_cycle_detected() -> Result<(), Box<dyn std::error::Error>> {
        let dm = DependencyManager::<TestEnv>::new();
        let a = DependencyKey::new("-R/a"); // top
        let b = DependencyKey::new("-R/b"); // left
        let c = DependencyKey::new("-R/c"); // right
        let d = DependencyKey::new("-R/d"); // bottom

        let va = Version::new(1);
        let vb = Version::new(2);
        let vc = Version::new(3);
        let vd = Version::new(4);

        dm.register_version(&a, va).await;
        dm.register_version(&b, vb).await;
        dm.register_version(&c, vc).await;
        dm.register_version(&d, vd).await;

        // a depends on b and c; b and c both depend on d
        dm.add_dependency(&a, &b, vb).await?;
        dm.add_dependency(&a, &c, vc).await?;
        dm.add_dependency(&b, &d, vd).await?;
        dm.add_dependency(&c, &d, vd).await?;

        // d→a would close d→a→{b,c}→d (cycle via both branches)
        assert!(dm.would_create_cycle(&d, &a).await);
        let result = dm.add_dependency(&d, &a, va).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().error_type, ErrorType::DependencyCycle);

        Ok(())
    }

    /// 3d: Stale version rejected at add_dependency time.
    #[tokio::test]
    async fn stale_version_rejected() -> Result<(), Box<dyn std::error::Error>> {
        let dm = DependencyManager::<TestEnv>::new();
        let dep = DependencyKey::new("-R/dep");
        let dependent = DependencyKey::new("-R/dependent");
        let ver_old = Version::new(100);
        let ver_new = Version::new(101);

        dm.register_version(&dep, ver_old).await;
        dm.register_version(&dependent, Version::new(999)).await;

        // Update dep to newer version
        dm.register_version(&dep, ver_new).await;

        // Attempting to add with stale ver_old should fail
        let result = dm.add_dependency(&dependent, &dep, ver_old).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().error_type, ErrorType::DependencyVersionMismatch);

        // With current version it succeeds
        dm.add_dependency(&dependent, &dep, ver_new).await?;

        Ok(())
    }

    /// 3e: Dependency on unregistered key fails with version mismatch (not tracked).
    #[tokio::test]
    async fn unregistered_dependency_rejected() -> Result<(), Box<dyn std::error::Error>> {
        let dm = DependencyManager::<TestEnv>::new();
        let dep = DependencyKey::new("-R/not-yet-registered");
        let dependent = DependencyKey::new("-R/dependent");

        dm.register_version(&dependent, Version::new(1)).await;
        // dep is NOT registered

        let result = dm.add_dependency(&dependent, &dep, Version::new(1)).await;
        assert!(result.is_err());
        // Reported as version mismatch (version unknown = version mismatch)
        assert_eq!(result.unwrap_err().error_type, ErrorType::DependencyVersionMismatch);

        Ok(())
    }

    /// 3f: version_consistent detects stale versions reliably.
    #[tokio::test]
    async fn version_consistent_check() -> Result<(), Box<dyn std::error::Error>> {
        let dm = DependencyManager::<TestEnv>::new();
        let key = DependencyKey::new("-R/versioned");
        let v_old = Version::new(100);
        let v_new = Version::new(101);

        dm.register_version(&key, v_old).await;
        assert!(dm.version_consistent(&key, v_old).await);
        assert!(!dm.version_consistent(&key, v_new).await);

        dm.register_version(&key, v_new).await;
        assert!(!dm.version_consistent(&key, v_old).await);
        assert!(dm.version_consistent(&key, v_new).await);

        // Unregistered key: always inconsistent
        let unknown = DependencyKey::new("-R/unknown");
        assert!(!dm.version_consistent(&unknown, v_old).await);

        Ok(())
    }
}
```

---

## Unit Tests

**File:** `liquers-core/src/dependencies.rs` (inline `#[cfg(test)] mod tests`)

Comprehensive unit tests covering all `DependencyManager<E>` API methods.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::SimpleEnvironment;
    use crate::value::Value;

    type TestEnv = SimpleEnvironment<Value>;

    // --- Version constructors ---

    #[test]
    fn version_ordering() {
        assert!(Version::new(1) < Version::new(2));
        assert!(Version::new(2) > Version::new(1));
    }

    #[test]
    fn version_from_bytes_is_deterministic() {
        let v1 = Version::from_bytes(b"test data");
        let v2 = Version::from_bytes(b"test data");
        assert_eq!(v1, v2);
    }

    #[test]
    fn version_from_bytes_differs_on_different_data() {
        assert_ne!(Version::from_bytes(b"data a"), Version::from_bytes(b"data b"));
    }

    #[test]
    fn version_from_specific_time_is_consistent() {
        use std::time::{Duration, UNIX_EPOCH};
        let t = UNIX_EPOCH + Duration::from_secs(1_000_000);
        assert_eq!(Version::from_specific_time(t), Version::from_specific_time(t));
    }

    #[test]
    fn version_from_specific_time_respects_order() {
        use std::time::{Duration, UNIX_EPOCH};
        let t1 = UNIX_EPOCH + Duration::from_secs(1_000_000);
        let t2 = UNIX_EPOCH + Duration::from_secs(2_000_000);
        assert!(Version::from_specific_time(t1) < Version::from_specific_time(t2));
    }

    #[test]
    fn version_new_unique_produces_distinct_values() {
        assert_ne!(Version::new_unique(), Version::new_unique());
    }

    // --- register_version / get_version ---

    #[tokio::test]
    async fn version_register_and_get() -> Result<(), Box<dyn std::error::Error>> {
        let dm = DependencyManager::<TestEnv>::new();
        let key = DependencyKey::new("-R/x");
        let ver = Version::new(42);
        dm.register_version(&key, ver).await;
        assert_eq!(dm.get_version(&key).await, Some(ver));
        Ok(())
    }

    #[tokio::test]
    async fn version_get_unregistered_returns_none() -> Result<(), Box<dyn std::error::Error>> {
        let dm = DependencyManager::<TestEnv>::new();
        assert_eq!(dm.get_version(&DependencyKey::new("-R/unknown")).await, None);
        Ok(())
    }

    #[tokio::test]
    async fn version_register_update_overwrites() -> Result<(), Box<dyn std::error::Error>> {
        let dm = DependencyManager::<TestEnv>::new();
        let key = DependencyKey::new("-R/x");
        dm.register_version(&key, Version::new(1)).await;
        dm.register_version(&key, Version::new(2)).await;
        assert_eq!(dm.get_version(&key).await, Some(Version::new(2)));
        Ok(())
    }

    // --- version_consistent ---

    #[tokio::test]
    async fn version_consistent_matches() -> Result<(), Box<dyn std::error::Error>> {
        let dm = DependencyManager::<TestEnv>::new();
        let key = DependencyKey::new("-R/x");
        let ver = Version::new(100);
        dm.register_version(&key, ver).await;
        assert!(dm.version_consistent(&key, ver).await);
        Ok(())
    }

    #[tokio::test]
    async fn version_consistent_mismatches() -> Result<(), Box<dyn std::error::Error>> {
        let dm = DependencyManager::<TestEnv>::new();
        let key = DependencyKey::new("-R/x");
        dm.register_version(&key, Version::new(100)).await;
        assert!(!dm.version_consistent(&key, Version::new(200)).await);
        Ok(())
    }

    #[tokio::test]
    async fn version_consistent_unregistered_returns_false() -> Result<(), Box<dyn std::error::Error>> {
        let dm = DependencyManager::<TestEnv>::new();
        assert!(!dm.version_consistent(&DependencyKey::new("-R/x"), Version::new(1)).await);
        Ok(())
    }

    // --- add_dependency ---

    #[tokio::test]
    async fn add_dependency_succeeds() -> Result<(), Box<dyn std::error::Error>> {
        let dm = DependencyManager::<TestEnv>::new();
        let dep = DependencyKey::new("-R/dep");
        let dependent = DependencyKey::new("-R/dependent");
        let ver = Version::new(1);
        dm.register_version(&dep, ver).await;
        dm.add_dependency(&dependent, &dep, ver).await?;
        Ok(())
    }

    #[tokio::test]
    async fn add_dependency_fails_stale_version() -> Result<(), Box<dyn std::error::Error>> {
        let dm = DependencyManager::<TestEnv>::new();
        let dep = DependencyKey::new("-R/dep");
        dm.register_version(&dep, Version::new(2)).await;
        let result = dm.add_dependency(&DependencyKey::new("-R/d"), &dep, Version::new(1)).await;
        assert!(result.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn add_dependency_fails_unregistered_dep() -> Result<(), Box<dyn std::error::Error>> {
        let dm = DependencyManager::<TestEnv>::new();
        let result = dm.add_dependency(
            &DependencyKey::new("-R/d"),
            &DependencyKey::new("-R/unregistered"),
            Version::new(1),
        ).await;
        assert!(result.is_err());
        Ok(())
    }

    // --- expire ---

    #[tokio::test]
    async fn expire_cascade_chain() -> Result<(), Box<dyn std::error::Error>> {
        let dm = DependencyManager::<TestEnv>::new();
        let a = DependencyKey::new("-R/a");
        let b = DependencyKey::new("-R/b");
        let c = DependencyKey::new("-R/c");
        dm.register_version(&a, Version::new(1)).await;
        dm.register_version(&b, Version::new(2)).await;
        dm.register_version(&c, Version::new(3)).await;
        dm.add_dependency(&b, &a, Version::new(1)).await?;
        dm.add_dependency(&c, &b, Version::new(2)).await?;
        let expired = dm.expire(&a).await;
        assert!(expired.keys.contains(&a));
        assert!(expired.keys.contains(&b));
        assert!(expired.keys.contains(&c));
        Ok(())
    }

    #[tokio::test]
    async fn expire_removes_from_versions() -> Result<(), Box<dyn std::error::Error>> {
        let dm = DependencyManager::<TestEnv>::new();
        let key = DependencyKey::new("-R/x");
        dm.register_version(&key, Version::new(1)).await;
        dm.expire(&key).await;
        assert_eq!(dm.get_version(&key).await, None);
        Ok(())
    }

    #[tokio::test]
    async fn expire_single_key_no_dependents() -> Result<(), Box<dyn std::error::Error>> {
        let dm = DependencyManager::<TestEnv>::new();
        let key = DependencyKey::new("-R/lone");
        dm.register_version(&key, Version::new(1)).await;
        let expired = dm.expire(&key).await;
        assert_eq!(expired.keys.len(), 1);
        assert!(expired.assets.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn expire_nonexistent_key_is_noop() -> Result<(), Box<dyn std::error::Error>> {
        let dm = DependencyManager::<TestEnv>::new();
        let expired = dm.expire(&DependencyKey::new("-R/nonexistent")).await;
        assert!(expired.keys.is_empty());
        assert!(expired.assets.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn expire_multiple_dependents_of_one_key() -> Result<(), Box<dyn std::error::Error>> {
        let dm = DependencyManager::<TestEnv>::new();
        let dep = DependencyKey::new("-R/shared");
        let d1 = DependencyKey::new("-R/d1");
        let d2 = DependencyKey::new("-R/d2");
        let vd = Version::new(1);
        dm.register_version(&dep, vd).await;
        dm.register_version(&d1, Version::new(2)).await;
        dm.register_version(&d2, Version::new(3)).await;
        dm.add_dependency(&d1, &dep, vd).await?;
        dm.add_dependency(&d2, &dep, vd).await?;
        let expired = dm.expire(&dep).await;
        assert_eq!(expired.keys.len(), 3);
        Ok(())
    }

    // --- would_create_cycle ---

    #[tokio::test]
    async fn would_create_cycle_true_for_back_edge() -> Result<(), Box<dyn std::error::Error>> {
        let dm = DependencyManager::<TestEnv>::new();
        let a = DependencyKey::new("-R/a");
        let b = DependencyKey::new("-R/b");
        let c = DependencyKey::new("-R/c");
        dm.register_version(&a, Version::new(1)).await;
        dm.register_version(&b, Version::new(2)).await;
        dm.register_version(&c, Version::new(3)).await;
        dm.add_dependency(&b, &a, Version::new(1)).await?;
        dm.add_dependency(&c, &b, Version::new(2)).await?;
        // a→c would create a→c→b→a
        assert!(dm.would_create_cycle(&a, &c).await);
        Ok(())
    }

    #[tokio::test]
    async fn would_create_cycle_false_for_valid_shortcut() -> Result<(), Box<dyn std::error::Error>> {
        let dm = DependencyManager::<TestEnv>::new();
        let a = DependencyKey::new("-R/a");
        let b = DependencyKey::new("-R/b");
        let c = DependencyKey::new("-R/c");
        dm.register_version(&a, Version::new(1)).await;
        dm.register_version(&b, Version::new(2)).await;
        dm.register_version(&c, Version::new(3)).await;
        dm.add_dependency(&b, &a, Version::new(1)).await?;
        dm.add_dependency(&c, &b, Version::new(2)).await?;
        // c→a is a shortcut (c already reaches a transitively); not a new cycle
        assert!(!dm.would_create_cycle(&c, &a).await);
        Ok(())
    }

    // --- remove ---

    #[tokio::test]
    async fn remove_clears_version() -> Result<(), Box<dyn std::error::Error>> {
        let dm = DependencyManager::<TestEnv>::new();
        let key = DependencyKey::new("-R/x");
        dm.register_version(&key, Version::new(1)).await;
        dm.remove(&key).await;
        assert_eq!(dm.get_version(&key).await, None);
        Ok(())
    }

    #[tokio::test]
    async fn remove_nonexistent_is_noop() -> Result<(), Box<dyn std::error::Error>> {
        let dm = DependencyManager::<TestEnv>::new();
        dm.remove(&DependencyKey::new("-R/nonexistent")).await; // must not panic
        Ok(())
    }

    // --- load_from_records ---

    #[tokio::test]
    async fn load_from_records_registers_known() -> Result<(), Box<dyn std::error::Error>> {
        let dm = DependencyManager::<TestEnv>::new();
        let dep = DependencyKey::new("-R/dep");
        let dependent = DependencyKey::new("-R/dependent");
        let ver = Version::new(100);
        dm.register_version(&dep, ver).await;
        dm.register_version(&dependent, Version::new(999)).await;
        dm.load_from_records(&dependent, &[DependencyRecord { key: dep.clone(), version: ver }]).await;
        // Edge registered: expiring dep now cascades to dependent
        let expired = dm.expire(&dep).await;
        assert!(expired.keys.contains(&dependent));
        Ok(())
    }

    #[tokio::test]
    async fn load_from_records_skips_unknown() -> Result<(), Box<dyn std::error::Error>> {
        let dm = DependencyManager::<TestEnv>::new();
        let unknown = DependencyKey::new("-R/not-loaded-yet");
        let dependent = DependencyKey::new("-R/dependent");
        dm.register_version(&dependent, Version::new(1)).await;
        // No panic; unknown dep silently skipped
        dm.load_from_records(&dependent, &[DependencyRecord { key: unknown.clone(), version: Version::new(1) }]).await;
        assert_eq!(dm.get_version(&unknown).await, None);
        Ok(())
    }

    #[tokio::test]
    async fn load_from_empty_records_is_noop() -> Result<(), Box<dyn std::error::Error>> {
        let dm = DependencyManager::<TestEnv>::new();
        let dependent = DependencyKey::new("-R/d");
        dm.load_from_records(&dependent, &[]).await; // no panic, no effect
        assert_eq!(dm.get_version(&dependent).await, None);
        Ok(())
    }

    // --- DependencyKey named constructors ---

    #[test]
    fn dependency_key_for_command_metadata_format() {
        use liquers_core::command_metadata::CommandKey;
        // Use non-default namespace ("pl", not "root") — CommandKey::new normalizes
        // DEFAULT_NAMESPACE ("root") to "" in storage, which changes the Display output.
        // With realm="" and namespace="pl": Display → "-pl-greet",
        // so for_command_metadata → "ns-dep/command_metadata--pl-greet".
        let key = CommandKey::new("", "pl", "greet");
        let dk = DependencyKey::for_command_metadata(&key);
        assert_eq!(dk.as_str(), "ns-dep/command_metadata--pl-greet");
    }

    #[test]
    fn dependency_key_for_command_implementation_format() {
        use liquers_core::command_metadata::CommandKey;
        let key = CommandKey::new("", "pl", "select");
        let dk = DependencyKey::for_command_implementation(&key);
        assert_eq!(dk.as_str(), "ns-dep/command_implementation--pl-select");
    }

    // TODO: test_add_untracked_dependent — requires a real AssetRef<E> from a running
    // asset manager (construction shown in integration tests below).
    // Assert: weak ref appears in ExpiredDependents.assets when dependency is expired.
    // Assert: dead weak refs (after AssetRef drop) are silently ignored in expire().
}
```

---

## Integration Tests

**File:** `liquers-core/tests/dependency_manager_integration.rs` (to be created in Phase 4)

These sketches show how `DependencyManager<E>` integrates with `DefaultAssetManager<E>`.
Note: tests are non-compilable until Phase 4 implements the API.

```rust
// Integration test sketches — require Phase 4 implementation
// File: liquers-core/tests/dependency_manager_integration.rs

// ---- Test 1: set_state triggers cascade expiration via DefaultAssetManager ----
//
// Scenario: Asset B depends on asset A. Update A (new version) → B should
// auto-expire. Verifies that DefaultAssetManager::set_state() hooks into DM.
//
// #[tokio::test]
// async fn set_state_triggers_cascade_expiration() -> Result<(), Box<dyn std::error::Error>> {
//     type Env = SimpleEnvironment<Value>;
//     let env = Env::new();
//     let envref = env.to_ref();
//     let manager = envref.get_asset_manager();
//     let dm = manager.dependency_manager();
//
//     let key_a = parse_key("test/a")?;
//     let key_b = parse_key("test/b")?;
//
//     // Register A at v1, B depending on A
//     let dep_a = DependencyKey::from(&key_a);
//     let dep_b = DependencyKey::from(&key_b);
//     let ver_a = Version::new(1);
//     dm.register_version(&dep_a, ver_a).await;
//     dm.register_version(&dep_b, Version::new(2)).await;
//     dm.add_dependency(&dep_b, &dep_a, ver_a).await?;
//
//     // Set A to a new version — should cascade-expire B
//     let new_state_a = State { data: Arc::new(Value::from("new")), metadata: ... };
//     manager.set_state(&key_a, new_state_a).await?;
//
//     // B is now expired (get returns Expired status or None)
//     assert_eq!(dm.get_version(&dep_b).await, None);
//     Ok(())
// }

// ---- Test 2: MetadataRecord deserialization into DM on asset load ----
//
// Scenario: Simulate a server restart. Load asset from store with persisted
// MetadataRecord.dependencies. Verify DM reconstructs edges.
//
// #[tokio::test]
// async fn metadata_deserialized_into_dm_on_load() -> Result<(), Box<dyn std::error::Error>> {
//     // ... set up MemoryStore with pre-serialized MetadataRecord ...
//     // ... load asset → manager.get() → DM.load_from_records() called internally ...
//     // ... expire dep → cascade reaches loaded asset ...
//     Ok(())
// }

// ---- Test 3: Concurrent expiration serialized by expiration_lock ----
//
// Scenario: 3 tasks race to expire the same key simultaneously.
// Verify expiration_lock prevents double-counting.
//
// #[tokio::test]
// async fn concurrent_expiration_serialized() -> Result<(), Box<dyn std::error::Error>> {
//     let dm = Arc::new(DependencyManager::<TestEnv>::new());
//     // ... register A→B→C chain ...
//     // spawn 3 tasks, each calling dm.expire(&key_a).await
//     // collect all ExpiredDependents.keys
//     // assert each key appears at most once across all results
//     Ok(())
// }

// ---- Test 4: evaluate_with_retry on DependencyVersionMismatch ----
//
// Scenario: A recipe evaluation encounters a DependencyVersionMismatch (stale dep).
// DefaultAssetManager retries up to max_dependency_retries times.
//
// #[tokio::test]
// async fn evaluate_with_retry_succeeds_after_mismatch() -> Result<(), Box<dyn std::error::Error>> {
//     // ... set up environment where dep expires mid-evaluation ...
//     // ... verify that evaluate_with_retry catches DependencyVersionMismatch and retries ...
//     Ok(())
// }
```

---

## Corner Cases

### 1. Memory

- **Large graphs:** `expire()` uses iterative BFS (not recursive) — 1000+ node graphs won't
  stack overflow. A `visited` set prevents revisiting (safe even if data were corrupted).
- **Dead WeakAssetRef entries:** Untracked dependents accumulate in `untracked_dependents`
  until the next `expire()` call, which prunes dead weak refs lazily. No memory leak —
  `WeakAssetRef` does not keep the asset alive.
- **Max memory:** Each `DependencyKey` is a `String`. For 10,000 tracked assets with an
  average 3 dependencies each, memory is ~5–10 MB — well within acceptable bounds.

### 2. Concurrency

- **expiration_lock:** Serializes cascade expiration globally. Concurrent reads (version checks,
  cycle detection) are unblocked via `scc::HashMap`; only `expire()` takes the mutex.
- **Race: register_version + expire:** A version can be registered while expiration BFS is in
  progress. The new version goes into the map; subsequent expire calls may re-expire it if it's
  added back to the graph. This is safe — consistent with the "always remove invalid" invariant.
- **Race: add_dependency + expire:** `add_dependency` acquires no lock; if the dep is expired
  concurrently, the version check will fail (version becomes None during removal). The call
  returns `DependencyVersionMismatch`. Caller retries after re-registering.

### 3. Errors

- **Unknown dependency version:** `add_dependency` returns `DependencyVersionMismatch` —
  not just "key not found". This unifies two cases under one retryable error type.
- **Expired key expiration:** `expire()` called on an already-expired (untracked) key returns
  empty `ExpiredDependents`. No panic, no error.
- **Malformed DependencyKey:** `DependencyKey::new()` accepts any string; format validation
  only happens at `TryFrom<&DependencyKey> for Key` (only `-R/<key>` format succeeds).

### 4. Serialization

- **Version(0):** Serializes as `"00000000000000000000000000000000"` (32 zeros).
- **Version(u128::MAX):** Serializes as `"ffffffffffffffffffffffffffffffff"` (all bits set).
- **Round-trip:** `u128` → hex → `u128` is lossless (bit-for-bit identical).
- **Backward compatibility:** `MetadataRecord.dependencies` has `#[serde(default)]` — old
  records without the field deserialize to an empty `Vec`, triggering no dependency edges.

### 5. Integration

- **Volatile assets:** `AssetManager` checks `is_volatile` before calling any `DependencyManager`
  method. Volatile assets never appear in the manager — they cannot be tracked dependencies and
  are not registered as dependents. No special DM logic needed; the check lives in `AssetManager`.
- **Delete vs. new version:** Per Phase 1 design: *deleting* a stored value for an asset that has
  a recipe (i.e., was `Ready`) does **not** trigger cascade expiration. Only setting a new `Ready`
  value with a new version does. `DependencyManager::remove()` removes the key from tracking
  without expiring dependents; `expire()` is called only when a genuinely new value arrives.
- **Recipe key format:** `DependencyKey::from_recipe_key(key)` produces `-R-recipe/<key>`.
  These keys can be used to track a dependency on an asset's *recipe* (separate from its data).
  `TryFrom<&DependencyKey> for Key` fails for recipe keys (not a plain `-R/<key>` path), so
  `AssetManager` skips them during cascade-expire-to-Key conversion.
- **Command metadata keys:** `DependencyKey::for_command_metadata(key)` produces
  `ns-dep/command_metadata-{realm}-{namespace}-{name}`. These keys have no corresponding
  store `Key`; `TryFrom<&DependencyKey> for Key` returns `Err`, and `AssetManager` silently
  skips them during cascade-expire-to-Key conversion.
- **ContextEvaluate deps:** These are registered via `add_untracked_dependent` (not
  `add_dependency`), so their `DependencyRelation` is not stored or recoverable — accepted
  limitation documented in Phase 1. Integration tests should verify that a `context::evaluate`
  call adds the evaluated asset as an untracked dependent.
- **DependencyRelation (plan-only):** `DependencyRelation` variants (`StateArgument`,
  `ParameterLink`, etc.) live only in `PlanDependency`. They are never stored in `DependencyManager`
  or `MetadataRecord`. Testing of plan-level dependency extraction belongs in `plan.rs` tests
  (Phase 4 scope), not here.
- **New `ErrorType` variants:** `DependencyCycle` and `DependencyVersionMismatch` are new variants
  to be added to `liquers-core/src/error.rs` in Phase 4, Step 1. Until then, examples that assert
  on these error types will not compile.

---

## Test Plan

### Unit Tests

**File:** `liquers-core/src/dependencies.rs` — inline `#[cfg(test)] mod tests`

**Run:** `cargo test -p liquers-core dep`

| Test | What it validates |
|---|---|
| `version_ordering` | `Version` is `Ord` |
| `version_from_bytes_is_deterministic` | Same bytes → same version |
| `version_from_specific_time_respects_order` | Earlier time → smaller version |
| `version_new_unique_produces_distinct_values` | Entropy works |
| `version_register_and_get` | Basic registration |
| `version_get_unregistered_returns_none` | Missing key |
| `version_register_update_overwrites` | Re-register updates version |
| `version_consistent_matches` | Exact match |
| `version_consistent_mismatches` | Different version |
| `add_dependency_succeeds` | Happy path |
| `add_dependency_fails_stale_version` | Version mismatch |
| `add_dependency_fails_unregistered_dep` | Unknown dep key |
| `expire_cascade_chain` | Transitive cascade |
| `expire_removes_from_versions` | Cleanup after expire |
| `expire_single_key_no_dependents` | Leaf expiration |
| `expire_nonexistent_key_is_noop` | Safety |
| `expire_multiple_dependents_of_one_key` | Fan-out |
| `would_create_cycle_true_for_back_edge` | BFS detects cycle |
| `would_create_cycle_false_for_valid_shortcut` | DAG shortcut allowed |
| `remove_clears_version` | Remove effect |
| `remove_nonexistent_is_noop` | Safety |
| `load_from_records_registers_known` | Progressive reconstruction |
| `load_from_records_skips_unknown` | Graceful skip |
| `load_from_empty_records_is_noop` | Empty input safety |
| `dependency_key_for_command_metadata_format` | Named constructor output |
| `dependency_key_for_command_implementation_format` | Named constructor output |

### Integration Tests

**File:** `liquers-core/tests/dependency_manager_integration.rs`

**Run:** `cargo test -p liquers-core --test dependency_manager_integration`

| Test | What it validates |
|---|---|
| `set_state_triggers_cascade_expiration` | `DefaultAssetManager` → DM wiring |
| `metadata_deserialized_into_dm_on_load` | Store → DM progressive reconstruction |
| `concurrent_expiration_serialized` | `expiration_lock` correctness |
| `evaluate_with_retry_succeeds_after_mismatch` | Retry loop in `DefaultAssetManager` |

### Manual Validation

```bash
cargo test -p liquers-core dep
cargo test -p liquers-core --test dependency_manager_integration
# After Phase 4 implementation:
cargo check -p liquers-core
cargo check -p liquers-lib
```

---

## Auto-Invoke: liquers-unittest Skill Output

Test templates are integrated inline (see Unit Tests section above).
All 26 unit tests follow `#[tokio::test]` / `#[test]` conventions from `liquers-unittest`.
Integration test sketches are in commented form pending Phase 4 implementation.
