# Phase 3: Examples & Use-cases — Sidecar-colliding keys refused by the path builders

The examples here are, almost entirely, tests. That is what this change is: a refusal that already
exists in one place and must exist in three, so what needs demonstrating is not a new capability but
**every way of getting the refusal wrong**. The list below was written to one standard — a test
that passes only after the fix, without also failing against a plausible *half*-fix, records the
change rather than checking it.

Examples are complete runnable code (chosen at the Phase 3 gate). Every key shape used was checked
with `liquers-validate --no-registry` before being written down; every API was checked against HEAD.

## Overview Table

The examples run in order of what they explain: the bug as a user meets it, then the shape of it
the issue did not name, then the four things that bite while fixing it.

| # | Example | Demonstrates |
|---|---|---|
| 1 | The corruption, end to end | The Phase 1 purpose. `is_supported` refuses, the store writes anyway, and `report.txt` becomes a file that exists and cannot be described. Shows why the read-back assertion, not the error type, is what proves the fix |
| 2 | A store carrying the legacy metadata folder | The segment-scoped half of the rule, and the three states of the code — HEAD, path-builders-only, and complete. The middle state is *worse than the bug*: `keys()` stops working entirely |
| 3 | Pitfalls | The lock collision (a permanent write deadlock, worse than the metadata one); recovering a store already corrupted; why the error type is `KeyNotAbsolute` for a relative-and-reserved key; why over-reserving is a defect too |

| Test | Checks | Fails if the fix is |
|---|---|---|
| `reserved01` | `ReservedNames` recognises both forms and, crucially, the five near-misses it must not touch | a `contains("__metadata__")` shortcut |
| `reserved02` | Every fallible method of `AsyncFileStore` refuses a colliding key, and the victim's metadata and data survive | applied to `is_supported` only, or applied after the write |
| `reserved03` | A reserved segment anywhere is refused, including the legacy folder form | still scoped to `Key::filename()` |
| `reserved04` | `FileStore` reserves the metadata name and **not** the lock name | one global reserved list shared by every store |
| `reserved05` | Relative-and-reserved reports `KeyNotAbsolute`, not `KeyNotSupported` | reordered so the reserved check runs before `as_absolute()` |
| `reserved06` | `keys()` skips a real `__metadata__` directory instead of failing on it | applied to the path builders without the listing filters |
| `reserved07` | `FileStore`'s listing uses the same predicate and its *own* reserved set | applied to `AsyncFileStore::listdir` and not its synchronous twin |
| `reserved08` | The three recovery routes out of an already-corrupted store all work | shipped with an upgrade path that was never run |
| `C2` conformance | `sidecar03` passes with no allowed failure; `prefix03` and `sibling05` run for the first time | incomplete in any of the above ways |
| `pathmap03`, `pathmap07`, `pathmap08` | `AsyncOpenDALStore` refuses and skips the same shapes, **and refuses in the same order** | applied to the file stores only, or leaving OpenDAL answering `KeyNotSupported` where the file stores answer `KeyNotAbsolute` |

Eight unit tests, one fixture change, three OpenDAL tests. Every row names a *specific way to get the
fix wrong*, which is the standard this list was written to: a test that only passes after the fix,
without also failing against a plausible half-fix, records the change rather than checking it.

## Example 1 — The corruption, end to end

This is the Phase 1 purpose in one run: a routing hint that nobody is obliged to consult, and a
write that lands anyway.

The sequence, before any code. `liquers-axum`'s store handler takes the key from the URL and calls
`store.set(&key, &body, &metadata)` (`store/handlers.rs:80`) — it does not ask `is_supported`,
because `is_supported` exists for `AsyncStoreRouter` to pick a member, not to validate input. The
store then builds two paths from that key: the data path `report.txt.__metadata__` and the metadata
path `report.txt.__metadata__.__metadata__`. The first of those is, byte for byte, the metadata path
of the key `report.txt`. Nothing between the URL and `tokio::fs::write` compares them.

```rust
use liquers_core::error::{Error, ErrorType};
use liquers_core::metadata::{Metadata, MetadataRecord};
use liquers_core::parse::parse_key;
use liquers_core::query::Key;
use liquers_core::store::{AsyncFileStore, AsyncStore};

/// What `PUT /api/store/data/report.txt.__metadata__` does to the store behind it.
async fn sidecar_collision(root: &str) -> Result<(), Error> {
    let store = AsyncFileStore::new(root, &Key::new());

    // An ordinary asset, with metadata worth keeping.
    let report = parse_key("report.txt")?;
    let mut record = MetadataRecord::new();
    record
        .with_key(report.clone())
        .with_title("Quarterly report".to_owned());
    store
        .set(&report, b"body", &Metadata::MetadataRecord(record))
        .await?;

    // The colliding key. Its data path IS the metadata path of `report.txt`.
    let collide = parse_key("report.txt.__metadata__")?;

    // The routing hint refuses it — and always did, even at HEAD.
    assert!(!store.is_supported(&collide));

    // Nothing consults that hint. At HEAD this write SUCCEEDS.
    let blank = Metadata::MetadataRecord(MetadataRecord::new());
    let outcome = store.set(&collide, b"not json at all", &blank).await;

    // After the fix, the path builders refuse and the write never reaches the filesystem.
    let error = outcome.expect_err("the write must be refused");
    assert_eq!(error.error_type, ErrorType::KeyNotSupported);

    // The point of the whole change: the metadata of `report.txt` is still its own.
    match store.get_metadata(&report).await? {
        Metadata::MetadataRecord(record) => assert_eq!(record.title, "Quarterly report"),
        Metadata::LegacyMetadata(_) => panic!("metadata was replaced"),
    }
    Ok(())
}
```

**What the last two assertions replace.** At HEAD, `set` returns `Ok(())`, and then:

- `get_metadata("report.txt")` returns `Err(KeyReadError, "Metadata parsing error")` — the sidecar
  now holds `not json at all`, which parses as neither `MetadataRecord` nor `LegacyMetadata`.
- `get("report.txt")` returns `Ok`, because it *repairs* what it cannot read: it synthesizes fresh
  metadata, attaches two warnings, and writes it back over the sidecar. The title is gone, and so
  are the attacker's bytes — the two keys are fighting over one file, and whichever is written last
  wins.
