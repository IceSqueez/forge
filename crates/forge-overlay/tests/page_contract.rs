#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;

use forge_overlay::config::{
    ACCENT, DESIGN_WIDTH, FONT, HEADLINE, MARGIN_LEFT, MARGIN_TOP, POSITION, TEXT_SIZE,
};
use forge_overlay::descriptor::ConfigSection;
use forge_overlay::{
    LookConfigKey, OverlayConfig, OverlayKindRegistry, PAGE_CONTRACT, RUNTIME_SOURCE,
    SampleContext, latest_binding, latest_content, look_contract, register_builtin_kinds,
    sample_content, sample_latest_value,
};

const RUNTIME_APPLIED_LOOK_KEYS: [&str; 4] = [ACCENT, FONT, TEXT_SIZE, POSITION];

fn registry() -> OverlayKindRegistry {
    let mut reg = OverlayKindRegistry::new();
    register_builtin_kinds(&mut reg).expect("the builtin overlay kinds register");
    reg
}

fn header() -> &'static str {
    let end = RUNTIME_SOURCE
        .find("*/")
        .expect("the runtime opens with a header comment");
    &RUNTIME_SOURCE[..end]
}

fn function_body(name: &str) -> &'static str {
    let opening = format!("function {name}(");
    let start = RUNTIME_SOURCE
        .find(&opening)
        .unwrap_or_else(|| panic!("the runtime no longer declares '{opening}'"));
    let rest = &RUNTIME_SOURCE[start..];
    let end = rest
        .find("\n  }")
        .unwrap_or_else(|| panic!("'{opening}' is never closed"));
    &rest[..end]
}

fn published_names() -> BTreeSet<String> {
    let opening = "window.forge = Object.freeze({";
    let start = RUNTIME_SOURCE
        .find(opening)
        .expect("the runtime publishes window.forge")
        + opening.len();
    let rest = &RUNTIME_SOURCE[start..];
    let end = rest.find('}').expect("the published object is closed");
    rest[..end]
        .lines()
        .filter_map(|line| line.split(':').next())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect()
}

fn shape_up_to_close(text: &str) -> Option<String> {
    text.find(')').map(|end| text[..=end].to_owned())
}

fn documented_call_shapes() -> BTreeSet<(String, Option<String>)> {
    let prefix = format!("{}.", PAGE_CONTRACT.namespace);
    header()
        .lines()
        .filter_map(|line| line.trim_start_matches([' ', '*']).strip_prefix(&prefix))
        .filter_map(|after_namespace| {
            let call = shape_up_to_close(after_namespace)?;
            let callback = after_namespace[call.len()..]
                .trim_start()
                .strip_prefix("callback(")
                .and_then(shape_up_to_close)
                .map(|args| format!("callback({args}"));
            Some((format!("{prefix}{call}"), callback))
        })
        .collect()
}

fn custom_properties_written_by_runtime() -> BTreeSet<String> {
    RUNTIME_SOURCE
        .split('"')
        .skip(1)
        .step_by(2)
        .filter(|literal| literal.starts_with("--"))
        .map(str::to_owned)
        .collect()
}

fn dataset_name(attribute: &str) -> String {
    let mut words = attribute
        .strip_prefix("data-")
        .unwrap_or_else(|| panic!("'{attribute}' is not a data attribute"))
        .split('-');
    let mut name = words.next().unwrap_or_default().to_owned();
    for word in words {
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            name.extend(first.to_uppercase());
            name.push_str(chars.as_str());
        }
    }
    name
}

fn keys_of(content: &OverlayConfig) -> BTreeSet<String> {
    content.keys().cloned().collect()
}

fn keys_a_page_can_receive(kind_id: &str) -> BTreeSet<String> {
    let reg = registry();
    let descriptor = reg.get(kind_id).expect("the kind is registered");
    let stored = OverlayConfig::new();
    match latest_binding(descriptor, &stored) {
        Some(binding) => {
            let sample = sample_latest_value(&binding.slot);
            let mut keys = keys_of(&latest_content(descriptor, &stored, Some(&sample)));
            keys.extend(keys_of(&latest_content(descriptor, &stored, None)));
            keys
        }
        None => keys_of(&sample_content(
            descriptor,
            &stored,
            &SampleContext::neutral(),
        )),
    }
}

#[test]
fn the_runtime_publishes_exactly_the_functions_the_contract_declares() {
    let declared: BTreeSet<String> = PAGE_CONTRACT
        .functions
        .iter()
        .map(|function| function.name.to_owned())
        .collect();

    assert_eq!(published_names(), declared);
}

