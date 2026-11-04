mod common;
use common::registry;

use std::collections::BTreeMap;

use forge_overlay::ConfigSection;
use forge_overlay::config::{
    AUTHOR, AUTHOR_COLOR, AVATAR, BACKGROUND_COLOR, BACKGROUND_OPACITY, BADGES, BORDER_COLOR,
    CARD_HEIGHT_LIMITED, CARD_MIN_HEIGHT, CARD_MIN_WIDTH, CARD_WIDTH_LIMITED, COMMAND_PREFIX,
    DESIGN_HEIGHT, DESIGN_WIDTH, DURATION, EVENT, FONT, HEADLINE, HIDDEN_ACCOUNTS, HIDE_COMMANDS,
    HIDE_ROWS, HIDE_ROWS_AFTER_SECS, ICON, IMAGE, IMAGE_HEIGHT, IMAGE_POSITION, LABEL,
    LABEL_OFFSET_X, LABEL_OFFSET_Y, LABEL_SIDE, MARGIN_BOTTOM, MARGIN_LEFT, MARGIN_RIGHT,
    MARGIN_TOP, MESSAGE, MESSAGE_ID, PLACEHOLDER, PLATFORM, POSITION, ROW_LIMIT, SHOW_AVATARS,
    SLOT, SOUND, SPEECH, SPEECH_VOICE, SUBLINE, TARGET, TEXT_COLOR, TEXT_OVERFLOW, TEXT_SIZE,
    THIRD_PARTY_EMOTES, USER_ID, VALUE,
};
use forge_overlay::motion::{
    ENTRANCE, ENTRANCE_MS, EXIT, EXIT_MS, INTENSITY, TEXT_EFFECT, TEXT_STAGGER_CUSTOM, TEXT_UNIT,
};

fn agreed_section(key: &str) -> Option<ConfigSection> {
    let section = match key {
        HEADLINE | SUBLINE | LABEL | VALUE | TARGET | PLACEHOLDER | SLOT | PLATFORM | ICON
        | IMAGE | AUTHOR | AUTHOR_COLOR | BADGES | MESSAGE | MESSAGE_ID | USER_ID | EVENT
        | AVATAR => ConfigSection::Content,
        TEXT_COLOR | FONT | TEXT_SIZE | TEXT_OVERFLOW => ConfigSection::Text,
        BORDER_COLOR | BACKGROUND_COLOR | BACKGROUND_OPACITY | CARD_MIN_WIDTH | CARD_MIN_HEIGHT
        | CARD_WIDTH_LIMITED | CARD_HEIGHT_LIMITED | IMAGE_POSITION | IMAGE_HEIGHT | LABEL_SIDE
        | LABEL_OFFSET_X | LABEL_OFFSET_Y => ConfigSection::Card,
        POSITION | DESIGN_WIDTH | DESIGN_HEIGHT | MARGIN_TOP | MARGIN_RIGHT | MARGIN_BOTTOM
        | MARGIN_LEFT => ConfigSection::Placement,
        ENTRANCE | ENTRANCE_MS | EXIT | EXIT_MS | TEXT_EFFECT | TEXT_UNIT | TEXT_STAGGER_CUSTOM
        | INTENSITY => ConfigSection::Motion,
        ROW_LIMIT | HIDE_ROWS | HIDE_ROWS_AFTER_SECS | HIDE_COMMANDS | COMMAND_PREFIX
        | HIDDEN_ACCOUNTS | SHOW_AVATARS | THIRD_PARTY_EMOTES => ConfigSection::Chat,
        SOUND | SPEECH | SPEECH_VOICE => ConfigSection::Audio,
        DURATION => ConfigSection::Timing,
        _ => return None,
    };
    Some(section)
}

#[test]
fn every_field_of_every_look_sits_in_the_section_the_panel_layout_assigns_it() {
    for descriptor in registry().all() {
        for sectioned in descriptor.config_fields() {
            let key = sectioned.field.key();
            let agreed = agreed_section(key).unwrap_or_else(|| {
                panic!(
                    "{} declares {key}, which the panel layout never placed in a section",
                    descriptor.id()
                )
            });
            assert_eq!(sectioned.section, agreed, "{} {key}", descriptor.id());
        }
    }
}

#[test]
fn fields_shared_by_two_looks_keep_the_same_relative_order_inside_their_section() {
    let mut orders: Vec<(String, BTreeMap<String, Vec<&'static str>>)> = Vec::new();
    for descriptor in registry().all() {
        let mut by_section: BTreeMap<String, Vec<&'static str>> = BTreeMap::new();
        for sectioned in descriptor.config_fields() {
            by_section
                .entry(format!("{:?}", sectioned.section))
                .or_default()
                .push(sectioned.field.key());
        }
        orders.push((descriptor.id().to_owned(), by_section));
    }

    let mut compared_pairs = 0;
    for (index, (left_id, left)) in orders.iter().enumerate() {
        for (right_id, right) in &orders[index + 1..] {
            for (section, left_keys) in left {
                let Some(right_keys) = right.get(section) else {
                    continue;
                };
                let left_shared: Vec<_> = left_keys
                    .iter()
                    .filter(|key| right_keys.contains(key))
                    .collect();
                let right_shared: Vec<_> = right_keys
                    .iter()
                    .filter(|key| left_keys.contains(key))
                    .collect();
                if left_shared.len() > 1 {
                    compared_pairs += 1;
                }
                assert_eq!(
                    left_shared, right_shared,
                    "{section}: {left_id} and {right_id} order their shared fields differently"
                );
            }
        }
    }
    assert!(
        compared_pairs > 0,
        "no two looks share two fields of a section, so the order check compared nothing"
    );
}