- The data of `report.txt` is untouched throughout. That is the shape of the damage the issue
  describes: **a file that exists and cannot be described.**

Asserting only `ErrorType::KeyNotSupported` would not catch a fix that wrote first and errored
afterwards, which is why the metadata read-back is part of the example rather than a flourish.

## Example 2 — A store carrying the legacy metadata folder, and why the listing filter is not optional

Example 1 is the filename form. This one is the segment form, and it is where a half-applied fix
does more damage than none.

Earlier Liquers versions kept metadata in a folder: `data/__metadata__/report.txt.json`. A store
root that has been through a migration — or that was never migrated — still contains one.

```rust
use std::path::Path;

/// A root that still carries the pre-sidecar layout.
async fn legacy_metadata_folder(root: &str) -> Result<(), Error> {
    let legacy = Path::new(root).join("data").join("__metadata__");
    tokio::fs::create_dir_all(&legacy).await.expect("legacy folder");
    tokio::fs::write(legacy.join("report.txt.json"), b"{}")
        .await
        .expect("legacy sidecar");

    let store = AsyncFileStore::new(root, &Key::new());
    let keys = store.keys().await?;

    // Nothing the store cannot address is enumerated …
    let reserved = parse_key("data/__metadata__")?;
    assert!(!keys.contains(&reserved));
    // … and asking for it directly is refused rather than silently served.
    assert!(!store.is_supported(&reserved));
    let error = store.get_bytes(&reserved).await.expect_err("refused");
    assert_eq!(error.error_type, ErrorType::KeyNotSupported);
    Ok(())
}
```

Three states of the code, and only the third is acceptable:

| | `listdir("data")` | `keys()` |
|---|---|---|
| **HEAD** | returns `__metadata__` — the filter matches `.__metadata__` with the dot, so the bare folder slips through | succeeds, and returns `data/__metadata__` and `data/__metadata__/report.txt.json` as ordinary keys |
| **Path builders fixed, filter not** | still returns `__metadata__` | **fails outright.** `listdir_keys_deep` calls `is_dir` on every child (`store.rs:535`); `is_dir` goes through `key_to_path`, which now refuses — and the error propagates out of the whole enumeration |
| **Both fixed** | skips `__metadata__` | succeeds, and the folder is simply not there |

The middle row is the reason the listing filters are in scope. `STORE_SEMANTICS.md` §8 already
forbids it in words — "a path a store cannot decode is **skipped** by listings rather than failing
them — one unexpected object in a shared bucket must not make a directory unlistable" — and it is
the same failure shape as `CORE-STORE-ROUTER-KEYS-FAILS-ON-AN-EMPTY-MEMBER`, where one member's
missing prefix directory takes down `keys()` for the whole router.

## Example 3 — Pitfalls

### The lock file is the worse collision, and `sidecar03` never looks at it

`AsyncFileStore` takes `foo.__lock__` while writing `foo`. A *data* file at that path is therefore
not a corruption but a deadlock:

```rust
let lock_shaped = parse_key("report.txt.__lock__")?;
// At HEAD this succeeds and leaves a permanent file at `report.txt.__lock__`.
store.set(&lock_shaped, b"anything", &blank).await?;

// From here on, every write to `report.txt` fails — `acquire_lock` opens the lock path with
// `create_new(true)`, which fails with AlreadyExists, retries 300 times at 10ms, and gives up.
let error = store.set(&report, b"new body", &blank).await.expect_err("timed out");
```

Three seconds per attempt, forever, until someone deletes the file out of band. This is why `Q1`
put `.__lock__` in scope even though the issue names only `.__metadata__`: `is_supported` already
refused it, and the conformance suite has no rule that would have found it.

### Recovering a store that was already corrupted

The fix refuses the key, which also means you can no longer address the orphan file *as a key* to
clean it up. Three routes remain, and they are worth knowing before someone concludes the data is
stranded:

| Want | Do |
|---|---|
| Repair the metadata of `report.txt` | `store.get(&report)` — it already rewrites unparseable metadata, with warnings |
| Replace it deliberately | `store.set_metadata(&report, &good)` |
| Remove both the asset and the orphan | `store.remove(&report)` — `remove` unlinks the data path *and* the metadata path |

A leftover `report.txt.__lock__` is the one case needing a filesystem-level delete, because no key
addresses it any more. Worth a sentence in the guide.

### The error type depends on which refusal comes first

`as_absolute()?` runs before the reserved-name check in every path builder, deliberately:

```rust
let both = parse_key("../report.txt.__metadata__")?;   // relative AND reserved
let error = store.key_to_path(&both).expect_err("refused");
assert_eq!(error.error_type, ErrorType::KeyNotAbsolute);   // not KeyNotSupported
```

A relative key is not a store address at all, so that answer is the more fundamental one, and
`keyabs08`/`keyabs09` depend on it. `reserved05` pins the ordering so nobody "tidies" the checks
into the other order.

### Over-reserving is a bug too

`x.__lock__` is a perfectly good key on `AsyncOpenDALStore`, which takes no locks. A single global
reserved list would refuse it for nothing — which is why `ReservedNames` is declared per store and
why `FileStore`, the synchronous one, reserves the metadata suffix only.

```rust
// liquers-store: this must keep working.
assert!(!PathMap::RESERVED.is_reserved_key(&parse_key("x.__lock__")?));
```

## Unit Tests

### Where these tests go

**Not in `mod tests`.** `liquers-core/src/store.rs` has two test modules at HEAD, and the difference
matters: `mod tests` (line 2145) is the general store suite, and `mod key_absolute_tests` (line
2515) is the suite for the absolute-key precondition, carrying `keyabs01`-`keyabs17`, its own
`use crate::error::ErrorType`, and the `unique_temp_dir` helper at line 2524. **`unique_temp_dir`
exists only there** — `mod tests`'s one file-store test builds its temp path inline (line 2438) —
so appending the `reserved` tests to `mod tests` would not compile.

They get a third module, a sibling of the second:

