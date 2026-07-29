use std::time::Duration;

use forge_components::{
    BORDER_THIN, ForgePalette, Icon, body_family, empty_state, ghost_button_with_icon, icon,
    mono_family, section_label, tr, with_alpha,
};
use forge_overlay::config::{DURATION, DURATION_MAX_SECS, DURATION_MIN_SECS, HEADLINE, SUBLINE};
use forge_overlay::{
    OverlayConfig, PreviewComposition, PreviewFont, PreviewLineRole, PreviewPosition, PreviewShape,
    effective_overlay_config,
};
use forge_runtime::TestFire;
use forge_storage::{OverlayDefinition, OverlayId};
use forge_types::{ArgStack, Variant};
use gpui::{
    AnyElement, ClickEvent, Context, FontWeight, Pixels, Rgba, SharedString, div, prelude::*, px,
    relative,
};

use crate::async_bridge;

use super::OverlaysView;
use super::kind_visuals::accent_color;

const REGION_PAD: Pixels = px(20.0);
const HEAD_GAP: Pixels = px(10.0);

const STAGE_MIN_H: Pixels = px(300.0);
const STAGE_RADIUS: Pixels = px(10.0);
const STAGE_PAD: Pixels = px(24.0);
const CANVAS_NOTE_TOP: Pixels = px(8.0);
const CANVAS_NOTE_LEFT: Pixels = px(10.0);
const CANVAS_NOTE_FS: Pixels = px(9.0);
const CANVAS_NOTE_OPACITY: f32 = 0.6;

const HINT_TOP: Pixels = px(8.0);
const HINT_GAP: Pixels = px(6.0);
const HINT_FS: Pixels = px(10.5);
const HINT_GLYPH: Pixels = px(12.0);

const BANNER_GAP: Pixels = px(14.0);
const BANNER_PAD_V: Pixels = px(16.0);
const BANNER_PAD_H: Pixels = px(22.0);
const BANNER_RADIUS: Pixels = px(12.0);
const BADGE: Pixels = px(46.0);
const BADGE_RADIUS: Pixels = px(10.0);
const BADGE_GLYPH: Pixels = px(22.0);
const HEADLINE_FS: Pixels = px(20.0);
const SUBLINE_FS: Pixels = px(13.0);
const SURFACE_ALPHA: f32 = 0.86;

const FRAME_W: f32 = 0.7;
const FRAME_H: Pixels = px(180.0);
const FRAME_BORDER: Pixels = px(3.0);
const FRAME_RADIUS: Pixels = px(14.0);
const FRAME_LABEL_FS: Pixels = px(13.0);
const FRAME_LABEL_PAD_V: Pixels = px(3.0);
const FRAME_LABEL_PAD_H: Pixels = px(12.0);
const FRAME_LABEL_CORNER_TR: Pixels = px(10.0);
const FRAME_LABEL_CORNER_BL: Pixels = px(12.0);
const FRAME_WASH_ALPHA: f32 = 0.15;

const STRIP_PAD_V: Pixels = px(10.0);
const STRIP_PAD_H: Pixels = px(18.0);
const STRIP_RADIUS: Pixels = px(8.0);
const STRIP_GAP: Pixels = px(6.0);
const STRIP_FS: Pixels = px(16.0);
const STRIP_SUB_OPACITY: f32 = 0.7;

const PLACEHOLDER: &str = "-";
const LABEL_JOIN: &str = " - ";

const UNTIMED_DECAY: Duration = Duration::from_secs(5);

enum TestFirePhase {
    Sending,
    Landed { args: ArgStack, delivered: bool },
}

pub(super) struct TestFireRun {
    overlay: OverlayId,
    phase: TestFirePhase,
}

impl OverlaysView {
    pub(super) fn clear_test(&mut self) {
        self.fire_epoch = self.fire_epoch.wrapping_add(1);
        self.fire = None;
    }

    fn send_test(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.selected.clone() else {
            return;
        };
        if self.is_sending() {
            return;
        }

        self.fire_epoch = self.fire_epoch.wrapping_add(1);
        let epoch = self.fire_epoch;
        self.fire = Some(TestFireRun {
            overlay: id.clone(),
            phase: TestFirePhase::Sending,
        });

        let service = self.service.clone();
        let target = id.clone();
        async_bridge::run_async(
            &self.rt_handle,
            async move { service.test_fire(&target).await.map_err(|e| e.to_string()) },
            move |this, result: Result<TestFire, String>, cx| this.on_test_fired(epoch, result, cx),
            cx,
        );
        cx.notify();
    }

