use super::*;
use crate::automation::globals::variant_kind_color;
use crate::integrations::integration_catalog::{catalog_text, tint_color};
use crate::shared::option_choices::ChoiceOption;
use crate::shared::user_text::resolve_message;
use forge_components::{ActivePresentation, FONT_XS, ForgePalette, Icon, tr};
use forge_host::integration_catalog::declaration_of;
use forge_platform_core::IntegrationDeclaration;
use forge_registry::{StepCardDetail, SubActionCategory, SubActionRegistry, SubActionRunner};
use forge_runtime::{
    CHAT_SEND_KIND_ID, FILE_READ_KIND_ID, GLOBAL_INCREMENT_KIND_ID, GLOBAL_SET_KIND_ID,
    RANDOM_INT_KIND_ID,
};
use forge_storage::SCRIPT_RUN_NAMED_KIND;
use forge_types::{SubActionStep, VariantKind};
use gpui::{AnyElement, App, HighlightStyle, Hsla, Rgba, SharedString, StyledText, div};
use std::collections::HashMap;
use std::ops::Range;

pub(in crate::automation::actions_screen) const UNKNOWN_STEP_ICON: &str = "bolt";

fn option_label_detail(
    options_key: &str,
    value: &str,
    select_options: &HashMap<String, Vec<ChoiceOption>>,
) -> String {
    let is_templated = forge_types::variable_references(value).next().is_some();
    if value.is_empty() || is_templated {
        return value.to_owned();
    }
    let Some(options) = select_options.get(options_key) else {
        return String::new();
    };
    options
        .iter()
        .find(|option| option.value == value)
        .map(|option| option.label.clone())
        .unwrap_or_else(|| tr!("action_editor_option_missing"))
}

pub(super) fn shown_card_detail(
    detail: StepCardDetail,
    select_options: &HashMap<String, Vec<ChoiceOption>>,
) -> String {
    match detail {
        StepCardDetail::Text(text) => text,
        StepCardDetail::Message(message) => resolve_message(&message).inline(),
        StepCardDetail::OptionLabel { options_key, value } => {
            option_label_detail(options_key, &value, select_options)
        }
    }
}

pub(in crate::automation::actions_screen) fn step_card_detail(
    runner: Option<&dyn SubActionRunner>,
    step: &SubActionStep,
    select_options: &HashMap<String, Vec<ChoiceOption>>,
) -> Option<String> {
    let detail = runner?.card_detail(&step.config)?;
    Some(shown_card_detail(detail, select_options))
}

struct CategoryLook {
    slug: &'static str,
    label: fn() -> String,
    color: fn(&ForgePalette) -> Rgba,
}

fn category_look(cat: SubActionCategory) -> CategoryLook {
    match cat {
        SubActionCategory::Chat => CategoryLook {
            slug: "chat",
            label: || tr!("sub_cat_chat"),
            color: |palette| palette.theme_brand,
        },
        SubActionCategory::Moderation => CategoryLook {
            slug: "moderation",
            label: || tr!("sub_cat_moderation"),
            color: |palette| palette.danger,
        },
        SubActionCategory::ChannelPoints => CategoryLook {
            slug: "channel-points",
            label: || tr!("sub_cat_channel_points"),
            color: |palette| palette.accent_pink_light,
        },
        SubActionCategory::PollsPredictions => CategoryLook {
            slug: "polls",
            label: || tr!("sub_cat_polls_predictions"),
            color: |palette| palette.warning,
        },
        SubActionCategory::Globals => CategoryLook {
            slug: "globals",
            label: || tr!("sub_cat_globals"),
            color: |palette| palette.warning,
        },
        SubActionCategory::Logic => CategoryLook {
            slug: "logic",
            label: || tr!("sub_cat_logic"),
            color: |palette| palette.text_muted,
        },
        SubActionCategory::Delay => CategoryLook {
            slug: "delay",
            label: || tr!("sub_cat_delay"),
            color: |palette| palette.text_muted,
        },
        SubActionCategory::Scripts => CategoryLook {
            slug: "scripts",
            label: || tr!("sub_cat_scripts"),
            color: |palette| palette.warning,
        },
        SubActionCategory::Files => CategoryLook {
            slug: "files",
            label: || tr!("sub_cat_files"),
            color: |palette| palette.danger,
        },
        SubActionCategory::Audio => CategoryLook {
            slug: "audio",
            label: || tr!("sub_cat_audio"),
            color: |palette| palette.success,
        },
        SubActionCategory::Tts => CategoryLook {
            slug: "tts",
            label: || tr!("sub_cat_tts"),
            color: |palette| palette.success,
        },
        SubActionCategory::Http => CategoryLook {
            slug: "http",
            label: || tr!("sub_cat_http"),
            color: |palette| palette.info,
        },
        SubActionCategory::Server => CategoryLook {
            slug: "server",
            label: || tr!("sub_cat_server"),
            color: |palette| palette.info,
        },
        SubActionCategory::Overlay => CategoryLook {
            slug: "overlay",
            label: || tr!("sub_cat_overlay"),
            color: |palette| palette.info,
        },
        SubActionCategory::Util | SubActionCategory::Integration => CategoryLook {
            slug: "util",
            label: || tr!("sub_cat_util"),
            color: |palette| palette.text_muted,
        },
    }
}