```rust
/// Tests for the reserved-name rule (`specs/design/sidecar-colliding-keys/`).
///
/// A sibling of `key_absolute_tests`, and deliberately not part of it. That module asks whether a
/// key is an *address* at all; this one asks whether this store can represent it. They are two
/// refusals with two error types, and they meet in exactly one place — the order they are checked
/// in, which `reserved05` pins.
#[cfg(test)]
mod reserved_name_tests {
    use super::*;
    use crate::error::ErrorType;
    use crate::parse::parse_key;

    /// As `key_absolute_tests` does it — nanosecond-stamped, because `cargo test` runs these in
    /// parallel. A third copy of six lines, following the precedent rather than introducing a
    /// shared test-support module in a change this size.
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

    // … assert_not_supported, then reserved01 … reserved08.
}
```

`Metadata`, `MetadataRecord`, `Error`, `Key`, `AsyncFileStore`, `FileStore`, `AsyncStore` and
`Store` all arrive through `use super::*` (they are imported or defined at `store.rs:53-56`).
`ErrorType` and `parse_key` do not, which is why both are named explicitly above.

Then a shared helper. Every `reserved` test asserts the same two things about a refusal — that it
*is* one, and that it is the right kind — and writing that out per method is what makes a test
list methods rather than check them.

```rust
/// Assert that a store refused a key as unrepresentable, rather than failing some other way.
///
/// Generic over the success type so one helper covers `PathBuf`, `Vec<u8>`, `bool` and `()`.
fn assert_not_supported<T>(result: Result<T, Error>, what: &str) {
    match result {
        Ok(_) => panic!("{what} must be refused"),
        Err(error) => assert_eq!(error.error_type, ErrorType::KeyNotSupported, "{what}"),
    }
}
```

### `reserved01` — the predicate itself

```rust
/// `reserved01` — `ReservedNames` recognises both forms of a reserved name, and nothing else.
///
/// The negatives are the half that matters. A predicate written as `name.contains("__metadata__")`
/// passes every positive below and refuses four keys a store can address perfectly well, so the
/// positives alone would not distinguish a correct implementation from a destructive one.
#[test]
fn reserved01_reserved_names_recognises_both_forms() -> Result<(), Error> {
    let file_store =
        ReservedNames::new(&[METADATA_SUFFIX, LOCK_SUFFIX], &[METADATA_FOLDER]);

    for name in [
        "collide.__metadata__",   // the sidecar of `collide`
        "__metadata__",           // the legacy metadata folder
        "collide.__lock__",       // the lock taken while writing `collide`
    ] {
        assert!(file_store.is_reserved_name(name), "{name} must be reserved");
    }

    for name in [
        "metadata",               // not the reserved name at all
        "x.__metadata__.txt",     // the suffix is not final — this is an ordinary file
        "__metadata__x",          // the bare form is a prefix here, not the whole name
        "x.__metadata",           // truncated
        "x.__lock",
        // Reserved *exactly* is declared per name, not derived from every suffix: no layout has
        // ever used a `__lock__` directory, so this is an ordinary name. See Phase 2 §Data
        // Structures — the Phase 4 review caught the earlier derivation reserving it for nothing.
        "__lock__",
    ] {
        assert!(!file_store.is_reserved_name(name), "{name} must NOT be reserved");
    }

    // A key is reserved when ANY segment is — the filename is not privileged.
    for text in ["collide.__metadata__", "data/collide.__metadata__", "data/__metadata__/x.json"] {
        assert!(file_store.is_reserved_key(&parse_key(text)?), "{text}");
    }
    for text in ["data/report.txt", "metadata/report.txt", "data/x.__metadata__.txt"] {
        assert!(!file_store.is_reserved_key(&parse_key(text)?), "{text}");
    }
    Ok(())
}
```

### `reserved02` — the refusal is uniform, and the metadata survives

```rust
/// `reserved02` — `AsyncFileStore` refuses a sidecar-colliding key from every fallible method,
/// and the metadata it collides with is still intact afterwards.
///
/// This is the reproduction of `CORE-FILE-STORE-WRITES-METADATA-COLLIDING-KEYS`, and three details
/// are what make it one rather than a restatement of the guard:
///
/// 1. **Every fallible method, not a representative sample.** The bug was precisely that
///    `is_supported` and the operations disagreed; a test that checked two operations could pick
///    the two that happened to be guarded.
/// 2. **The metadata of `collide` is read back after the refused write.** Asserting only
///    `KeyNotSupported` would still pass a fix that wrote the bytes and then returned an error —
///    which is the exact failure being fixed, merely with a better return value.
/// 3. **`get_metadata` is asserted, not `get`.** `get` *repairs* metadata it cannot parse, by
///    synthesizing a fresh record and writing it back. Against unfixed code it therefore returns
///    `Ok` and hides the corruption; `get_metadata` is what makes it visible.
#[tokio::test]
async fn reserved02_async_file_store_refuses_a_colliding_key_uniformly() -> Result<(), Error> {
    let sandbox = unique_temp_dir("reserved02");
    let root = sandbox.join("root");
    tokio::fs::create_dir_all(&root).await.expect("create root");
    let store = AsyncFileStore::new(root.to_string_lossy().as_ref(), &Key::new());

    // An ordinary asset whose metadata is worth protecting.
    let victim = parse_key("collide")?;
    let mut record = MetadataRecord::new();
    record.with_key(victim.clone()).with_title("do not lose me".to_owned());
    store.set(&victim, b"body", &Metadata::MetadataRecord(record)).await?;

    // Its data path is byte-identical to the metadata path of `collide`.
    let collide = parse_key("collide.__metadata__")?;
    assert!(!store.is_supported(&collide), "the routing hint already refused it at HEAD");

    let blank = Metadata::MetadataRecord(MetadataRecord::new());
    // Detail 1: every fallible method.
    assert_not_supported(store.key_to_path(&collide), "key_to_path");
    assert_not_supported(store.key_to_path_metadata(&collide), "key_to_path_metadata");
    assert_not_supported(store.get_bytes(&collide).await, "get_bytes");
    assert_not_supported(store.get(&collide).await, "get");
    assert_not_supported(store.get_metadata(&collide).await, "get_metadata");
    assert_not_supported(store.contains(&collide).await, "contains");
    assert_not_supported(store.is_dir(&collide).await, "is_dir");
    assert_not_supported(store.listdir(&collide).await, "listdir");
    assert_not_supported(store.set(&collide, b"corrupt", &blank).await, "set");
    assert_not_supported(store.set_metadata(&collide, &blank).await, "set_metadata");
    assert_not_supported(store.remove(&collide).await, "remove");
    assert_not_supported(store.makedir(&collide).await, "makedir");
    assert_not_supported(store.removedir(&collide).await, "removedir");

    // Details 2 and 3: the write did not happen, and the victim can still be described.
    match store.get_metadata(&victim).await? {
        Metadata::MetadataRecord(record) => assert_eq!(record.title, "do not lose me"),
        Metadata::LegacyMetadata(_) => panic!("the sidecar was overwritten"),
    }
    assert_eq!(store.get_bytes(&victim).await?, b"body".to_vec());

    tokio::fs::remove_dir_all(&sandbox).await.expect("cleanup");
    Ok(())
}
```