    /// A result whose epoch has been superseded belongs to a selection the user has already left.
    fn on_test_fired(
        &mut self,
        epoch: u64,
        result: Result<TestFire, String>,
        cx: &mut Context<Self>,
    ) {
        if self.fire_epoch != epoch {
            return;
        }
        let Some(run) = self.fire.take() else {
            return;
        };

        match result {
            Ok(fired) => {
                let overlay = run.overlay;
                self.start_decay(self.decay_delay(&overlay), epoch, cx);
                self.fire = Some(TestFireRun {
                    overlay,
                    phase: TestFirePhase::Landed {
                        args: args_of(&fired.payload),
                        delivered: fired.delivered,
                    },
                });
            }
            Err(message) => self.report(&message, cx),
        }
        cx.notify();
    }

    fn start_decay(&self, delay: Duration, epoch: u64, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(delay).await;
            let _ = this.update(cx, |this, cx| this.settle_test(epoch, cx));
        })
        .detach();
    }

    fn settle_test(&mut self, epoch: u64, cx: &mut Context<Self>) {
        if self.fire_epoch != epoch {
            return;
        }
        self.fire = None;
        cx.notify();
    }

    fn is_sending(&self) -> bool {
        matches!(
            self.fire.as_ref().map(|run| &run.phase),
            Some(TestFirePhase::Sending)
        )
    }

    fn decay_delay(&self, id: &OverlayId) -> Duration {
        let seconds = self
            .overlays
            .iter()
            .find(|item| &item.id == id)
            .and_then(|definition| self.effective_config(definition))
            .and_then(|config| config.get(DURATION).and_then(Variant::as_int));

        match seconds {
            Some(secs) => {
                Duration::from_secs(secs.clamp(DURATION_MIN_SECS, DURATION_MAX_SECS) as u64)
            }
            None => UNTIMED_DECAY,
        }
    }

    fn effective_config(&self, definition: &OverlayDefinition) -> Option<OverlayConfig> {
        let descriptor = self.kinds.get(&definition.kind_id)?;
        Some(effective_overlay_config(descriptor, &definition.config))
    }

    /// Idle renders the stored template verbatim, tokens and all; only a landed sample expands them.
    fn preview_composition(&self, definition: &OverlayDefinition) -> Option<PreviewComposition> {
        let descriptor = self.kinds.get(&definition.kind_id)?;
        let effective = effective_overlay_config(descriptor, &definition.config);
        let config = match self.landed_args(&definition.id) {
            Some(args) => expand_lines(&effective, args),
            None => effective,
        };
        Some(descriptor.preview(&config))
    }

    fn landed_args(&self, id: &OverlayId) -> Option<&ArgStack> {
        let run = self.fire.as_ref().filter(|run| &run.overlay == id)?;
        match &run.phase {
            TestFirePhase::Landed { args, .. } => Some(args),
            TestFirePhase::Sending => None,
        }
    }

    pub(super) fn render_design_stage(
        &self,
        definition: &OverlayDefinition,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let region = div()
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .p(REGION_PAD)
            .flex()
            .flex_col();

        let Some(composition) = self.preview_composition(definition) else {
            return region
                .items_center()
                .justify_center()
                .child(
                    empty_state(tr!("overlays_preview_unavailable"), palette)
                        .glyph(Icon::AlertTriangle),
                )
                .into_any_element();
        };

        let visuals = self.visuals(definition, palette);

        region
            .child(self.render_stage_head(palette, cx))
            .child(render_canvas(&composition, visuals.icon, palette))
            .child(self.render_hints(definition, palette))
            .into_any_element()
    }

    fn render_stage_head(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex_none()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .pb(HEAD_GAP)
            .child(section_label(
                tr!("overlays_preview_label").to_uppercase(),
                palette,
            ))
            .child(
                ghost_button_with_icon(Icon::PlayerPlay, tr!("overlays_test_send"), palette)
                    .ink(palette.brand)
                    .disabled(self.is_sending())
                    .on_click(
                        "overlays-send-test",
                        cx.listener(|this, _: &ClickEvent, _, cx| this.send_test(cx)),
                    ),
            )
            .into_any_element()
    }

    fn render_hints(&self, definition: &OverlayDefinition, palette: &ForgePalette) -> AnyElement {
        div()
            .flex_none()
            .w_full()
            .flex()
            .flex_col()
            .gap(HINT_GAP)
            .pt(HINT_TOP)
            .child(hint_row(
                Icon::InfoCircle,
                palette.text_faint,
                tr!("overlays_preview_approximate"),
                palette.text_faint,
            ))
            .children(self.render_delivery_hint(definition, palette))
            .into_any_element()
    }

    fn render_delivery_hint(
        &self,
        definition: &OverlayDefinition,
        palette: &ForgePalette,
    ) -> Option<AnyElement> {
        let run = self
            .fire
            .as_ref()
            .filter(|run| run.overlay == definition.id)?;

        let (glyph, tint, message) = match &run.phase {
            TestFirePhase::Sending => (
                Icon::PlayerPlay,
                palette.text_muted,
                tr!("overlays_test_sending"),
            ),
            TestFirePhase::Landed {
                delivered: true, ..
            } => (
                Icon::CircleCheck,
                palette.success,
                tr!("overlays_test_delivered"),
            ),
            TestFirePhase::Landed {
                delivered: false, ..
            } => (
                Icon::AlertTriangle,
                palette.warning,
                tr!("overlays_test_undelivered"),
            ),
        };

        Some(hint_row(glyph, tint, message, tint).into_any_element())
    }
}

