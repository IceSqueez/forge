use forge_registry::FormField;
use forge_speech_types::ENGINE_VOICE_OPTIONS_KEY;
use forge_types::Variant;

use super::keys::*;
use super::text;
use crate::catalog::descriptor::{ConfigSection, OverlayConfig, SectionedField};
use crate::metrics::{ALERT_SIZING, ElementSizing};

pub(crate) fn shared_fields() -> Vec<SectionedField> {
    let mut fields = vec![
        delivered_in(
            ConfigSection::Content,
            FormField::Text {
                key: HEADLINE,
                label: "Headline",
                placeholder: "Thanks for the sub!",
            },
        ),
        delivered_in(
            ConfigSection::Content,
            FormField::Text {
                key: SUBLINE,
                label: "Subline",
                placeholder: "Three months subscribed",
            },
        ),
    ];
    fields.extend(shared_style_fields());
    fields
}

pub(crate) fn shared_style_fields() -> Vec<SectionedField> {
    vec![
        in_section(
            ConfigSection::Text,
            FormField::Swatch {
                key: MAIN_TEXT_COLOR,
                label: "Main text color",
                options: MAIN_TEXT_COLOR_OPTIONS,
            },
        ),
        in_section(
            ConfigSection::Text,
            FormField::Swatch {
                key: TEXT_COLOR,
                label: "Secondary text color",
                options: ACCENT_OPTIONS,
            },
        ),
        in_section(
            ConfigSection::Card,
            FormField::Swatch {
                key: BORDER_COLOR,
                label: "Border color",
                options: ACCENT_OPTIONS,
            },
        ),
        in_section(
            ConfigSection::Text,
            FormField::Select {
                key: FONT,
                label: "Font",
                options: FONT_OPTIONS,
            },
        ),
        in_section(
            ConfigSection::Text,
            FormField::Integer {
                key: TEXT_SIZE,
                label: "Text size",
                min: TEXT_SIZE_MIN_PX,
                max: TEXT_SIZE_MAX_PX,
            },
        ),
    ]
}

pub(crate) fn position_field(label: &'static str) -> SectionedField {
    in_section(
        ConfigSection::Placement,
        FormField::Select {
            key: POSITION,
            label,
            options: POSITION_OPTIONS,
        },
    )
}

pub(crate) fn design_size_fields() -> [SectionedField; 2] {
    [
        in_section(
            ConfigSection::Placement,
            FormField::Integer {
                key: DESIGN_WIDTH,
                label: "Source width",
                min: DESIGN_SIZE_MIN_PX,
                max: DESIGN_SIZE_MAX_PX,
            },
        ),
        in_section(
            ConfigSection::Placement,
            FormField::Integer {
                key: DESIGN_HEIGHT,
                label: "Source height",
                min: DESIGN_SIZE_MIN_PX,
                max: DESIGN_SIZE_MAX_PX,
            },
        ),
    ]
}

pub(crate) fn margin_fields() -> [SectionedField; 4] {
    [
        (MARGIN_TOP, "Margin top"),
        (MARGIN_RIGHT, "Margin right"),
        (MARGIN_BOTTOM, "Margin bottom"),
        (MARGIN_LEFT, "Margin left"),
    ]
    .map(|(key, label)| {
        in_section(
            ConfigSection::Placement,
            FormField::Slider {
                key,
                label,
                min: MARGIN_MIN_PERCENT,
                max: MARGIN_MAX_PERCENT,
                unit: "%",
            },
        )
    })
}

pub(crate) fn author_field() -> SectionedField {
    delivered_in(
        ConfigSection::Content,
        FormField::Text {
            key: AUTHOR,
            label: "Author",
            placeholder: "PixelPal",
        },
    )
}

pub(crate) fn author_color_field() -> SectionedField {
    delivered_in(
        ConfigSection::Content,
        FormField::Text {
            key: AUTHOR_COLOR,
            label: "Name color",
            placeholder: "#f9e2af",
        },
    )
}

pub(crate) fn badges_field() -> SectionedField {
    delivered_in(
        ConfigSection::Content,
        FormField::Text {
            key: BADGES,
            label: "Badges",
            placeholder: "MOD",
        },
    )
}

pub(crate) fn message_field() -> SectionedField {
    delivered_in(
        ConfigSection::Content,
        FormField::Text {
            key: MESSAGE,
            label: "Message",
            placeholder: "hey chat!",
        },
    )
}

pub(crate) fn chat_platform_field() -> SectionedField {
    delivered_in(
        ConfigSection::Content,
        FormField::Text {
            key: PLATFORM,
            label: "Platform",
            placeholder: "twitch",
        },
    )
}