### `reserved03` — any segment, not the filename

```rust
/// `reserved03` — a reserved name anywhere in the key is refused, not only as the filename.
///
/// `dir.__metadata__/child` has an innocent filename. It is still unaddressable: this key needs
/// `dir.__metadata__` to be a directory, while the metadata of `dir` needs it to be a file, and a
/// filesystem will not be both. `Key::filename()` returns the last segment only, which is exactly
/// how the check at HEAD missed this shape.
///
/// `a/__metadata__/b.json` is the legacy folder layout, reserved so that layout can be supported
/// again — see `STORE-METADATA-LAYOUT-HARDCODED-PER-STORE`.
#[tokio::test]
async fn reserved03_a_reserved_segment_anywhere_is_refused() -> Result<(), Error> {
    let sandbox = unique_temp_dir("reserved03");
    let root = sandbox.join("root");
    tokio::fs::create_dir_all(&root).await.expect("create root");
    let store = AsyncFileStore::new(root.to_string_lossy().as_ref(), &Key::new());

    for text in [
        "dir.__metadata__/child",   // interior sidecar name
        "a/__metadata__/b.json",    // the legacy metadata folder
        "a/x.__lock__/b",           // interior lock name
        "__metadata__",             // the folder itself, as a key
    ] {
        let key = parse_key(text)?;
        assert!(!store.is_supported(&key), "{text} must not route here");
        assert_not_supported(store.key_to_path(&key), text);
        assert_not_supported(store.key_to_path_metadata(&key), text);
        assert_not_supported(store.get_bytes(&key).await, text);
        assert_not_supported(store.set(&key, b"x", &Metadata::MetadataRecord(MetadataRecord::new())).await, text);
    }

    tokio::fs::remove_dir_all(&sandbox).await.expect("cleanup");
    Ok(())
}
```

### `reserved04` — each store reserves what its own layout uses

```rust
/// `reserved04` — the synchronous `FileStore` reserves the metadata name and **not** the lock name.
///
/// This is the test that pins the reserved set to the store rather than to the crate. `FileStore`
/// takes no lock files, so `x.__lock__` is a key it can address, and a single global reserved list
/// would refuse it for nothing. Over-reserving is a defect in the same family as under-reserving:
/// both make the store disagree with its own layout.
#[test]
fn reserved04_file_store_reserves_metadata_but_not_lock() -> Result<(), Error> {
    let sandbox = unique_temp_dir("reserved04");
    let root = sandbox.join("root");
    std::fs::create_dir_all(&root).expect("create root");
    let store = FileStore::new(root.to_string_lossy().as_ref(), &Key::new());

    // Both forms, and an interior segment — `FileStore` is "identical to `AsyncFileStore`, minus
    // the lock", so the segment rule has to hold here too and not only in the async twin.
    for text in ["file.__metadata__", "__metadata__", "data/__metadata__/file.json"] {
        let key = parse_key(text)?;
        assert!(!store.is_supported(&key), "{text}");
        assert_not_supported(store.key_to_path(&key), text);
        assert_not_supported(store.key_to_path_metadata(&key), text);
    }

    // The lock suffix belongs to `AsyncFileStore`'s layout, not this one.
    let lock_shaped = parse_key("file.__lock__")?;
    assert!(store.is_supported(&lock_shaped), "FileStore takes no locks — this key is addressable");
    assert!(store.key_to_path(&lock_shaped).is_ok());

    std::fs::remove_dir_all(&sandbox).expect("cleanup");
    Ok(())
}
```

### `reserved05` — which refusal comes first

```rust
/// `reserved05` — a key that is both relative and reserved reports `KeyNotAbsolute`.
///
/// `as_absolute()?` runs before the reserved-name check in every path builder, and this pins that
/// order. A relative key is not a store address at all, so that is the more fundamental answer;
/// `keyabs08` and `keyabs09` assert `KeyNotAbsolute` for traversal shapes and would start failing
/// if someone reordered the two checks while "tidying" them into one guard.
#[tokio::test]
async fn reserved05_relative_and_reserved_reports_key_not_absolute() -> Result<(), Error> {
    let sandbox = unique_temp_dir("reserved05");
    let root = sandbox.join("root");
    tokio::fs::create_dir_all(&root).await.expect("create root");
    let store = AsyncFileStore::new(root.to_string_lossy().as_ref(), &Key::new());

    for text in ["../x.__metadata__", "a/../x.__metadata__", "a/./x.__metadata__"] {
        let key = parse_key(text)?;
        assert!(key.is_relative(), "{text} must be relative for this test to mean anything");
        let error = store.key_to_path(&key).expect_err("refused");
        assert_eq!(error.error_type, ErrorType::KeyNotAbsolute, "{text}");
        let error = store.get_bytes(&key).await.expect_err("refused");
        assert_eq!(error.error_type, ErrorType::KeyNotAbsolute, "{text}");
    }

    tokio::fs::remove_dir_all(&sandbox).await.expect("cleanup");
    Ok(())
}
```

### `reserved06` — the listing filter, which is the half a partial fix forgets

