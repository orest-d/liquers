//! Derived directory structure for a backend that has no directory objects.
//!
//! Most storage backends are flat: a key set with no directories in it. `is_dir`, `contains` and
//! `listdir` then have to be *derived* — every proper prefix of a stored key is a directory.
//!
//! Before this module, every store that faced the problem solved it privately, and no two the same:
//! [`AsyncMemoryStore`](crate::store::AsyncMemoryStore) with a reference-counted concurrent index,
//! the synchronous `MemoryStore` with no index at all (an O(n) key scan per call), `liquers-web`'s
//! `FetchStore` with an immutable map built once from a configured key set, and its
//! `LocalStorageStore` with a mutable map *plus* a separate set of explicitly created directories.
//! `AsyncOpenDALStore` had none, which is how a directory could be visible to `listdir` and denied
//! by `is_dir`. See `CORE-DIRECTORY-INDEX-NOT-SHARED`.
//!
//! # A store supplies its source of truth; this supplies the derivation
//!
//! There are three sources of directory truth, and a store uses whichever its backend offers:
//!
//! | Backend shape | Source | Stores |
//! |---|---|---|
//! | Real directories | `stat` the path | `AsyncFileStore` |
//! | A listing, no directory objects | a bounded listing | `AsyncOpenDALStore` |
//! | Neither | this index | `AsyncMemoryStore`, `FetchStore`, `LocalStorageStore` |
//!
//! A store whose backend is authoritative and writable by other processes — an object store —
//! should **not** use this index: it would go stale, and rebuilding it means listing the whole
//! bucket. It asks the backend instead. What such a store still shares is the semantics downstream
//! of `is_dir`, which live on [`AsyncStore`](crate::store::AsyncStore).
//!
//! # Derived and explicit directories are different things
//!
//! A directory derived from its children retires when the last child goes. A directory created by
//! `makedir` has no children and must persist until `removedir`. A derived index alone cannot
//! express the second, which is why `AsyncMemoryStore::makedir` recorded nothing at all and
//! `LocalStorageStore` had to grow a private `explicit_dirs` set beside its derived map. Both kinds
//! live here, and [`DirectoryIndex::is_dir`] answers for either.

use std::sync::Arc;

use crate::query::Key;

/// The parent-to-children index behind a store's directory answers.
///
/// Concurrent and interior-mutable, so a store holds it behind `&self` like the rest of its state.
///
/// **Not atomic across operations.** [`insert_key`](Self::insert_key) walks a key's ancestor edges
/// one at a time, so a concurrent reader can observe a partially inserted path. This is the
/// behaviour `AsyncMemoryStore` had before the index was extracted, preserved deliberately;
/// strengthening it would be a design change rather than a refactor.
#[derive(Debug, Default)]
pub struct DirectoryIndex {
    /// parent -> child key -> how many stored keys keep that child alive.
    ///
    /// Reference counts rather than a set: removing one key must not retire a directory another
    /// key still occupies.
    derived: scc::HashMap<Key, Arc<scc::HashMap<Key, usize>>>,
    /// Directories that exist because they were created, not because they hold anything.
    explicit: scc::HashSet<Key>,
}

impl DirectoryIndex {
    /// An index of nothing.
    pub fn new() -> Self {
        Self {
            derived: scc::HashMap::new(),
            explicit: scc::HashSet::new(),
        }
    }

    /// Every (parent, child) edge a key implies, from the root down to the key itself.
    ///
    /// Pure and synchronous, so the derivation can be tested without a runtime. The root key
    /// implies nothing: it has no ancestors and is not a child of anything.
    pub fn edges_for_key(key: &Key) -> Vec<(Key, Key)> {
        let mut edges = Vec::new();
        for depth in 0..key.len() {
            let parent = if depth == 0 {
                Key::new()
            } else {
                key.prefix_of_size(depth).unwrap_or_default()
            };
            if let Some(child) = key.prefix_of_size(depth + 1) {
                edges.push((parent, child));
            }
        }
        edges
    }

    /// An index built from a known key set — the shape a store with a configured key list needs.
    ///
    /// Equivalent to [`insert_key`](Self::insert_key) applied to each key in turn.
    pub async fn from_keys(keys: impl IntoIterator<Item = Key>) -> Self {
        let index = Self::new();
        for key in keys {
            index.insert_key(&key).await;
        }
        index
    }

    async fn get_or_create_children_map(&self, parent: &Key) -> Arc<scc::HashMap<Key, usize>> {
        if let Some(children) = self
            .derived
            .read_async(parent, |_, children| children.clone())
            .await
        {
            return children;
        }
        let fresh = Arc::new(scc::HashMap::new());
        match self
            .derived
            .insert_async(parent.clone(), fresh.clone())
            .await
        {
            Ok(()) => fresh,
            Err((_parent, _fresh)) => self
                .derived
                .read_async(parent, |_, children| children.clone())
                .await
                .unwrap_or_else(|| Arc::new(scc::HashMap::new())),
        }
    }

