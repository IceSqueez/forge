use std::time::Duration;

use forge_components::{FONT_XXS, ForgePalette, Icon, icon, mono_family, tr};
use forge_overlay::motion::{
    DEFAULT_ENTRANCE_MS, ENTRANCE, ENTRANCE_MS, EXIT, EXIT_MS, INTENSITY, MOTION_MS_MAX,
    MOTION_MS_MIN, NO_MOTION, TEXT_EFFECT, TEXT_STAGGER_CUSTOM, TEXT_STAGGER_MS,
    TEXT_STAGGER_MS_MAX, TEXT_STAGGER_MS_MIN, TEXT_UNIT, TEXT_UNIT_WORD,
};
use forge_overlay::{
    OverlayConfig, OverlayKindDescriptor, ShowTiming, effective_overlay_config, show_timing,
};
use forge_storage::OverlayDefinition;
use forge_types::Variant;
use gpui::{AnyElement, ClickEvent, Context, Pixels, div, prelude::*, px};

use super::OverlaysView;
use super::preview_stage::HINT_GLYPH;
use crate::motion_labels::preset_label;

const LINE_TOP: Pixels = px(8.0);
const LINE_GAP: Pixels = px(12.0);
const PART_GAP: Pixels = px(5.0);
const REPLAY_GAP: Pixels = px(3.0);
const MILLIS_PER_SEC: u128 = 1_000;
const TENTHS_PER_SEC: u128 = 10;
const MILLIS_PER_TENTH: u128 = MILLIS_PER_SEC / TENTHS_PER_SEC;
const HALF_TENTH_MILLIS: u128 = MILLIS_PER_TENTH / 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TimedPreset {
    pub(super) preset: String,
    pub(super) duration: Option<Duration>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TextUnit {
    Letter,
    Word,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TextPreset {
    pub(super) preset: String,
    pub(super) unit: Option<TextUnit>,
    pub(super) stagger: Option<Duration>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MotionTimeline {
    pub(super) entrance: Option<TimedPreset>,
    pub(super) text: Option<TextPreset>,
    pub(super) on_screen: Option<Duration>,
    pub(super) exit: Option<TimedPreset>,
    pub(super) total: Option<Duration>,
}

fn preset_of(config: &OverlayConfig, key: &str) -> String {
    match config.get(key).and_then(Variant::as_str) {
        Some(preset) if !preset.is_empty() => preset.to_owned(),
        _ => NO_MOTION.to_owned(),
    }
}

fn bounded_millis(
    config: &OverlayConfig,
    key: &str,
    fallback: i64,
    min: i64,
    max: i64,
) -> Duration {
    let millis = config
        .get(key)
        .and_then(Variant::as_int)
        .unwrap_or(fallback)
        .clamp(min, max);
    Duration::from_millis(millis.unsigned_abs())
}

fn entrance_of(config: &OverlayConfig) -> TimedPreset {
    let preset = preset_of(config, ENTRANCE);
    let duration = (preset != NO_MOTION).then(|| {
        bounded_millis(
            config,
            ENTRANCE_MS,
            DEFAULT_ENTRANCE_MS,
            MOTION_MS_MIN,
            MOTION_MS_MAX,
        )
    });
    TimedPreset { preset, duration }
}

fn text_of(config: &OverlayConfig) -> TextPreset {
    let preset = preset_of(config, TEXT_EFFECT);
    if preset == NO_MOTION {
        return TextPreset {
            preset,
            unit: None,
            stagger: None,
        };
    }
    let unit = match config.get(TEXT_UNIT).and_then(Variant::as_str) {
        Some(TEXT_UNIT_WORD) => TextUnit::Word,
        _ => TextUnit::Letter,
    };
    let custom = config
        .get(TEXT_STAGGER_CUSTOM)
        .and_then(Variant::as_bool)
        .unwrap_or(false);
    let stagger = custom.then(|| {
        bounded_millis(
            config,
            TEXT_STAGGER_MS,
            TEXT_STAGGER_MS_MIN,
            TEXT_STAGGER_MS_MIN,
            TEXT_STAGGER_MS_MAX,
        )
    });
    TextPreset {
        preset,
        unit: Some(unit),
        stagger,
    }
}

fn exit_of(config: &OverlayConfig, timing: ShowTiming) -> TimedPreset {
    let preset = preset_of(config, EXIT);
    let duration = (!timing.exit_tail.is_zero()).then_some(timing.exit_tail);
    TimedPreset { preset, duration }
}

pub(super) fn motion_timeline(
    descriptor: &dyn OverlayKindDescriptor,
    stored: &OverlayConfig,
) -> Option<MotionTimeline> {
    let axes = descriptor.motion().axes;
    if !axes.any() {
        return None;
    }
    let effective = effective_overlay_config(descriptor, stored);
    let timing = show_timing(descriptor, stored, None);
    Some(MotionTimeline {
        entrance: axes.entrance.then(|| entrance_of(&effective)),
        text: axes.text_effect.then(|| text_of(&effective)),
        on_screen: timing.map(|timing| timing.window),
        exit: timing
            .filter(|_| axes.exit)
            .map(|timing| exit_of(&effective, timing)),
        total: timing.map(ShowTiming::total),
    })
}

pub(super) fn span_text(span: Duration) -> String {
    let millis = span.as_millis();
    if millis < MILLIS_PER_SEC {
        let value = millis.to_string();
        return tr!("overlays_motion_ms", value = value.as_str());
    }
    let rounded_tenths = (millis + HALF_TENTH_MILLIS) / MILLIS_PER_TENTH;
    let whole = rounded_tenths / TENTHS_PER_SEC;
    let tenths = rounded_tenths % TENTHS_PER_SEC;
    let value = if tenths == 0 {
        whole.to_string()
    } else {
        format!("{whole}.{tenths}")
    };
    tr!("overlays_motion_secs", value = value.as_str())
}

fn timed_text(key: &str, timed: &TimedPreset) -> String {
    let label = preset_label(key, &timed.preset);
    match timed.duration {
        Some(duration) => format!("{label} {}", span_text(duration)),
        None => label,
    }
}

fn unit_text(unit: TextUnit) -> String {
    match unit {
        TextUnit::Letter => tr!("overlays_motion_unit_letter"),
        TextUnit::Word => tr!("overlays_motion_unit_word"),
    }
}

fn text_preset_text(text: &TextPreset) -> String {
    let Some(unit) = text.unit else {
        return preset_label(TEXT_EFFECT, &text.preset);
    };
    let unit = unit_text(unit);
    let effect = preset_label(TEXT_EFFECT, &text.preset);
    match text.stagger {
        Some(stagger) => {
            let stagger = span_text(stagger);
            tr!(
                "overlays_motion_text_staggered",
                effect = effect.as_str(),
                unit = unit.as_str(),
                stagger = stagger.as_str()
            )
        }
        None => tr!(
            "overlays_motion_text_by",
            effect = effect.as_str(),
            unit = unit.as_str()
        ),
    }
}

impl MotionTimeline {
    pub(super) fn parts(&self) -> Vec<(String, String)> {
        let mut parts = Vec::new();
        if let Some(entrance) = &self.entrance {
            parts.push((
                tr!("overlays_motion_entrance"),
                timed_text(ENTRANCE, entrance),
            ));
        }
        if let Some(text) = &self.text {
            parts.push((tr!("overlays_motion_text"), text_preset_text(text)));
        }
        if let Some(on_screen) = self.on_screen {
            parts.push((tr!("overlays_motion_on_screen"), span_text(on_screen)));
        }
        if let Some(exit) = &self.exit {
            parts.push((tr!("overlays_motion_exit"), timed_text(EXIT, exit)));
        }
        if let Some(total) = self.total {
            parts.push((tr!("overlays_motion_total"), span_text(total)));
        }
        parts
    }
}

pub(super) fn motion_label(key: &str) -> Option<String> {
    match key {
        ENTRANCE => Some(tr!("overlays_motion_field_entrance")),
        ENTRANCE_MS => Some(tr!("overlays_motion_field_entrance_ms")),
        TEXT_EFFECT => Some(tr!("overlays_motion_field_text_effect")),
        TEXT_UNIT => Some(tr!("overlays_motion_field_text_unit")),
        TEXT_STAGGER_CUSTOM => Some(tr!("overlays_motion_field_stagger_custom")),
        TEXT_STAGGER_MS => Some(tr!("overlays_motion_field_stagger")),
        EXIT => Some(tr!("overlays_motion_field_exit")),
        EXIT_MS => Some(tr!("overlays_motion_field_exit_ms")),
        INTENSITY => Some(tr!("overlays_motion_field_intensity")),
        _ => None,
    }
}

pub(super) fn motion_hint(key: &str) -> Option<String> {
    match key {
        TEXT_EFFECT => Some(tr!("overlays_motion_field_text_effect_hint")),
        EXIT => Some(tr!("overlays_motion_field_exit_hint")),
        _ => None,
    }
}

impl OverlaysView {
    pub(super) fn render_motion_timing(
        &self,
        definition: &OverlayDefinition,
        served: bool,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let descriptor = self.kinds.get(&definition.kind_id)?;
        let timeline = motion_timeline(descriptor, &definition.config)?;

        let mut line = div()
            .flex_none()
            .w_full()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(LINE_GAP)
            .pt(LINE_TOP)
            .font_family(mono_family())
            .text_size(FONT_XXS)
            .child(icon(Icon::Clock, HINT_GLYPH, palette.text_faint));
        for (label, value) in timeline.parts() {
            line = line.child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(PART_GAP)
                    .child(
                        div()
                            .text_color(palette.text_faint)
                            .child(label.to_uppercase()),
                    )
                    .child(div().text_color(palette.text_secondary).child(value)),
            );
        }

        let replay = (served && descriptor.has_visual_page()).then(|| {
            div()
                .id("overlays-motion-replay")
                .flex_none()
                .flex()
                .items_center()
                .gap(REPLAY_GAP)
                .cursor_pointer()
                .text_color(palette.brand)
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.open_preview_page(cx)))
                .child(icon(Icon::Repeat, HINT_GLYPH, palette.brand))
                .child(tr!("overlays_motion_replay"))
        });

        Some(
            line.child(div().flex_1().min_w(px(0.0)))
                .children(replay)
                .into_any_element(),
        )
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use forge_overlay::config::DURATION;
    use forge_overlay::kinds::alert::AlertOverlayKind;
    use forge_overlay::kinds::blank::BlankOverlayKind;
    use forge_overlay::kinds::frame::FrameOverlayKind;
    use forge_overlay::kinds::goal::GoalOverlayKind;

    use super::*;

    fn config(entries: &[(&str, Variant)]) -> OverlayConfig {
        entries
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect()
    }

    fn text(value: &str) -> Variant {
        Variant::String(value.to_owned())
    }

    fn millis(value: u64) -> Option<Duration> {
        Some(Duration::from_millis(value))
    }

    #[test]
    fn timeline_spans_match_the_overlay_crates_show_timing_for_each_config() {
        for (stored, window, tail) in [
            (
                config(&[
                    (DURATION, Variant::Int(7)),
                    (EXIT, text("fade")),
                    (EXIT_MS, Variant::Int(800)),
                ]),
                7_000,
                800,
            ),
            (
                config(&[(DURATION, Variant::Int(3)), (EXIT, text(NO_MOTION))]),
                3_000,
                0,
            ),
            (
                config(&[
                    (EXIT, text("pop")),
                    (EXIT_MS, Variant::Int(MOTION_MS_MIN - 1)),
                ]),
                5_000,
                MOTION_MS_MIN.unsigned_abs(),
            ),
            (
                config(&[
                    (EXIT, text("pop")),
                    (EXIT_MS, Variant::Int(MOTION_MS_MAX + 1)),
                ]),
                5_000,
                MOTION_MS_MAX.unsigned_abs(),
            ),
        ] {
            let timeline = motion_timeline(&AlertOverlayKind, &stored).expect("alert has motion");
            let oracle = show_timing(&AlertOverlayKind, &stored, None).expect("alert is transient");

            assert_eq!(timeline.on_screen, Some(oracle.window), "{stored:?}");
            assert_eq!(timeline.total, Some(oracle.total()), "{stored:?}");
            assert_eq!(timeline.on_screen, millis(window), "{stored:?}");
            assert_eq!(timeline.total, millis(window + tail), "{stored:?}");
            assert_eq!(
                timeline.exit.and_then(|exit| exit.duration),
                (tail > 0).then(|| Duration::from_millis(tail)),
                "{stored:?}"
            );
        }
    }

    #[test]
    fn replacing_kinds_have_no_on_screen_exit_or_total_spans() {
        let timeline =
            motion_timeline(&GoalOverlayKind, &OverlayConfig::new()).expect("goal has motion");

        assert_eq!(timeline.on_screen, None);
        assert_eq!(timeline.exit, None);
        assert_eq!(timeline.total, None);
        assert!(timeline.entrance.is_some());
        assert!(timeline.text.is_some());
    }

    #[test]
    fn axes_a_kind_lacks_are_absent_and_a_still_kind_has_no_timeline() {
        let frame =
            motion_timeline(&FrameOverlayKind, &OverlayConfig::new()).expect("frame has motion");

        assert!(frame.entrance.is_some());
        assert_eq!(frame.text, None);
        assert!(motion_timeline(&BlankOverlayKind, &OverlayConfig::new()).is_none());
    }

    #[test]
    fn entrance_duration_is_hidden_for_no_motion_and_clamped_otherwise() {
        let duration_for = |stored: OverlayConfig| {
            motion_timeline(&AlertOverlayKind, &stored)
                .and_then(|timeline| timeline.entrance)
                .and_then(|entrance| entrance.duration)
        };

        assert_eq!(duration_for(config(&[(ENTRANCE, text(NO_MOTION))])), None);
        assert_eq!(
            duration_for(config(&[
                (ENTRANCE, text("pop")),
                (ENTRANCE_MS, Variant::Int(1))
            ])),
            millis(MOTION_MS_MIN.unsigned_abs())
        );
        assert_eq!(
            duration_for(config(&[
                (ENTRANCE, text("pop")),
                (ENTRANCE_MS, Variant::Int(MOTION_MS_MAX + 1))
            ])),
            millis(MOTION_MS_MAX.unsigned_abs())
        );
        assert_eq!(
            duration_for(config(&[(ENTRANCE, text("pop"))])),
            millis(DEFAULT_ENTRANCE_MS.unsigned_abs())
        );
    }

    #[test]
    fn text_effect_reports_unit_and_stagger_only_when_an_effect_is_chosen_and_stagger_is_custom() {
        let text_for = |stored: OverlayConfig| {
            motion_timeline(&GoalOverlayKind, &stored)
                .and_then(|timeline| timeline.text)
                .expect("goal has a text axis")
        };

        let off = text_for(config(&[
            (TEXT_EFFECT, text(NO_MOTION)),
            (TEXT_UNIT, text(TEXT_UNIT_WORD)),
        ]));
        assert_eq!((off.unit, off.stagger), (None, None));

        let by_word = text_for(config(&[
            (TEXT_EFFECT, text("wave")),
            (TEXT_UNIT, text(TEXT_UNIT_WORD)),
        ]));
        assert_eq!(
            (by_word.unit, by_word.stagger),
            (Some(TextUnit::Word), None)
        );

        let too_fast = text_for(config(&[
            (TEXT_EFFECT, text("wave")),
            (TEXT_STAGGER_CUSTOM, Variant::Bool(true)),
            (TEXT_STAGGER_MS, Variant::Int(TEXT_STAGGER_MS_MIN - 1)),
        ]));
        assert_eq!(
            (too_fast.unit, too_fast.stagger),
            (
                Some(TextUnit::Letter),
                millis(TEXT_STAGGER_MS_MIN.unsigned_abs())
            )
        );

        let too_slow = text_for(config(&[
            (TEXT_EFFECT, text("wave")),
            (TEXT_STAGGER_CUSTOM, Variant::Bool(true)),
            (TEXT_STAGGER_MS, Variant::Int(TEXT_STAGGER_MS_MAX + 1)),
        ]));
        assert_eq!(too_slow.stagger, millis(TEXT_STAGGER_MS_MAX.unsigned_abs()));
    }

    fn plain(rendered: String) -> String {
        rendered.replace(['\u{2068}', '\u{2069}'], "")
    }

    fn english() {
        crate::i18n::install_language(forge_storage::Language::En);
    }

    #[test]
    fn spans_under_a_second_read_in_ms_and_longer_ones_in_seconds_with_a_tenth_only_when_nonzero() {
        english();
        for (span, expected) in [
            (0, "0 ms"),
            (999, "999 ms"),
            (1_000, "1 s"),
            (1_200, "1.2 s"),
            (7_800, "7.8 s"),
            (12_000, "12 s"),
        ] {
            assert_eq!(plain(span_text(Duration::from_millis(span))), expected);
        }
    }

    #[test]
    fn rendered_line_lists_each_phase_in_order_with_the_total_as_window_plus_tail() {
        english();
        let stored = config(&[
            (DURATION, Variant::Int(7)),
            (ENTRANCE, text("slide-up")),
            (ENTRANCE_MS, Variant::Int(600)),
            (TEXT_EFFECT, text("wave")),
            (TEXT_UNIT, text(TEXT_UNIT_WORD)),
            (EXIT, text("dust")),
            (EXIT_MS, Variant::Int(800)),
        ]);

        let parts: Vec<(String, String)> = motion_timeline(&AlertOverlayKind, &stored)
            .expect("alert has motion")
            .parts()
            .into_iter()
            .map(|(label, value)| (label, plain(value)))
            .collect();

        assert_eq!(
            parts,
            vec![
                ("Entrance".to_owned(), "Slide up 600 ms".to_owned()),
                ("Text".to_owned(), "Wave by word".to_owned()),
                ("On screen".to_owned(), "7 s".to_owned()),
                ("Exit".to_owned(), "Dust 800 ms".to_owned()),
                ("Total show".to_owned(), "7.8 s".to_owned()),
            ]
        );
    }
}