```rust
/// `reserved06` — a real `__metadata__` directory is skipped by `keys()`, not fallen over.
///
/// This is the test for the middle row of Example 2, and it is the one that fails against a fix
/// applied to the path builders alone. The chain is `keys()` → `listdir_keys_deep` →
/// `listdir_keys` → `listdir`; `listdir_keys_deep` calls `is_dir` on every child it was handed
/// (`store.rs:535`), and `is_dir` goes through `key_to_path`. So an unfiltered reserved name turns
/// a refusal into a **failed enumeration** — the store stops being listable at all.
///
/// The directory is created on the filesystem directly rather than through `makedir`, because
/// after the fix `makedir` refuses it — which is the point: this state is left behind by an older
/// Liquers or an outside process, not reachable through the API.
#[tokio::test]
async fn reserved06_a_reserved_directory_is_skipped_by_listings() -> Result<(), Error> {
    let sandbox = unique_temp_dir("reserved06");
    let root = sandbox.join("root");
    tokio::fs::create_dir_all(root.join("__metadata__"))
        .await
        .expect("legacy metadata folder");
    tokio::fs::write(root.join("__metadata__").join("report.txt.json"), b"{}")
        .await
        .expect("legacy sidecar");
    // An ordinary asset beside it, so a passing test means "skipped", not "listed nothing".
    tokio::fs::write(root.join("report.txt"), b"body")
        .await
        .expect("data file");

    let store = AsyncFileStore::new(root.to_string_lossy().as_ref(), &Key::new());

    let names = store.listdir(&Key::new()).await?;
    assert!(!names.contains(&"__metadata__".to_owned()), "{names:?}");
    assert!(names.contains(&"report.txt".to_owned()), "{names:?}");

    // The enumeration must succeed. Against a path-builders-only fix this line is where it fails.
    let keys = store.keys().await?;
    let encoded: Vec<String> = keys.iter().map(|k| k.encode()).collect();
    assert!(
        !encoded.iter().any(|k| k.starts_with("__metadata__")),
        "keys() must skip the reserved subtree: {encoded:?}"
    );
    assert!(encoded.iter().any(|k| k == "report.txt"), "{encoded:?}");

    tokio::fs::remove_dir_all(&sandbox).await.expect("cleanup");
    Ok(())
}
```

### `reserved07` — the sync store's listing filter, which nothing else would catch

```rust
/// `reserved07` — `FileStore` filters its listing by the same predicate, and by *its own* set.
///
/// Added at the Phase 3 review, which found this the one edit in Phase 2 with no test behind it.
/// The synchronous store is obsolete and unreachable (`CORE-SYNC-STORE-TRAIT-OBSOLETE`), and that
/// is precisely why it needs its own test rather than being trusted to follow `AsyncFileStore`:
/// nothing else exercises it, so a filter updated in one store and forgotten in the other would
/// stay invisible until the trait is revived or deleted.
///
/// The second assertion is the per-store half. `FileStore` takes no locks, so a file genuinely
/// named `x.__lock__` is an ordinary asset here and must still be listed — the same claim
/// `reserved04` makes about the path builders, made about the listing.
#[test]
fn reserved07_file_store_listing_uses_its_own_reserved_set() -> Result<(), Error> {
    let sandbox = unique_temp_dir("reserved07");
    let root = sandbox.join("root");
    std::fs::create_dir_all(root.join("__metadata__")).expect("legacy metadata folder");
    std::fs::write(root.join("__metadata__").join("report.txt.json"), b"{}")
        .expect("legacy sidecar");
    std::fs::write(root.join("report.txt"), b"body").expect("data file");
    std::fs::write(root.join("report.txt.__metadata__"), b"{}").expect("sidecar");
    std::fs::write(root.join("notes.__lock__"), b"not a lock here").expect("lock-shaped file");

    let store = FileStore::new(root.to_string_lossy().as_ref(), &Key::new());
    let names = store.listdir(&Key::new())?;

    // Reserved by this store's layout — dropped.
    assert!(!names.contains(&"__metadata__".to_owned()), "{names:?}");
    assert!(!names.contains(&"report.txt.__metadata__".to_owned()), "{names:?}");
    // Not reserved by this store's layout — listed.
    assert!(names.contains(&"report.txt".to_owned()), "{names:?}");
    assert!(names.contains(&"notes.__lock__".to_owned()), "{names:?}");

    std::fs::remove_dir_all(&sandbox).expect("cleanup");
    Ok(())
}
```

### `reserved08` — the upgrade path, which Example 3 promises and nothing else checks

```rust
/// `reserved08` — a store that already holds a corrupted sidecar can still be repaired.
///
/// Added at the Phase 3 review. The fix refuses the colliding key, which also means the orphan can
/// no longer be addressed *as a key* in order to clean it up — so Example 3 lists three routes out,
/// and this is what keeps that list honest. A documented upgrade path nobody exercises is a rumour.
///
/// The corruption is written with `tokio::fs` rather than through the store, because after the fix
/// no API can produce it. That is the point, and it is also why this test cannot be written as a
/// conformance rule: the suite only ever reaches a store through the trait.
#[tokio::test]
async fn reserved08_an_existing_corruption_can_still_be_repaired() -> Result<(), Error> {
    let sandbox = unique_temp_dir("reserved08");
    let root = sandbox.join("root");
    tokio::fs::create_dir_all(&root).await.expect("create root");
    let store = AsyncFileStore::new(root.to_string_lossy().as_ref(), &Key::new());

    let report = parse_key("report.txt")?;
    let mut record = MetadataRecord::new();
    record.with_key(report.clone()).with_title("before".to_owned());
    store.set(&report, b"body", &Metadata::MetadataRecord(record)).await?;

    // Exactly what a pre-fix `set("report.txt.__metadata__", …)` left behind.
    let sidecar = root.join("report.txt.__metadata__");
    tokio::fs::write(&sidecar, b"not json at all").await.expect("corrupt the sidecar");
    assert!(store.get_metadata(&report).await.is_err(), "the corruption must be real");

    // Route 1 — `get` repairs metadata it cannot parse, and returns the data intact.
    let (data, _) = store.get(&report).await?;
    assert_eq!(data, b"body".to_vec());
    match store.get_metadata(&report).await? {
        Metadata::MetadataRecord(_) => {}
        Metadata::LegacyMetadata(_) => panic!("repaired into legacy metadata"),
    }

    // Route 2 — replace it deliberately.
    let mut good = MetadataRecord::new();
    good.with_key(report.clone()).with_title("after".to_owned());
    store.set_metadata(&report, &Metadata::MetadataRecord(good)).await?;
    match store.get_metadata(&report).await? {
        Metadata::MetadataRecord(record) => assert_eq!(record.title, "after"),
        Metadata::LegacyMetadata(_) => panic!("unexpected legacy metadata"),
    }

    // Route 3 — `remove` unlinks the data path *and* the metadata path, so the orphan goes too.
    store.remove(&report).await?;
    assert!(!sidecar.exists(), "remove must unlink the sidecar");
    assert!(!root.join("report.txt").exists());

    tokio::fs::remove_dir_all(&sandbox).await.expect("cleanup");
    Ok(())
}
```


