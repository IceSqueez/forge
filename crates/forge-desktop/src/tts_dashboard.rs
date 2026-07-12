use forge_components::{
    BORDER_THIN, ConfirmTone, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_SM, FONT_XS,
    ForgePalette, Icon, InputEvent, OverlayPosition, Radius, Spacing, TextInput, badge, card,
    confirm_modal, icon, overlay, radius, slider, spacing, status_dot,
};
use gpui::{
    AnyElement, ClickEvent, Context, Entity, Pixels, Rgba, Subscription, Window, div, prelude::*,
    px,
};

use crate::presentation::ActivePresentation;

/// Volume slider track width — the parity source pins it at a fixed 90px, off the
/// `Spacing` scale, so it is carried as a named literal.
const VOL_SLIDER_W: Pixels = px(90.0);
/// Test-speak input width (the source's fixed 180px).
const TEST_INPUT_W: Pixels = px(180.0);
/// Right session/engines rail width (the source's fixed 236px).
const RIGHT_PANE_W: Pixels = px(236.0);
/// Queue-position gutter width (the source's fixed 14px mono column).
const QUEUE_POS_W: Pixels = px(14.0);
/// Engine health-dot diameter (the source's fixed 7px dot).
const ENGINE_DOT: Pixels = px(7.0);
/// Pause/resume button leading-glyph size (the source's fixed 13px icon).
const PAUSE_GLYPH: Pixels = px(13.0);
/// Volume leading-glyph size (the source's fixed 14px icon).
const VOLUME_GLYPH: Pixels = px(14.0);
/// Initial session volume the dashboard seeds (72%), matching the design roster.
const SEED_VOLUME: f32 = 0.72;

/// The utterance the queue is currently voicing. A cached read of the speak
/// queue's now-playing slot; `forge-desktop` wires no speak queue yet, so it is
/// seeded static and refreshed over the runtime→UI bridge (a `SpeakEvent` topic)
/// once wired.
struct NowSpeaking {
    viewer_name: String,
    engine_voice: String,
    text: String,
    elapsed_secs: u32,
    total_secs: u32,
}

/// One pending utterance in the up-next queue. A cached view-model of a speak
/// request; the live queue is fed by the runtime bridge, never owned here.
struct QueueItem {
    viewer_name: String,
    engine_voice: String,
    text: String,
    duration_secs: u32,
    is_high_priority: bool,
    bits_amount: Option<u32>,
}

/// The session counters shown in the right rail. Runtime-fed once the bridge is
/// wired; seeded representative here.
struct SessionStats {
    spoken: u32,
    skipped: u32,
    filtered: u32,
    avg_latency_ms: Option<u32>,
}

/// One configured engine's health line in the right rail. `warn` inks the caution
/// hue (e.g. nearing a character quota); otherwise the ready hue.
struct EngineStatus {
    name: &'static str,
    meta: &'static str,
    warn: bool,
}

/// The TTS Dashboard section view-entity: a control strip (pause/skip/stop-all,
/// volume, test-speak), a now-speaking panel over the up-next queue, and a right
/// rail of session counters and engine health, plus a stop-all confirm overlay.
///
/// Owns its dashboard state as seeded stub state — `forge-desktop` wires no speak
/// queue yet, so the now-speaking slot, queue, session counters and engine roster
/// are seeded representative and the controls mutate this cached state with
/// feedback. The real screen drives pause/resume/skip/stop/volume/test-speak
/// through `forge-speak-queue`'s dispatch handle (`SpeakCommand::{Pause, Resume,
/// Skip, Clear, SetVolume, Enqueue}`) and reads the live now-playing slot, queue
/// and counters back over the runtime→UI bridge (a `SpeakEvent` topic).
pub struct TtsDashboardView {
    paused: bool,
    volume: f32,
    now_speaking: Option<NowSpeaking>,
    queue: Vec<QueueItem>,
    stats: SessionStats,
    engines: Vec<EngineStatus>,
    /// Two-phase stop-all gate: armed by the control strip's Stop button, rendered
    /// by the shared confirm overlay. `false` = no confirm showing.
    pending_stop_all: bool,
    test_input: Entity<TextInput>,
    _test_sub: Subscription,
}

