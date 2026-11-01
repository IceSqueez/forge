use super::select_options::CLIP_OPTIONS_KEY;
use super::*;
use crate::globals::variant_kind_color;
use crate::integration_catalog::{catalog_text, tint_color};
use crate::option_choices::ChoiceOption;
use forge_components::{ActivePresentation, FONT_XS, ForgePalette, Icon, tr};
use forge_host::integration_catalog::declaration_of;
use forge_platform_core::IntegrationDeclaration;
use forge_registry::{SubActionCategory, SubActionRegistry, SubActionRunner};
use forge_runtime::{
    CHAT_SEND_DEFAULT_TARGET, CHAT_SEND_KIND_ID, CHAT_SEND_MESSAGE_KEY, CHAT_SEND_TARGET_KEY,
    FILE_READ_KIND_ID, FILE_READ_PATH_KEY, FILE_READ_TARGET_VAR_KEY, GLOBAL_AMOUNT_KEY,
    GLOBAL_DEFAULT_AMOUNT, GLOBAL_INCREMENT_KIND_ID, GLOBAL_NAME_KEY, GLOBAL_SET_KIND_ID,
    GLOBAL_VALUE_KEY, LOG_DEFAULT_LEVEL, LOG_LEVEL_KEY, LOG_MESSAGE_KEY, LOG_WRITE_KIND_ID,
    RANDOM_INT_KIND_ID, RANDOM_INT_MAX_KEY, RANDOM_INT_MIN_KEY, RANDOM_INT_TARGET_VAR_KEY,
    SCRIPT_TARGET_VAR_KEY, WAIT_KIND_ID, WAIT_MS_KEY,
};
use forge_runtime::{SPEAK_KIND_ID, SPEECH_TEXT_KEY};
use forge_soundboard::vocabulary::config_keys::CLIP_ID;
use forge_soundboard::vocabulary::sub_action_ids::SOUND_PLAY;
use forge_storage::{SCRIPT_NAME_KEY, SCRIPT_RUN_NAMED_KIND};
use forge_types::{SubActionStep, Variant, VariantKind};
use gpui::{AnyElement, App, HighlightStyle, Hsla, Rgba, SharedString, StyledText, div};
use std::collections::HashMap;
use std::ops::Range;

fn clip_step_detail(clip_id: &str, select_options: &HashMap<String, Vec<ChoiceOption>>) -> String {
    let is_templated = forge_types::variable_references(clip_id).next().is_some();
    if clip_id.is_empty() || is_templated {
        return clip_id.to_owned();
    }
    let Some(clips) = select_options.get(CLIP_OPTIONS_KEY) else {
        return String::new();
    };
    clips
        .iter()
        .find(|option| option.value == clip_id)
        .map(|option| option.label.clone())
        .unwrap_or_else(|| tr!("action_editor_clip_missing"))
}