## Conformance and OpenDAL Tests

### The `C2` fixture, and the allowed failure that must go

```rust
/// `C2` — `AsyncFileStore` over a temporary directory.
///
/// **`derived_directories` is false**, and that is the point of the capability: a real filesystem
/// directory is an object in its own right and survives its last file, so `explicit02` must not be
/// asked of it.
#[tokio::test]
async fn c2_async_file_store() {
    let root = unique_temp_dir("c2");
    tokio::fs::create_dir_all(&root).await.expect("temp root");

    let mut capabilities = index_backed();
    capabilities.derived_directories = false;

    let fixture = GenericFixture::new(
        "AsyncFileStore(temp dir)",
        Box::new(AsyncFileStore::new(root.to_string_lossy().as_ref(), &Key::new())),
        Key::new(),
        capabilities,
        SafetyLevel::Scratch,
    )
    // The sidecar suffix makes this key's data path collide with `collide`'s metadata path.
    .with_metadata_collision(parse_key("collide.__metadata__").expect("key"))
    // The same key, declared as a shape the store cannot address. Until this was declared,
    // `prefix03` and `sibling05` had never run against a file store: the report said "not run"
    // and nothing failed, which is the quiet way for coverage to be missing.
    .with_unsupported_shape(parse_key("collide.__metadata__").expect("key"));

    // No allowed failures. `sidecar03` was listed here until the path builders learned to refuse
    // a reserved key; `H5` then reported the entry as stale, which is how a fixed issue forces its
    // own bookkeeping out. See CORE-FILE-STORE-WRITES-METADATA-COLLIDING-KEYS.
    let report = run_all(&fixture).await;

    // `assert_conformant` inspects failures and stale allowed-failures; a rule that declined its
    // precondition is invisible to it, and the report only reaches stderr, which `cargo test`
    // swallows on success. So a `with_unsupported_shape` that was dropped or mistyped would leave
    // these two "not run" and this test would still pass — which is exactly how they came to have
    // never run against a file store in the first place. Assert the outcome, not the absence of a
    // failure. Raised by the Phase 4 final review.
    for rule in ["prefix03", "sibling05"] {
        let entry = report
            .entries
            .iter()
            .find(|entry| entry.id == rule)
            .unwrap_or_else(|| panic!("{rule} is not in the report at all"));
        assert!(
            matches!(entry.outcome, RuleOutcome::Passed),
            "{rule} must run and pass for C2, not decline its precondition: {:?}",
            entry.outcome
        );
    }

    check(report, &[]);

    let _ = tokio::fs::remove_dir_all(&root).await;
}
```

Removing the entry is **required, not tidy**: `H5` fails the assertion when an allowed rule starts
passing, so leaving it would turn a fixed bug into a red test.

### What changes in the `C2` report

| Rule | At HEAD | After | Why |
|---|---|---|---|
| `sidecar01` | passes | passes | It only ever checked `is_supported`, which refused the key at HEAD too. Unchanged — and this is exactly why it was not enough |
| `sidecar03` | **fails** (allowed) | passes | `set` reaches `acquire_lock` → `key_to_lock_path` → refusal, before any directory is created or byte written |
| `prefix03` | **not run** | passes | The fixture had no `UnsupportedShape` key, so the rule declined its precondition. Now it checks that `is_supported` is false for a shape the store cannot address |
| `sibling05` | **not run** | passes | Same precondition. It then asserts the key is not addressable as a *directory* either — `is_dir` returns `Err`, which the rule accepts as a legitimate refusal (only `Ok(true)` breaks the contract) |

Two rules moving from "not run" to "passing" is the quieter half of this change: no test was failing
because no test was running.

### OpenDAL — the same rule, the same predicate

```rust
// PathMap: `const METADATA` and `is_suffix_ambiguous` both go.
impl PathMap {
    /// What this store's layout reserves. It takes no locks, so the lock suffix is not here —
    /// `x.__lock__` is a key `AsyncOpenDALStore` can address perfectly well.
    pub const RESERVED: ReservedNames = ReservedNames::new(&[METADATA_SUFFIX]);
}

// reject_ambiguous (opendal_store.rs:144) and is_supported (:521):
if PathMap::RESERVED.is_reserved_key(key) { … }
```

The two listing sites each gain one guard. In `listdir`, immediately after the decode:

```rust
let Ok(decoded) = PathMap::decode(entry.path()) else {
    continue;
};
// A key this store cannot address is skipped for the same reason an undecodable path is:
// returning it would hand the caller a key that every other method then refuses.
if PathMap::RESERVED.is_reserved_key(decoded.key()) {
    continue;
}
```

and the same three lines in `listdir_keys_deep`, before the prefixes are extended — skipping the
whole entry, so a reserved *interior* segment takes its subtree with it. Note what this does **not**
break: `decode("x.__metadata__")` yields key `x`, which is not reserved, so sidecars still collapse
onto their data key as `pathmap02` requires. `decode("x.__metadata__.__metadata__")` yields
`x.__metadata__`, which *is* reserved and is now skipped — an orphan from before the fix.

The two renamed tests keep their IDs and gain the newly reserved shapes:

```rust
/// `PATHMAP03` — a reserved key is refused by every path entry point.
#[test]
fn pathmap03_reserved_keys_are_refused_everywhere() -> Result<(), Error> {
    use liquers_core::error::ErrorType;
    let store = memory_store();
    for text in [
        "a.__metadata__",
        "sub/a.__metadata__",
        "__metadata__",             // the legacy folder name
        "sub/__metadata__/a.json",  // and an interior segment
    ] {
        let key = parse_key(text)?;
        assert!(PathMap::RESERVED.is_reserved_key(&key), "{text} is reserved");
        assert!(!store.is_supported(&key), "{text} must not route here");
        for error in [
            store.key_to_path(&key).err(),
            store.key_to_path_metadata(&key).err(),
            store.key_to_path_dir(&key).err(),
        ] {
            let error = error.unwrap_or_else(|| panic!("{text} must be refused"));
            assert_eq!(error.error_type, ErrorType::KeyNotSupported, "{text}");
        }
    }
    // A name that merely *contains* the suffix is fine — and so is the lock suffix, which this
    // store's layout does not use.
    for text in ["x.__metadata__.txt", "x.__lock__"] {
        let ok = parse_key(text)?;
        assert!(!PathMap::RESERVED.is_reserved_key(&ok), "{text}");
        assert!(store.is_supported(&ok), "{text}");
    }
    Ok(())
}
```

`pathmap07_directory_form_refuses_reserved_keys` extends the same way, adding the bare folder name
to the shapes it drives through `makedir`, `removedir` and `listdir`.

```rust
/// `PATHMAP08` — a reserved path in a listing is skipped, not enumerated.
///
/// The counterpart of `pathmap06`, which covers a path the store cannot *decode*. This one covers
/// a path it decodes perfectly well and then cannot address, which is the newer of the two hazards
/// and the one a bare `__metadata__` folder produces.
#[tokio::test]
async fn pathmap08_reserved_listing_entries_are_skipped() -> Result<(), Error> {
    let op = Operator::new(Memory::default())
        .expect("memory operator")
        .finish();
    op.write("__metadata__/report.txt.json", "{}")
        .await
        .expect("legacy sidecar");
    op.write("report.txt", "body").await.expect("data object");
    let store = AsyncOpenDALStore::new(op, Key::new());

    let names = store.listdir(&Key::new()).await?;
    assert!(!names.contains(&"__metadata__".to_owned()), "{names:?}");
    assert!(names.contains(&"report.txt".to_owned()), "{names:?}");

    let encoded: Vec<String> = store.keys().await?.iter().map(|k| k.encode()).collect();
    assert!(
        !encoded.iter().any(|k| k.starts_with("__metadata__")),
        "keys() must skip the reserved subtree: {encoded:?}"
    );
    Ok(())
}
```

## Corner Cases

| Case | Resolution |
|---|---|
| **The root key** | `is_reserved_key(&Key::new())` iterates nothing and is `false`. It has to be: a reserved root would refuse every key in every store. Asserted in `reserved01` — cheap, and the one failure that would be total |
| **wasm32** | No gate needed on the new tests. `scripts/check-build-matrix.sh`'s `CORE_CONFIGS` wasm32 row is **library-only** (no `--tests`), which is why `keyabs08` already uses `AsyncFileStore` ungated. But `ReservedNames`, `METADATA_SUFFIX` and `LOCK_SUFFIX` must be declared **before** `store.rs:874`, outside the `#[cfg(not(target_arch = "wasm32"))]` region that begins there — otherwise the wasm library row loses a type that has no OS dependency |
| **A store prefix that is itself reserved** | `AsyncFileStore::new(root, &parse_key("__metadata__")?)` builds a store in which every key is refused. Out of scope: `new` is infallible and making it fallible is an API break for every caller. Named here so the next reader knows it was seen and not missed |
| **Lock-guard leakage** | The refusal happens *inside* `acquire_lock`, at `key_to_lock_path`, before `create_new` runs — so no lock file is created and no `FileLockGuard` is constructed. A refusal cannot leave a stale lock that blocks later writes, which would have turned this fix into the very deadlock it prevents |
| **Metadata-only keys** | The listing filters change *which* predicate they use, not their drop-versus-report behaviour. A sidecar with no data file is still dropped rather than reported as its implied data key, which §8 requires and `AsyncOpenDALStore` does. Pre-existing, out of scope, and filed while it was in view: `CORE-FILE-STORE-LISTDIR-DROPS-METADATA-ONLY-KEYS` (P2, S) |
| **Router dispatch** | `AsyncStoreRouter` asks each member's `is_supported`; a reserved key now matches no member and the router answers `KeyNotSupported`. Unchanged behaviour, reached by one more class of key. `C3` covers it |
| **Serialization, features, memory** | `ReservedNames` derives no `Serialize` and enters no config — that is `STORE-METADATA-LAYOUT-HARDCODED-PER-STORE`'s job. It adds no feature flag, and `is_reserved_key` allocates nothing: it borrows each segment's `String` and compares |
| **`listdir_asset_info` / `get_asset_info`** | Both build on `listdir_keys`, which is built on the now-filtered `listdir`, so a reserved key never reaches them. No test: they have no path to the shape. Raised by the Phase 3 review and declined with this reason rather than left unanswered |
| **The router, asked for a reserved key directly** | `C3` already runs the whole suite against `AsyncStoreRouter` over a memory store and a file store. Adding a reserved key to that fixture would test `AsyncFileStore`'s refusal a second time through one more layer of dispatch, which `keyabs10` already establishes works. Declined |
| **`get` repairs metadata it cannot parse** | Load-bearing for recovery route 1 in `reserved08`, and **not stated anywhere in `STORE_SEMANTICS.md`** — it is behaviour the code has and the contract does not describe. Pre-existing, not introduced here; carried into the Phase 5 documentation pass, where §8 is being edited anyway |
| **Keys of these shapes parse at all** | Verified with `liquers-validate --no-registry`, not assumed: `-R/collide.__metadata__`, `-R/data/__metadata__/x.json`, `-R/dir.__metadata__/child` and `-R/report.txt.__lock__` all parse, and the third encodes back to `-R/dir.__metadata__/child` with `dir.__metadata__` as its own segment. If they did not parse, most of this test list would be unwritable |

## Test Plan

Run in this order — cheapest and most diagnostic first. A failure in step 1 means the predicate is
wrong; a failure that first appears in step 2 means a caller was missed.