impl TtsDashboardView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let palette = cx.palette();
        let test_input =
            cx.new(|cx| TextInput::new("Type to test a voice…", cx).with_palette(palette));
        let test_sub = cx.subscribe(
            &test_input,
            |this, _input, event: &InputEvent, cx| match event {
                InputEvent::Submitted(_) => this.speak_test(cx),
                InputEvent::Changed(_) => cx.notify(),
                InputEvent::Cancelled => {}
            },
        );

        Self {
            paused: false,
            volume: SEED_VOLUME,
            now_speaking: Some(seed_now_speaking()),
            queue: seed_queue(),
            stats: seed_stats(),
            engines: seed_engines(),
            pending_stop_all: false,
            test_input,
            _test_sub: test_sub,
        }
    }

    // --- control handlers (view-state stubs) ------------------------------

    /// Toggles pause/resume of the speak queue. Real path: `SpeakCommand::Pause` /
    /// `SpeakCommand::Resume` through the speak-queue dispatch handle.
    fn toggle_pause(&mut self, cx: &mut Context<Self>) {
        self.paused = !self.paused;
        cx.notify();
    }

    /// Skips the current utterance, advancing the counter. Real path:
    /// `SpeakCommand::Skip`; the bridge then clears the now-playing slot.
    fn skip(&mut self, cx: &mut Context<Self>) {
        if self.now_speaking.take().is_some() {
            self.stats.skipped = self.stats.skipped.saturating_add(1);
        }
        cx.notify();
    }

    /// Arms the stop-all confirm gate.
    fn arm_stop_all(&mut self, cx: &mut Context<Self>) {
        self.pending_stop_all = true;
        cx.notify();
    }

    fn cancel_stop_all(&mut self, cx: &mut Context<Self>) {
        self.pending_stop_all = false;
        cx.notify();
    }

    /// Clears the queue and the now-playing slot. Real path: `SpeakCommand::Clear`
    /// through the dispatch handle; the bridge reflects the emptied queue back.
    fn confirm_stop_all(&mut self, cx: &mut Context<Self>) {
        self.pending_stop_all = false;
        self.queue.clear();
        self.now_speaking = None;
        cx.notify();
    }

    /// Stores the new volume. Real path: `SpeakCommand::SetVolume`.
    fn set_volume(&mut self, volume: f32, cx: &mut Context<Self>) {
        self.volume = volume;
        cx.notify();
    }

    /// Enqueues the test-speak text. Real path: `SpeakCommand::Enqueue` with a
    /// test request; the bridge pushes the resulting queued item back. Here it
    /// appends a cached queue item and clears the field.
    fn speak_test(&mut self, cx: &mut Context<Self>) {
        let text = self.test_input.read(cx).content().trim().to_owned();
        if text.is_empty() {
            return;
        }
        self.queue.push(QueueItem {
            viewer_name: "Test".to_owned(),
            engine_voice: String::new(),
            text,
            duration_secs: 0,
            is_high_priority: false,
            bits_amount: None,
        });
        self.test_input.update(cx, |ti, cx| ti.set_content("", cx));
        cx.notify();
    }

    // --- control strip ----------------------------------------------------

    fn control_strip(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let gap = spacing(Spacing::Xs, density);

        // Pause/resume: a filled button inking the success hue when paused (Resume)
        // and the danger hue while running (Pause).
        let (pause_label, pause_glyph, btn_bg) = if self.paused {
            ("Resume", Icon::PlayerPlay, palette.success)
        } else {
            ("Pause queue", Icon::PlayerPause, palette.random)
        };
        let pause_btn = div()
            .id("tts-pause")
            .flex()
            .items_center()
            .gap(gap)
            .py(spacing(Spacing::Xxs, density))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Sm))
            .bg(btn_bg)
            .cursor_pointer()
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_pause(cx)))
            .child(icon(pause_glyph, PAUSE_GLYPH, palette.shell))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.shell)
                    .child(pause_label),
            );

        let skip_btn = self.ghost_strip_button(
            "tts-skip",
            "Skip",
            palette,
            density,
            cx.listener(|this, _: &ClickEvent, _, cx| this.skip(cx)),
        );
        let stop_btn = self.ghost_strip_button(
            "tts-stop",
            "Stop all",
            palette,
            density,
            cx.listener(|this, _: &ClickEvent, _, cx| this.arm_stop_all(cx)),
        );

        let vol_pct = (self.volume * 100.0).round() as u32;
        let vol_text = div()
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(FONT_SM)
            .text_color(palette.text_muted)
            .child(format!("{vol_pct}%"));
        let vol_slider =
            div()
                .w(VOL_SLIDER_W)
                .child(slider(self.volume, 0.0, 1.0, palette).on_change(
                    "tts-vol",
                    cx.listener(|this, v: &f32, _, cx| this.set_volume(*v, cx)),
                ));
        let volume_row = div()
            .flex()
            .items_center()
            .gap(gap)
            .child(icon(Icon::Volume, VOLUME_GLYPH, palette.text_muted))
            .child(vol_slider)
            .child(vol_text);

        let left = div()
            .flex()
            .items_center()
            .gap(gap)
            .child(pause_btn)
            .child(skip_btn)
            .child(stop_btn)
            .child(volume_row);

        let test_input = div().w(TEST_INPUT_W).child(self.test_input.clone());
        let speak_btn = div()
            .id("tts-speak")
            .flex()
            .items_center()
            .py(spacing(Spacing::Xxs, density))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Sm))
            .bg(palette.brand)
            .cursor_pointer()
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.speak_test(cx)))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.shell)
                    .child("Speak"),
            );
        let right = div()
            .flex()
            .items_center()
            .gap(gap)
            .child(test_input)
            .child(speak_btn);

        let inner = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .child(left)
            .child(right);

        card(inner, palette)
            .split_radius(px(0.0), px(0.0))
            .padding_xy(spacing(Spacing::Xs, density), spacing(Spacing::Md, density))
            .full_width()
            .into_any_element()
    }

    fn ghost_strip_button(
        &self,
        id: &'static str,
        label: &'static str,
        palette: &ForgePalette,
        density: Density,
        handler: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
    ) -> AnyElement {
        let hover = palette.elevated;
        div()
            .id(id)
            .flex()
            .items_center()
            .py(spacing(Spacing::Xxs, density))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Sm))
            .bg(palette.surface_overlay)
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .cursor_pointer()
            .hover(move |s| s.bg(hover))
            .on_click(handler)
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_secondary)
                    .child(label),
            )
            .into_any_element()
    }

    // --- now speaking -----------------------------------------------------

    fn now_speaking_panel(&self, palette: &ForgePalette, density: Density) -> AnyElement {
        let header = div()
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(FONT_XS)
            .text_color(palette.text_muted)
            .child("NOW SPEAKING");

        let body = match &self.now_speaking {
            Some(ns) => {
                let progress = format!(
                    "{}:{:02} / {}:{:02}",
                    ns.elapsed_secs / 60,
                    ns.elapsed_secs % 60,
                    ns.total_secs / 60,
                    ns.total_secs % 60
                );
                div()
                    .flex()
                    .flex_col()
                    .gap(spacing(Spacing::Xs, density))
                    .child(header)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(spacing(Spacing::Xs, density))
                            .child(
                                div()
                                    .font_family(DEFAULT_BODY_FAMILY)
                                    .text_size(FONT_SM)
                                    .text_color(palette.success)
                                    .child(ns.viewer_name.clone()),
                            )
                            .child(
                                div()
                                    .font_family(DEFAULT_MONO_FAMILY)
                                    .text_size(FONT_SM)
                                    .text_color(palette.text_muted)
                                    .child(ns.engine_voice.clone()),
                            ),
                    )
                    .child(
                        div()
                            .font_family(DEFAULT_BODY_FAMILY)
                            .text_size(FONT_SM)
                            .text_color(palette.text_primary)
                            .child(ns.text.clone()),
                    )
                    .child(
                        div()
                            .font_family(DEFAULT_MONO_FAMILY)
                            .text_size(FONT_SM)
                            .text_color(palette.text_muted)
                            .child(progress),
                    )
            }
            None => div()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Xs, density))
                .child(header)
                .child(
                    div()
                        .font_family(DEFAULT_BODY_FAMILY)
                        .text_size(FONT_SM)
                        .text_color(palette.text_muted)
                        .child("—"),
                ),
        };

        card(body, palette)
            .split_radius(px(0.0), px(0.0))
            .padding_xy(spacing(Spacing::Sm, density), spacing(Spacing::Md, density))
            .full_width()
            .into_any_element()
    }

    // --- queue ------------------------------------------------------------

    fn queue_section(&self, palette: &ForgePalette, density: Density) -> AnyElement {
        let count = self.queue.len();
        let count_pill = div()
            .px(spacing(Spacing::Xs, density))
            .rounded(radius(Radius::Pill))
            .bg(palette.surface_overlay)
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_muted)
                    .child(count.to_string()),
            );
        let header = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Md, density))
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child("Up next"),
            )
            .child(count_pill);

        let list: AnyElement = if self.queue.is_empty() {
            div()
                .w_full()
                .p(spacing(Spacing::Md, density))
                .child(
                    div()
                        .font_family(DEFAULT_BODY_FAMILY)
                        .text_size(FONT_SM)
                        .text_color(palette.text_muted)
                        .child("Queue is empty"),
                )
                .into_any_element()
        } else {
            let mut col = div().w_full().flex().flex_col();
            for (index, item) in self.queue.iter().enumerate() {
                col = col.child(queue_item_row(index, item, palette, density));
            }
            col.into_any_element()
        };

        div()
            .w_full()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .child(header)
            .child(
                div()
                    .id("tts-queue-scroll")
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .child(list),
            )
            .into_any_element()
    }

    // --- right rail -------------------------------------------------------

    fn right_pane(&self, palette: &ForgePalette, density: Density) -> AnyElement {
        let latency = self
            .stats
            .avg_latency_ms
            .map(|ms| format!("{ms}ms"))
            .unwrap_or_else(|| "—".to_owned());

        let stats_col = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(rail_header("SESSION", palette))
            .child(stat_row(
                "Spoken",
                self.stats.spoken.to_string(),
                palette.brand,
                palette,
                density,
                true,
            ))
            .child(stat_row(
                "Skipped",
                self.stats.skipped.to_string(),
                palette.warning,
                palette,
                density,
                true,
            ))
            .child(stat_row(
                "Filtered",
                self.stats.filtered.to_string(),
                palette.random,
                palette,
                density,
                true,
            ))
            .child(stat_row(
                "Avg latency",
                latency,
                palette.success,
                palette,
                density,
                false,
            ));

        let mut engines = div().flex().flex_col().gap(spacing(Spacing::Xs, density));
        for eng in &self.engines {
            let color = if eng.warn {
                palette.warning
            } else {
                palette.success
            };
            engines = engines.child(engine_card(eng.name, eng.meta, color, palette, density));
        }
        let engines_col = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(rail_header("ENGINES", palette))
            .child(engines);

        div()
            .w(RIGHT_PANE_W)
            .flex_shrink_0()
            .h_full()
            .flex()
            .flex_col()
            .bg(palette.shell)
            .border_l(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(
                div()
                    .id("tts-session-scroll")
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .p(spacing(Spacing::Sm, density))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(spacing(Spacing::Sm, density))
                            .child(stats_col)
                            .child(engines_col),
                    ),
            )
            .into_any_element()
    }

    // --- stop-all confirm -------------------------------------------------

    fn render_stop_confirm(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let card = confirm_modal(
            "Stop all TTS",
            "Currently speaking message will be cut off and all queued messages \
             dropped. Engines remain ready to handle new messages.",
            ConfirmTone::Destructive,
            palette,
        )
        .esc_hint("to cancel")
        .on_cancel(
            "tts-stop-cancel",
            "Cancel",
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_stop_all(cx)),
        )
        .on_confirm(
            "tts-stop-confirm",
            "Stop all",
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_stop_all(cx)),
        );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("tts-stop-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel_stop_all(cx));
            })
    }
}

