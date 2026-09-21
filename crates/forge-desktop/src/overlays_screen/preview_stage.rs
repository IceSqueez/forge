use std::time::Duration;

use forge_components::{
    BORDER_THIN, ForgePalette, Icon, body_family, empty_state, ghost_button_with_icon, icon,
    mono_family, section_label, segment, segmented, tr,
};
use forge_overlay::config::{DURATION, DURATION_MAX_SECS, DURATION_MIN_SECS};
use forge_overlay::{
    OverlayConfig, PreviewCanvas, PreviewComposition, PreviewPosition, effective_overlay_config,
    preview_page_url,
};
use forge_runtime::TestFire;
use forge_storage::{OverlayDefinition, OverlayId};
use forge_types::Variant;
use gpui::{
    AnyElement, Bounds, ClickEvent, Context, Pixels, Rgba, Size, Window, canvas, div, fill, point,
    prelude::*, px, size,
};

use crate::async_bridge::{self, ErrorSink};

use super::OverlaysView;
use super::preview_shapes::{
    Scale, body_padding, centers_horizontally, fills_canvas, render_composition,
};

const REGION_PAD: Pixels = px(20.0);
const HEAD_GAP: Pixels = px(10.0);

const STAGE_MIN_H: Pixels = px(300.0);
const STAGE_RADIUS: Pixels = px(10.0);
const CANVAS_NOTE_TOP: Pixels = px(8.0);
const CANVAS_NOTE_LEFT: Pixels = px(10.0);
const CANVAS_NOTE_FS: Pixels = px(9.0);
const CANVAS_NOTE_OPACITY: f32 = 0.6;
const CHECKER_CELL: Pixels = px(11.0);

const HINT_TOP: Pixels = px(8.0);
pub(super) const HINT_GAP: Pixels = px(6.0);
pub(super) const HINT_FS: Pixels = px(10.5);
pub(super) const HINT_GLYPH: Pixels = px(12.0);

const ZOOM_FACTOR: f32 = 1.0;

const UNTIMED_DECAY: Duration = Duration::from_secs(5);