```bash
# 1. The predicate and the two file stores. reserved01-reserved06, plus the keyabs regressions.
cargo test -p liquers-core --lib reserved   # reserved01-reserved08, in mod reserved_name_tests
cargo test -p liquers-core --lib keyabs        # must be unchanged — reserved05 exists to protect these

# 2. The contract, against every liquers-core store. C2 is the one that changes.
cargo test -p liquers-core --features store-conformance --test store_conformance_CONF

# 3. The OpenDAL half: pathmap01-pathmap08 and its own conformance suites.
cargo test -p liquers-store

# 4. The wasm32 library row, because a new public type lands in store.rs and must not fall inside
#    the target-gated region.
bash scripts/check-build-matrix.sh
```

**What must not change.** `keyabs08`, `keyabs09`, `keyabs10` and `keyabs17` assert `KeyNotAbsolute`
for traversal shapes; `reserved05` exists precisely so that the ordering they depend on is pinned
rather than incidental. `C1`, `C3`, `C4`, `C5` and the `liquers-store` suites must report no new
failures and no newly-stale allowed failures — `H5` turns the latter into an assertion failure, so
a rule that starts passing forces its own bookkeeping out.

**The one expected report change**, and the only one: `C2` loses its `AllowedFailure` for
`sidecar03`, and `prefix03` and `sibling05` move from "not run" to passing.
## Documentation and Learning Log

What belongs in the two documents Phase 2 committed to, and which executable artefact backs each.

| Question a reader arrives with | Goes in | Backed by |
|---|---|---|
| "Which keys must my store refuse?" | `STORE_SEMANTICS.md` §8 — the rule, restated as reserved names in any segment, declared by the layout | `reserved01`, `sidecar01` |
| "Where do I put the check?" | `STORE_IMPLEMENTATION_GUIDE.md` §"The key space" — one predicate, three kinds of caller; `is_supported` returns `bool` and cannot carry an error, which is *why* it is a predicate and not a fallible builder | `reserved02` |
| "Why isn't `is_supported` enough?" | Guide, same section — it is a routing hint; `AsyncStoreRouter` consults it and nothing else does, so a store that refuses only there corrupts data through any direct caller | `reserved02`, `sidecar03` |
| "Do listings need to know?" | Guide, same section — yes, and skipping is mandatory rather than tidy: `listdir_keys_deep` calls `is_dir` on every child, so an unfiltered reserved name turns a refusal into a failed enumeration | `reserved06`, `pathmap08` |
| "My store already has one of these files. Now what?" | Guide, short recovery note | Example 3's table |
| "How do I check I got it right?" | Guide §5, already written — add that `prefix03` and `sibling05` need a fixture to *declare* an unsupported shape or they silently do not run | the `C2` fixture change |

**Learning worth carrying to Phase 5**, recorded now while it is fresh:

1. **A routing hint that is also a correctness check is a trap.** `is_supported` has two jobs in the
   trait — pick a router member, and describe what the store can address — and only the first has a
   caller obliged to use it. Every store that answers the second question there and nowhere else has
   this bug latent.
2. **The half-fix is worse than the bug.** Guarding the path builders without the listing filters
   turns silent corruption into a store whose `keys()` fails. Phase 2 found this by tracing
   `listdir_keys_deep`; nothing in the issue or the conformance report pointed at it.
3. **The conformance suite found the collision but could not have found the lock.** `sidecar03`
   exists because a review noticed `sidecar01` checked only `is_supported`. The identical gap for
   `.__lock__` has no rule, and its consequence — a permanent write deadlock — is worse. Whether
   that deserves a rule is a question for `STORE-METADATA-LAYOUT-HARDCODED-PER-STORE`, which is
   where the layout becomes describable enough for a rule to ask about it.
4. **A fixture that does not declare a shape silently skips the rules about it.** `prefix03` and
   `sibling05` have never run against `AsyncFileStore`. Nothing failed; the report said "not run",
   and no one read it as a gap.

## Review Record

Three independent reviewers (focused tier), run in parallel per the workflow. Two findings were
acted on, one was found outside the reviews, and four reported findings were rejected — recorded
here rather than quietly dropped, because a rejected finding that leaves no trace gets re-raised.

**Acted on:**

| From | Finding | Response |
|---|---|---|
| Conformity reviewer | `FileStore::listdir` was the one edit in Phase 2 with no test behind it | `reserved07` |
| Adversarial reviewer | `FileStore::is_supported` interior segments untested — `reserved03` covers the async store only | `reserved04` extended to both forms and an interior segment |
| Adversarial reviewer | The upgrade path in Example 3 — three recovery routes out of an already-corrupted store — is asserted by nothing | `reserved08`, the best finding of the round |
| Adversarial reviewer | `listdir_asset_info`, the router, and `get`'s repair-on-read | Answered in Corner Cases with reasons; the third carried to Phase 5 |
| *Not from a reviewer* | The document said to append to `mod tests`, but `store.rs` has **two** test modules and `unique_temp_dir` lives only in `key_absolute_tests` — the code would not have compiled | New `mod reserved_name_tests`, a sibling of that module |

**Rejected, with reasons:**

- *"`ReservedNames`, `METADATA_SUFFIX` and `PathMap::RESERVED` do not exist — 6 tests cannot
  compile"* (compile reviewer, filed as blocking). They do not exist **yet**; Phase 4 creates them.
  A Phase 3 document that only used APIs already at HEAD could not describe a change. The same
  reviewer's five "advisory" findings are the tests correctly failing against unfixed code, which
  is the property the overview table is built on.
- *"`AsyncOpenDALStore::is_supported` is not tested for reserved keys"* (adversarial). It is —
  `pathmap03` asserts `!store.is_supported(&key)` for all four shapes.
- *"Phase 3 does not explain that `reserved05` enforces the ordering the `keyabs` tests depend
  on"* (adversarial). It does, in `reserved05`'s own doc comment and again in the Test Plan.

The compile reviewer's genuine contribution was verification rather than criticism: it confirmed
against HEAD that `MetadataRecord::with_key(..).with_title(..)` chains, that `set` preserves the
title through to `get_metadata`, that every `AsyncStore` return type fits the generic
`assert_not_supported<T>`, that `Key::is_relative()` behaves as `reserved05` assumes, and that
`AsyncOpenDALStore::new(op, Key::new())` matches the real constructor.
