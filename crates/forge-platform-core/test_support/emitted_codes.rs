use std::collections::BTreeSet;

use forge_types::MessageCatalog;
use serde_json::Value;

pub(crate) fn emitted_codes(payload: &Value) -> BTreeSet<String> {
    let mut codes = BTreeSet::new();
    collect(payload, &mut codes);
    codes
}

fn collect(value: &Value, codes: &mut BTreeSet<String>) {
    match value {
        Value::Object(entries) => {
            if let Some(Value::String(code)) = entries.get("message").and_then(|m| m.get("code")) {
                codes.insert(code.clone());
            }
            entries.values().for_each(|nested| collect(nested, codes));
        }
        Value::Array(items) => items.iter().for_each(|nested| collect(nested, codes)),
        _ => {}
    }
}

fn declares(ftl: &str, key: &str) -> bool {
    ftl.lines().any(|line| {
        line.strip_prefix(key)
            .is_some_and(|rest| rest.starts_with(" ="))
    })
}

pub(crate) fn uncatalogued(emitted: &BTreeSet<String>, catalogs: &[MessageCatalog]) -> Vec<String> {
    emitted
        .iter()
        .filter(|code| {
            !catalogs.iter().any(|catalog| {
                catalog.codes.iter().any(|known| {
                    known.to_string() == **code
                        && declares(catalog.en_ftl, &known.ftl_key())
                        && declares(catalog.uk_ftl, &known.ftl_key())
                })
            })
        })
        .cloned()
        .collect()
}