    /// Records a stored key, making every proper ancestor of it a directory.
    pub async fn insert_key(&self, key: &Key) {
        for (parent, child) in Self::edges_for_key(key) {
            let children = self.get_or_create_children_map(&parent).await;
            if children
                .update_async(&child, |_k, count| {
                    *count += 1;
                })
                .await
                .is_none()
            {
                let _ = children.insert_async(child, 1).await;
            }
        }
    }

    /// Forgets a stored key, retiring any ancestor left with no children.
    pub async fn remove_key(&self, key: &Key) {
        for (parent, child) in Self::edges_for_key(key) {
            if let Some(children) = self
                .derived
                .read_async(&parent, |_, children| children.clone())
                .await
            {
                if let Some(current_count) = children.read_async(&child, |_k, count| *count).await {
                    if current_count <= 1 {
                        let _ = children.remove_async(&child).await;
                    } else {
                        let _ = children
                            .update_async(&child, |_k, count| {
                                *count -= 1;
                            })
                            .await;
                    }
                    if children.is_empty() {
                        let _ = self.derived.remove_async(&parent).await;
                    }
                }
            }
        }
    }

    /// Records a directory that exists in its own right, with or without children.
    ///
    /// Its ancestors become directories too, so `makedir("a/b")` makes `a` a directory.
    pub async fn insert_directory(&self, key: &Key) {
        if key.is_empty() {
            return;
        }
        let _ = self.explicit.insert_async(key.to_owned()).await;
        self.insert_key(key).await;
    }

    /// Forgets an explicitly created directory. Children, if any, keep it derived.
    pub async fn remove_directory(&self, key: &Key) {
        if key.is_empty() {
            return;
        }
        if self.explicit.remove_async(key).await.is_some() {
            self.remove_key(key).await;
        }
    }

    /// True when the key holds children, or was explicitly created.
    pub async fn is_dir(&self, key: &Key) -> bool {
        if self.explicit.contains_async(key).await {
            return true;
        }
        self.derived
            .read_async(key, |_k, children| !children.is_empty())
            .await
            .unwrap_or(false)
    }

    /// The names directly under the key, sorted and deduplicated.
    pub async fn children(&self, key: &Key) -> Vec<String> {
        self.child_keys(key)
            .await
            .iter()
            .filter_map(|child| child.filename().map(|name| name.encode().to_string()))
            .collect()
    }