#[test]
fn the_runtime_header_documents_exactly_the_call_shapes_and_callbacks_the_contract_declares() {
    let declared: BTreeSet<(String, Option<String>)> = PAGE_CONTRACT
        .functions
        .iter()
        .flat_map(|function| {
            function
                .calls
                .iter()
                .map(move |call| ((*call).to_owned(), function.callback.map(str::to_owned)))
        })
        .collect();

    assert_eq!(documented_call_shapes(), declared);
}

#[test]
fn the_declared_bind_writer_fills_the_nodes_carrying_the_declared_attribute() {
    let bind = PAGE_CONTRACT.bind;
    let writer = function_body(bind.writer);

    for usage in [
        format!("querySelectorAll(\"[{}]\")", bind.attribute),
        format!("getAttribute(\"{}\")", bind.attribute),
    ] {
        assert!(
            writer.contains(&usage),
            "forge.{} no longer reaches nodes through '{usage}'",
            bind.writer
        );
    }
}

#[test]
fn the_runtime_writes_exactly_the_custom_properties_the_contract_declares() {
    let declared: BTreeSet<String> = PAGE_CONTRACT
        .custom_properties
        .iter()
        .map(|property| property.property.to_owned())
        .collect();

    assert_eq!(custom_properties_written_by_runtime(), declared);
}

#[test]
fn every_custom_property_is_fed_from_the_config_key_the_contract_pairs_it_with() {
    let appearance = function_body("applyAppearance");

    for property in PAGE_CONTRACT.custom_properties {
        let read_by_appearance = appearance.contains(&format!("values.{}", property.config_key));
        let paired_margin = RUNTIME_SOURCE.contains(&format!(
            "key: \"{}\", property: \"{}\"",
            property.config_key, property.property
        ));
        assert!(
            read_by_appearance || paired_margin,
            "the runtime never feeds {} from config key '{}'",
            property.property,
            property.config_key
        );
    }
}

#[test]
fn the_runtime_mirrors_the_declared_config_key_onto_the_declared_body_attribute() {
    let body = PAGE_CONTRACT.body_attribute;
    let assignment = format!(
        "document_.{}.dataset.{} = values.{}",
        body.element,
        dataset_name(body.attribute),
        body.config_key
    );

    assert!(
        function_body("applyAppearance").contains(&assignment),
        "the runtime no longer performs '{assignment}'"
    );
}

#[test]
fn the_runtime_toggles_visibility_with_the_declared_hidden_class() {
    let declaration = format!("var HIDDEN_CLASS = \"{}\";", PAGE_CONTRACT.hidden_class);

    assert!(RUNTIME_SOURCE.contains(&declaration));
}

#[test]
fn applies_config_key_covers_the_body_attribute_and_every_custom_property_and_nothing_else() {
    for (key, applied) in [
        (POSITION, true),
        (ACCENT, true),
        (MARGIN_TOP, true),
        (MARGIN_LEFT, true),
        (DESIGN_WIDTH, false),
        (HEADLINE, false),
        ("", false),
        ("Accent", false),
        ("--accent", false),
        ("data-position", false),
    ] {
        assert_eq!(
            PAGE_CONTRACT.applies_config_key(key),
            applied,
            "applies_config_key({key:?})"
        );
    }
}

#[test]
fn every_look_declares_exactly_the_content_keys_its_page_can_receive() {
    for descriptor in registry().all() {
        let declared: BTreeSet<String> = look_contract(descriptor)
            .content_keys
            .iter()
            .map(|key| (*key).to_owned())
            .collect();

        assert_eq!(
            declared,
            keys_a_page_can_receive(descriptor.id()),
            "{} declares content keys its page never receives, or misses some it does",
            descriptor.id()
        );
    }
}

#[test]
fn a_look_lists_its_own_config_keys_in_descriptor_order_without_the_ones_the_runtime_applies() {
    let mut runtime_applied_seen = 0;
    for descriptor in registry().all() {
        let mut expected = Vec::new();
        for sectioned in descriptor.look_fields() {
            let key = sectioned.field.key();
            if sectioned.section == ConfigSection::Content {
                continue;
            }
            if RUNTIME_APPLIED_LOOK_KEYS.contains(&key) {
                runtime_applied_seen += 1;
                continue;
            }
            expected.push(LookConfigKey {
                key,
                section: sectioned.section,
            });
        }

        assert_eq!(
            look_contract(descriptor).config_keys,
            expected,
            "{}",
            descriptor.id()
        );
    }
    assert!(
        runtime_applied_seen > 0,
        "no builtin look carries a runtime-applied key, so the exclusion went unexercised"
    );
}

#[test]
fn the_chat_look_declares_the_platform_content_key() {
    let reg = registry();
    let chat = reg
        .get("overlay.chat")
        .expect("the chat kind is registered");

    assert!(look_contract(chat).content_keys.contains(&"platform"));
}
