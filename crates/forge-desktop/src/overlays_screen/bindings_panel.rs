use std::ops::Range;

use forge_components::{
    BORDER_THIN, FONT_XXS, ForgePalette, body_family, mono_family, section_label, tr,
};
use forge_overlay::config::{
    ACCENT, AUTHOR, AUTHOR_COLOR, BADGES, FONT, HEADLINE, ICON, LABEL, MESSAGE, PLACEHOLDER,
    PLATFORM, POSITION, SLOT, SUBLINE, TARGET, TEXT_SIZE, VALUE,
};
use forge_overlay::page_contract::{
    CONTENT_FUNCTION, READY_FUNCTION, SET_FUNCTION, SHOW_FUNCTION, SOUND_FUNCTION,
};
use forge_overlay::{LookContract, PAGE_CONTRACT, PageContract, PageFunction};
use gpui::{AnyElement, Div, HighlightStyle, Hsla, Pixels, Rgba, StyledText, div, prelude::*, px};

use super::base_sections::base_label;

const HEADING_GAP: Pixels = px(10.0);
const GROUP_TOP_GAP: Pixels = px(14.0);
const GROUP_HEADING_GAP: Pixels = px(8.0);

const INTRO_FS: Pixels = px(11.5);
const INTRO_LINE_H: Pixels = px(18.4);
const INTRO_GAP: Pixels = px(14.0);

const CODE_FS: Pixels = px(11.0);
const ENTRY_GAP: Pixels = px(8.0);

const CHIP_FS: Pixels = px(10.0);
const CHIP_RADIUS: Pixels = px(4.0);
const CHIP_PAD_V: Pixels = px(2.0);
const CHIP_PAD_H: Pixels = px(6.0);
const CHIP_GAP: Pixels = px(5.0);

const EXAMPLE_NAME: &str = "name";
const EXAMPLE_TEXT: &str = "text";
const CONFIG_ARGUMENT: &str = "config";

const FORMAT_ISOLATION_MARKS: [char; 2] = ['\u{2068}', '\u{2069}'];

pub(super) fn bindings_reference(
    look: Option<&LookContract>,
    palette: &ForgePalette,
) -> Vec<AnyElement> {
    let contract = &PAGE_CONTRACT;
    vec![
        div()
            .pb(HEADING_GAP)
            .child(section_label(
                tr!("overlays_bindings_heading").to_uppercase(),
                palette,
            ))
            .into_any_element(),
        intro(contract, palette),
        group(tr!("overlays_bindings_api_heading"), palette)
            .children(
                contract
                    .functions
                    .iter()
                    .map(|function| function_entry(function, palette)),
            )
            .into_any_element(),
        group(tr!("overlays_bindings_content_heading"), palette)
            .child(content_fields(look, palette))
            .into_any_element(),
        group(tr!("overlays_bindings_runtime_heading"), palette)
            .children(runtime_entries(contract, palette))
            .into_any_element(),
        group(tr!("overlays_bindings_config_heading"), palette)
            .child(config_keys(look, palette))
            .into_any_element(),
    ]
}

fn group(heading: String, palette: &ForgePalette) -> Div {
    div().flex().flex_col().pt(GROUP_TOP_GAP).child(
        div()
            .pb(GROUP_HEADING_GAP)
            .child(section_label(heading.to_uppercase(), palette)),
    )
}

fn intro(contract: &PageContract, palette: &ForgePalette) -> AnyElement {
    let marker = format!("{}=\"{EXAMPLE_NAME}\"", contract.bind.attribute);
    let writer = format!("{}.{}", contract.namespace, contract.bind.writer);
    let call = format!("{writer}({EXAMPLE_NAME}, {EXAMPLE_TEXT})");
    let text = strip_isolation(&tr!(
        "overlays_bindings_intro",
        marker = marker.as_str(),
        call = call.as_str(),
        writer = writer.as_str()
    ));
    let ranges = code_ranges(&text, &[&call, &marker, &writer]);
    let highlight = HighlightStyle {
        color: Some(Hsla::from(palette.brand)),
        ..HighlightStyle::default()
    };
    let mono = mono_family();
    let styled = StyledText::new(text)
        .with_highlights(ranges.iter().map(|range| (range.clone(), highlight)))
        .with_font_family_overrides(ranges.into_iter().map(|range| (range, mono.clone())));

    div()
        .pb(INTRO_GAP)
        .font_family(body_family())
        .text_size(INTRO_FS)
        .line_height(INTRO_LINE_H)
        .text_color(palette.text_secondary)
        .child(styled)
        .into_any_element()
}

