# Phase 3: Examples & Use-cases - Expired-Safe Binary Reads

Examples are **runnable tests** (user's choice at the Phase 3 gate), written against the Phase 2
signatures so Phase 4 can land them rather than rewrite them.

## Overview Table

### Examples

| # | Example | Demonstrates | Location |
|---|---|---|---|
| 1 | Expired asset with cached bytes | The bug and its fix: `poll_binary`/`try_poll_binary` → `None`, `get_binary` → `Err`, bytes still retained | `assets.rs` `mod tests` |
| 2 | Explicit recovery | `*_binary_any_status` at all three layers returns the retained bytes | `assets.rs` `mod tests` |
| 3 | Statuses with no valid binary | `Error` reuses its recorded error, `Cancelled` gets a constructed one; `poll_state` unchanged | `assets.rs` `mod tests` |

### Tests

| # | Test | Pins down | Kind |
|---|---|---|---|
| U1 | `read_exposure` for all 15 statuses | Each variant's bucket, asserted individually | unit, `metadata.rs` |
| U2 | Classifier exhaustiveness guard | Adding a `Status` variant fails the build | unit, `metadata.rs` |
| U3 | `has_data()` is not the gate | `has_data()` true for `Expired`+`Partial` where exposure is not `Value` | unit, `metadata.rs` |
| U4 | `poll_state` agrees with `read_exposure` | Phase 2's claim that state reads are unchanged | unit, `assets.rs` |
| U5 | `poll_binary` gating across statuses | `Some` only for `Value`; `None` for the other 11 | unit, `assets.rs` |
| U6 | `poll_binary_any_status` | `Some` for `Value` and `Expired`; `None` otherwise | unit, `assets.rs` |
| U7 | `binary_unchecked` is status-blind | Returns bytes for **every** status — its contract | unit, `assets.rs` |
| U8 | `get_binary` error identity | `Error` reuses recorded error; `Cancelled`/`Directory` constructed | unit, `assets.rs` |
| U9 | `get` pre-wait expiry check | `get` on an already-`Expired` asset returns `Err` instead of blocking — the state-side twin of the `get_binary` fix | unit, `assets.rs` |
| U10 | `get_binary` serializes on demand | `Value` exposure with **no** cached bytes still yields bytes — the gate is not "binary required" | unit, `assets.rs` |
| I1 | Persistence does not regress | Bytes reach the store when status is not `Value`-exposure | integration, `assets.rs` (see §Access) |
| I2 | End-to-end expiry | Real evaluation → expiry → matrix behaviour | integration, `expiration_integration.rs` |
| I3 | Manager re-request still rebuilds | `Expired` remains a cache miss at the request boundary | integration, `expiration_integration.rs` |
| I4 | Fast-track after expiry | An evicted+expired keyed asset does not fast-track stale bytes back | integration, `expiration_integration.rs` |

## Verified Setup Facts

The drafting pass produced code against several APIs that **do not exist**. These were checked
against the source; Phase 4 should treat this list as binding.

| Assumption | Reality |
|---|---|
| `AssetRef::set_binary(...)` | **Does not exist.** `set_binary` is an `AssetManager` method (`assets.rs:2802`, `:4302`), not on `AssetRef`. |
| `State::new_with_value(v)` | **Does not exist.** Use `State::from_parts(Arc<V>, Arc<Metadata>)` or `State::from_value_and_metadata(v, meta)`. |
| `envref.evaluate("...")` | **Exists** — `EnvRef::evaluate<Q: TryToQuery>` (`context.rs:278`). An earlier draft of this table wrongly said otherwise. It returns `AssetRef` after submission, *not* after evaluation: with a queued manager you must still `get()` to wait for a value. |
| `try_poll_binary` is new | **Already exists** (`assets.rs:2456`). It is gated, not added. |
| `Recipe::new(a, b, c)` | Check `recipes.rs:62` before use; the drafted 3-argument form was not verified. |

**Test module locations** (verified, for Phase 4): `metadata.rs` already has `#[cfg(test)] mod tests`
at `:2314`; `assets.rs` at `:5472`, with `use super::*;` at `:5480` — so U4–U8 and I1 can reach
private fields and `pub(crate)` methods without new plumbing.

**These facts were double-checked** by an independent reviewer against the source, not taken on
trust from the drafting pass.

**How to get an asset that is `Expired` *and* still holds cached bytes** — the crux of the whole
suite. Only two sites populate `AssetData::binary`:

- `try_fast_track` (`assets.rs:679`) sets `binary`, `data`, `status` and `metadata` together from a
  store entry. **This is the cleanest setup**: write bytes to an `AsyncMemoryStore` with
  `Status::Ready`, build `AssetData::new(id, key.into(), envref)`, call `try_fast_track()`, then
  expire. No evaluation, no scheduler, no timing.
- `serialize_to_binary` (`assets.rs:2017`), reached via `save_to_store` during persistence — the
  path `evaluate_and_store()` takes. Use this for the end-to-end tests only.

Note that `expire()` accepts only `Ready` and `Override` (`assets.rs:2216`); everything else errors
or is a no-op. Fast-tracking a `Ready` entry therefore lands exactly where expiry is legal.

### Access: what tests can reach

`AssetRef::set_state`, `set_value` and `Context::set_state` are **`pub(crate)`**. So I1, which must
drive `set_state` with a non-`Value` status, **cannot be an integration test** in
`liquers-core/tests/` — it has to be an in-file test in `assets.rs`'s `mod tests`. Several drafted
"integration" tests made this mistake. Unit tests inside `assets.rs` can also assign
`asset_data.status` / `.binary` directly, which is how the existing tests at `assets.rs:5560-5760`
work.

## Example 1 — The bug, and its fix

An asset holding both a value and cached bytes expires. Every normal read must now decline; the
bytes must still be there.

```rust
#[tokio::test]
async fn test_expired_asset_hides_cached_binary() -> Result<(), Box<dyn std::error::Error>> {
    let key = parse_key("expired_with_binary.txt")?;
    let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
    env.with_async_store(Box::new(AsyncMemoryStore::new(&Key::new())));
    env.get_async_store()
        .set(&key, b"stale payload", &{
            let mut m = MetadataRecord::new();
            m.with_key(key.clone());
            m.with_type_identifier("text".to_owned());
            m.with_status(Status::Ready);
            Metadata::MetadataRecord(m)
        })
        .await?;

    let envref = env.to_ref();
    let mut asset_data =
        AssetData::<SimpleEnvironment<Value>>::new(9001, key.clone().into(), envref.clone());

    // Fast-track populates data AND binary together, leaving status Ready.
    assert!(asset_data.try_fast_track().await?);
    assert!(asset_data.poll_binary().is_some(), "precondition: bytes are cached");

    let assetref = asset_data.to_ref();
    assetref.expire().await?;
    assert_eq!(assetref.status().await, Status::Expired);

    // The fix: normal binary reads decline.
    assert!(assetref.poll_binary().await.is_none(), "poll_binary must hide expired bytes");
    assert!(assetref.try_poll_binary().is_none(), "try_poll_binary must hide expired bytes");

    // get_binary errors promptly rather than returning stale bytes or blocking.
    let err = assetref.get_binary().await.expect_err("get_binary must fail when expired");
    assert!(err.to_string().to_lowercase().contains("expired"));

    // State reads unchanged.
    assert!(assetref.poll_state().await.is_none());

    // The bytes were hidden, not dropped.
    let (recovered, _) = assetref
        .poll_binary_any_status()
        .await
        .ok_or("retained bytes must remain reachable")?;
    assert_eq!(recovered.as_ref().as_slice(), b"stale payload");

    Ok(())
}
```

`get_binary` is the assertion that matters most: it is the method whose current short-circuit
returns the stale bytes, and the one that would **hang** if the gate were added without Phase 2's
pre-wait expiry check.

## Example 2 — Explicit recovery at all three layers

```rust
#[tokio::test]
async fn test_binary_recovery_across_layers() -> Result<(), Box<dyn std::error::Error>> {
    // ... same fast-track setup as Example 1, key = "recover.txt", then expire ...

    // Layer 1: AssetData (sync, no wrapper)
    assert!(asset_data.poll_binary().is_none());
    assert!(asset_data.poll_binary_any_status().is_some());

    // Layer 2: AssetRef — get_binary_any_status is an alias, so both agree.
    let via_poll = assetref.poll_binary_any_status().await;
    let via_get = assetref.get_binary_any_status().await;
    match (via_poll, via_get) {
        (Some((a, _)), Some((b, _))) => assert_eq!(a.as_ref(), b.as_ref()),
        (None, _) | (_, None) => return Err("both recovery reads must yield bytes".into()),
    }

    // Layer 3: AssetManager, in-memory hit.
    let manager = envref.get_asset_manager();
    let (bytes, _) = manager
        .get_binary_any_status(&key)
        .await?
        .ok_or("manager recovery must find the expired asset")?;
    assert_eq!(bytes.as_ref().as_slice(), b"stale payload");

    Ok(())
}
```

### The store-fallback test that proves the efficiency claim

Phase 2 claims `AssetManager::get_binary_any_status` is **cheaper** than its state twin because it
skips `deserialize_stored_value`. That is testable, not merely assertable: store bytes whose
declared `type_identifier` this build cannot deserialize, with no in-memory asset. The binary read
must succeed; the state read must fail.

```rust
#[tokio::test]
async fn test_manager_binary_recovery_skips_deserialization()
    -> Result<(), Box<dyn std::error::Error>> {
    let key = parse_key("undeserializable.bin")?;
    let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
    env.with_async_store(Box::new(AsyncMemoryStore::new(&Key::new())));
    env.get_async_store()
        .set(&key, b"opaque bytes", &{
            let mut m = MetadataRecord::new();
            m.with_key(key.clone());
            m.with_type_identifier("no_such_type_in_this_build".to_owned());
            m.with_status(Status::Ready);
            Metadata::MetadataRecord(m)
        })
        .await?;

    let envref = env.to_ref();
    let manager = envref.get_asset_manager();

    // Binary recovery succeeds: no value round-trip.
    let (bytes, _) = manager
        .get_binary_any_status(&key)
        .await?
        .ok_or("binary recovery must not need a deserializer")?;
    assert_eq!(bytes.as_ref().as_slice(), b"opaque bytes");

    // State recovery cannot: it must deserialize, and there is no deserializer.
    assert!(
        manager.get_any_status(&key).await.is_err(),
        "state recovery must attempt deserialization and fail"
    );

    Ok(())
}
```

This test would fail if someone later "simplified" the binary path into a wrapper around the state
path — which is exactly the regression worth guarding.

## Example 3 — Statuses with no valid binary

The `Error`-versus-`Cancelled` distinction is the sharpest case in the design, because today they
differ *by accident*: `State::as_bytes` checks `value_error()` first, which is `Some` for `Error`
and `None` for `Cancelled`, so cancellation currently serializes a `none` value into some byte
string instead of failing.

All three no-valid-binary statuses are covered. The setup assigns `status` and `binary` directly,
which the in-file `mod tests` can do (`assets.rs:5480` has `use super::*;`) — this keeps the test
about the gate rather than about how each status is reached.

```rust
/// Build an AssetData holding cached bytes, parked in `status`.
fn asset_with_binary(
    id: u64,
    status: Status,
    envref: EnvRef<SimpleEnvironment<Value>>,
) -> AssetData<SimpleEnvironment<Value>> {
    let key = parse_key("no_valid_binary.txt").expect("test key");
    let mut d = AssetData::<SimpleEnvironment<Value>>::new(id, key.into(), envref);
    d.binary = Some(Arc::new(b"bytes that must not escape".to_vec()));
    d.status = status;
    d
}

#[tokio::test]
async fn test_get_binary_error_identity() -> Result<(), Box<dyn std::error::Error>> {
    let env: SimpleEnvironment<Value> = SimpleEnvironment::new();
    let envref = env.to_ref();

    // --- Error: the returned error is the asset's OWN recorded failure. ---
    let failed = asset_with_binary(9101, Status::Ready, envref.clone()).to_ref();
    failed.fail_asset(Error::general_error("recipe blew up".to_owned())).await?;
    assert_eq!(failed.status().await, Status::Error);

    let err = failed.get_binary().await.expect_err("Error must not yield bytes");
    assert!(
        err.to_string().contains("recipe blew up"),
        "must reuse the recorded failure, not a generic message: {}", err
    );
    assert!(failed.poll_binary().await.is_none());
    assert!(failed.poll_state().await.is_some(), "poll_state unchanged: metadata-only state");

    // --- Cancelled: no error is recorded, so one is constructed. ---
    let cancelled = asset_with_binary(9102, Status::Cancelled, envref.clone()).to_ref();
    let err = cancelled.get_binary().await.expect_err("Cancelled must not yield bytes");
    assert!(err.to_string().to_lowercase().contains("cancel"));
    assert!(cancelled.poll_binary().await.is_none());
    assert!(cancelled.poll_state().await.is_some());

    // --- Directory: likewise constructed. ---
    let dir = asset_with_binary(9103, Status::Directory, envref).to_ref();
    let err = dir.get_binary().await.expect_err("Directory must not yield bytes");
    assert!(err.to_string().to_lowercase().contains("director"));
    assert!(dir.poll_binary().await.is_none());
    assert!(dir.poll_state().await.is_some());

    Ok(())
}
```

The three arms differ in exactly one respect — where the error comes from — which is the whole
content of Phase 1's decision, and the reason `Error` and `Cancelled` are separate match arms in
`get_binary` despite sharing a `ReadExposure`.

**`Pending` waits, it does not error.** `get_binary` on a `Processing` asset must block until the
asset finishes, then reflect the outcome. Test it by driving completion from a second task under
`tokio::time::timeout`, never by sleeping a guessed interval:

```rust
let waiter = tokio::spawn({ let a = assetref.clone(); async move { a.get_binary().await } });
// ... drive the asset to Ready from this task ...
let result = tokio::time::timeout(Duration::from_secs(5), waiter).await??;
assert!(result.is_ok(), "a Pending asset that becomes Ready must yield bytes");
```

## Test Plan

The suite is U1–U8 (unit) plus I1–I4 (integration), enumerated in the Overview Table. Run with
`cargo test -p liquers-core`. U1–U3 need no environment; U4–U8 and I1 must live in `assets.rs`'s
`mod tests` for the access reasons in §Access; I2–I4 belong in `liquers-core/tests/`.

### Unit Tests (U1–U8)

U1–U3 live in `metadata.rs` and need no environment — the cheapest and most durable tests here.

```rust
#[test]
fn test_read_exposure_all_statuses() {
    assert_eq!(Status::Ready.read_exposure(), ReadExposure::Value);
    assert_eq!(Status::Source.read_exposure(), ReadExposure::Value);
    assert_eq!(Status::Override.read_exposure(), ReadExposure::Value);
    assert_eq!(Status::Volatile.read_exposure(), ReadExposure::Value);

    assert_eq!(Status::Directory.read_exposure(), ReadExposure::MetadataOnly);
    assert_eq!(Status::Error.read_exposure(), ReadExposure::MetadataOnly);
    assert_eq!(Status::Cancelled.read_exposure(), ReadExposure::MetadataOnly);

    assert_eq!(Status::Expired.read_exposure(), ReadExposure::Expired);

    assert_eq!(Status::None.read_exposure(), ReadExposure::Pending);
    assert_eq!(Status::Recipe.read_exposure(), ReadExposure::Pending);
    assert_eq!(Status::Submitted.read_exposure(), ReadExposure::Pending);
    assert_eq!(Status::Dependencies.read_exposure(), ReadExposure::Pending);
    assert_eq!(Status::Processing.read_exposure(), ReadExposure::Pending);
    assert_eq!(Status::Partial.read_exposure(), ReadExposure::Pending);
    assert_eq!(Status::Storing.read_exposure(), ReadExposure::Pending);
}

/// U3 — the shortcut that looks right and is not.
#[test]
fn test_has_data_is_not_the_read_gate() {
    assert!(Status::Expired.has_data());
    assert_ne!(Status::Expired.read_exposure(), ReadExposure::Value);
    assert!(Status::Partial.has_data());
    assert_ne!(Status::Partial.read_exposure(), ReadExposure::Value);
}
```

**U2, the exhaustiveness guard, needs care.** A `Vec` of variants does *not* fail to compile when a
variant is added — a drafted version claimed it did, which is wrong. What actually works is an
exhaustive `match` in the test whose arms name every variant:

```rust
#[test]
fn test_read_exposure_guard_is_exhaustive() {
    // Adding a Status variant makes THIS match non-exhaustive: a compile error,
    // which is the entire purpose of the test.
    fn expected(s: Status) -> ReadExposure {
        match s {
            Status::Ready | Status::Source | Status::Override | Status::Volatile => ReadExposure::Value,
            Status::Directory | Status::Error | Status::Cancelled => ReadExposure::MetadataOnly,
            Status::Expired => ReadExposure::Expired,
            Status::None | Status::Recipe | Status::Submitted | Status::Dependencies
            | Status::Processing | Status::Partial | Status::Storing => ReadExposure::Pending,
        }
    }
    // `Status::all()` does NOT exist (verified), so the fifteen literals are listed here.
    // The compile-time guarantee comes from `expected`'s match, not from this list.
    for s in [
        Status::None, Status::Directory, Status::Recipe, Status::Submitted,
        Status::Dependencies, Status::Processing, Status::Partial, Status::Error,
        Status::Storing, Status::Expired, Status::Cancelled, Status::Ready,
        Status::Source, Status::Override, Status::Volatile,
    ] {
        assert_eq!(s.read_exposure(), expected(s), "wrong bucket for {:?}", s);
    }
}
```

Adding a `Status` variant makes `expected`'s match non-exhaustive — a compile error. It does *not*
make the array literal fail, so the array alone would be no guard at all; that is why the match is
written out rather than looping over a helper.

U4–U10 live in `assets.rs`'s `mod tests`, which can set `asset_data.status` and `.binary` directly.
U7 is the contract test for `binary_unchecked`: **bytes for every one of the fifteen statuses**,
because it is the persistence accessor and must not consult the gate.

U4 deserves spelling out, since it is what proves Phase 2's claim that extracting the classifier
changes no state-read behaviour. It is not "assert `poll_state` returns something" — it must assert
the *behaviour class* for each of the fifteen statuses:

```rust
#[tokio::test]
async fn test_poll_state_agrees_with_read_exposure()
    -> Result<(), Box<dyn std::error::Error>> {
    let env: SimpleEnvironment<Value> = SimpleEnvironment::new();
    let envref = env.to_ref();

    for (i, status) in [/* the fifteen literals */].into_iter().enumerate() {
        let mut d = asset_with_binary(9200 + i as u64, status, envref.clone());
        d.data = Some(Arc::new(Value::from("v")));
        match status.read_exposure() {
            ReadExposure::Value => {
                let st = d.poll_state().ok_or("Value exposure must yield a state")?;
                assert!(!st.is_none(), "{:?} must carry a value", status);
            }
            ReadExposure::MetadataOnly => {
                let st = d.poll_state().ok_or("MetadataOnly must yield a state")?;
                assert!(st.is_none(), "{:?} must carry no value", status);
            }
            ReadExposure::Expired | ReadExposure::Pending => {
                assert!(d.poll_state().is_none(), "{:?} must be hidden", status);
            }
        }
    }
    Ok(())
}
```

**U9 — the state-side twin of the `get_binary` fix.** Without it, Phase 2's `get`/`Expired` matrix
cell is unverified:

```rust
#[tokio::test]
async fn test_get_on_expired_errors_instead_of_blocking()
    -> Result<(), Box<dyn std::error::Error>> {
    // ... fast-track setup as Example 1, then expire ...
    let result = tokio::time::timeout(Duration::from_secs(2), assetref.get()).await;
    let inner = result.map_err(|_| "get() blocked on an expired asset instead of erroring")?;
    assert!(inner.is_err(), "get() on Expired must return Err");
    Ok(())
}
```

The `timeout` is the assertion: today this test fails by timing out, which is precisely the latent
hang. It must not be written as a bare `assert!(assetref.get().await.is_err())`, because that would
hang the suite rather than fail it.

**U10** covers the corner case where exposure is `Value` but no bytes are cached: `get_binary` must
serialize through `serialize_to_binary` and return bytes. This guards against over-reading the gate
as "binary required".

### On test bodies

Phase 3 fixes *what each test pins down* and the setup that makes it reachable; Phase 4 writes the
remaining bodies. Where a body appears above, it is because the test is easy to get subtly wrong —
U2's guard, U4's behaviour classes, U9's timeout, the store-fallback test — not because the others
are optional.

### Integration Tests (I1–I4)

**I1 — persistence does not regress.** The one genuinely risky part of the change. `set_state`
persists with whatever status the caller supplies (`assets.rs:2548`), so a non-`Value` status must
still reach the store. Because `set_state` is `pub(crate)`, this is an in-file test.

**I2 — end-to-end expiry** through a command registered with `expires:`, following
`expiration_integration.rs` conventions, asserting the full Behaviour Matrix row by row.

**I3 — the manager re-request boundary.** Phase 2 claims manager routing is unchanged; prove it
with a call-counting command: evaluate, expire, request again, assert the count incremented and the
bytes are fresh. Without this, the fix could silently convert "expired → recompute" into
"expired → error" at the request boundary, which would be a serious regression.

**I4 — fast-track after expiry.** `mark_expired_status` persists `Expired` to the store precisely
so an evicted asset cannot fast-track stale bytes back (the gap PR #11 found late). Since
`try_fast_track` is this suite's main setup tool, it is worth one test in its own right.

## What cannot be tested, and why

Stated plainly rather than covered by a flaky test:

1. **`liquers-axum` handler behaviour — a decision Phase 4 must make, not inherit.** The crate has
   no handler test scaffolding: no test router, no request helper. Both Phase 3 reviewers flagged
   this independently, and both observed that the axum loops are where the bug actually bites, so
   the gap sits over the most load-bearing part of the change. Phase 4 must pick one, explicitly:
   **(a)** build the scaffolding (a test `Router` plus a request helper — an estimated ~80 lines,
   reusable by every future handler test), or **(b)** accept review-plus-manual-request
   verification and **file an issue** for the missing coverage, per `CLAUDE.md`'s rule that a
   known gap is recorded rather than mentioned. Silently shipping (b) without the issue is not an
   option.
2. **`try_poll_binary` under lock contention.** Returning `None` because `try_read()` failed is a
   scheduler-timing outcome; there is no public API that holds the write lock for a controllable
   interval. A "spawn many and hope one contends" test is probabilistic and would be flaky in CI.
   The *status* gating of `try_poll_binary` is fully testable (U5) and is what this design changes;
   the contention branch is pre-existing behaviour.
3. **Exact interleaving of expiry with an in-flight read.** Both outcomes — bytes, or `None` — are
   correct, since the read either observes `Expired` under the lock or does not. A test asserting
   one specific outcome would be asserting scheduler behaviour. What *is* assertable: after the
   race resolves, status is `Expired` and the recovery read still yields the bytes.

## Corner Cases

- **Bytes absent, value present.** `get_binary` on a `Value`-exposure asset with no cached binary
  must serialize via `serialize_to_binary` — unchanged, but worth one assertion so the gate is not
  mistaken for "binary required".
- **Bytes present, value absent.** Reachable through fast-track failure paths that clear `data` but
  not `binary`. Under the gate, status governs; the presence of `data` does not.
- **`Volatile`.** `Value` exposure, so binary reads work — but a volatile value is use-once. The
  design does not change this; one assertion pins it.
- **Idempotent expiry.** `expire()` on an already-`Expired` asset is a no-op; the binary family must
  behave identically before and after the second call.
