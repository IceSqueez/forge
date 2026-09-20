#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use forge_overlay::{
    OverlayConfig, OverlayInstance, OverlayKindRegistry, register_builtin_kinds, sample_document,
};
use serde_json::Value;

const BUILTIN_IDS: &[&str] = &[
    "overlay.alert",
    "overlay.chat",
    "overlay.frame",
    "overlay.goal",
    "overlay.ticker",
];

const IDENTITY: &str = "sub-alert-1";
const PAGE_CREDENTIAL: &str = "7c1e4a90b2d84f36a5c0e9b71d3f8a24";

fn registry() -> OverlayKindRegistry {
    let mut reg = OverlayKindRegistry::new();
    register_builtin_kinds(&mut reg).expect("the builtin overlay kinds register");
    reg
}

fn raw_sample(kind_id: &str, credential: Option<&str>) -> String {
    let reg = registry();
    let descriptor = reg.get(kind_id).expect("a registered builtin kind");
    let instance = OverlayInstance {
        id: IDENTITY.to_owned(),
        display_name: "Sub alert".to_owned(),
        kind_id: kind_id.to_owned(),
        config: OverlayConfig::new(),
        source_overrides: Vec::new(),
        credential: credential.map(str::to_owned),
    };

    sample_document(&instance, descriptor).expect("the sample document builds")
}

fn sample(kind_id: &str) -> Value {
    serde_json::from_str(&raw_sample(kind_id, None)).expect("the sample document is valid JSON")
}

fn content_of(document: &Value) -> &serde_json::Map<String, Value> {
    document["content"]
        .as_object()
        .expect("the preview page reads content as an object")
}

fn mentions(value: &Value, name: &str) -> bool {
    match value {
        Value::Object(fields) => {
            fields.contains_key(name) || fields.values().any(|held| mentions(held, name))
        }
        Value::Array(items) => items.iter().any(|held| mentions(held, name)),
        _ => false,
    }
}

fn carries_tagged_variant(value: &Value) -> bool {
    match value {
        Value::Object(fields) => {
            (fields.contains_key("type") && fields.contains_key("value"))
                || fields.values().any(carries_tagged_variant)
        }
        Value::Array(items) => items.iter().any(carries_tagged_variant),
        _ => false,
    }
}

#[test]
fn the_sample_envelope_names_exactly_the_fields_the_preview_page_reads() {
    for kind_id in BUILTIN_IDS {
        let document = sample(kind_id);
        let mut keys: Vec<&str> = document
            .as_object()
            .expect("the sample document is an object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();

        assert_eq!(
            keys,
            [
                "content",
                "documentVersion",
                "generatorVersion",
                "kindId",
                "overlayId",
            ],
            "{kind_id} wrote a sample envelope the preview page cannot read"
        );
        assert_eq!(
            (document["overlayId"].as_str(), document["kindId"].as_str()),
            (Some(IDENTITY), Some(*kind_id)),
            "{kind_id} named the wrong overlay in its own sample"
        );
    }
}

#[test]
fn a_sample_document_never_carries_the_page_credential() {
    for kind_id in BUILTIN_IDS {
        let raw = raw_sample(kind_id, Some(PAGE_CREDENTIAL));

        assert!(
            !raw.contains(PAGE_CREDENTIAL),
            "{kind_id} wrote the socket credential into a file the browser fetches by name"
        );

        let document: Value =
            serde_json::from_str(&raw).expect("the sample document is valid JSON");
        assert!(
            !mentions(&document, "credential"),
            "{kind_id} left a credential slot in its sample document"
        );
    }
}

#[test]
fn every_sample_content_value_reaches_the_page_as_a_plain_json_value() {
    for kind_id in BUILTIN_IDS {
        let document = sample(kind_id);
        let content = content_of(&document);

        assert!(
            !content.is_empty(),
            "{kind_id} samples nothing, so its preview page has nothing to draw"
        );
        assert!(
            !carries_tagged_variant(&document["content"]),
            "{kind_id} sampled a tagged value, which the page draws as [object Object]"
        );
        for (key, value) in content {
            assert!(
                value.is_string() || value.is_number() || value.is_boolean(),
                "{kind_id} samples {key} as {value}, which the page cannot render as text"
            );
        }
    }
}

#[test]
fn the_sample_content_expands_the_overlays_own_wording_against_the_sampled_payload() {
    let document = sample("overlay.chat");
    let content = content_of(&document);

    assert_eq!(
        (content["author"].as_str(), content["message"].as_str(),),
        (Some("PixelPal"), Some("sample payload")),
        "the sample reached the page with its variable tokens unexpanded"
    );
}