fn function_entry(function: &PageFunction, palette: &ForgePalette) -> AnyElement {
    let callback = function
        .callback
        .map(|shape| code_line(shape, palette.text_muted));
    div()
        .pb(ENTRY_GAP)
        .flex()
        .flex_col()
        .children(
            function
                .calls
                .iter()
                .map(|call| code_line(call, palette.success)),
        )
        .children(callback)
        .children(function_summary(function.name).map(|text| summary(text, palette)))
        .into_any_element()
}

fn runtime_entries(contract: &PageContract, palette: &ForgePalette) -> Vec<AnyElement> {
    let attribute = &contract.body_attribute;
    contract
        .custom_properties
        .iter()
        .map(|property| (property.property.to_owned(), property.config_key))
        .chain(std::iter::once((
            format!("{}[{}]", attribute.element, attribute.attribute),
            attribute.config_key,
        )))
        .map(|(code, config_key)| {
            div()
                .pb(ENTRY_GAP)
                .flex()
                .flex_col()
                .child(code_line(&code, palette.info))
                .children(runtime_summary(config_key).map(|text| summary(text, palette)))
                .into_any_element()
        })
        .collect()
}

fn content_fields(look: Option<&LookContract>, palette: &ForgePalette) -> AnyElement {
    let Some(look) = look else {
        return summary(tr!("overlays_bindings_look_unavailable"), palette);
    };
    if look.content_keys.is_empty() {
        return summary(tr!("overlays_bindings_content_empty"), palette);
    }
    div()
        .flex()
        .flex_col()
        .gap(CHIP_GAP)
        .children(look.content_keys.iter().map(|key| {
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(CHIP_GAP)
                .child(key_chip(key, palette))
                .children(binding_label(key).map(|label| summary(label, palette)))
        }))
        .into_any_element()
}

fn config_keys(look: Option<&LookContract>, palette: &ForgePalette) -> AnyElement {
    let Some(look) = look else {
        return summary(tr!("overlays_bindings_look_unavailable"), palette);
    };
    if look.config_keys.is_empty() {
        return summary(tr!("overlays_bindings_config_empty"), palette);
    }
    div()
        .flex()
        .flex_col()
        .children(look.config_keys.iter().map(|entry| {
            div()
                .pb(ENTRY_GAP)
                .flex()
                .flex_col()
                .child(code_line(
                    &format!("{CONFIG_ARGUMENT}.{}", entry.key),
                    palette.warning,
                ))
                .children(binding_label(entry.key).map(|label| summary(label, palette)))
        }))
        .into_any_element()
}

fn key_chip(key: &str, palette: &ForgePalette) -> AnyElement {
    div()
        .flex_none()
        .px(CHIP_PAD_H)
        .py(CHIP_PAD_V)
        .rounded(CHIP_RADIUS)
        .border(BORDER_THIN)
        .border_color(palette.border_regular)
        .bg(palette.elevated)
        .font_family(mono_family())
        .text_size(CHIP_FS)
        .text_color(palette.warning)
        .child(key.to_owned())
        .into_any_element()
}

fn code_line(code: &str, tone: Rgba) -> AnyElement {
    div()
        .font_family(mono_family())
        .text_size(CODE_FS)
        .text_color(tone)
        .child(code.to_owned())
        .into_any_element()
}

fn summary(text: String, palette: &ForgePalette) -> AnyElement {
    div()
        .font_family(body_family())
        .text_size(FONT_XXS)
        .text_color(palette.text_faint)
        .child(text)
        .into_any_element()
}

