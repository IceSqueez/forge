#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;

use forge_overlay::{
    BEHAVIOR_FILE, OverlayKindRegistry, RESERVED_DIRECTORY, RUNTIME_ASSET, STYLE_FILE,
    register_builtin_kinds,
};
use forge_registry::FormField;

fn registry() -> OverlayKindRegistry {
    let mut reg = OverlayKindRegistry::new();
    register_builtin_kinds(&mut reg).expect("the builtin overlay kinds register");
    reg
}

fn field_key(field: &FormField) -> &'static str {
    match field {
        FormField::Text { key, .. }
        | FormField::TextArea { key, .. }
        | FormField::Code { key, .. }
        | FormField::Integer { key, .. }
        | FormField::Slider { key, .. }
        | FormField::Toggle { key, .. }
        | FormField::FilePicker { key, .. }
        | FormField::DateTime { key, .. }
        | FormField::Select { key, .. }
        | FormField::DynamicSelect { key, .. }
        | FormField::Swatch { key, .. }
        | FormField::Optional { key, .. }
        | FormField::SubChain { key, .. }
        | FormField::CaseList { key, .. } => key,
    }
}

fn quoted_after(source: &str, marker: &str) -> Vec<String> {
    source
        .match_indices(marker)
        .filter_map(|(at, _)| {
            let rest = &source[at + marker.len()..];
            rest.find('"').map(|end| rest[..end].to_owned())
        })
        .collect()
}

fn without_line_comments(source: &str) -> String {
    source
        .lines()
        .map(|line| line.split("//").next().unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n")
}

fn members_after(source: &str, marker: &str) -> Vec<String> {
    source
        .match_indices(marker)
        .map(|(at, _)| {
            source[at + marker.len()..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect()
        })
        .filter(|member: &String| !member.is_empty())
        .collect()
}

#[test]
fn every_kind_markup_loads_the_shared_runtime_and_its_own_sibling_files() {
    let shared_runtime = format!("../{RESERVED_DIRECTORY}/{RUNTIME_ASSET}");

    for descriptor in registry().all() {
        let markup = descriptor.page_assets().markup;

        for needle in [shared_runtime.as_str(), STYLE_FILE, BEHAVIOR_FILE] {
            assert!(
                markup.contains(needle),
                "{} markup never references '{needle}', so the materialized page cannot load it",
                descriptor.id()
            );
        }
    }
}

#[test]
fn a_page_only_binds_config_keys_its_own_form_declares() {
    for descriptor in registry().all() {
        let declared: BTreeSet<&str> = descriptor
            .config_fields()
            .iter()
            .map(|field| field_key(field))
            .collect();
        let bindings = quoted_after(descriptor.page_assets().markup, "data-bind=\"");

        assert!(
            !bindings.is_empty(),
            "{} markup declares no bindings at all",
            descriptor.id()
        );
        for bound in &bindings {
            assert!(
                declared.contains(bound.as_str()),
                "{} markup binds '{bound}', which its form never declares",
                descriptor.id()
            );
        }
    }
}

#[test]
fn a_page_only_reads_config_members_its_own_form_declares() {
    for descriptor in registry().all() {
        let declared: BTreeSet<&str> = descriptor
            .config_fields()
            .iter()
            .map(|field| field_key(field))
            .collect();
        let behavior = without_line_comments(descriptor.page_assets().behavior);
        let members = members_after(&behavior, "config.");

        assert!(
            !members.is_empty(),
            "{} behavior reads nothing from its config",
            descriptor.id()
        );
        for member in &members {
            assert!(
                declared.contains(member.as_str()),
                "{} behavior reads config.{member}, which its form never declares",
                descriptor.id()
            );
        }
    }
}

#[test]
fn every_kind_stylesheet_takes_its_accent_and_font_from_runtime_custom_properties() {
    for descriptor in registry().all() {
        let style = descriptor.page_assets().style;

        for property in ["var(--accent)", "var(--font)"] {
            assert!(
                style.contains(property),
                "{} hardcodes what {property} should supply at runtime",
                descriptor.id()
            );
        }
    }
}
