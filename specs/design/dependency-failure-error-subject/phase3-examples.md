# Phase 3: Examples and Tests

| # | Kind | Checks |
|---|---|---|
| T1 | unit | Cancelled keyed dependency, no stored error → dependent error message contains the key text and not `"asset "` followed by digits |
| T2 | unit | Expired-and-evicted dependency → message contains the key and the legacy phrase |
| T3 | regression | Existing test asserting the "expired and was evicted…" substring stays green |

## Setup

`liquers-core/src/assets.rs` `mod tests`, `DefaultAssetManager` with an `AsyncMemoryStore`:

- T1: create a keyed dependency asset for `data/b.txt`, put it in `Status::Cancelled` without a
  stored error (use the existing cancellation path: `cancel()` before run), then call
  `manager.wait_for_dependency(&parent, &dep)`. Assert
  `err.message.contains("data/b.txt")`.
- T2: reuse the setup of the existing evicted-expiry test (the one at the substring assertion)
  and add `assert!(err.message.contains("<its key>"))`.

Names: `dependency_failure_error_names_key`, `evicted_dependency_error_names_key`.
