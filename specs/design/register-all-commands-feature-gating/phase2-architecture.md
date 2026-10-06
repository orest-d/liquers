# Phase 2: Solution and Architecture

## Twins

In `liquers-lib/src/commands.rs`, after `register_all_commands!`:

```rust
/// No-op stand-in when the `egui` feature is off, so `register_all_commands!` compiles.
#[cfg(not(feature = "egui"))]
#[macro_export]
macro_rules! register_egui_commands {
    ($cr:expr) => {{ let _ = &$cr; Ok::<(), liquers_core::error::Error>(()) }};
}
```

The same goes for `polars` and `records`, and for `image-support` if Phase 4 step 1 shows that
`register_image_commands!` fails without it. In that case the existing image macro also gets
`#[cfg(feature = "image-support")]`. `let _ = &$cr;` avoids an unused-variable warning at call
sites where `cr` is used only by the macros.

## Master macro

Unchanged. The doc comment gains "registers the command domains whose features are enabled".

## Callers

- `liquers-lib/tests/registry_export.rs`: optionally replace the mirror with the macro. Keep the
  mirror if the test deliberately checks per-group behaviour.
- `src/bin/export_command_registry.rs`: unchanged (it selects groups). Update its comment.

## Alternatives

- A `register_all_commands_fn` generic over the environment. It already exists for
  `DefaultEnvironment` only. The macro serves arbitrary `CommandEnvironment`s.
- Cargo features on the caller. Rejected: caller features do not control `liquers-lib`'s modules.

## Known-issue preflight

None.

## Relevant commands

All domains (`core`, `egui`, `img`, `pl`, `rec`, `lui`). No signature change.

## Documentation architecture

Doc comments, plus the guide line if one exists.

## Risk table

| Area | Assessment |
|---|---|
| Likely files | `liquers-lib/src/commands.rs`; possibly `src/image/commands.rs`; `tests/registry_export.rs`; the binary's comment |
| Existing tests | Unaffected |
| New validation | A test that calls `register_all_commands!` in every feature configuration (runs in the matrix) |
| Compatibility | A previously failing build now compiles. Default builds are unchanged. |
| Recovery | Revert |
| Certainty | High |