pub(in crate::actions_screen) fn sub_action_summary(
    step: &SubActionStep,
    select_options: &HashMap<String, Vec<ChoiceOption>>,
) -> (&'static str, String, Option<String>) {
    fn as_str(v: &Variant) -> &str {
        if let Variant::String(s) = v {
            s.as_str()
        } else {
            ""
        }
    }
    fn as_i64(v: &Variant) -> i64 {
        if let Variant::Int(n) = v { *n } else { 0 }
    }
    fn wrap_var(name: &str) -> String {
        format!("%{}%", name.trim_matches('%'))
    }
    match step.kind_id.as_str() {
        CHAT_SEND_KIND_ID => {
            let target = step
                .config
                .get(CHAT_SEND_TARGET_KEY)
                .map(as_str)
                .unwrap_or(CHAT_SEND_DEFAULT_TARGET);
            let message = step
                .config
                .get(CHAT_SEND_MESSAGE_KEY)
                .map(as_str)
                .unwrap_or("");
            (
                "send",
                tr!("action_editor_kind_send_chat"),
                Some(format!("\u{2192} {target}: \"{message}\"")),
            )
        }
        GLOBAL_SET_KIND_ID => {
            let name = step.config.get(GLOBAL_NAME_KEY).map(as_str).unwrap_or("");
            let value = step.config.get(GLOBAL_VALUE_KEY).map(as_str).unwrap_or("");
            (
                "variable",
                tr!("action_editor_kind_set_global"),
                Some(format!("{name} = \"{value}\"")),
            )
        }
        GLOBAL_INCREMENT_KIND_ID => {
            let name = step.config.get(GLOBAL_NAME_KEY).map(as_str).unwrap_or("");
            let amount = step
                .config
                .get(GLOBAL_AMOUNT_KEY)
                .map(as_i64)
                .unwrap_or(GLOBAL_DEFAULT_AMOUNT);
            (
                "variable",
                tr!("action_editor_kind_incr_global"),
                Some(tr!(
                    "action_editor_global_increment_detail",
                    name = name,
                    amount = amount
                )),
            )
        }
        WAIT_KIND_ID => {
            let ms = step.config.get(WAIT_MS_KEY).map(as_i64).unwrap_or(0);
            (
                "clock",
                tr!("action_editor_kind_delay"),
                Some(tr!("action_editor_step_wait_ms", ms = ms)),
            )
        }
        LOG_WRITE_KIND_ID => {
            let level = step
                .config
                .get(LOG_LEVEL_KEY)
                .map(as_str)
                .unwrap_or(LOG_DEFAULT_LEVEL);
            let message = step.config.get(LOG_MESSAGE_KEY).map(as_str).unwrap_or("");
            (
                "info-circle",
                tr!("action_editor_kind_log"),
                Some(format!("[{level}] \"{message}\"")),
            )
        }
        SCRIPT_RUN_NAMED_KIND => {
            let script_name = step.config.get(SCRIPT_NAME_KEY).map(as_str).unwrap_or("");
            let target = step
                .config
                .get(SCRIPT_TARGET_VAR_KEY)
                .map(as_str)
                .unwrap_or("");
            let detail = if target.is_empty() {
                script_name.to_owned()
            } else {
                format!("{script_name} \u{2192} {}", wrap_var(target))
            };
            ("script", tr!("action_editor_kind_run_script"), Some(detail))
        }
        SOUND_PLAY => {
            let clip_id = step.config.get(CLIP_ID).map(as_str).unwrap_or("");
            (
                "music",
                tr!("action_editor_kind_play_sound"),
                Some(clip_step_detail(clip_id, select_options)),
            )
        }
        SPEAK_KIND_ID => {
            let text = step.config.get(SPEECH_TEXT_KEY).map(as_str).unwrap_or("");
            (
                "volume",
                tr!("action_editor_kind_speak"),
                Some(text.to_owned()),
            )
        }
        FILE_READ_KIND_ID => {
            let path = step
                .config
                .get(FILE_READ_PATH_KEY)
                .map(as_str)
                .unwrap_or("");
            let var = step
                .config
                .get(FILE_READ_TARGET_VAR_KEY)
                .map(as_str)
                .unwrap_or("");
            (
                "file",
                tr!("action_editor_kind_read_file"),
                Some(format!("{path} \u{2192} {}", wrap_var(var))),
            )
        }
        RANDOM_INT_KIND_ID => {
            let bound = |key: &str| {
                step.config
                    .get(key)
                    .map(crate::config_text::editable_line)
                    .unwrap_or_default()
            };
            let (min, max) = (bound(RANDOM_INT_MIN_KEY), bound(RANDOM_INT_MAX_KEY));
            let var = step
                .config
                .get(RANDOM_INT_TARGET_VAR_KEY)
                .map(as_str)
                .unwrap_or("");
            (
                "dice",
                tr!("action_editor_kind_random_int"),
                Some(format!("[{min}..{max}] \u{2192} {}", wrap_var(var))),
            )
        }
        _ => ("bolt", tr!("action_editor_kind_sub_action"), None),
    }
}

fn sub_category_label(cat: SubActionCategory) -> String {
    match cat {
        SubActionCategory::Chat => tr!("sub_cat_chat"),
        SubActionCategory::Moderation => tr!("sub_cat_moderation"),
        SubActionCategory::ChannelPoints => tr!("sub_cat_channel_points"),
        SubActionCategory::PollsPredictions => tr!("sub_cat_polls_predictions"),
        SubActionCategory::Globals => tr!("sub_cat_globals"),
        SubActionCategory::Logic => tr!("sub_cat_logic"),
        SubActionCategory::Delay => tr!("sub_cat_delay"),
        SubActionCategory::Scripts => tr!("sub_cat_scripts"),
        SubActionCategory::Files => tr!("sub_cat_files"),
        SubActionCategory::Audio => tr!("sub_cat_audio"),
        SubActionCategory::Tts => tr!("sub_cat_tts"),
        SubActionCategory::Http => tr!("sub_cat_http"),
        SubActionCategory::Server => tr!("sub_cat_server"),
        SubActionCategory::Overlay => tr!("sub_cat_overlay"),
        SubActionCategory::Util | SubActionCategory::Integration => tr!("sub_cat_util"),
    }
}

fn sub_category_slug(cat: SubActionCategory) -> &'static str {
    match cat {
        SubActionCategory::Chat => "chat",
        SubActionCategory::Moderation => "moderation",
        SubActionCategory::ChannelPoints => "channel-points",
        SubActionCategory::PollsPredictions => "polls",
        SubActionCategory::Globals => "globals",
        SubActionCategory::Logic => "logic",
        SubActionCategory::Delay => "delay",
        SubActionCategory::Scripts => "scripts",
        SubActionCategory::Files => "files",
        SubActionCategory::Audio => "audio",
        SubActionCategory::Tts => "tts",
        SubActionCategory::Http => "http",
        SubActionCategory::Server => "server",
        SubActionCategory::Overlay => "overlay",
        SubActionCategory::Util | SubActionCategory::Integration => "util",
    }
}

