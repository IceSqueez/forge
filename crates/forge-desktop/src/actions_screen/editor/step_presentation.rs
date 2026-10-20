use super::select_options::CLIP_OPTIONS_KEY;
use super::*;
use crate::globals::variant_kind_color;
use crate::platform_visuals::platform_color;
use forge_components::{FONT_XS, ForgePalette, Icon, mono_family, tr};
use forge_registry::SubActionCategory;
use forge_types::{PlatformId, SubActionStep, Variant, VariantKind};
use gpui::{AnyElement, HighlightStyle, Hsla, Rgba, SharedString, StyledText, div};
use std::collections::HashMap;
use std::ops::Range;

fn clip_step_detail(
    clip_id: &str,
    select_options: &HashMap<String, Vec<(String, String)>>,
) -> String {
    let is_templated = forge_types::variable_references(clip_id).next().is_some();
    if clip_id.is_empty() || is_templated {
        return clip_id.to_owned();
    }
    let Some(clips) = select_options.get(CLIP_OPTIONS_KEY) else {
        return String::new();
    };
    clips
        .iter()
        .find(|(id, _)| id == clip_id)
        .map(|(_, name)| name.clone())
        .unwrap_or_else(|| tr!("action_editor_clip_missing"))
}

pub(in crate::actions_screen) fn sub_action_summary(
    step: &SubActionStep,
    select_options: &HashMap<String, Vec<(String, String)>>,
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
        "twitch.chat.send_message" => {
            let target = step.config.get("target").map(as_str).unwrap_or("twitch");
            let message = step.config.get("message").map(as_str).unwrap_or("");
            (
                "send",
                tr!("action_editor_kind_send_chat"),
                Some(format!("\u{2192} {target}: \"{message}\"")),
            )
        }
        "core.globals.set" => {
            let name = step.config.get("name").map(as_str).unwrap_or("");
            let value = step.config.get("value").map(as_str).unwrap_or("");
            (
                "variable",
                tr!("action_editor_kind_set_global"),
                Some(format!("{name} = \"{value}\"")),
            )
        }
        "core.globals.increment" => {
            let name = step.config.get("name").map(as_str).unwrap_or("");
            let amount = step.config.get("amount").map(as_i64).unwrap_or(1);
            (
                "variable",
                tr!("action_editor_kind_incr_global"),
                Some(format!(
                    "{name} += {amount} {note}",
                    note = tr!("action_editor_persisted_note")
                )),
            )
        }
        "core.logic.wait" => {
            let ms = step.config.get("ms").map(as_i64).unwrap_or(0);
            (
                "clock",
                tr!("action_editor_kind_delay"),
                Some(format!("{ms} ms")),
            )
        }
        "core.log.write" => {
            let level = step.config.get("level").map(as_str).unwrap_or("info");
            let message = step.config.get("message").map(as_str).unwrap_or("");
            (
                "info-circle",
                tr!("action_editor_kind_log"),
                Some(format!("[{level}] \"{message}\"")),
            )
        }
        "script.run.named" => {
            let script_name = step.config.get("script_name").map(as_str).unwrap_or("");
            let target = step.config.get("target_var").map(as_str).unwrap_or("");
            let detail = if target.is_empty() {
                script_name.to_owned()
            } else {
                format!("{script_name} \u{2192} {}", wrap_var(target))
            };
            ("script", tr!("action_editor_kind_run_script"), Some(detail))
        }
        "soundboard.sound.play" => {
            let clip_id = step.config.get("clip_id").map(as_str).unwrap_or("");
            (
                "music",
                tr!("action_editor_kind_play_sound"),
                Some(clip_step_detail(clip_id, select_options)),
            )
        }
        "tts.speak.text" => {
            let text = step.config.get("text").map(as_str).unwrap_or("");
            (
                "volume",
                tr!("action_editor_kind_speak"),
                Some(text.to_owned()),
            )
        }
        "core.file.read" => {
            let path = step.config.get("path").map(as_str).unwrap_or("");
            let var = step.config.get("target_var").map(as_str).unwrap_or("");
            (
                "file",
                tr!("action_editor_kind_read_file"),
                Some(format!("{path} \u{2192} {}", wrap_var(var))),
            )
        }
        "core.random.int" => {
            let min = step.config.get("min").map(as_i64).unwrap_or(0);
            let max = step.config.get("max").map(as_i64).unwrap_or(0);
            let var = step.config.get("target_var").map(as_str).unwrap_or("");
            (
                "dice",
                tr!("action_editor_kind_random_int"),
                Some(format!("[{min}..{max}] \u{2192} {}", wrap_var(var))),
            )
        }
        _ => ("bolt", tr!("action_editor_kind_sub_action"), None),
    }
}

