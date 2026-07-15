use forge_components::{
    BORDER_THIN, ConfirmTone, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_SM, FONT_XS,
    ForgePalette, Icon, InputEvent, OverlayPosition, Radius, Spacing, TextInput, badge, card,
    confirm_modal, icon, overlay, radius, slider, spacing, status_dot,
};
use forge_speak_queue::{Priority, RequestId, SpeakCommand, SpeakQueueHandle, SpeakRequest};
use gpui::{
    AnyElement, ClickEvent, Context, Entity, Pixels, Rgba, Subscription, Window, div, prelude::*,
    px,
};

use crate::presentation::ActivePresentation;
use crate::speak_state::{NowSpeaking, QueueItem, SessionStats, SpeakState};

const VOL_SLIDER_W: Pixels = px(90.0);
const TEST_INPUT_W: Pixels = px(180.0);
const RIGHT_PANE_W: Pixels = px(236.0);
const QUEUE_POS_W: Pixels = px(14.0);
const ENGINE_DOT: Pixels = px(7.0);
const PAUSE_GLYPH: Pixels = px(13.0);
const VOLUME_GLYPH: Pixels = px(14.0);
const SEED_VOLUME: f32 = 0.72;
const TEST_SPEAKER_NAME: &str = "Test";

struct EngineStatus {
    name: &'static str,
    meta: &'static str,
    warn: bool,
}

pub struct TtsDashboardView {
    speak_state: Entity<SpeakState>,
    speak: Option<SpeakQueueHandle>,
    rt_handle: tokio::runtime::Handle,
    volume: f32,
    engines: Vec<EngineStatus>,
    pending_stop_all: bool,
    test_input: Entity<TextInput>,
    _test_sub: Subscription,
    _speak_obs: Subscription,
}

impl TtsDashboardView {
    pub fn new(
        speak_state: Entity<SpeakState>,
        speak: Option<SpeakQueueHandle>,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
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
        let speak_obs = cx.observe(&speak_state, |_this, _state, cx| cx.notify());

        Self {
            speak_state,
            speak,
            rt_handle,
            volume: SEED_VOLUME,
            engines: seed_engines(),
            pending_stop_all: false,
            test_input,
            _test_sub: test_sub,
            _speak_obs: speak_obs,
        }
    }

    fn dispatch(&self, cmd: SpeakCommand) {
        let Some(handle) = self.speak.clone() else {
            eprintln!("forge-desktop: TTS command dropped — speak queue unavailable");
            return;
        };
        self.rt_handle.spawn(async move {
            if let Err(err) = handle.send(cmd).await {
                eprintln!("forge-desktop: TTS command dispatch failed: {err}");
            }
        });
    }

    fn toggle_pause(&mut self, cx: &mut Context<Self>) {
        let paused = self.speak_state.read(cx).paused();
        let cmd = if paused {
            SpeakCommand::Resume
        } else {
            SpeakCommand::Pause
        };
        self.speak_state.update(cx, |state, cx| {
            state.set_paused(!paused);
            cx.notify();
        });
        self.dispatch(cmd);
    }

    fn skip(&mut self, _cx: &mut Context<Self>) {
        self.dispatch(SpeakCommand::Skip);
    }

    fn arm_stop_all(&mut self, cx: &mut Context<Self>) {
        self.pending_stop_all = true;
        cx.notify();
    }

    fn cancel_stop_all(&mut self, cx: &mut Context<Self>) {
        self.pending_stop_all = false;
        cx.notify();
    }

    fn confirm_stop_all(&mut self, cx: &mut Context<Self>) {
        self.pending_stop_all = false;
        self.speak_state.update(cx, |state, cx| {
            state.clear_all();
            cx.notify();
        });
        self.dispatch(SpeakCommand::Clear);
        cx.notify();
    }

    fn set_volume(&mut self, volume: f32, cx: &mut Context<Self>) {
        self.volume = volume;
        self.dispatch(SpeakCommand::SetVolume(volume));
        cx.notify();
    }

    fn speak_test(&mut self, cx: &mut Context<Self>) {
        let text = self.test_input.read(cx).content().trim().to_owned();
        if text.is_empty() {
            return;
        }
        self.dispatch(SpeakCommand::Enqueue(test_speak_request(text)));
        self.test_input.update(cx, |ti, cx| ti.set_content("", cx));
        cx.notify();
    }

    fn control_strip(
        &self,
        paused: bool,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let gap = spacing(Spacing::Xs, density);

        let (pause_label, pause_glyph, btn_bg) = if paused {
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

    fn right_pane(
        &self,
        stats: &SessionStats,
        palette: &ForgePalette,
        density: Density,
    ) -> AnyElement {
        let latency = stats
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
                stats.spoken.to_string(),
                palette.brand,
                palette,
                density,
                true,
            ))
            .child(stat_row(
                "Skipped",
                stats.skipped.to_string(),
                palette.warning,
                palette,
                density,
                true,
            ))
            .child(stat_row(
                "Filtered",
                stats.filtered.to_string(),
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

        let paused = self.speak_state.read(cx).paused();
        let now = self.speak_state.read(cx).now_speaking_snapshot();
        let queue = self.speak_state.read(cx).queue_snapshot();
        let stats = self.speak_state.read(cx).stats_snapshot();

        let control_strip = self.control_strip(paused, &palette, density, cx);
        let now_speaking = now_speaking_panel(now.as_ref(), &palette, density);
        let queue_section = queue_section(&queue, &palette, density);
        let right_pane = self.right_pane(&stats, &palette, density);

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

fn now_speaking_panel(
    now: Option<&NowSpeaking>,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    let header = div()
        .font_family(DEFAULT_MONO_FAMILY)
        .text_size(FONT_XS)
        .text_color(palette.text_muted)
        .child("NOW SPEAKING");

    let body = match now {
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

fn queue_section(queue: &[QueueItem], palette: &ForgePalette, density: Density) -> AnyElement {
    let count = queue.len();
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

    let list: AnyElement = if queue.is_empty() {
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
        for (index, item) in queue.iter().enumerate() {
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

fn rail_header(label: &'static str, palette: &ForgePalette) -> impl IntoElement {
    div()
        .font_family(DEFAULT_MONO_FAMILY)
        .text_size(FONT_XS)
        .text_color(palette.text_muted)
        .child(label)
}

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

fn test_speak_request(text: String) -> SpeakRequest {
    SpeakRequest {
        request_id: RequestId::new(),
        viewer_id: String::new(),
        viewer_name: TEST_SPEAKER_NAME.to_owned(),
        text,
        priority: Priority::Normal,
        alias_override: None,
        engine_override: None,
        voice_override: None,
        source_event_id: forge_types::EventId::new(),
        is_reward: false,
    }
}

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