    /// The keys directly under the key, sorted.
    pub async fn child_keys(&self, key: &Key) -> Vec<Key> {
        let Some(children) = self
            .derived
            .read_async(key, |_k, children| children.clone())
            .await
        else {
            return Vec::new();
        };
        let mut keys = Vec::new();
        let _ = children
            .iter_async(|child, _count| {
                keys.push(child.clone());
                true
            })
            .await;
        keys.sort();
        keys.dedup();
        keys
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Error;
    use crate::parse::parse_key;

    /// `DIRIDX01` — every ancestor edge a key implies, and no others.
    #[test]
    fn diridx01_edges_for_key() -> Result<(), Error> {
        assert!(
            DirectoryIndex::edges_for_key(&Key::new()).is_empty(),
            "the root implies no edges"
        );
        assert_eq!(DirectoryIndex::edges_for_key(&parse_key("a")?).len(), 1);
        // a/b/c -> (root, a), (a, a/b), (a/b, a/b/c)
        assert_eq!(DirectoryIndex::edges_for_key(&parse_key("a/b/c")?).len(), 3);
        let edges = DirectoryIndex::edges_for_key(&parse_key("a/b")?);
        assert_eq!(edges[0], (Key::new(), parse_key("a")?));
        assert_eq!(edges[1], (parse_key("a")?, parse_key("a/b")?));
        Ok(())
    }

    /// `DIRIDX02` — building from a key set and inserting incrementally agree.
    ///
    /// `FetchStore` does the first and `AsyncMemoryStore` the second. If they disagreed, two
    /// callers would derive different trees from the same keys.
    #[tokio::test]
    async fn diridx02_from_keys_matches_incremental_insertion() -> Result<(), Error> {
        let keys = ["a/b/c.txt", "a/b/d.txt", "a/e.txt", "f.txt"]
            .iter()
            .map(|k| parse_key(k))
            .collect::<Result<Vec<_>, _>>()?;

        let built = DirectoryIndex::from_keys(keys.clone()).await;
        let incremental = DirectoryIndex::new();
        for key in &keys {
            incremental.insert_key(key).await;
        }

        for probe in ["", "a", "a/b", "a/b/c.txt", "g"] {
            let key = parse_key(probe)?;
            assert_eq!(
                built.is_dir(&key).await,
                incremental.is_dir(&key).await,
                "is_dir {probe}"
            );
            assert_eq!(
                built.children(&key).await,
                incremental.children(&key).await,
                "children {probe}"
            );
        }
        Ok(())
    }

    /// `DIRIDX03` — a directory outlives all but its last child.
    ///
    /// The case the reference counts exist for.
    #[tokio::test]
    async fn diridx03_directory_retires_only_with_its_last_child() -> Result<(), Error> {
        let index = DirectoryIndex::new();
        let (c, d) = (parse_key("a/b/c.txt")?, parse_key("a/b/d.txt")?);
        index.insert_key(&c).await;
        index.insert_key(&d).await;
        let dir = parse_key("a/b")?;
        assert!(index.is_dir(&dir).await);

        index.remove_key(&c).await;
        assert!(index.is_dir(&dir).await, "one child left, still a directory");

        index.remove_key(&d).await;
        assert!(!index.is_dir(&dir).await, "no children left");
        assert!(
            !index.is_dir(&parse_key("a")?).await,
            "and the retirement propagates upward"
        );
        Ok(())
    }

    /// `DIRIDX04` — an explicitly created directory needs no children.
    ///
    /// The capability a derived index cannot express, and the reason `makedir` recorded nothing.
    #[tokio::test]
    async fn diridx04_explicit_directory_survives_without_children() -> Result<(), Error> {
        let index = DirectoryIndex::new();
        let dir = parse_key("empty/folder")?;
        assert!(!index.is_dir(&dir).await);

        index.insert_directory(&dir).await;
        assert!(index.is_dir(&dir).await, "explicit, so childless is fine");
        assert!(index.children(&dir).await.is_empty());
        assert!(
            index.is_dir(&parse_key("empty")?).await,
            "its parent is a directory too"
        );

        index.remove_directory(&dir).await;
        assert!(!index.is_dir(&dir).await);
        Ok(())
    }

    /// `DIRIDX05` — explicit and derived compose.
    ///
    /// `makedir` then `set` then `remove` must leave the directory the caller explicitly created,
    /// not nothing. Nothing in the codebase stated this before; it is what `makedir` means.
    #[tokio::test]
    async fn diridx05_explicit_and_derived_compose() -> Result<(), Error> {
        let index = DirectoryIndex::new();
        let dir = parse_key("mixed")?;
        let child = parse_key("mixed/file.txt")?;
        index.insert_directory(&dir).await;
        index.insert_key(&child).await;

        index.remove_key(&child).await;
        assert!(
            index.is_dir(&dir).await,
            "explicitly created, so it outlives its children"
        );
        Ok(())
    }

    /// `DIRIDX06` — `children` is direct, sorted and deduplicated.
    #[tokio::test]
    async fn diridx06_children_are_direct_sorted_and_unique() -> Result<(), Error> {
        let index = DirectoryIndex::new();
        for k in ["z/1.txt", "a/2.txt", "a/3.txt", "a/deep/4.txt"] {
            index.insert_key(&parse_key(k)?).await;
        }
        assert_eq!(
            index.children(&Key::new()).await,
            vec!["a".to_string(), "z".to_string()]
        );
        assert_eq!(
            index.children(&parse_key("a")?).await,
            vec!["2.txt".to_string(), "3.txt".to_string(), "deep".to_string()],
            "direct children only - 4.txt is not among them"
        );
        Ok(())
    }

    /// `DIRIDX07` — a directory whose name is a prefix of another is not confused with it.
    ///
    /// The index is keyed by `Key`, so this cannot fail the way the *path*-keyed store did. The
    /// test exists so a future rewrite to a string-keyed index fails loudly.
    #[tokio::test]
    async fn diridx07_sibling_prefixes_are_distinct() -> Result<(), Error> {
        let index = DirectoryIndex::new();
        index.insert_key(&parse_key("sub/a.txt")?).await;
        index.insert_key(&parse_key("subway/b.txt")?).await;

        assert_eq!(
            index.children(&parse_key("sub")?).await,
            vec!["a.txt".to_string()]
        );
        assert_eq!(
            index.children(&parse_key("subway")?).await,
            vec!["b.txt".to_string()]
        );

        index.remove_key(&parse_key("sub/a.txt")?).await;
        assert!(!index.is_dir(&parse_key("sub")?).await);
        assert!(
            index.is_dir(&parse_key("subway")?).await,
            "the sibling is untouched"
        );
        Ok(())
    }

    /// `DIRIDX08` — concurrent insertion under one parent keeps the counts right.
    ///
    /// Checks the reference counts under contention. It does **not** check cross-operation
    /// atomicity: `insert_key` walks ancestor edges one at a time, so a concurrent reader can see a
    /// partially inserted path. That is preserved behaviour, not a promise.
    #[tokio::test]
    async fn diridx08_concurrent_insertion_is_consistent() -> Result<(), Error> {
        let index = Arc::new(DirectoryIndex::new());
        let mut handles = Vec::new();
        for i in 0..32 {
            let index = index.clone();
            handles.push(tokio::spawn(async move {
                if let Ok(key) = parse_key(&format!("shared/file{i}.txt")) {
                    index.insert_key(&key).await;
                }
            }));
        }
        for handle in handles {
            handle
                .await
                .map_err(|e| Error::general_error(e.to_string()))?;
        }
        assert_eq!(index.children(&parse_key("shared")?).await.len(), 32);
        Ok(())
    }
}