#[derive(Clone)]
pub(in crate::automation::actions_screen) struct StepGroup {
    category: SubActionCategory,
    owner: Option<IntegrationDeclaration>,
}

impl StepGroup {
    pub(in crate::automation::actions_screen) fn of(
        registry: &SubActionRegistry,
        runner: &dyn SubActionRunner,
    ) -> Self {
        let category = runner.category();
        let owner = (category == SubActionCategory::Integration)
            .then(|| registry.owning_integration(runner.id()))
            .flatten()
            .and_then(declaration_of);
        Self { category, owner }
    }

    pub(in crate::automation::actions_screen) fn is_files(&self) -> bool {
        self.category == SubActionCategory::Files
    }

    pub(super) fn scope(&self) -> String {
        match &self.owner {
            Some(owner) => owner.id.to_string(),
            None => category_look(self.category).slug.to_owned(),
        }
    }

    pub(super) fn label(&self) -> String {
        match &self.owner {
            Some(owner) => catalog_text(owner.catalog_group.label),
            None => (category_look(self.category).label)(),
        }
    }

    pub(in crate::automation::actions_screen) fn color(&self, palette: &ForgePalette) -> Rgba {
        match &self.owner {
            Some(owner) => tint_color(owner.catalog_group.step_tint, palette),
            None => (category_look(self.category).color)(palette),
        }
    }
}

pub(in crate::automation::actions_screen) fn step_glyph(
    kind_id: &str,
    fallback_icon: &str,
    fallback_color: Option<Rgba>,
    palette: &ForgePalette,
) -> (Icon, Rgba) {
    let (name, color) = match kind_id {
        FILE_READ_KIND_ID => ("file-text", palette.info),
        RANDOM_INT_KIND_ID => ("dice", palette.danger),
        SCRIPT_RUN_NAMED_KIND => ("code", palette.success),
        GLOBAL_INCREMENT_KIND_ID | GLOBAL_SET_KIND_ID => ("variable", palette.warning),
        CHAT_SEND_KIND_ID => ("send", palette.theme_brand),
        _ => {
            return (
                Icon::from_name(fallback_icon),
                fallback_color.unwrap_or(palette.text_secondary),
            );
        }
    };
    (Icon::from_name(name), color)
}

