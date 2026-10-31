#![allow(clippy::expect_used)]
mod common;
use common::{
    ALERT_KIND, BLANK_KIND, CHAT_KIND, FRAME_KIND, GOAL_KIND, IDENTITY, LATEST_KIND,
    PAGE_CREDENTIAL, TICKER_KIND, registry, sub_alert_instance,
};

use std::collections::BTreeMap;

use forge_overlay::{OverlayInstance, sample_document};
use serde_json::Value;

const BUILTIN_IDS: &[&str] = &[
    ALERT_KIND,
    BLANK_KIND,
    CHAT_KIND,
    FRAME_KIND,
    GOAL_KIND,
    LATEST_KIND,
    TICKER_KIND,
];

const EXPECTED_SAMPLE_CONTENT: &[(&str, &[(&str, &str)])] = &[
    (
        ALERT_KIND,
        &[
            ("headline", "Thanks for the sub!"),
            ("subline", "61 months subscribed"),
        ],
    ),
    (BLANK_KIND, &[]),
    (
        CHAT_KIND,
        &[
            ("author", "testuser"),
            ("author_color", "#89dceb"),
            ("badges", ""),
            ("message", "hey, great stream!"),
            ("platform", ""),
        ],
    ),
    (FRAME_KIND, &[("headline", ""), ("subline", "LIVE")]),
    (
        GOAL_KIND,
        &[("label", "Sub goal"), ("target", "100"), ("value", "42")],
    ),
    (
        LATEST_KIND,
        &[
            ("headline", "PixelPal"),
            ("label", "Last donation"),
            ("subline", "100.00 UAH"),
        ],
    ),
    (
        TICKER_KIND,
        &[
            ("headline", "Latest cheer: 5001 bits"),
            ("subline", "\"hey, great stream!\""),
        ],
    ),
];

fn raw_sample(kind_id: &str, credential: Option<&str>) -> String {
    let reg = registry();
    let descriptor = reg.get(kind_id).expect("a registered builtin kind");
    let instance = OverlayInstance {
        credential: credential.map(forge_types::OverlayCredential::new),
        ..sub_alert_instance(kind_id)
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

fn content_text(kind_id: &str) -> BTreeMap<String, String> {
    content_of(&sample(kind_id))
        .iter()
        .map(|(key, value)| {
            let text = value
                .as_str()
                .map_or_else(|| value.to_string(), str::to_owned);
            (key.clone(), text)
        })
        .collect()
}

fn expected_content(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect()
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
    let reg = registry();
    for kind_id in BUILTIN_IDS {
        let document = sample(kind_id);
        let content = content_of(&document);
        let draws = reg
            .get(kind_id)
            .expect("a registered builtin kind")
            .has_visual_page();

        assert!(
            !draws || !content.is_empty(),
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
    assert_eq!(
        EXPECTED_SAMPLE_CONTENT
            .iter()
            .map(|(kind_id, _)| *kind_id)
            .collect::<Vec<_>>(),
        BUILTIN_IDS,
        "a builtin overlay kind has no pinned sample wording"
    );

    for (kind_id, pairs) in EXPECTED_SAMPLE_CONTENT {
        assert_eq!(
            content_text(kind_id),
            expected_content(pairs),
            "{kind_id} reached the page with wording its sampled payload never produces"
        );
    }
}

#[test]
fn no_builtin_default_reaches_the_page_with_an_unexpanded_token() {
    for kind_id in BUILTIN_IDS {
        for (key, text) in content_text(kind_id) {
            assert!(
                !text.contains('%'),
                "{kind_id} sent {key} to the page as {text:?}, so the viewer reads a raw token"
            );
        }
    }
}
