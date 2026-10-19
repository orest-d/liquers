//! Agent memory: the `ns-mem` commands over a folder of Markdown documents with front-matter.
//!
//! A memory is a store folder (its *root*) holding `_schema/memory.yaml`, which declares the
//! collections, their fields and vocabularies, and the L0/L1 bridge. The commands read and write
//! documents through the store and log every write under `<root>/_log/`.
//! Design: `specs/design/agent-memory-mvp/`.

pub mod check;
pub mod commands;
pub mod front_matter;
pub mod layout;
pub mod named_rules;
pub mod read;
pub mod rules;
pub mod tiers;
pub mod write;