/// Only top-level scalars: the shipped page expander reads own properties of the payload root, so a
/// nested entity has no token a template could name.
fn args_of(payload: &serde_json::Value) -> ArgStack {
    let mut stack = ArgStack::new();
    let Some(fields) = payload.as_object() else {
        return stack;
    };
    for (name, value) in fields {
        let Ok(variant) = Variant::from_json(value.clone()) else {
            continue;
        };
        if matches!(
            variant,
            Variant::Int(_) | Variant::Float(_) | Variant::Bool(_) | Variant::String(_)
        ) {
            stack = stack.set(name.clone(), variant);
        }
    }
    stack
}

fn expand_lines(config: &OverlayConfig, args: &ArgStack) -> OverlayConfig {
    let mut expanded = config.clone();
    for key in [HEADLINE, SUBLINE] {
        let Some(template) = expanded.get(key).and_then(Variant::as_str) else {
            continue;
        };
        let text = args.interpolate(template);
        expanded.insert(key.to_owned(), Variant::String(text));
    }
    expanded
}

fn line_text(composition: &PreviewComposition, role: PreviewLineRole) -> Option<SharedString> {
    composition
        .lines
        .iter()
        .find(|line| line.role == role)
        .map(|line| SharedString::from(line.text.clone()))
}

fn render_canvas(
    composition: &PreviewComposition,
    badge: Icon,
    palette: &ForgePalette,
) -> AnyElement {
    let canvas = div()
        .flex_1()
        .min_h(STAGE_MIN_H)
        .w_full()
        .relative()
        .overflow_hidden()
        .rounded(STAGE_RADIUS)
        .border(BORDER_THIN)
        .border_color(palette.border_regular)
        .bg(palette.shell)
        .flex()
        .flex_col()
        .items_center()
        .p(STAGE_PAD);

    let canvas = match composition.position {
        PreviewPosition::Top => canvas.justify_start(),
        PreviewPosition::Center => canvas.justify_center(),
        PreviewPosition::Bottom => canvas.justify_end(),
    };

    canvas
        .child(
            div()
                .absolute()
                .top(CANVAS_NOTE_TOP)
                .left(CANVAS_NOTE_LEFT)
                .font_family(mono_family())
                .text_size(CANVAS_NOTE_FS)
                .text_color(palette.text_faint)
                .opacity(CANVAS_NOTE_OPACITY)
                .child(tr!("overlays_preview_canvas_note")),
        )
        .child(render_composition(composition, badge, palette))
        .into_any_element()
}

fn render_composition(
    composition: &PreviewComposition,
    badge: Icon,
    palette: &ForgePalette,
) -> AnyElement {
    let accent = accent_color(composition.accent, palette);
    let family = match composition.font {
        PreviewFont::Sans => body_family(),
        PreviewFont::Mono => mono_family(),
    };

    match composition.shape {
        PreviewShape::BadgeBanner => {
            render_badge_banner(composition, accent, family, badge, palette)
        }
        PreviewShape::BorderedFrame => render_bordered_frame(composition, accent, family, palette),
        PreviewShape::Strip => render_strip(composition, accent, family, palette),
    }
}

