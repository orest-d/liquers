//! Regenerates `liquers-core/src/entities_data.rs` from `liquers-core/data/entities.json`.
//!
//! ```bash
//! cargo run -p liquers-core --features cli --bin generate-entities
//! ```
//!
//! The logic lives in [`liquers_core::entities::codegen`] so that `entity_table_is_current` can
//! call it directly; this binary is the operator-facing wrapper. A stale checked-in table
//! therefore fails the test suite rather than drifting silently, which is the same contract
//! `registry_export` enforces for `specs/command_registry.yaml`.

use std::collections::BTreeMap;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let json = std::fs::read_to_string(root.join("data/entities.json"))?;
    let table: BTreeMap<String, String> = serde_json::from_str(&json)?;

    let generated = liquers_core::entities::codegen::generate(&table)?;
    let out = root.join("src/entities_data.rs");
    std::fs::write(&out, &generated)?;

    eprintln!(
        "wrote {} ({} names, {} curated)",
        out.display(),
        table.len(),
        liquers_core::entities::codegen::CURATED.len()
    );
    Ok(())
}
