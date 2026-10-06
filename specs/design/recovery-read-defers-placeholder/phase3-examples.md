# Phase 3: Examples and Tests

The tests build the race window deterministically: a placeholder is mapped but never run.

| # | Kind | Checks |
|---|---|---|
| T1 | unit | `get_binary_any_status` returns the stored bytes for a key with a mapped `Recipe`/`None` placeholder |
| T2 | unit | `get_any_status` returns the stored value for the same setup |
| T3 | unit | With a placeholder and nothing stored, both return `Ok(None)` |
| T4 | regression | An existing test with a live `Expired` retained value still answers from memory |

## Setup (T1–T3)

In `liquers-core/src/assets.rs` `mod tests`, both managers where practical (the immediate manager
is the cheaper fixture):

1. Build an environment with `AsyncMemoryStore::new(&Key::new())`.
2. `store.set(&parse_key("data/a.txt")?, b"hello", &metadata_ready_text)`, with metadata status
   `Ready` and type `Text`.
3. Map a placeholder: `let asset = AssetRef::new_from_recipe(manager.next_id(), key.clone().into(),
   Some(key.clone()), envref.clone());` then `manager.try_insert_key_asset(&key, asset)`. Use the
   crate-private insert the managers use. The test lives inside the crate.
4. Assert `asset.status().await` is `Status::None` or `Status::Recipe`.

Test names:

- `recovery_binary_read_defers_placeholder_to_store`
- `recovery_state_read_defers_placeholder_to_store`
- `recovery_read_of_placeholder_without_store_entry_is_none`

Expected values: T1 gives `Some((bytes == b"hello", metadata.status() == Ready))`. T2 gives a state
with `try_into_string() == "hello"`. T3 gives `None`.

No query is involved. The tests call the manager API directly.
