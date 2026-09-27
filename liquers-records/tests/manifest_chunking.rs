//! Phase 3 §5.6: `ManifestSource::chunks` for a template-only manifest, and `ChunkNaming`'s
//! generated keys and their inverse, exercised as an integration test (rather than a `manifest.rs`
//! unit test) since it is the kind of check a consumer of the published crate would write.

use liquers_core::query::Key;
use liquers_records::{ChunkList, ChunkNaming, ChunkTemplate, ManifestSource, ManifestSpec, RecordSource};

#[test]
fn manifest_with_only_a_template_has_no_explicit_chunks() {
    let spec = ManifestSpec {
        chunks: vec![],
        template: Some(ChunkTemplate {
            query: "ns-sql/sql_query".to_string(),
            first_offset: 0,
            step: 1000,
            batch_size: 1000,
        }),
        extension: Some("csv".to_string()),
        stored: true,
        cached: true,
        uniform_schema: None,
        ..ManifestSpec::default()
    };
    let source = ManifestSource::new(spec, None).expect("template-only source");
    match source.chunks() {
        ChunkList::Unbounded { computed } => assert_eq!(computed.len(), 0),
        ChunkList::Known(_) => panic!("expected Unbounded for a template-only manifest"),
    }
}

#[test]
fn chunk_naming_generates_padded_template_keys() {
    let naming = ChunkNaming {
        folder: Key::new(),
        prefix: "daily".to_string(),
        extension: "csv".to_string(),
    };
    assert!(naming.key(0).to_string().ends_with("daily_0000.csv"));
    assert!(naming.key(42).to_string().ends_with("daily_0042.csv"));
}

#[test]
fn chunk_naming_index_of_is_the_inverse_of_key() {
    let naming = ChunkNaming {
        folder: Key::new(),
        prefix: "daily".to_string(),
        extension: "csv".to_string(),
    };
    for n in [0u64, 1, 42, 9_999] {
        assert_eq!(naming.index_of(&naming.key(n).to_string()), Some(n));
    }
    assert_eq!(naming.index_of("other.csv"), None);
}