fn function_summary(name: &str) -> Option<String> {
    match name {
        READY_FUNCTION => Some(tr!("overlays_bindings_fn_ready")),
        CONTENT_FUNCTION => Some(tr!("overlays_bindings_fn_content")),
        SET_FUNCTION => Some(tr!("overlays_bindings_fn_set")),
        SHOW_FUNCTION => Some(tr!("overlays_bindings_fn_show")),
        SOUND_FUNCTION => Some(tr!("overlays_bindings_fn_sound")),
        _ => None,
    }
}

fn runtime_summary(config_key: &str) -> Option<String> {
    match config_key {
        ACCENT => Some(tr!("overlays_bindings_runtime_accent")),
        FONT => Some(tr!("overlays_bindings_runtime_font")),
        TEXT_SIZE => Some(tr!("overlays_bindings_runtime_text_size")),
        POSITION => Some(tr!("overlays_bindings_runtime_position")),
        _ => base_label(config_key),
    }
}

fn binding_label(key: &str) -> Option<String> {
    match key {
        HEADLINE => Some(tr!("overlays_bindings_field_headline")),
        SUBLINE => Some(tr!("overlays_bindings_field_subline")),
        AUTHOR => Some(tr!("overlays_bindings_field_author")),
        AUTHOR_COLOR => Some(tr!("overlays_bindings_field_author_color")),
        BADGES => Some(tr!("overlays_bindings_field_badges")),
        MESSAGE => Some(tr!("overlays_bindings_field_message")),
        LABEL => Some(tr!("overlays_bindings_field_label")),
        VALUE => Some(tr!("overlays_bindings_field_value")),
        TARGET => Some(tr!("overlays_bindings_field_target")),
        PLACEHOLDER => Some(tr!("overlays_bindings_field_placeholder")),
        ICON => Some(tr!("overlays_bindings_field_icon")),
        SLOT => Some(tr!("overlays_bindings_field_slot")),
        PLATFORM => Some(tr!("overlays_bindings_field_platform")),
        _ => base_label(key),
    }
}

fn strip_isolation(text: &str) -> String {
    text.replace(FORMAT_ISOLATION_MARKS, "")
}