pub(crate) fn chat_message_id_field() -> SectionedField {
    delivered_in(
        ConfigSection::Content,
        FormField::Text {
            key: MESSAGE_ID,
            label: "Message ID",
            placeholder: "%message_id%",
        },
    )
}

pub(crate) fn chat_user_id_field() -> SectionedField {
    delivered_in(
        ConfigSection::Content,
        FormField::Text {
            key: USER_ID,
            label: "User ID",
            placeholder: "%user_id%",
        },
    )
}

pub(crate) fn chat_event_field() -> SectionedField {
    delivered_in(
        ConfigSection::Content,
        FormField::Text {
            key: EVENT,
            label: "Event",
            placeholder: "Raid",
        },
    )
}

pub(crate) fn chat_avatar_field() -> SectionedField {
    delivered_in(
        ConfigSection::Content,
        FormField::Text {
            key: AVATAR,
            label: "Avatar",
            placeholder: "https://static-cdn.jtvnw.net/...",
        },
    )
}

pub(crate) fn chat_platform_icon_fields() -> [SectionedField; 3] {
    [
        (TWITCH_ICON, "Twitch icon"),
        (YOUTUBE_ICON, "YouTube icon"),
        (KICK_ICON, "Kick icon"),
    ]
    .map(|(key, label)| {
        in_section(
            ConfigSection::Content,
            FormField::DynamicSelect {
                key,
                label,
                options_key: ICON_OPTIONS_KEY,
            },
        )
    })
}

pub(crate) fn chat_platform_icon_defaults() -> OverlayConfig {
    PLATFORM_ICON_KEYS
        .into_iter()
        .map(|key| (key.to_owned(), text("")))
        .collect()
}

pub(crate) fn row_width_field() -> SectionedField {
    in_section(
        ConfigSection::Card,
        FormField::Select {
            key: ROW_WIDTH,
            label: "Row width",
            options: ROW_WIDTH_OPTIONS,
        },
    )
}

pub(crate) fn show_avatars_field() -> SectionedField {
    in_section(
        ConfigSection::Chat,
        FormField::Toggle {
            key: SHOW_AVATARS,
            label: "Show avatars",
        },
    )
}

pub(crate) fn third_party_emotes_field() -> SectionedField {
    in_section(
        ConfigSection::Chat,
        FormField::Toggle {
            key: THIRD_PARTY_EMOTES,
            label: "Third-party emotes",
        },
    )
}

pub(crate) fn chat_row_fields() -> [SectionedField; 3] {
    [
        in_section(
            ConfigSection::Chat,
            FormField::Integer {
                key: ROW_LIMIT,
                label: "Messages kept",
                min: ROW_LIMIT_MIN,
                max: ROW_LIMIT_MAX,
            },
        ),
        in_section(
            ConfigSection::Chat,
            FormField::Toggle {
                key: HIDE_ROWS,
                label: "Hide messages",
            },
        ),
        in_section(
            ConfigSection::Chat,
            FormField::Slider {
                key: HIDE_ROWS_AFTER_SECS,
                label: "Hide after",
                min: HIDE_ROWS_AFTER_MIN_SECS,
                max: HIDE_ROWS_AFTER_MAX_SECS,
                unit: "s",
            },
        ),
    ]
}

pub(crate) fn chat_row_defaults() -> OverlayConfig {
    OverlayConfig::from([
        (ROW_LIMIT.to_owned(), Variant::Int(DEFAULT_ROW_LIMIT)),
        (HIDE_ROWS.to_owned(), Variant::Bool(false)),
        (
            HIDE_ROWS_AFTER_SECS.to_owned(),
            Variant::Int(DEFAULT_HIDE_ROWS_AFTER_SECS),
        ),
    ])
}

pub(crate) fn chat_filter_fields() -> [SectionedField; 3] {
    [
        in_section(
            ConfigSection::Chat,
            FormField::Toggle {
                key: HIDE_COMMANDS,
                label: "Hide commands",
            },
        ),
        in_section(
            ConfigSection::Chat,
            FormField::Text {
                key: COMMAND_PREFIX,
                label: "Command prefix",
                placeholder: DEFAULT_COMMAND_PREFIX,
            },
        ),
        in_section(
            ConfigSection::Chat,
            FormField::Text {
                key: HIDDEN_ACCOUNTS,
                label: "Hidden accounts",
                placeholder: "Nightbot, StreamElements",
            },
        ),
    ]
}

pub(crate) fn chat_filter_defaults() -> OverlayConfig {
    OverlayConfig::from([
        (HIDE_COMMANDS.to_owned(), Variant::Bool(false)),
        (COMMAND_PREFIX.to_owned(), text(DEFAULT_COMMAND_PREFIX)),
        (HIDDEN_ACCOUNTS.to_owned(), text("")),
    ])
}