fn render_badge_banner(
    composition: &PreviewComposition,
    accent: Rgba,
    family: SharedString,
    badge: Icon,
    palette: &ForgePalette,
) -> AnyElement {
    let mut lines = div().flex().flex_col().min_w(px(0.0));

    if let Some(text) = line_text(composition, PreviewLineRole::Headline) {
        lines = lines.child(headline_text(text, family.clone(), palette));
    }
    if let Some(text) = line_text(composition, PreviewLineRole::Subline) {
        lines = lines.child(
            div()
                .font_family(family.clone())
                .text_size(SUBLINE_FS)
                .text_color(accent)
                .child(text),
        );
    }
    if composition.lines.is_empty() {
        lines = lines.child(headline_text(PLACEHOLDER.into(), family, palette));
    }

    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(BANNER_GAP)
        .py(BANNER_PAD_V)
        .px(BANNER_PAD_H)
        .rounded(BANNER_RADIUS)
        .border(BORDER_THIN)
        .border_color(accent)
        .bg(with_alpha(palette.shell, SURFACE_ALPHA))
        .child(
            div()
                .flex_none()
                .size(BADGE)
                .rounded(BADGE_RADIUS)
                .bg(accent)
                .flex()
                .items_center()
                .justify_center()
                .child(icon(badge, BADGE_GLYPH, palette.shell)),
        )
        .child(lines)
        .into_any_element()
}

fn headline_text(
    text: SharedString,
    family: SharedString,
    palette: &ForgePalette,
) -> impl IntoElement {
    div()
        .font_family(family)
        .font_weight(FontWeight::BOLD)
        .text_size(HEADLINE_FS)
        .text_color(palette.text_primary)
        .child(text)
}

fn render_bordered_frame(
    composition: &PreviewComposition,
    accent: Rgba,
    family: SharedString,
    palette: &ForgePalette,
) -> AnyElement {
    let joined = composition
        .lines
        .iter()
        .map(|line| line.text.as_str())
        .collect::<Vec<_>>()
        .join(LABEL_JOIN);
    let label = if joined.is_empty() {
        PLACEHOLDER.to_owned()
    } else {
        joined
    };

    div()
        .flex_none()
        .w(relative(FRAME_W))
        .h(FRAME_H)
        .relative()
        .rounded(FRAME_RADIUS)
        .border(FRAME_BORDER)
        .border_color(accent)
        .bg(with_alpha(palette.base, FRAME_WASH_ALPHA))
        .child(
            div()
                .absolute()
                .bottom_0()
                .left_0()
                .max_w(relative(1.0))
                .truncate()
                .py(FRAME_LABEL_PAD_V)
                .px(FRAME_LABEL_PAD_H)
                .rounded_tr(FRAME_LABEL_CORNER_TR)
                .rounded_bl(FRAME_LABEL_CORNER_BL)
                .bg(accent)
                .font_family(family)
                .font_weight(FontWeight::BOLD)
                .text_size(FRAME_LABEL_FS)
                .text_color(palette.shell)
                .child(label),
        )
        .into_any_element()
}

fn render_strip(
    composition: &PreviewComposition,
    accent: Rgba,
    family: SharedString,
    palette: &ForgePalette,
) -> AnyElement {
    let headline = line_text(composition, PreviewLineRole::Headline)
        .unwrap_or_else(|| SharedString::from(PLACEHOLDER));
    let subline = line_text(composition, PreviewLineRole::Subline);

    div()
        .flex_none()
        .w_full()
        .flex()
        .items_center()
        .gap(STRIP_GAP)
        .py(STRIP_PAD_V)
        .px(STRIP_PAD_H)
        .rounded(STRIP_RADIUS)
        .bg(accent)
        .font_family(family)
        .font_weight(FontWeight::SEMIBOLD)
        .text_size(STRIP_FS)
        .text_color(palette.shell)
        .child(div().flex_none().truncate().child(headline))
        .children(subline.map(|text| {
            div()
                .flex_1()
                .min_w(px(0.0))
                .truncate()
                .opacity(STRIP_SUB_OPACITY)
                .child(text)
        }))
        .into_any_element()
}

fn hint_row(glyph: Icon, tint: Rgba, message: String, text_color: Rgba) -> impl IntoElement {
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(HINT_GAP)
        .child(icon(glyph, HINT_GLYPH, tint))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .font_family(body_family())
                .text_size(HINT_FS)
                .text_color(text_color)
                .child(message),
        )
}