fn sub_category_color(cat: SubActionCategory, palette: &ForgePalette) -> Rgba {
    match cat {
        SubActionCategory::Chat => palette.theme_brand,
        SubActionCategory::Tts | SubActionCategory::Audio => palette.success,
        SubActionCategory::Globals => palette.warning,
        SubActionCategory::Files => palette.danger,
        SubActionCategory::Http | SubActionCategory::Server | SubActionCategory::Overlay => {
            palette.info
        }
        SubActionCategory::Moderation => palette.danger,
        SubActionCategory::ChannelPoints => palette.accent_pink_light,
        SubActionCategory::PollsPredictions | SubActionCategory::Scripts => palette.warning,
        SubActionCategory::Logic
        | SubActionCategory::Delay
        | SubActionCategory::Util
        | SubActionCategory::Integration => palette.text_muted,
    }
}

#[derive(Clone)]
pub(in crate::actions_screen) struct StepGroup {
    category: SubActionCategory,
    owner: Option<IntegrationDeclaration>,
}

impl StepGroup {
    pub(in crate::actions_screen) fn of(
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

    pub(in crate::actions_screen) fn is_files(&self) -> bool {
        self.category == SubActionCategory::Files
    }

    pub(super) fn scope(&self) -> String {
        match &self.owner {
            Some(owner) => owner.id.to_string(),
            None => sub_category_slug(self.category).to_owned(),
        }
    }

    pub(super) fn label(&self) -> String {
        match &self.owner {
            Some(owner) => catalog_text(owner.catalog_group.label),
            None => sub_category_label(self.category),
        }
    }

    pub(in crate::actions_screen) fn color(&self, palette: &ForgePalette) -> Rgba {
        match &self.owner {
            Some(owner) => tint_color(owner.catalog_group.step_tint, palette),
            None => sub_category_color(self.category, palette),
        }
    }
}

pub(in crate::actions_screen) fn step_glyph(
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

    const PLAY_SOUND: &str = "soundboard.sound.play";
    const AIRHORN_ID: &str = "01JAAAAAAAAAAAAAAAAAAAAAAA";
    const BELL_ID: &str = "01JBBBBBBBBBBBBBBBBBBBBBBB";
    const DANGLING_ID: &str = "01JZZZZZZZZZZZZZZZZZZZZZZZ";
    const MISSING_LABEL: &str = "Missing clip";

    fn play_step(clip_id: &str) -> SubActionStep {
        let mut step = crate::test_support::step(PLAY_SOUND, true);
        step.config
            .insert("clip_id".to_owned(), Variant::String(clip_id.to_owned()));
        step
    }

    fn clip_options(clips: &[(&str, &str)]) -> HashMap<String, Vec<ChoiceOption>> {
        HashMap::from([(
            CLIP_OPTIONS_KEY.to_owned(),
            clips
                .iter()
                .map(|(id, name)| ChoiceOption::new(*id, *name))
                .collect(),
        )])
    }

    fn card_detail(clip_id: &str, options: &HashMap<String, Vec<ChoiceOption>>) -> String {
        sub_action_summary(&play_step(clip_id), options)
            .2
            .unwrap_or_default()
    }

    #[test]
    fn play_sound_card_detail_resolves_the_clip_id_against_the_loaded_clip_list() {
        crate::i18n::install_language(Language::En);
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
    fn play_sound_card_detail_stays_blank_until_the_clip_list_loads() {
        crate::i18n::install_language(Language::En);

        assert_eq!(card_detail(AIRHORN_ID, &HashMap::new()), "");
    }

    #[test]
    fn play_sound_card_detail_flags_a_clip_id_when_the_library_is_empty() {
        crate::i18n::install_language(Language::En);

        assert_eq!(card_detail(AIRHORN_ID, &clip_options(&[])), MISSING_LABEL);
    }

    fn random_int_detail(min: Option<Variant>, max: Option<Variant>) -> String {
        let mut step = crate::test_support::step(RANDOM_INT_KIND_ID, true);
        for (key, bound) in [(RANDOM_INT_MIN_KEY, min), (RANDOM_INT_MAX_KEY, max)] {
            if let Some(bound) = bound {
                step.config.insert(key.to_owned(), bound);
            }
        }
        step.config.insert(
            RANDOM_INT_TARGET_VAR_KEY.to_owned(),
            Variant::String("roll".to_owned()),
        );
        sub_action_summary(&step, &HashMap::new())
            .2
            .unwrap_or_default()
    }

    #[test]
    fn random_int_card_shows_the_bounds_as_the_editor_stored_them() {
        crate::i18n::install_language(Language::En);
        let text = |s: &str| Some(Variant::String(s.to_owned()));

        for (min, max, expected) in [
            (text("1"), text("100"), "[1..100] \u{2192} %roll%"),
            (
                Some(Variant::Int(1)),
                Some(Variant::Int(100)),
                "[1..100] \u{2192} %roll%",
            ),
            (text("%low%"), text("-5"), "[%low%..-5] \u{2192} %roll%"),
            (None, None, "[..] \u{2192} %roll%"),
        ] {
            assert_eq!(
                random_int_detail(min.clone(), max.clone()),
                expected,
                "{min:?}..{max:?}"
            );
        }
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
