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

const LINE_TOP: Pixels = px(8.0);
const LINE_GAP: Pixels = px(12.0);
const PART_GAP: Pixels = px(5.0);
const REPLAY_GAP: Pixels = px(3.0);
const MILLIS_PER_SEC: u128 = 1_000;
const TENTHS_PER_SEC: u128 = 10;
const MILLIS_PER_TENTH: u128 = MILLIS_PER_SEC / TENTHS_PER_SEC;

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
    let whole = millis / MILLIS_PER_SEC;
    let tenths = (millis % MILLIS_PER_SEC) / MILLIS_PER_TENTH;
    let value = if tenths == 0 {
        whole.to_string()
    } else {
        format!("{whole}.{tenths}")
    };
    tr!("overlays_motion_secs", value = value.as_str())
}

fn timed_text(timed: &TimedPreset) -> String {
    match timed.duration {
        Some(duration) => format!("{} {}", timed.preset, span_text(duration)),
        None => timed.preset.clone(),
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
        return text.preset.clone();
    };
    let unit = unit_text(unit);
    match text.stagger {
        Some(stagger) => {
            let stagger = span_text(stagger);
            tr!(
                "overlays_motion_text_staggered",
                effect = text.preset.as_str(),
                unit = unit.as_str(),
                stagger = stagger.as_str()
            )
        }
        None => tr!(
            "overlays_motion_text_by",
            effect = text.preset.as_str(),
            unit = unit.as_str()
        ),
    }
}

impl MotionTimeline {
    pub(super) fn parts(&self) -> Vec<(String, String)> {
        let mut parts = Vec::new();
        if let Some(entrance) = &self.entrance {
            parts.push((tr!("overlays_motion_entrance"), timed_text(entrance)));
        }
        if let Some(text) = &self.text {
            parts.push((tr!("overlays_motion_text"), text_preset_text(text)));
        }
        if let Some(on_screen) = self.on_screen {
            parts.push((tr!("overlays_motion_on_screen"), span_text(on_screen)));
        }
        if let Some(exit) = &self.exit {
            parts.push((tr!("overlays_motion_exit"), timed_text(exit)));
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