pub(crate) fn parse_variable_segments(s: &str) -> Vec<(&str, bool)> {
    forge_types::TemplatePieces::new(s)
        .map(|piece| match piece {
            forge_types::TemplatePiece::Literal(text) => (text, false),
            forge_types::TemplatePiece::Reference { raw, .. } => (raw, true),
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum VariableSegment<'a> {
    Text(&'a str),
    Variable(&'a str),
    FormattedVariable { name: &'a str, pattern: &'a str },
}

pub(super) fn variable_segments(s: &str) -> Vec<VariableSegment<'_>> {
    forge_types::TemplatePieces::new(s)
        .map(|piece| match piece {
            forge_types::TemplatePiece::Literal(text) => VariableSegment::Text(text),
            forge_types::TemplatePiece::Reference {
                name,
                format: Some(pattern),
                ..
            } => VariableSegment::FormattedVariable { name, pattern },
            forge_types::TemplatePiece::Reference { raw, .. } => VariableSegment::Variable(raw),
        })
        .collect()
}

fn push_highlighted(
    text: &mut String,
    highlights: &mut Vec<(Range<usize>, HighlightStyle)>,
    part: &str,
    color: Rgba,
) {
    let start = text.len();
    text.push_str(part);
    highlights.push((
        start..text.len(),
        HighlightStyle {
            color: Some(Hsla::from(color)),
            ..HighlightStyle::default()
        },
    ));
}

pub(super) fn variable_text(s: &str, palette: &ForgePalette, cx: &App) -> AnyElement {
    let mut text = String::with_capacity(s.len());
    let mut highlights = Vec::new();
    for segment in variable_segments(s) {
        match segment {
            VariableSegment::Text(part) => text.push_str(part),
            VariableSegment::Variable(raw) => {
                push_highlighted(&mut text, &mut highlights, raw, palette.warning)
            }
            VariableSegment::FormattedVariable { name, pattern } => {
                push_highlighted(
                    &mut text,
                    &mut highlights,
                    &format!("%{name}"),
                    palette.warning,
                );
                push_highlighted(
                    &mut text,
                    &mut highlights,
                    &format!(":{pattern}"),
                    variant_kind_color(VariantKind::Datetime, palette),
                );
                push_highlighted(&mut text, &mut highlights, "%", palette.warning);
            }
        }
    }
    div()
        .w_full()
        .min_w_0()
        .font_family(cx.mono_family())
        .text_size(FONT_XS)
        .text_color(palette.text_muted)
        .child(StyledText::new(SharedString::from(text)).with_highlights(highlights))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use forge_storage::Language;

    use super::*;

    const CLIP_OPTIONS: &str = "clips";
    const AIRHORN_ID: &str = "01JAAAAAAAAAAAAAAAAAAAAAAA";
    const BELL_ID: &str = "01JBBBBBBBBBBBBBBBBBBBBBBB";
    const DANGLING_ID: &str = "01JZZZZZZZZZZZZZZZZZZZZZZZ";
    const MISSING_LABEL: &str = "Not found";

    fn clip_options(clips: &[(&str, &str)]) -> HashMap<String, Vec<ChoiceOption>> {
        HashMap::from([(
            CLIP_OPTIONS.to_owned(),
            clips
                .iter()
                .map(|(id, name)| ChoiceOption::new(*id, *name))
                .collect(),
        )])
    }

    fn card_detail(clip_id: &str, options: &HashMap<String, Vec<ChoiceOption>>) -> String {
        shown_card_detail(
            StepCardDetail::OptionLabel {
                options_key: CLIP_OPTIONS,
                value: clip_id.to_owned(),
            },
            options,
        )
    }

    #[test]
    fn an_option_label_detail_resolves_the_value_against_the_loaded_options() {
        crate::app::i18n::install_language(Language::En);
        let options = clip_options(&[(AIRHORN_ID, "Airhorn"), (BELL_ID, "Bell - file missing")]);

        for (clip_id, expected) in [
            (AIRHORN_ID, "Airhorn"),
            (BELL_ID, "Bell - file missing"),
            (DANGLING_ID, MISSING_LABEL),
            ("", ""),
            ("%clip%", "%clip%"),
            ("prefix %x%", "prefix %x%"),
        ] {
            assert_eq!(card_detail(clip_id, &options), expected, "{clip_id:?}");
        }
    }

    #[test]
    fn an_option_label_detail_stays_blank_until_the_options_load() {
        crate::app::i18n::install_language(Language::En);

        assert_eq!(card_detail(AIRHORN_ID, &HashMap::new()), "");
    }

    #[test]
    fn an_option_label_detail_flags_a_value_when_the_list_is_empty() {
        crate::app::i18n::install_language(Language::En);

        assert_eq!(card_detail(AIRHORN_ID, &clip_options(&[])), MISSING_LABEL);
    }

    #[test]
    fn variable_segments_split_text_plain_references_and_formatted_references() {
        use VariableSegment::{FormattedVariable, Text, Variable};

        for (template, expected) in [
            ("", vec![]),
            ("a %x% b", vec![Text("a "), Variable("%x%"), Text(" b")]),
            (
                "%at:HH:mm%",
                vec![FormattedVariable {
                    name: "at",
                    pattern: "HH:mm",
                }],
            ),
            (
                "%at:%",
                vec![FormattedVariable {
                    name: "at",
                    pattern: "",
                }],
            ),
            (
                "%bad name:HH%",
                vec![Text("%"), Text("bad name:HH"), Text("%")],
            ),
            ("100%", vec![Text("100"), Text("%")]),
            ("50% off", vec![Text("50"), Text("% off")]),
            ("%a%%b%", vec![Variable("%a%"), Variable("%b%")]),
            (
                "%user% at %at:d MMMM%!",
                vec![
                    Variable("%user%"),
                    Text(" at "),
                    FormattedVariable {
                        name: "at",
                        pattern: "d MMMM",
                    },
                    Text("!"),
                ],
            ),
        ] {
            assert_eq!(variable_segments(template), expected, "{template:?}");
        }
    }

    #[test]
    fn push_highlighted_ranges_are_byte_offsets_that_land_on_char_boundaries() {
        let warning = gpui::rgb(0x00ff_aa00);
        let datetime = gpui::rgb(0x0000_aaff);
        let mut text = "Привіт 👋 ".to_owned();
        let mut highlights = Vec::new();

        push_highlighted(&mut text, &mut highlights, "%час", warning);
        push_highlighted(&mut text, &mut highlights, ":HH:mm", datetime);
        push_highlighted(&mut text, &mut highlights, "%", warning);

        assert_eq!(text, "Привіт 👋 %час:HH:mm%");
        let ranges: Vec<Range<usize>> = highlights.iter().map(|(range, _)| range.clone()).collect();
        assert_eq!(ranges, vec![18..25, 25..31, 31..32]);
        for range in &ranges {
            assert!(text.is_char_boundary(range.start) && text.is_char_boundary(range.end));
        }
        assert_eq!(&text[ranges[0].clone()], "%час");
        let colors: Vec<Option<Hsla>> = highlights.iter().map(|(_, style)| style.color).collect();
        assert_eq!(
            colors,
            vec![
                Some(Hsla::from(warning)),
                Some(Hsla::from(datetime)),
                Some(Hsla::from(warning)),
            ]
        );
    }
}
