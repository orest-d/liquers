# Phase 2: Solution and Architecture

## Helper (`liquers-core/src/assets.rs`)

```rust
/// A value written already `Expired`: log the expiry now (the moment Liquers learns of it) and
/// say that the diagnostics come after the fact. The structured reason is left as supplied.
fn log_supplied_expiry(metadata: &mut Metadata, key: &Key, route: &str) {
    if let Metadata::MetadataRecord(_) = metadata {
        let _ = metadata.add_log_entry(LogEntry::warning("Asset expired".to_string()));
        let detail = match metadata.expiry_reason() {
            Some(reason) => format!(
                "Expiry recorded after the fact: {key} was written already expired ({route}); \
                 supplied reason: {}", reason.log_entry(&key.to_string()).message),
            None => format!(
                "Expiry recorded after the fact: {key} was written already expired ({route}); \
                 its original cause is unknown"),
        };
        let _ = metadata.add_log_entry(LogEntry::info(detail));
    }
}
```

The `match metadata` in the guard lists both variants explicitly in the implementation (no `if
let` shortcut), per CLAUDE.md. Check the exact `LogEntry` constructors (`LogEntry::warning` exists,
used in `note_expired_dependency`) and `expiry_reason()` (exists on `Metadata`, ~2378).

`set_binary` works on a `MetadataRecord`. Call the record-level `add_log_entry`, or wrap the record
once. Pick whichever the site already does for its other log entries.

## Call sites

Right after `final_status` is decided (four sites: both managers' `set_binary` and `set_state`):
`if final_status == Status::Expired { log_supplied_expiry(&mut metadata, key, "set_state") }`.

## Known-issue preflight

| Issue | Relation | Blocks? |
|---|---|---|
| `IMMEDIATE-SET-STATE-STATUS-MATCH-HAS-DEFAULT-ARM` | Same sites | Order first or merge |
| `EXTERNAL-MANAGER-CANNOT-NOTIFY-REPLACED-ASSET` | A future `new_installed` with `Expired` should call the same helper | No |

## Relevant commands

None.

## Documentation architecture

`DEPENDENCIES_STATUS.md` sentence, with History and `reviewed:`.

## Risk table

| Area | Assessment |
|---|---|
| Likely files | `liquers-core/src/assets.rs` |
| Existing tests | Tests writing `Expired` and comparing whole metadata may see two extra log entries |
| Data | Additive log entries only |
| Recovery | Revert |
| Certainty | High |