enum TestFirePhase {
    Sending,
    Landed {
        content: OverlayConfig,
        delivered: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeliveryHint {
    Sending,
    Delivered,
    NoBrowserSource,
    Undelivered,
}

fn delivery_hint(phase: &TestFirePhase, server_running: bool) -> DeliveryHint {
    match phase {
        TestFirePhase::Sending => DeliveryHint::Sending,
        TestFirePhase::Landed {
            delivered: true, ..
        } => DeliveryHint::Delivered,
        TestFirePhase::Landed {
            delivered: false, ..
        } if server_running => DeliveryHint::NoBrowserSource,
        TestFirePhase::Landed {
            delivered: false, ..
        } => DeliveryHint::Undelivered,
    }
}

pub(super) struct TestFireRun {
    overlay: OverlayId,
    phase: TestFirePhase,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub(super) enum PreviewScale {
    #[default]
    Zoom,
    True,
}

struct StagePreview {
    overlay: OverlayId,
    composition: PreviewComposition,
}

#[derive(Default)]
pub(super) struct StageState {
    scale: PreviewScale,
    area: Option<Size<Pixels>>,
    preview: Option<StagePreview>,
}

impl StageState {
    fn composition_for(&self, id: &OverlayId) -> Option<&PreviewComposition> {
        self.preview
            .as_ref()
            .filter(|preview| &preview.overlay == id)
            .map(|preview| &preview.composition)
    }
}

#[derive(Clone, Copy)]
struct CanvasFit {
    width: Pixels,
    height: Pixels,
    scale: f32,
}

fn fit_canvas(area: Size<Pixels>, canvas: PreviewCanvas) -> Option<CanvasFit> {
    let available_width = f32::from(area.width);
    let available_height = f32::from(area.height);
    if available_width <= 0.0 || available_height <= 0.0 {
        return None;
    }

    let canvas_width = canvas.width as f32;
    let canvas_height = canvas.height as f32;
    if canvas_width <= 0.0 || canvas_height <= 0.0 {
        return None;
    }

    let scale = (available_width / canvas_width).min(available_height / canvas_height);
    Some(CanvasFit {
        width: px(canvas_width * scale),
        height: px(canvas_height * scale),
        scale,
    })
}

impl OverlaysView {
    pub(super) fn clear_test(&mut self) {
        self.fire_epoch = self.fire_epoch.wrapping_add(1);
        self.fire = None;
        self.sync_preview();
    }

    /// The composition only moves when the record, the selection or a landed sample does, so it is
    /// rebuilt at those edges rather than on every frame the stage draws.
    pub(super) fn sync_preview(&mut self) {
        let next = self.selected_definition().and_then(|definition| {
            Some(StagePreview {
                overlay: definition.id.clone(),
                composition: self.preview_composition(definition)?,
            })
        });
        self.stage.preview = next;
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
                        content: fired.content,
                        delivered: fired.delivered,
                    },
                });
                self.sync_preview();
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
        self.sync_preview();
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

    /// Idle renders the overlay's own wording; a landed test renders the content the page took.
    fn preview_composition(&self, definition: &OverlayDefinition) -> Option<PreviewComposition> {
        let descriptor = self.kinds.get(&definition.kind_id)?;
        let mut config = effective_overlay_config(descriptor, &definition.config);
        if let Some(content) = self.landed_content(&definition.id) {
            config.extend(content.iter().map(|(key, v)| (key.clone(), v.clone())));
        }
        Some(descriptor.preview(&config))
    }

    fn landed_content(&self, id: &OverlayId) -> Option<&OverlayConfig> {
        let run = self.fire.as_ref().filter(|run| &run.overlay == id)?;
        match &run.phase {
            TestFirePhase::Landed { content, .. } => Some(content),
            TestFirePhase::Sending => None,
        }
    }

    fn set_scale(&mut self, scale: PreviewScale, cx: &mut Context<Self>) {
        if self.stage.scale == scale {
            return;
        }
        self.stage.scale = scale;
        cx.notify();
    }

    /// Measured during layout, so an unchanged area must not repaint or the stage never settles.
    fn set_stage_area(&mut self, area: Size<Pixels>, cx: &mut Context<Self>) {
        if self.stage.area == Some(area) {
            return;
        }
        self.stage.area = Some(area);
        cx.notify();
    }

    fn preview_address(&self) -> Option<String> {
        let id = self.selected.as_ref()?;
        Some(preview_page_url(&self.overlay_url(id)?))
    }

    fn open_preview_page(&mut self, cx: &mut Context<Self>) {
        let Some(address) = self.preview_address() else {
            return;
        };
        async_bridge::open_external(
            &self.rt_handle,
            address,
            ErrorSink::Toast,
            tr!("overlays_preview_open_failed"),
            cx,
        );
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

        let Some(composition) = self.stage.composition_for(&definition.id) else {
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
        let served = self.preview_address().is_some();

        region
            .child(self.render_stage_head(composition.canvas, definition, served, palette, cx))
            .child(self.render_arena(composition, visuals.icon, palette, cx))
            .child(self.render_hints(definition, served, palette))
            .into_any_element()
    }

    fn render_stage_head(
        &self,
        canvas: PreviewCanvas,
        definition: &OverlayDefinition,
        served: bool,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let descriptor = self.kinds.get(&definition.kind_id);
        let page = descriptor.is_some_and(|d| d.has_visual_page());
        let test_fire_ok = descriptor.is_some_and(|d| !d.content_is_machine_filled());

        let size_label = page.then(|| {
            let width = canvas.width.to_string();
            let height = canvas.height.to_string();
            let label = tr!(
                "overlays_preview_label",
                width = width.as_str(),
                height = height.as_str()
            );
            div()
                .flex_none()
                .child(section_label(label.to_uppercase(), palette))
        });
        let scale_switch = page.then(|| self.render_scale_switch(palette, cx));
        let open_button = page.then(|| {
            ghost_button_with_icon(Icon::ExternalLink, tr!("overlays_preview_open"), palette)
                .disabled(!served)
                .on_click(
                    "overlays-open-preview",
                    cx.listener(|this, _: &ClickEvent, _, cx| this.open_preview_page(cx)),
                )
        });

        div()
            .flex_none()
            .w_full()
            .flex()
            .items_center()
            .gap(HEAD_GAP)
            .pb(HEAD_GAP)
            .children(size_label)
            .children(scale_switch)
            .child(div().flex_1().min_w(px(0.0)))
            .children(open_button)
            .child(
                ghost_button_with_icon(Icon::PlayerPlay, tr!("overlays_test_send"), palette)
                    .ink(palette.brand)
                    .disabled(self.is_sending() || !test_fire_ok)
                    .on_click(
                        "overlays-send-test",
                        cx.listener(|this, _: &ClickEvent, _, cx| this.send_test(cx)),
                    ),
            )
            .into_any_element()
    }

    fn render_scale_switch(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let zoom = cx.listener(|this, _: &ClickEvent, _, cx| {
            this.set_scale(PreviewScale::Zoom, cx);
        });
        let truthful = cx.listener(|this, _: &ClickEvent, _, cx| {
            this.set_scale(PreviewScale::True, cx);
        });

        div()
            .flex_none()
            .child(
                segmented(
                    vec![
                        segment(
                            "overlays-scale-true",
                            tr!("overlays_preview_scale_true"),
                            self.stage.scale == PreviewScale::True,
                            truthful,
                        ),
                        segment(
                            "overlays-scale-zoom",
                            tr!("overlays_preview_scale_zoom"),
                            self.stage.scale == PreviewScale::Zoom,
                            zoom,
                        ),
                    ],
                    palette,
                )
                .subtle(palette),
            )
            .into_any_element()
    }

    fn render_arena(
        &self,
        composition: &PreviewComposition,
        badge: Icon,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity().downgrade();
        let arena = div()
            .flex_1()
            .min_h(STAGE_MIN_H)
            .w_full()
            .relative()
            .flex()
            .items_center()
            .justify_center()
            .child(
                canvas(
                    move |bounds: Bounds<Pixels>, _window, cx| {
                        let _ = view.update(cx, |this, cx| this.set_stage_area(bounds.size, cx));
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            );

        let Some(fit) = self
            .stage
            .area
            .and_then(|area| fit_canvas(area, composition.canvas))
        else {
            return arena.into_any_element();
        };

        arena
            .child(render_canvas(
                composition,
                fit,
                self.stage.scale,
                badge,
                palette,
            ))
            .into_any_element()
    }

    fn render_hints(
        &self,
        definition: &OverlayDefinition,
        served: bool,
        palette: &ForgePalette,
    ) -> AnyElement {
        let test_fire_ok = self
            .kinds
            .get(&definition.kind_id)
            .is_some_and(|d| !d.content_is_machine_filled());
        let stopped = (!served).then(|| {
            hint_row(
                Icon::AlertTriangle,
                palette.text_faint,
                tr!("overlays_url_not_served"),
                palette.text_faint,
            )
        });
        let test_unavailable = (!test_fire_ok).then(|| {
            hint_row(
                Icon::InfoCircle,
                palette.text_faint,
                tr!("overlays_test_unavailable"),
                palette.text_faint,
            )
        });

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
            .child(hint_row(
                Icon::Browser,
                palette.text_faint,
                tr!("overlays_preview_browser_counts"),
                palette.text_faint,
            ))
            .children(stopped)
            .children(test_unavailable)
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

        let (glyph, tint, message) = match delivery_hint(&run.phase, self.server_running) {
            DeliveryHint::Sending => (
                Icon::PlayerPlay,
                palette.text_muted,
                tr!("overlays_test_sending"),
            ),
            DeliveryHint::Delivered => (
                Icon::CircleCheck,
                palette.success,
                tr!("overlays_test_delivered"),
            ),
            DeliveryHint::NoBrowserSource => (
                Icon::AlertTriangle,
                palette.warning,
                tr!("overlays_test_no_browser_source"),
            ),
            DeliveryHint::Undelivered => (
                Icon::AlertTriangle,
                palette.warning,
                tr!("overlays_test_undelivered"),
            ),
        };

        Some(hint_row(glyph, tint, message, tint).into_any_element())
    }
}

fn render_canvas(
    composition: &PreviewComposition,
    fit: CanvasFit,
    mode: PreviewScale,
    badge: Icon,
    palette: &ForgePalette,
) -> AnyElement {
    let scale = Scale::new(match mode {
        PreviewScale::Zoom => ZOOM_FACTOR,
        PreviewScale::True => fit.scale,
    });
    let shape = composition.shape;
    let tint = palette.base;

    let mut stage = div()
        .flex_none()
        .w(fit.width)
        .h(fit.height)
        .relative()
        .overflow_hidden()
        .rounded(STAGE_RADIUS)
        .border(BORDER_THIN)
        .border_color(palette.border_regular)
        .bg(palette.shell)
        .flex()
        .flex_row()
        .child(
            canvas(
                |_, _, _| {},
                move |bounds: Bounds<Pixels>, _prepaint, window, _cx| {
                    paint_checkerboard(bounds, tint, window);
                },
            )
            .absolute()
            .size_full(),
        )
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
        );

    if !fills_canvas(shape) {
        stage = match composition.position {
            PreviewPosition::Top => stage.items_start(),
            PreviewPosition::Center => stage.items_center(),
            PreviewPosition::Bottom => stage.items_end(),
        };
        stage = if centers_horizontally(shape) {
            stage.justify_center()
        } else {
            stage.justify_start()
        };
        if let Some(padding) = body_padding(shape) {
            stage = stage.p(scale.at(padding));
        }
    }

    stage
        .child(render_composition(composition, scale, badge, palette))
        .into_any_element()
}

fn paint_checkerboard(bounds: Bounds<Pixels>, tint: Rgba, window: &mut Window) {
    let cell = f32::from(CHECKER_CELL);
    let width = f32::from(bounds.size.width);
    let height = f32::from(bounds.size.height);
    if cell <= 0.0 || width <= 0.0 || height <= 0.0 {
        return;
    }

    let columns = (width / cell).ceil() as usize;
    let rows = (height / cell).ceil() as usize;
    let left = f32::from(bounds.origin.x);
    let top = f32::from(bounds.origin.y);

    for row in 0..rows {
        for column in 0..columns {
            if (row + column).is_multiple_of(2) {
                continue;
            }
            let offset_x = column as f32 * cell;
            let offset_y = row as f32 * cell;
            let square = Bounds {
                origin: point(px(left + offset_x), px(top + offset_y)),
                size: size(
                    px(cell.min(width - offset_x)),
                    px(cell.min(height - offset_y)),
                ),
            };
            window.paint_quad(fill(square, tint));
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn landed(delivered: bool) -> TestFirePhase {
        TestFirePhase::Landed {
            content: OverlayConfig::new(),
            delivered,
        }
    }

    #[test]
    fn a_fire_in_flight_or_already_delivered_reads_the_same_whether_the_server_runs() {
        for (phase, expected) in [
            (TestFirePhase::Sending, DeliveryHint::Sending),
            (landed(true), DeliveryHint::Delivered),
        ] {
            for server_running in [false, true] {
                assert_eq!(
                    delivery_hint(&phase, server_running),
                    expected,
                    "server running: {server_running}"
                );
            }
        }
    }

    #[test]
    fn an_undelivered_landing_blames_the_browser_source_only_while_the_server_runs() {
        assert_eq!(
            delivery_hint(&landed(false), true),
            DeliveryHint::NoBrowserSource
        );
        assert_eq!(
            delivery_hint(&landed(false), false),
            DeliveryHint::Undelivered
        );
    }
}