impl Render for TtsDashboardView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let control_strip = self.control_strip(&palette, density, cx);
        let now_speaking = self.now_speaking_panel(&palette, density);
        let queue_section = self.queue_section(&palette, density);
        let right_pane = self.right_pane(&palette, density);

        let left_col = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .min_h(px(0.0))
            .child(now_speaking)
            .child(queue_section);

        let main_row = div()
            .w_full()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_row()
            .child(left_col)
            .child(right_pane);

        let confirm = self
            .pending_stop_all
            .then(|| self.render_stop_confirm(&palette, cx));

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(control_strip)
            .child(main_row)
            .children(confirm)
    }
}

// ── view-specific fragments ───────────────────────────────────────────────

/// One queue row: a fixed-width mono position gutter, a viewer/voice header over
/// the ellipsised message, and a trailing mono duration.
fn queue_item_row(
    index: usize,
    item: &QueueItem,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    let pos = div()
        .w(QUEUE_POS_W)
        .flex_shrink_0()
        .font_family(DEFAULT_MONO_FAMILY)
        .text_size(FONT_SM)
        .text_color(palette.text_muted)
        .child(format!("{}", index + 1));

    let mut name_row = div()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xs, density))
        .child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
                .text_color(palette.success)
                .child(item.viewer_name.clone()),
        );
    if item.is_high_priority {
        let label = item
            .bits_amount
            .map(|b| format!("BITS {b}"))
            .unwrap_or_else(|| "HIGH".to_owned());
        name_row = name_row.child(badge(palette.warning, palette.shell, label, true, FONT_XS));
    }
    if !item.engine_voice.is_empty() {
        name_row = name_row.child(
            div()
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(FONT_SM)
                .text_color(palette.text_muted)
                .child(item.engine_voice.clone()),
        );
    }

    let content = div()
        .flex_1()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Xxs, density))
        .child(name_row)
        .child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
                .text_color(palette.text_muted)
                .child(item.text.clone()),
        );

    let duration = div()
        .font_family(DEFAULT_MONO_FAMILY)
        .text_size(FONT_SM)
        .text_color(palette.text_muted)
        .child(format!("0:{:02}", item.duration_secs));

    div()
        .w_full()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xs, density))
        .py(spacing(Spacing::Xs, density))
        .px(spacing(Spacing::Md, density))
        .border_b(BORDER_THIN)
        .border_color(palette.border_regular)
        .child(pos)
        .child(content)
        .child(duration)
        .into_any_element()
}