pub(crate) fn label_field() -> SectionedField {
    delivered_in(
        ConfigSection::Content,
        FormField::Text {
            key: LABEL,
            label: "Label",
            placeholder: "Sub goal",
        },
    )
}

pub(crate) fn value_field() -> SectionedField {
    delivered_in(
        ConfigSection::Content,
        FormField::Text {
            key: VALUE,
            label: "Current value",
            placeholder: "42",
        },
    )
}

pub(crate) fn target_field() -> SectionedField {
    delivered_in(
        ConfigSection::Content,
        FormField::Text {
            key: TARGET,
            label: "Target",
            placeholder: "100",
        },
    )
}

pub(crate) fn slot_field(options: &'static [&'static str]) -> SectionedField {
    in_section(
        ConfigSection::Content,
        FormField::Select {
            key: SLOT,
            label: "Slot",
            options,
        },
    )
}

pub(crate) fn platform_scope_field() -> SectionedField {
    in_section(
        ConfigSection::Content,
        FormField::Text {
            key: PLATFORM,
            label: "Platform",
            placeholder: "All platforms",
        },
    )
}

pub(crate) fn headline_field(placeholder: &'static str) -> SectionedField {
    delivered_in(
        ConfigSection::Content,
        FormField::Text {
            key: HEADLINE,
            label: "Headline",
            placeholder,
        },
    )
}

pub(crate) fn subline_field(placeholder: &'static str) -> SectionedField {
    delivered_in(
        ConfigSection::Content,
        FormField::Text {
            key: SUBLINE,
            label: "Subline",
            placeholder,
        },
    )
}

pub(crate) fn placeholder_field() -> SectionedField {
    delivered_in(
        ConfigSection::Content,
        FormField::Text {
            key: PLACEHOLDER,
            label: "Empty text",
            placeholder: "No donations yet",
        },
    )
}

pub(crate) fn duration_field() -> SectionedField {
    in_section(
        ConfigSection::Timing,
        FormField::Slider {
            key: DURATION,
            label: "Duration",
            min: DURATION_MIN_SECS,
            max: DURATION_MAX_SECS,
            unit: "s",
        },
    )
}

pub(crate) fn icon_field() -> SectionedField {
    in_section(
        ConfigSection::Content,
        FormField::DynamicSelect {
            key: ICON,
            label: "Icon",
            options_key: ICON_OPTIONS_KEY,
        },
    )
}

pub(crate) fn background_fields() -> [SectionedField; 2] {
    [
        in_section(
            ConfigSection::Card,
            FormField::Swatch {
                key: BACKGROUND_COLOR,
                label: "Background color",
                options: BACKGROUND_COLOR_OPTIONS,
            },
        ),
        in_section(
            ConfigSection::Card,
            FormField::Slider {
                key: BACKGROUND_OPACITY,
                label: "Background opacity",
                min: BACKGROUND_OPACITY_MIN_PERCENT,
                max: BACKGROUND_OPACITY_MAX_PERCENT,
                unit: "%",
            },
        ),
    ]
}

pub(crate) fn image_fields() -> [SectionedField; 3] {
    [
        in_section(
            ConfigSection::Content,
            FormField::DynamicSelect {
                key: IMAGE,
                label: "Image",
                options_key: ICON_OPTIONS_KEY,
            },
        ),
        in_section(
            ConfigSection::Card,
            FormField::Select {
                key: IMAGE_POSITION,
                label: "Image position",
                options: IMAGE_POSITION_OPTIONS,
            },
        ),
        in_section(
            ConfigSection::Card,
            FormField::Slider {
                key: IMAGE_HEIGHT,
                label: "Image height",
                min: IMAGE_HEIGHT_MIN_PX,
                max: IMAGE_HEIGHT_MAX_PX,
                unit: "px",
            },
        ),
    ]
}

pub(crate) fn card_size_fields() -> [SectionedField; 5] {
    [
        card_size_field(CARD_MIN_WIDTH, "Min card width"),
        card_size_field(CARD_MIN_HEIGHT, "Min card height"),
        card_size_field(CARD_MAX_WIDTH, "Max card width"),
        card_size_field(CARD_MAX_HEIGHT, "Max card height"),
        in_section(
            ConfigSection::Text,
            FormField::Select {
                key: TEXT_OVERFLOW,
                label: "Text overflow",
                options: TEXT_OVERFLOW_OPTIONS,
            },
        ),
    ]
}