pub(super) fn sub_category_label(cat: SubActionCategory) -> String {
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
        SubActionCategory::Twitch => "Twitch".to_owned(),
        SubActionCategory::YouTube => "YouTube".to_owned(),
        SubActionCategory::Kick => "Kick".to_owned(),
        SubActionCategory::Obs => "OBS".to_owned(),
        SubActionCategory::VTube => "VTube Studio".to_owned(),
        SubActionCategory::Discord => "Discord".to_owned(),
        SubActionCategory::Midi => "MIDI".to_owned(),
        SubActionCategory::Hotkey => tr!("sub_cat_hotkey"),
        SubActionCategory::Audio => tr!("sub_cat_audio"),
        SubActionCategory::Tts => tr!("sub_cat_tts"),
        SubActionCategory::Http => tr!("sub_cat_http"),
        SubActionCategory::Server => tr!("sub_cat_server"),
        SubActionCategory::Overlay => tr!("sub_cat_overlay"),
        SubActionCategory::Util => tr!("sub_cat_util"),
    }
}

pub(super) fn sub_category_slug(cat: SubActionCategory) -> &'static str {
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
        SubActionCategory::Twitch => "twitch",
        SubActionCategory::YouTube => "youtube",
        SubActionCategory::Kick => "kick",
        SubActionCategory::Obs => "obs",
        SubActionCategory::VTube => "vtube",
        SubActionCategory::Discord => "discord",
        SubActionCategory::Midi => "midi",
        SubActionCategory::Hotkey => "hotkey",
        SubActionCategory::Audio => "audio",
        SubActionCategory::Tts => "tts",
        SubActionCategory::Http => "http",
        SubActionCategory::Server => "server",
        SubActionCategory::Overlay => "overlay",
        SubActionCategory::Util => "util",
    }
}

pub(in crate::actions_screen) fn sub_category_color(
    cat: SubActionCategory,
    palette: &ForgePalette,
) -> Rgba {
    match cat {
        SubActionCategory::Chat | SubActionCategory::Twitch => palette.brand,
        SubActionCategory::Tts | SubActionCategory::Audio => palette.success,
        SubActionCategory::Globals => palette.warning,
        SubActionCategory::Files => palette.random,
        SubActionCategory::YouTube => platform_color(PlatformId::YouTube, palette),
        SubActionCategory::Kick => platform_color(PlatformId::Kick, palette),
        SubActionCategory::Obs => palette.text_secondary,
        SubActionCategory::VTube => palette.accent_teal,
        SubActionCategory::Discord
        | SubActionCategory::Http
        | SubActionCategory::Server
        | SubActionCategory::Overlay => palette.info,
        SubActionCategory::Midi | SubActionCategory::Moderation => palette.random,
        SubActionCategory::ChannelPoints => palette.accent_pink_light,
        SubActionCategory::Hotkey
        | SubActionCategory::PollsPredictions
        | SubActionCategory::Scripts => palette.warning,
        SubActionCategory::Logic | SubActionCategory::Delay | SubActionCategory::Util => {
            palette.text_muted
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
        "core.file.read" => ("file-text", palette.info),
        "core.random.int" => ("dice", palette.random),
        "script.run.named" => ("code", palette.success),
        "core.globals.increment" | "core.globals.set" => ("variable", palette.warning),
        "twitch.chat.send_message" => ("send", palette.brand),
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

pub(super) fn variable_text(s: &str, palette: &ForgePalette) -> AnyElement {
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
        .font_family(mono_family())
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

    fn clip_options(clips: &[(&str, &str)]) -> HashMap<String, Vec<(String, String)>> {
        HashMap::from([(
            CLIP_OPTIONS_KEY.to_owned(),
            clips
                .iter()
                .map(|(id, name)| ((*id).to_owned(), (*name).to_owned()))
                .collect(),
        )])
    }

    fn card_detail(clip_id: &str, options: &HashMap<String, Vec<(String, String)>>) -> String {
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
        let ranges: Vec<Range<usize>> = highlights.iter().map(|(r, _)| r.clone()).collect();
        assert_eq!(ranges, vec![18..25, 25..31, 31..32]);
        for range in &ranges {
            assert!(text.is_char_boundary(range.start) && text.is_char_boundary(range.end));
        }
        assert_eq!(&text[ranges[0].clone()], "%час");
        let colors: Vec<Option<Hsla>> = highlights.iter().map(|(_, s)| s.color).collect();
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