fn code_ranges(text: &str, tokens: &[&str]) -> Vec<Range<usize>> {
    let mut ranges: Vec<Range<usize>> = Vec::new();
    for token in tokens.iter().filter(|token| !token.is_empty()) {
        for (start, matched) in text.match_indices(token) {
            let range = start..start + matched.len();
            let overlaps = ranges
                .iter()
                .any(|taken| taken.start < range.end && range.start < taken.end);
            if !overlaps {
                ranges.push(range);
            }
        }
    }
    ranges.sort_by_key(|range| range.start);
    ranges
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use forge_overlay::{OverlayKindRegistry, look_contract, register_builtin_kinds};
    use forge_storage::Language;

    use super::*;
    use crate::i18n::install_language;

    const LOCALES: [Language; 2] = [Language::En, Language::Uk];

    fn is_missing_label(text: &str) -> bool {
        text.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    }

    fn intro_tokens() -> (String, String, String) {
        let contract = &PAGE_CONTRACT;
        let marker = format!("{}=\"{EXAMPLE_NAME}\"", contract.bind.attribute);
        let writer = format!("{}.{}", contract.namespace, contract.bind.writer);
        let call = format!("{writer}({EXAMPLE_NAME}, {EXAMPLE_TEXT})");
        (marker, call, writer)
    }

    fn spans<'a>(text: &'a str, ranges: &[Range<usize>]) -> Vec<&'a str> {
        ranges
            .iter()
            .map(|range| text.get(range.clone()).expect("range on a char boundary"))
            .collect()
    }

    #[test]
    fn localized_intro_highlights_marker_call_and_writer_once_each() {
        let (marker, call, writer) = intro_tokens();
        for locale in LOCALES {
            install_language(locale);
            let text = strip_isolation(&tr!(
                "overlays_bindings_intro",
                marker = marker.as_str(),
                call = call.as_str(),
                writer = writer.as_str()
            ));
            let mut found = spans(&text, &code_ranges(&text, &[&call, &marker, &writer]));
            found.sort_unstable();
            let mut expected = vec![call.as_str(), marker.as_str(), writer.as_str()];
            expected.sort_unstable();
            assert_eq!(found, expected, "{locale:?}: {text}");
        }
    }

    #[test]
    fn localized_intro_carries_no_isolation_marks_after_stripping() {
        let (marker, call, writer) = intro_tokens();
        for locale in LOCALES {
            install_language(locale);
            let raw = tr!(
                "overlays_bindings_intro",
                marker = marker.as_str(),
                call = call.as_str(),
                writer = writer.as_str()
            );
            assert!(raw.contains(FORMAT_ISOLATION_MARKS), "{locale:?}");
            assert!(
                !strip_isolation(&raw).contains(FORMAT_ISOLATION_MARKS),
                "{locale:?}"
            );
        }
    }

    #[test]
    fn code_ranges_marks_each_occurrence_once_in_text_order_without_overlap() {
        let cases: [(&str, &[&str], &[&str]); 8] = [
            (
                "forge.set then forge.set(a)",
                &["forge.set(a)", "forge.set"],
                &["forge.set", "forge.set(a)"],
            ),
            (
                "use forge.set(a) then forge.set",
                &["forge.set(a)", "forge.set"],
                &["forge.set(a)", "forge.set"],
            ),
            (
                "forge.set first, then x=\"y\"",
                &["x=\"y\"", "forge.set"],
                &["forge.set", "x=\"y\""],
            ),
            (
                "Позначте елемент x=\"y\", і forge.set(a) запише",
                &["forge.set(a)", "x=\"y\""],
                &["x=\"y\"", "forge.set(a)"],
            ),
            ("a token, a token", &["token"], &["token", "token"]),
            ("no code here", &["forge.set(a)", "x=\"y\""], &[]),
            ("empty tokens never match", &["", ""], &[]),
            ("", &["forge.set"], &[]),
        ];
        for (text, tokens, expected) in cases {
            let ranges = code_ranges(text, tokens);
            assert_eq!(spans(text, &ranges), expected, "{text:?}");
            assert!(
                ranges.windows(2).all(|pair| pair[0].end <= pair[1].start),
                "{text:?}"
            );
        }
    }

    #[test]
    fn every_page_function_has_a_localized_summary() {
        for locale in LOCALES {
            install_language(locale);
            for function in PAGE_CONTRACT.functions {
                let summary = function_summary(function.name);
                assert!(
                    summary
                        .as_deref()
                        .is_some_and(|text| !is_missing_label(text)),
                    "{locale:?}: {} -> {summary:?}",
                    function.name
                );
            }
        }
    }

    #[test]
    fn every_runtime_binding_has_a_localized_summary() {
        let attribute = PAGE_CONTRACT.body_attribute.config_key;
        let keys = PAGE_CONTRACT
            .custom_properties
            .iter()
            .map(|property| property.config_key)
            .chain(std::iter::once(attribute));
        let keys: Vec<&str> = keys.collect();
        for locale in LOCALES {
            install_language(locale);
            for key in &keys {
                let summary = runtime_summary(key);
                assert!(
                    summary
                        .as_deref()
                        .is_some_and(|text| !is_missing_label(text)),
                    "{locale:?}: {key} -> {summary:?}"
                );
            }
        }
    }

    #[test]
    fn every_content_and_config_key_of_every_builtin_look_has_a_localized_label() {
        let mut kinds = OverlayKindRegistry::new();
        register_builtin_kinds(&mut kinds).expect("the builtin overlay kinds register");
        let keys: Vec<(String, &str)> = kinds
            .all()
            .map(look_contract)
            .flat_map(|look| {
                let kind = look.kind_id.clone();
                look.content_keys
                    .iter()
                    .copied()
                    .chain(look.config_keys.iter().map(|entry| entry.key))
                    .map(move |key| (kind.clone(), key))
                    .collect::<Vec<_>>()
            })
            .collect();
        for locale in LOCALES {
            install_language(locale);
            for (kind, key) in &keys {
                let label = binding_label(key);
                assert!(
                    label.as_deref().is_some_and(|text| !is_missing_label(text)),
                    "{locale:?}: {kind}.{key} -> {label:?}"
                );
            }
        }
    }
}