fn card_size_field(key: &'static str, label: &'static str) -> SectionedField {
    in_section(
        ConfigSection::Card,
        FormField::Integer {
            key,
            label,
            min: CARD_SIZE_UNBOUNDED_PX,
            max: CARD_SIZE_MAX_PX,
        },
    )
}

pub(crate) fn card_size_defaults() -> OverlayConfig {
    let unbounded = |key: &str| (key.to_owned(), Variant::Int(CARD_SIZE_UNBOUNDED_PX));
    OverlayConfig::from([
        unbounded(CARD_MIN_WIDTH),
        unbounded(CARD_MIN_HEIGHT),
        unbounded(CARD_MAX_WIDTH),
        unbounded(CARD_MAX_HEIGHT),
        (TEXT_OVERFLOW.to_owned(), text(TEXT_OVERFLOW_WRAP)),
    ])
}

pub(crate) fn label_placement_fields() -> [SectionedField; 3] {
    [
        in_section(
            ConfigSection::Card,
            FormField::Select {
                key: LABEL_SIDE,
                label: "Caption side",
                options: LABEL_SIDE_OPTIONS,
            },
        ),
        in_section(
            ConfigSection::Card,
            FormField::Integer {
                key: LABEL_OFFSET_X,
                label: "Caption offset right",
                min: LABEL_OFFSET_MIN_PX,
                max: LABEL_OFFSET_MAX_PX,
            },
        ),
        in_section(
            ConfigSection::Card,
            FormField::Integer {
                key: LABEL_OFFSET_Y,
                label: "Caption offset down",
                min: LABEL_OFFSET_MIN_PX,
                max: LABEL_OFFSET_MAX_PX,
            },
        ),
    ]
}

pub(crate) fn sound_field() -> SectionedField {
    in_section(
        ConfigSection::Audio,
        FormField::DynamicSelect {
            key: SOUND,
            label: "Sound",
            options_key: SOUND_OPTIONS_KEY,
        },
    )
}

pub(crate) fn speech_field() -> SectionedField {
    delivered_in(
        ConfigSection::Audio,
        FormField::TextArea {
            key: SPEECH,
            label: "Speech",
        },
    )
}

pub(crate) fn speech_voice_field() -> SectionedField {
    in_section(
        ConfigSection::Audio,
        FormField::DynamicSelect {
            key: SPEECH_VOICE,
            label: "Speech voice",
            options_key: ENGINE_VOICE_OPTIONS_KEY,
        },
    )
}

pub(crate) fn in_section(section: ConfigSection, field: FormField) -> SectionedField {
    SectionedField {
        section,
        delivered: false,
        field,
    }
}

pub(crate) fn delivered_in(section: ConfigSection, field: FormField) -> SectionedField {
    SectionedField {
        section,
        delivered: true,
        field,
    }
}

pub(crate) fn card_look_fields() -> Vec<SectionedField> {
    let mut fields = shared_fields();
    fields.push(icon_field());
    fields.extend(background_fields());
    fields.extend(card_size_fields());
    fields.extend(image_fields());
    fields
}

pub(crate) fn card_look_defaults(
    accent: &str,
    font: &str,
    headline: &str,
    subline: &str,
) -> OverlayConfig {
    let mut defaults = shared_style_defaults(accent, font, ALERT_SIZING);
    defaults.extend([
        (HEADLINE.to_owned(), text(headline)),
        (SUBLINE.to_owned(), text(subline)),
        (ICON.to_owned(), text(DEFAULT_ICON)),
        (BACKGROUND_COLOR.to_owned(), text(DEFAULT_BACKGROUND_COLOR)),
        (
            BACKGROUND_OPACITY.to_owned(),
            Variant::Int(DEFAULT_BACKGROUND_OPACITY_PERCENT),
        ),
        (IMAGE.to_owned(), text("")),
        (IMAGE_POSITION.to_owned(), text(IMAGE_POSITION_ABOVE)),
        (
            IMAGE_HEIGHT.to_owned(),
            Variant::Int(DEFAULT_IMAGE_HEIGHT_PX),
        ),
    ]);
    defaults.extend(card_size_defaults());
    defaults
}

pub(crate) fn shared_style_defaults(
    accent: &str,
    font: &str,
    sizing: ElementSizing,
) -> OverlayConfig {
    OverlayConfig::from([
        (MAIN_TEXT_COLOR.to_owned(), text(DEFAULT_MAIN_TEXT_COLOR)),
        (TEXT_COLOR.to_owned(), text(accent)),
        (BORDER_COLOR.to_owned(), text(accent)),
        (FONT.to_owned(), text(font)),
        (
            TEXT_SIZE.to_owned(),
            Variant::Int(i64::from(sizing.default_text_size_px())),
        ),
    ])
}