/// A right-rail block heading — an uppercase monospace caption inking `text_muted`.
fn rail_header(label: &'static str, palette: &ForgePalette) -> impl IntoElement {
    div()
        .font_family(DEFAULT_MONO_FAMILY)
        .text_size(FONT_XS)
        .text_color(palette.text_muted)
        .child(label)
}

/// One session-counter row: a muted label and a hued value, with an optional
/// bottom hairline separating it from the next.
fn stat_row(
    label: &'static str,
    value: String,
    value_color: Rgba,
    palette: &ForgePalette,
    density: Density,
    border_bottom: bool,
) -> impl IntoElement {
    let mut row = div()
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .py(spacing(Spacing::Xxs, density))
        .child(
            div()
                .flex_1()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
                .text_color(palette.text_muted)
                .child(label),
        )
        .child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
                .text_color(value_color)
                .child(value),
        );
    if border_bottom {
        row = row
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular);
    }
    row
}

/// One engine health card: the engine name over a mono meta line, with a trailing
/// health dot inking `status_color`.
fn engine_card(
    name: &'static str,
    meta: &'static str,
    status_color: Rgba,
    palette: &ForgePalette,
    density: Density,
) -> impl IntoElement {
    card(
        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(
                div()
                    .flex()
                    .items_center()
                    .child(
                        div()
                            .flex_1()
                            .font_family(DEFAULT_BODY_FAMILY)
                            .text_size(FONT_SM)
                            .text_color(palette.text_primary)
                            .child(name),
                    )
                    .child(status_dot(status_color, ENGINE_DOT)),
            )
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child(meta),
            ),
        palette,
    )
    .padding_xy(spacing(Spacing::Xs, density), spacing(Spacing::Sm, density))
    .full_width()
}

