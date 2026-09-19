---
id: COMMAND-CACHE-FLAG-IS-DECLARED-BUT-NEVER-READ
kind: issue
title: CommandMetadata.cache is declared, documented and exported, but nothing reads it
status: draft
priority: P3
complexity: S
area: [core/commands, macro]
created: 2026-09-19
design:
github:
---
# `CommandMetadata.cache` is declared, documented and exported, but nothing reads it

## What is wrong

`CommandMetadata` carries a `cache: bool` field (`liquers-core/src/command_metadata.rs:1013-1019`).
It is public, documented as if meaningful, defaulted to `true`, and **serialized into
`specs/command_registry.yaml`** for every registered command.

Nothing reads it. The only mention outside its own declaration and the struct literals that default
it is an equality assertion in a test (`command_declaration.rs:987`). No caching decision consults
it, and `register_command!` has **no statement to set it** — the macro's statement list
(`registration.rs:835-886`) is volatile / payload / label / doc / namespace / realm / preset / next /
filename / expires / version.

So a command author has no way to set the flag through the supported registration path, and anyone
setting it by constructing metadata directly gets silence.

## Why it matters

A declared, exported knob that does nothing is worse than an absent one: it is visible in the
generated registry, so it reads as a capability that exists. The nearest real mechanism is
`volatile`, which **is** fully wired — `assets.rs` never stores and never registers a volatile keyed
asset, and volatility propagates to every downstream step.

Found while answering Phase 1 of `record-streams`. The Python prototype
(`liquer/ext/dataframe_batches.py`) marks its generator commands
`@command(volatile=True, cache=False)`, using both flags. In the Rust implementation `volatile` alone
covers that need, so this blocks nothing — but the question "what does `cache=False` do here?" has no
good answer today.

## Resolution options

1. **Remove it.** If `volatile` covers every case, the field is dead weight in the struct and noise
   in every registry entry. Removing it changes `command_registry.yaml` for every command, so it is a
   regeneration plus the `registry_export` test.
2. **Wire it.** Give it a meaning distinct from `volatile` — most plausibly "the result is
   deterministic and reusable, but not worth the cache space", which is a real distinction for large
   values — and add a macro statement for it.
3. **Document it as reserved** and default it out of the export until it means something.

Option 1 is the honest default unless a use case for the distinction appears. Deciding needs someone
to say whether "cacheable" and "volatile" were ever meant to be independent axes.

## Evidence

- `liquers-core/src/command_metadata.rs:1013-1019` — declaration and doc comment
- `liquers-core/src/command_metadata.rs:1077`, `:1109` — the defaults
- `liquers-core/src/command_metadata.rs:1705` — `"cache":true` in the serialized form
- `liquers-core/src/command_declaration.rs:987` — the only read, in a test
- `liquers-macro/src/registration.rs:835-886` — the macro statements, with no `cache`
