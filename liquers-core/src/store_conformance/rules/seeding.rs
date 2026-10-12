//! §10 — the data format of a written key.
//!
//! A record written with no filename is stored with the key's last segment as its filename, which
//! seeds the data format from the extension. A declared filename or data format is kept.

use crate::metadata::{Metadata, MetadataRecord};
use crate::query::Key;
use crate::store_conformance::rules::support::require_absent;
use crate::store_conformance::{failed_at, keys_for, Fixture, KeyRequest, RuleOutcome};

/// The stored record of `key`, or the outcome that ends the rule.
async fn stored_record(f: &dyn Fixture, key: &Key) -> Result<MetadataRecord, RuleOutcome> {
    match f.store().get_metadata(key).await {
        Ok(Metadata::MetadataRecord(record)) => Ok(record),
        Ok(Metadata::LegacyMetadata(_)) => Err(failed_at(
            format!("get_metadata({}) returned legacy metadata for a record it was given", key.encode()),
            vec![key.clone()],
        )),
        Err(e) => Err(e.into()),
    }
}

/// Write `record` under each fresh sibling key, after checking every key is absent.
async fn fresh_writes(
    f: &dyn Fixture,
    records: Vec<MetadataRecord>,
) -> Result<Vec<Key>, RuleOutcome> {
    let request = KeyRequest::FreshSiblings { count: records.len() };
    let keys = keys_for(f, request.clone()).await?;
    if keys.len() < records.len() {
        return Err(crate::store_conformance::failed(format!(
            "the fixture returned {} key(s) for FreshSiblings {{ count: {} }}",
            keys.len(),
            records.len()
        )));
    }
    for key in &keys {
        require_absent(f, key, request.clone()).await?;
    }
    for (key, record) in keys.iter().zip(records) {
        f.store()
            .set(key, b"a,b\n1,2\n", &Metadata::MetadataRecord(record))
            .await?;
        f.record_created(key);
    }
    Ok(keys)
}

/// `seed01` — data written with no filename takes the key's last segment as its filename, and the
/// data format follows its extension.
pub async fn seed01(f: &dyn Fixture) -> RuleOutcome {
    let keys = match fresh_writes(f, vec![MetadataRecord::new()]).await {
        Ok(keys) => keys,
        Err(outcome) => return outcome,
    };
    let key = &keys[0];
    let record = match stored_record(f, key).await {
        Ok(record) => record,
        Err(outcome) => return outcome,
    };
    let expected_name = key.filename().map(|name| name.encode().to_string());
    let expected_format = key.extension();
    if record.filename != expected_name || record.data_format != expected_format {
        return failed_at(
            format!(
                "set({}) with no filename stored filename {:?} and data format {:?}; expected {:?} \
                 and {:?}",
                key.encode(),
                record.filename,
                record.data_format,
                expected_name,
                expected_format
            ),
            vec![key.clone()],
        );
    }
    RuleOutcome::Passed
}

/// `seed02` — a declared filename, or a declared data format, is kept as written.
pub async fn seed02(f: &dyn Fixture) -> RuleOutcome {
    let mut named = MetadataRecord::new();
    named.set_filename("declared.csv");
    let mut formatted = MetadataRecord::new();
    formatted.data_format = Some("csv".to_string());
    let keys = match fresh_writes(f, vec![named, formatted]).await {
        Ok(keys) => keys,
        Err(outcome) => return outcome,
    };
    let named_record = match stored_record(f, &keys[0]).await {
        Ok(record) => record,
        Err(outcome) => return outcome,
    };
    if named_record.filename.as_deref() != Some("declared.csv")
        || named_record.data_format.as_deref() != Some("csv")
    {
        return failed_at(
            format!(
                "set({}) with filename declared.csv stored filename {:?} and data format {:?}",
                keys[0].encode(),
                named_record.filename,
                named_record.data_format
            ),
            vec![keys[0].clone()],
        );
    }
    let formatted_record = match stored_record(f, &keys[1]).await {
        Ok(record) => record,
        Err(outcome) => return outcome,
    };
    if formatted_record.data_format.as_deref() != Some("csv") {
        return failed_at(
            format!(
                "set({}) with data format csv stored data format {:?}",
                keys[1].encode(),
                formatted_record.data_format
            ),
            vec![keys[1].clone()],
        );
    }
    RuleOutcome::Passed
}