// ── seeded stub state ─────────────────────────────────────────────────────

/// The now-playing utterance the dashboard seeds before a speak queue is wired,
/// mirroring the design's now-speaking sample.
fn seed_now_speaking() -> NowSpeaking {
    NowSpeaking {
        viewer_name: "koval_dev".to_owned(),
        engine_voice: "Amazon Polly · Olena".to_owned(),
        text: "Дякую за стрім, GTNH контент топчик, продовжуй у тому ж дусі!".to_owned(),
        elapsed_secs: 3,
        total_secs: 8,
    }
}

/// The representative up-next queue, mirroring the design's queue roster so the
/// queue list, priority badge and now-speaking hand-off all render populated.
fn seed_queue() -> Vec<QueueItem> {
    vec![
        QueueItem {
            viewer_name: "olena_lv".to_owned(),
            engine_voice: "ElevenLabs · Rachel".to_owned(),
            text: "коли наступний стрім по фабриці?".to_owned(),
            duration_secs: 6,
            is_high_priority: false,
            bits_amount: None,
        },
        QueueItem {
            viewer_name: "stream_fan_kyiv".to_owned(),
            engine_voice: "Polly · Maksym".to_owned(),
            text: "keep going love the UA stream".to_owned(),
            duration_secs: 4,
            is_high_priority: true,
            bits_amount: Some(500),
        },
        QueueItem {
            viewer_name: "haash_".to_owned(),
            engine_voice: "Piper (local) · UA-1".to_owned(),
            text: "не забудь про aluminium bottleneck".to_owned(),
            duration_secs: 5,
            is_high_priority: false,
            bits_amount: None,
        },
        QueueItem {
            viewer_name: "ostap_pl".to_owned(),
            engine_voice: "Polly · Olena".to_owned(),
            text: "stainless steel вже відкрив?".to_owned(),
            duration_secs: 3,
            is_high_priority: false,
            bits_amount: None,
        },
        QueueItem {
            viewer_name: "danylo_ua".to_owned(),
            engine_voice: "ElevenLabs · Antoni".to_owned(),
            text: "го дотку після стріму".to_owned(),
            duration_secs: 3,
            is_high_priority: false,
            bits_amount: None,
        },
    ]
}

/// The seeded session counters, mirroring the design's SESSION panel.
fn seed_stats() -> SessionStats {
    SessionStats {
        spoken: 218,
        skipped: 14,
        filtered: 31,
        avg_latency_ms: Some(340),
    }
}

/// The seeded engine roster, mirroring the design's ENGINES panel.
fn seed_engines() -> Vec<EngineStatus> {
    vec![
        EngineStatus {
            name: "Amazon Polly",
            meta: "cloud · 142 calls today",
            warn: false,
        },
        EngineStatus {
            name: "ElevenLabs",
            meta: "cloud · 8.2k/10k chars",
            warn: true,
        },
        EngineStatus {
            name: "Piper",
            meta: "local · GPU · 12ms",
            warn: false,
        },
    ]
}
