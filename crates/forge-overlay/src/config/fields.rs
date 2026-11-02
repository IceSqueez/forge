use forge_registry::FormField;
use forge_speech_types::ENGINE_VOICE_OPTIONS_KEY;
use forge_types::Variant;

use super::keys::*;
use super::text;
use crate::catalog::descriptor::{ConfigSection, OverlayConfig, SectionedField};
use crate::metrics::ElementSizing;

pub(crate) fn shared_fields() -> Vec<SectionedField> {
    let mut fields = vec![
        in_section(
            ConfigSection::Content,
            FormField::Text {
                key: HEADLINE,
                label: "Headline",
                placeholder: "Thanks for the sub!",
            },
        ),
        in_section(
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
            ConfigSection::Style,
            FormField::Swatch {
                key: ACCENT,
                label: "Accent",
                options: ACCENT_OPTIONS,
            },
        ),
        in_section(
            ConfigSection::Style,
            FormField::Select {
                key: FONT,
                label: "Font",
                options: FONT_OPTIONS,
            },
        ),
        in_section(
            ConfigSection::Style,
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
        ConfigSection::Style,
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
            ConfigSection::Style,
            FormField::Integer {
                key: DESIGN_WIDTH,
                label: "Source width",
                min: DESIGN_SIZE_MIN_PX,
                max: DESIGN_SIZE_MAX_PX,
            },
        ),
        in_section(
            ConfigSection::Style,
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
            ConfigSection::Style,
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
    in_section(
        ConfigSection::Content,
        FormField::Text {
            key: AUTHOR,
            label: "Author",
            placeholder: "PixelPal",
        },
    )
}

pub(crate) fn author_color_field() -> SectionedField {
    in_section(
        ConfigSection::Content,
        FormField::Text {
            key: AUTHOR_COLOR,
            label: "Name color",
            placeholder: "#f9e2af",
        },
    )
}

pub(crate) fn badges_field() -> SectionedField {
    in_section(
        ConfigSection::Content,
        FormField::Text {
            key: BADGES,
            label: "Badges",
            placeholder: "MOD",
        },
    )
}

pub(crate) fn message_field() -> SectionedField {
    in_section(
        ConfigSection::Content,
        FormField::Text {
            key: MESSAGE,
            label: "Message",
            placeholder: "hey chat!",
        },
    )
}

pub(crate) fn chat_platform_field() -> SectionedField {
    in_section(
        ConfigSection::Content,
        FormField::Text {
            key: PLATFORM,
            label: "Platform",
            placeholder: "twitch",
        },
    )
}

pub(crate) fn chat_message_id_field() -> SectionedField {
    in_section(
        ConfigSection::Content,
        FormField::Text {
            key: MESSAGE_ID,
            label: "Message ID",
            placeholder: "%message_id%",
        },
    )
}

pub(crate) fn chat_user_id_field() -> SectionedField {
    in_section(
        ConfigSection::Content,
        FormField::Text {
            key: USER_ID,
            label: "User ID",
            placeholder: "%user_id%",
        },
    )
}

pub(crate) fn chat_row_fields() -> [SectionedField; 3] {
    [
        in_section(
            ConfigSection::Behavior,
            FormField::Integer {
                key: ROW_LIMIT,
                label: "Messages kept",
                min: ROW_LIMIT_MIN,
                max: ROW_LIMIT_MAX,
            },
        ),
        in_section(
            ConfigSection::Behavior,
            FormField::Toggle {
                key: HIDE_ROWS,
                label: "Hide messages",
            },
        ),
        in_section(
            ConfigSection::Behavior,
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
            ConfigSection::Behavior,
            FormField::Toggle {
                key: HIDE_COMMANDS,
                label: "Hide commands",
            },
        ),
        in_section(
            ConfigSection::Behavior,
            FormField::Text {
                key: COMMAND_PREFIX,
                label: "Command prefix",
                placeholder: DEFAULT_COMMAND_PREFIX,
            },
        ),
        in_section(
            ConfigSection::Behavior,
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
    in_section(
        ConfigSection::Content,
        FormField::Text {
            key: LABEL,
            label: "Label",
            placeholder: "Sub goal",
        },
    )
}

pub(crate) fn value_field() -> SectionedField {
    in_section(
        ConfigSection::Content,
        FormField::Text {
            key: VALUE,
            label: "Current value",
            placeholder: "42",
        },
    )
}

pub(crate) fn target_field() -> SectionedField {
    in_section(
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
        ConfigSection::Behavior,
        FormField::Select {
            key: SLOT,
            label: "Slot",
            options,
        },
    )
}

pub(crate) fn platform_scope_field() -> SectionedField {
    in_section(
        ConfigSection::Behavior,
        FormField::Text {
            key: PLATFORM,
            label: "Platform",
            placeholder: "All platforms",
        },
    )
}

pub(crate) fn headline_field(placeholder: &'static str) -> SectionedField {
    in_section(
        ConfigSection::Content,
        FormField::Text {
            key: HEADLINE,
            label: "Headline",
            placeholder,
        },
    )
}

pub(crate) fn subline_field(placeholder: &'static str) -> SectionedField {
    in_section(
        ConfigSection::Content,
        FormField::Text {
            key: SUBLINE,
            label: "Subline",
            placeholder,
        },
    )
}

pub(crate) fn placeholder_field() -> SectionedField {
    in_section(
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
        ConfigSection::Behavior,
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
        ConfigSection::Style,
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
            ConfigSection::Style,
            FormField::Swatch {
                key: BACKGROUND_COLOR,
                label: "Background color",
                options: BACKGROUND_COLOR_OPTIONS,
            },
        ),
        in_section(
            ConfigSection::Style,
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
            ConfigSection::Style,
            FormField::DynamicSelect {
                key: IMAGE,
                label: "Image",
                options_key: ICON_OPTIONS_KEY,
            },
        ),
        in_section(
            ConfigSection::Style,
            FormField::Select {
                key: IMAGE_POSITION,
                label: "Image position",
                options: IMAGE_POSITION_OPTIONS,
            },
        ),
        in_section(
            ConfigSection::Style,
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

pub(crate) fn label_placement_fields() -> [SectionedField; 3] {
    [
        in_section(
            ConfigSection::Style,
            FormField::Select {
                key: LABEL_SIDE,
                label: "Caption side",
                options: LABEL_SIDE_OPTIONS,
            },
        ),
        in_section(
            ConfigSection::Style,
            FormField::Integer {
                key: LABEL_OFFSET_X,
                label: "Caption offset right",
                min: LABEL_OFFSET_MIN_PX,
                max: LABEL_OFFSET_MAX_PX,
            },
        ),
        in_section(
            ConfigSection::Style,
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
        ConfigSection::Behavior,
        FormField::DynamicSelect {
            key: SOUND,
            label: "Sound",
            options_key: SOUND_OPTIONS_KEY,
        },
    )
}

pub(crate) fn speech_field() -> SectionedField {
    in_section(
        ConfigSection::Content,
        FormField::TextArea {
            key: SPEECH,
            label: "Speech",
        },
    )
}

pub(crate) fn speech_voice_field() -> SectionedField {
    in_section(
        ConfigSection::Behavior,
        FormField::DynamicSelect {
            key: SPEECH_VOICE,
            label: "Speech voice",
            options_key: ENGINE_VOICE_OPTIONS_KEY,
        },
    )
}

pub(crate) fn in_section(section: ConfigSection, field: FormField) -> SectionedField {
    SectionedField { section, field }
}

pub(crate) fn shared_style_defaults(
    accent: &str,
    font: &str,
    sizing: ElementSizing,
) -> OverlayConfig {
    OverlayConfig::from([
        (ACCENT.to_owned(), text(accent)),
        (FONT.to_owned(), text(font)),
        (
            TEXT_SIZE.to_owned(),
            Variant::Int(i64::from(sizing.default_text_size_px())),
        ),
    ])
}
