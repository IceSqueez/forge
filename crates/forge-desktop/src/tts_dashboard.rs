use forge_components::FONT_BADGE;
use forge_components::FONT_FIELD;
use forge_components::FONT_LABEL;
use forge_components::FONT_META;
use forge_components::FONT_MICRO;
use forge_components::FONT_ROW;
use forge_components::FONT_SECTION_LABEL;
use forge_components::FONT_XS;
use forge_components::{
    BORDER_THIN, ChipGlyph, ConfirmTone, DRAG_SOURCE_OPACITY, Density, FONT_SM, FONT_XXS,
    ForgePalette, Icon, InputEvent, OverlayPosition, Radius, ResizeEdge, ResizeRange, Spacing,
    TextInput, badge, body_family, card, chip, confirm_modal, empty_state, fmt_clock, hash_accent,
    icon, install_resize, mono_family, overlay, radius, scaled, section_label, slider, spacing,
    status_dot, tooltip_builder, tr, with_alpha,
};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use forge_speak_queue::{
    EmoteTokenSet, Priority, RequestId, SpeakCommand, SpeakQueueHandle, SpeakRequest,
};
use forge_speech_types::TtsVoice;
use forge_storage::SettingsRepo;
use forge_tts_core::{TtsRegistry, engine_display_name};
use gpui::{
    Animation, AnimationExt, AnyElement, ClickEvent, Context, CursorStyle, Entity, FontWeight,
    Pixels, Rgba, SharedString, Subscription, Window, bounce, div, ease_in_out, prelude::*, px,
    relative,
};

use crate::async_bridge;
use crate::speak_state::{NowSpeaking, QueueItem, SessionStats, SpeakState};
use forge_components::ActivePresentation;

const VOL_SLIDER_W: Pixels = px(90.0);
const TEST_INPUT_W: Pixels = px(160.0);
const RAIL_DEFAULT_W: Pixels = px(240.0);
const RAIL_MIN_W: Pixels = px(200.0);
const RAIL_MAX_W: Pixels = px(400.0);
const QUEUE_POS_W: Pixels = px(14.0);
const ENGINE_DOT: Pixels = px(7.0);
const PAUSE_GLYPH: Pixels = px(13.0);
const STRIP_BTN_GLYPH: Pixels = px(13.0);
const SPEAK_BTN_GLYPH: Pixels = px(11.0);
const EQ_BAR_W: Pixels = px(2.0);
const EQ_BAR_GAP: Pixels = px(2.0);
const EQ_BAR_MAX_H: Pixels = px(11.0);
const EQ_BAR_HEIGHTS: [f32; 4] = [5.0, 11.0, 7.0, 9.0];
const EQ_MIN_SCALE: f32 = 0.35;
const EQ_BAR_FPS: f32 = 30.0;
const VOLUME_GLYPH: Pixels = px(14.0);
const STRIP_DIVIDER_H: Pixels = px(16.0);

const BODY_PAD_Y: Pixels = px(18.0);
const BODY_PAD_X: Pixels = px(22.0);
const SECTION_GAP: Pixels = px(18.0);
const NOW_LABEL_GAP: Pixels = px(7.0);
const NOW_CARD_PAD: Pixels = px(14.0);
const NOW_TILE: Pixels = px(32.0);
const NOW_TILE_RADIUS: Pixels = px(8.0);
const NOW_TILE_GLYPH: Pixels = px(16.0);
const NOW_NAME_MB: Pixels = px(2.0);
const NOW_NAME_FONT: Pixels = FONT_ROW;
const NOW_PILL_FONT: Pixels = FONT_SECTION_LABEL;
const NOW_MSG_FONT: Pixels = FONT_XS;
const NOW_MSG_LINE_HEIGHT: f32 = 1.45;
const PROGRESS_GAP: Pixels = px(8.0);
const PROGRESS_MT: Pixels = px(7.0);
const PROGRESS_FONT: Pixels = FONT_MICRO;
const PROGRESS_BAR_H: Pixels = px(3.0);
const PROGRESS_BAR_RADIUS: Pixels = px(2.0);

const QUEUE_ROW_PAD_V: Pixels = px(9.0);
const QUEUE_ROW_PAD_H: Pixels = px(14.0);
const QUEUE_BITS_FONT: Pixels = FONT_BADGE;
const QUEUE_INDEX_FONT: Pixels = FONT_META;
const QUEUE_GRIP_GLYPH: Pixels = px(13.0);
const QUEUE_NAME_FONT: Pixels = FONT_XS;
const QUEUE_PREVIEW_FONT: Pixels = FONT_MICRO;
const QUEUE_MSG_FONT: Pixels = FONT_META;
const QUEUE_DUR_FONT: Pixels = FONT_MICRO;
const QUEUE_ACTION_GLYPH: Pixels = px(13.0);
const QUEUE_DROP_TINT_ALPHA: f32 = 0.06;
const QUEUE_GHOST_MAX_W: Pixels = px(280.0);

const TOOLBAR_PAD_V: Pixels = px(9.0);
const BTN_PAD_V: Pixels = px(5.0);
const SOLID_BTN_PAD_H: Pixels = px(12.0);
const GHOST_BTN_PAD_H: Pixels = px(11.0);
const BTN_GAP: Pixels = px(5.0);
const STRIP_GAP: Pixels = px(8.0);
const VOL_GAP: Pixels = px(7.0);
const VOL_PCT_FONT: Pixels = FONT_META;
const VOL_PCT_W: Pixels = px(32.0);
const TEST_ICON: Pixels = px(12.0);
const TEST_FONT: Pixels = FONT_LABEL;
const BTN_FONT: Pixels = FONT_XS;
const COUNT_BADGE_FONT: Pixels = FONT_MICRO;
const RAIL_PAD_Y: Pixels = px(18.0);
const RAIL_PAD_X: Pixels = px(14.0);
const STAT_CARD_PAD_V: Pixels = px(2.0);
const STAT_CARD_PAD_H: Pixels = px(11.0);
const STAT_LABEL_FONT: Pixels = FONT_LABEL;
const STAT_VALUE_FONT: Pixels = FONT_FIELD;
const ENGINE_PAD_V: Pixels = px(9.0);
const ENGINE_PAD_H: Pixels = px(11.0);
const ENGINE_NAME_FONT: Pixels = FONT_LABEL;
const ENGINE_META_FONT: Pixels = FONT_MICRO;
const ENGINE_NAME_MB: Pixels = px(3.0);

struct TtsRailResizeDrag;

struct QueueRowDrag {
    request_id: RequestId,
}

#[derive(Clone)]
struct QueueRowGhost {
    name: SharedString,
    name_color: Rgba,
    text: SharedString,
    text_color: Rgba,
    surface: Rgba,
    border: Rgba,
}

impl Render for QueueRowGhost {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let density = cx.density();
        div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .max_w(QUEUE_GHOST_MAX_W)
            .py(scaled(QUEUE_ROW_PAD_V, density))
            .px(spacing(Spacing::Sm, density))
            .bg(self.surface)
            .border(BORDER_THIN)
            .border_color(self.border)
            .rounded(radius(Radius::Sm))
            .child(
                div()
                    .flex_shrink_0()
                    .font_family(body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(QUEUE_NAME_FONT)
                    .text_color(self.name_color)
                    .child(self.name.clone()),
            )
            .child(
                div()
                    .truncate()
                    .font_family(body_family())
                    .text_size(QUEUE_MSG_FONT)
                    .text_color(self.text_color)
                    .child(self.text.clone()),
            )
    }
}

struct EngineStatus {
    id: String,
    name: String,
    meta: String,
    warn: bool,
}

pub struct TtsDashboardView {
    speak_state: Entity<SpeakState>,
    speak: Option<SpeakQueueHandle>,
    settings: Arc<dyn SettingsRepo>,
    rt_handle: tokio::runtime::Handle,
    volume: f32,
    rail_width: Pixels,
    engines: Vec<EngineStatus>,
    pending_stop_all: bool,
    dragged_row: Option<RequestId>,
    test_input: Entity<TextInput>,
    volume_debounce: async_bridge::Debounced,
    _test_sub: Subscription,
    _speak_obs: Subscription,
}

impl TtsDashboardView {
    pub fn new(
        speak_state: Entity<SpeakState>,
        speak: Option<SpeakQueueHandle>,
        settings: Arc<dyn SettingsRepo>,
        registry: Option<Arc<RwLock<TtsRegistry>>>,
        rt_handle: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Self {
        let palette = cx.palette();
        let volume = speak.as_ref().map(|h| h.master_volume()).unwrap_or(1.0);
        let test_input = cx.new(|cx| {
            TextInput::new(tr!("tts_dash_test_placeholder"), cx)
                .with_palette(palette)
                .plain()
                .with_font_size(TEST_FONT)
        });
        let test_sub = cx.subscribe(
            &test_input,
            |this, _input, event: &InputEvent, cx| match event {
                InputEvent::Submitted(_) => this.speak_test(cx),
                InputEvent::Changed(_) => cx.notify(),
                InputEvent::Cancelled | InputEvent::Blurred(_) => {}
            },
        );
        let speak_obs = cx.observe(&speak_state, |_this, _state, cx| cx.notify());

        Self {
            speak_state,
            speak,
            settings,
            rt_handle,
            volume,
            rail_width: RAIL_DEFAULT_W,
            engines: load_engine_roster(registry.as_ref()),
            pending_stop_all: false,
            dragged_row: None,
            test_input,
            volume_debounce: async_bridge::Debounced::new(async_bridge::SLIDER_PERSIST_DEBOUNCE),
            _test_sub: test_sub,
            _speak_obs: speak_obs,
        }
    }

    fn dispatch(&self, cmd: SpeakCommand) {
        let Some(handle) = self.speak.clone() else {
            tracing::warn!("TTS command dropped - speak queue unavailable");
            return;
        };
        self.rt_handle.spawn(async move {
            if let Err(err) = handle.send(cmd).await {
                tracing::error!(error = %err, "TTS command dispatch failed");
            }
        });
    }

    fn toggle_pause(&mut self, cx: &mut Context<Self>) {
        let paused = self.speak_state.read(cx).manual_paused();
        let cmd = if paused {
            SpeakCommand::Resume
        } else {
            SpeakCommand::Pause
        };
        self.speak_state.update(cx, |state, cx| {
            state.set_manual_paused(!paused);
            cx.notify();
        });
        self.dispatch(cmd);
    }

    fn skip(&mut self, _cx: &mut Context<Self>) {
        self.dispatch(SpeakCommand::Skip);
    }

    fn play_now(&mut self, request_id: RequestId, _cx: &mut Context<Self>) {
        self.dispatch(SpeakCommand::PlayNow(request_id));
    }

    fn remove_queued(&mut self, request_id: RequestId, _cx: &mut Context<Self>) {
        self.dispatch(SpeakCommand::RemoveQueued(request_id));
    }

    fn begin_row_drag(&mut self, request_id: RequestId, cx: &mut Context<Self>) {
        self.dragged_row = Some(request_id);
        cx.notify();
    }

    fn release_row_drag(&mut self, drag_active: bool) {
        if !drag_active {
            self.dragged_row = None;
        }
    }

    fn reorder_queued(
        &mut self,
        request_id: RequestId,
        before: Option<RequestId>,
        cx: &mut Context<Self>,
    ) {
        self.dragged_row = None;
        cx.notify();
        if before.as_ref() == Some(&request_id) {
            return;
        }
        self.dispatch(SpeakCommand::Reorder { request_id, before });
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
        let settings = Arc::clone(&self.settings);
        self.volume_debounce
            .schedule(&self.rt_handle, "tts master volume", async move {
                forge_storage::set_master_volume(settings.as_ref(), volume).await
            });
        cx.notify();
    }

    fn set_rail_width(&mut self, width: Pixels, cx: &mut Context<Self>) {
        self.rail_width = width;
        cx.notify();
    }

    fn speak_test(&mut self, cx: &mut Context<Self>) {
        let text = self.test_input.read(cx).content().trim().to_owned();
        if text.is_empty() {
            return;
        }
        self.dispatch(SpeakCommand::Enqueue(test_speak_request(
            text,
            tr!("tts_dash_test_speaker_name"),
        )));
        self.test_input.update(cx, |ti, cx| ti.set_content("", cx));
        cx.notify();
    }

    fn control_strip(
        &self,
        manual_paused: bool,
        gate_held: bool,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let (pause_label, pause_glyph, btn_bg) = if manual_paused {
            (
                tr!("tts_dash_resume_btn"),
                Icon::PlayerPlay,
                palette.success,
            )
        } else {
            (tr!("tts_dash_pause_btn"), Icon::PlayerPause, palette.danger)
        };
        let pause_btn = div()
            .id("tts-pause")
            .flex()
            .items_center()
            .gap(scaled(BTN_GAP, density))
            .py(scaled(BTN_PAD_V, density))
            .px(scaled(SOLID_BTN_PAD_H, density))
            .rounded(radius(Radius::Sm))
            .bg(btn_bg)
            .cursor_pointer()
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_pause(cx)))
            .child(icon(pause_glyph, PAUSE_GLYPH, palette.shell))
            .child(
                div()
                    .font_family(body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(BTN_FONT)
                    .text_color(palette.shell)
                    .child(pause_label),
            );

        let skip_btn = self.ghost_strip_button(
            "tts-skip",
            Icon::PlayerSkipForward,
            palette.text_secondary,
            palette.text_primary,
            tr!("tts_dash_skip_btn"),
            palette,
            cx.listener(|this, _: &ClickEvent, _, cx| this.skip(cx)),
            density,
        );
        let stop_btn = self.ghost_strip_button(
            "tts-stop",
            Icon::PlayerStop,
            palette.danger,
            palette.danger,
            tr!("tts_dash_stop_all_btn"),
            palette,
            cx.listener(|this, _: &ClickEvent, _, cx| this.arm_stop_all(cx)),
            density,
        );

        let vol_pct = (self.volume * 100.0).round() as u32;
        let vol_text = div()
            .w(VOL_PCT_W)
            .font_family(mono_family())
            .text_size(VOL_PCT_FONT)
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
            .gap(scaled(VOL_GAP, density))
            .child(icon(Icon::Volume, VOLUME_GLYPH, palette.text_muted))
            .child(vol_slider)
            .child(vol_text);

        let gate_indicator = gate_held.then(|| {
            div()
                .flex()
                .items_center()
                .gap(scaled(STRIP_GAP, density))
                .child(strip_divider(palette, density))
                .child(chip(
                    tr!("tts_dash_voice_gate_held"),
                    ChipGlyph::Icon(Icon::Microphone2, palette.warning),
                    true,
                    palette,
                ))
                .into_any_element()
        });

        let left = div()
            .flex()
            .items_center()
            .gap(scaled(STRIP_GAP, density))
            .child(pause_btn)
            .child(skip_btn)
            .child(stop_btn)
            .child(strip_divider(palette, density))
            .child(volume_row)
            .children(gate_indicator);

        let test_field = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .py(spacing(Spacing::Xxs, density))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Sm))
            .bg(palette.shell)
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(icon(Icon::TestPipe, TEST_ICON, palette.text_faint))
            .child(div().w(TEST_INPUT_W).child(self.test_input.clone()));
        let speak_btn = div()
            .id("tts-speak")
            .flex()
            .items_center()
            .gap(scaled(BTN_GAP, density))
            .py(scaled(BTN_PAD_V, density))
            .px(scaled(SOLID_BTN_PAD_H, density))
            .rounded(radius(Radius::Sm))
            .bg(palette.brand)
            .cursor_pointer()
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.speak_test(cx)))
            .child(icon(Icon::PlayerPlayFilled, SPEAK_BTN_GLYPH, palette.shell))
            .child(
                div()
                    .font_family(body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(BTN_FONT)
                    .text_color(palette.shell)
                    .child(tr!("tts_dash_speak_btn")),
            );
        let right = div()
            .flex()
            .items_center()
            .gap(scaled(STRIP_GAP, density))
            .child(test_field)
            .child(speak_btn);

        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .py(scaled(TOOLBAR_PAD_V, density))
            .px(spacing(Spacing::Md, density))
            .bg(palette.elevated)
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(left)
            .child(right)
            .into_any_element()
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "render helper takes each displayed value and handler separately"
    )]
    fn ghost_strip_button(
        &self,
        id: &'static str,
        glyph: Icon,
        text_color: Rgba,
        hover_text: Rgba,
        label: impl Into<SharedString>,
        palette: &ForgePalette,
        handler: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
        density: Density,
    ) -> AnyElement {
        let hover_border = palette.border_input;
        div()
            .id(id)
            .flex()
            .items_center()
            .gap(scaled(BTN_GAP, density))
            .py(scaled(BTN_PAD_V, density))
            .px(scaled(GHOST_BTN_PAD_H, density))
            .rounded(radius(Radius::Sm))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .text_color(text_color)
            .cursor_pointer()
            .hover(move |s| s.border_color(hover_border).text_color(hover_text))
            .on_click(handler)
            .child(icon(glyph, STRIP_BTN_GLYPH, text_color))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(BTN_FONT)
                    .child(label.into()),
            )
            .into_any_element()
    }

    fn right_pane(
        &self,
        stats: &SessionStats,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let latency = stats
            .avg_latency_ms
            .map(|ms| format!("{ms}ms"))
            .unwrap_or_else(|| "-".to_owned());

        let stat_rows = div()
            .flex()
            .flex_col()
            .child(stat_row(
                tr!("tts_dash_stat_spoken"),
                stats.spoken.to_string(),
                palette.brand,
                palette,
                true,
                density,
            ))
            .child(stat_row(
                tr!("tts_dash_stat_skipped"),
                stats.skipped.to_string(),
                palette.warning,
                palette,
                true,
                density,
            ))
            .child(stat_row(
                tr!("tts_dash_stat_filtered"),
                stats.filtered.to_string(),
                palette.danger,
                palette,
                true,
                density,
            ))
            .child(stat_row(
                tr!("tts_dash_stat_avg_latency"),
                latency,
                palette.success,
                palette,
                false,
                density,
            ));

        let mut content = div()
            .flex()
            .flex_col()
            .child(section_heading(
                section_label(tr!("tts_dash_session_header"), palette),
                None,
                density,
            ))
            .child(
                div().mb(scaled(SECTION_GAP, density)).child(
                    card(stat_rows, palette)
                        .padding_xy(
                            scaled(STAT_CARD_PAD_V, density),
                            scaled(STAT_CARD_PAD_H, density),
                        )
                        .full_width(),
                ),
            )
            .child(section_heading(
                section_label(tr!("tts_dash_engines_header"), palette),
                None,
                density,
            ));

        if self.engines.is_empty() {
            content = content.child(
                div()
                    .font_family(body_family())
                    .text_size(STAT_LABEL_FONT)
                    .text_color(palette.text_muted)
                    .child(tr!("tts_dash_engines_none")),
            );
        }
        let voices = self
            .speak
            .as_ref()
            .map(|h| h.available_voices())
            .unwrap_or_default();
        for eng in &self.engines {
            let voice_count = voices.iter().filter(|v| v.engine_id.0 == eng.id).count();
            let voiceless = voice_count == 0;
            let color = if eng.warn || voiceless {
                palette.warning
            } else {
                palette.success
            };
            let meta = if voiceless {
                format!("{} \u{b7} {}", eng.meta, tr!("tts_dash_engine_no_voices"))
            } else {
                eng.meta.clone()
            };
            content = content.child(engine_card(eng.name.clone(), meta, color, palette, density));
        }

        let panel = div()
            .w(self.rail_width)
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
                    .py(scaled(RAIL_PAD_Y, density))
                    .px(scaled(RAIL_PAD_X, density))
                    .child(content),
            );

        install_resize(
            panel,
            TtsRailResizeDrag,
            "tts-rail-resize",
            ResizeEdge::Left,
            ResizeRange {
                min: RAIL_MIN_W,
                max: RAIL_MAX_W,
            },
            palette,
            cx.listener(|this, width: &Pixels, _, cx| this.set_rail_width(*width, cx)),
        )
        .into_any_element()
    }

    fn render_stop_confirm(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let card = confirm_modal(
            tr!("tts_dash_stop_all_confirm_name"),
            tr!("tts_dash_stop_all_confirm_hint"),
            ConfirmTone::Destructive,
            palette,
        )
        .esc_hint(tr!("widget_confirm_esc_to_cancel"))
        .on_cancel(
            "tts-stop-cancel",
            tr!("common_cancel"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_stop_all(cx)),
        )
        .on_confirm(
            "tts-stop-confirm",
            tr!("tts_dash_stop_all_btn"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_stop_all(cx)),
        );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("tts-stop-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel_stop_all(cx));
            })
    }

    fn queue_section(
        &self,
        queue: &[QueueItem],
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let count = queue.len();
        let count_badge = badge(
            palette.surface_overlay,
            palette.text_muted,
            count.to_string(),
            false,
            COUNT_BADGE_FONT,
        );
        let title = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(section_label(tr!("tts_dash_queue_header"), palette))
            .child(count_badge);
        let total = (!queue.is_empty()).then(|| {
            let total: u32 = queue.iter().map(|item| item.duration_secs).sum();
            div()
                .font_family(mono_family())
                .text_size(FONT_XXS)
                .text_color(palette.text_faint)
                .child(tr!("tts_dash_queue_total", secs = total as i64))
                .into_any_element()
        });

        let list: AnyElement = if queue.is_empty() {
            empty_state(tr!("tts_dash_queue_empty"), palette)
                .density(density)
                .into_any_element()
        } else {
            let last = count - 1;
            let mut col = div().w_full().flex().flex_col();
            for (index, item) in queue.iter().enumerate() {
                col = col.child(self.queue_item_row(index, index == last, item, palette, cx));
            }
            col.into_any_element()
        };

        div()
            .w_full()
            .flex()
            .flex_col()
            .child(section_heading(title, total, density))
            .child(
                div()
                    .id("tts-queue-drop")
                    .w_full()
                    .on_drop(cx.listener(|this, drag: &QueueRowDrag, _window, cx| {
                        this.reorder_queued(drag.request_id.clone(), None, cx)
                    }))
                    .child(card(list, palette).padding(px(0.0)).full_width()),
            )
            .into_any_element()
    }

    fn queue_item_row(
        &self,
        index: usize,
        is_last: bool,
        item: &QueueItem,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let pos = div()
            .w(QUEUE_POS_W)
            .flex_shrink_0()
            .text_center()
            .font_family(mono_family())
            .text_size(QUEUE_INDEX_FONT)
            .text_color(palette.text_faint)
            .child(format!("{}", index + 1));

        let grip = icon(
            Icon::GripVertical,
            QUEUE_GRIP_GLYPH,
            palette.text_extreme_faint,
        );

        let mut name_row = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(
                div()
                    .font_family(body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(QUEUE_NAME_FONT)
                    .text_color(hash_accent(&item.viewer_name, palette))
                    .child(item.viewer_name.clone()),
            );
        if item.is_high_priority {
            let label = item
                .bits_amount
                .map(|b| tr!("tts_dash_priority_bits", amount = b as i64))
                .unwrap_or_else(|| tr!("tts_dash_priority_high"));
            name_row = name_row.child(badge(
                palette.warning,
                palette.shell,
                label,
                false,
                QUEUE_BITS_FONT,
            ));
        }
        if !item.engine_voice.is_empty() {
            name_row = name_row.child(
                div()
                    .font_family(mono_family())
                    .text_size(QUEUE_PREVIEW_FONT)
                    .text_color(palette.text_faint)
                    .child(item.engine_voice.clone()),
            );
        }

        let content = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .child(name_row)
            .child(
                div()
                    .truncate()
                    .font_family(body_family())
                    .text_size(QUEUE_MSG_FONT)
                    .text_color(palette.text_muted)
                    .child(item.text.clone()),
            );

        let duration = div()
            .flex_shrink_0()
            .font_family(mono_family())
            .text_size(QUEUE_DUR_FONT)
            .text_color(palette.text_faint)
            .child(format!("~{}", fmt_clock(u64::from(item.duration_secs))));

        let play_id = item.request_id.clone();
        let play_btn = div()
            .id(SharedString::from(format!(
                "tts-q-play-{}",
                item.request_id.0
            )))
            .flex_shrink_0()
            .cursor_pointer()
            .tooltip(tooltip_builder(tr!("tts_dash_play_now"), palette))
            .on_click(
                cx.listener(move |this, _: &ClickEvent, _, cx| this.play_now(play_id.clone(), cx)),
            )
            .child(icon(Icon::PlayerPlay, QUEUE_ACTION_GLYPH, palette.success));

        let remove_id = item.request_id.clone();
        let remove_btn = div()
            .id(SharedString::from(format!(
                "tts-q-remove-{}",
                item.request_id.0
            )))
            .flex_shrink_0()
            .cursor_pointer()
            .tooltip(tooltip_builder(tr!("tts_dash_remove_queued"), palette))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.remove_queued(remove_id.clone(), cx)
            }))
            .child(icon(Icon::X, QUEUE_ACTION_GLYPH, palette.text_faint));

        let ghost = QueueRowGhost {
            name: item.viewer_name.clone().into(),
            name_color: hash_accent(&item.viewer_name, palette),
            text: item.text.clone().into(),
            text_color: palette.text_muted,
            surface: palette.elevated,
            border: palette.border_regular,
        };
        let view = cx.entity().downgrade();
        let drop_tint = with_alpha(palette.brand, QUEUE_DROP_TINT_ALPHA);
        let brand = palette.brand;
        let hovered_id = item.request_id.clone();
        let dropped_on_id = item.request_id.clone();

        let mut row = div()
            .id(SharedString::from(format!(
                "tts-q-row-{}",
                item.request_id.0
            )))
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .py(scaled(QUEUE_ROW_PAD_V, density))
            .px(scaled(QUEUE_ROW_PAD_H, density))
            .cursor(CursorStyle::OpenHand)
            .on_drag(
                QueueRowDrag {
                    request_id: item.request_id.clone(),
                },
                move |drag, _offset, _window, cx| {
                    let dragged = drag.request_id.clone();
                    view.update(cx, |this, cx| this.begin_row_drag(dragged, cx))
                        .ok();
                    cx.new(|_| ghost.clone())
                },
            )
            .drag_over::<QueueRowDrag>(move |style, drag, _window, _cx| {
                if drag.request_id == hovered_id {
                    return style;
                }
                style
                    .bg(drop_tint)
                    .border_color(brand)
                    .border_t(BORDER_THIN)
                    .border_b(px(0.0))
            })
            .on_drop(cx.listener(move |this, drag: &QueueRowDrag, _window, cx| {
                this.reorder_queued(drag.request_id.clone(), Some(dropped_on_id.clone()), cx)
            }))
            .child(pos)
            .child(grip)
            .child(content)
            .child(duration)
            .child(play_btn)
            .child(remove_btn);
        if !is_last {
            row = row.border_b(BORDER_THIN).border_color(palette.elevated);
        }

        if self.dragged_row.as_ref() == Some(&item.request_id) {
            row.opacity(DRAG_SOURCE_OPACITY).into_any_element()
        } else {
            row.into_any_element()
        }
    }
}

impl Render for TtsDashboardView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();
        self.release_row_drag(cx.has_active_drag());

        let manual_paused = self.speak_state.read(cx).manual_paused();
        let gate_held = self.speak_state.read(cx).gate_held();
        let now = self.speak_state.read(cx).now_speaking_snapshot();
        let queue = self.speak_state.read(cx).queue_snapshot();
        let stats = self.speak_state.read(cx).stats_snapshot();
        let last_drop = self
            .speak_state
            .read(cx)
            .last_drop()
            .map(|reason| reason.to_owned());

        let voices = self
            .speak
            .as_ref()
            .map(|h| h.available_voices())
            .unwrap_or_default();
        let now_voice = now
            .as_ref()
            .and_then(|ns| resolve_now_voice(&ns.engine_id, &ns.voice_id, &voices));

        let control_strip = self.control_strip(manual_paused, gate_held, &palette, cx);
        let now_speaking = now_speaking_panel(
            now.as_ref(),
            now_voice,
            last_drop.as_deref(),
            manual_paused,
            &palette,
            density,
        );
        let queue_section = self.queue_section(&queue, &palette, density, cx);
        let right_pane = self.right_pane(&stats, &palette, cx);

        let left_col = div()
            .id("tts-dash-body")
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .overflow_y_scroll()
            .bg(palette.base)
            .py(scaled(BODY_PAD_Y, density))
            .px(scaled(BODY_PAD_X, density))
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .child(now_speaking)
                    .child(queue_section),
            );

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

fn strip_divider(palette: &ForgePalette, density: Density) -> impl IntoElement {
    div()
        .w(BORDER_THIN)
        .h(STRIP_DIVIDER_H)
        .mx(spacing(Spacing::Xxs, density))
        .bg(palette.border_regular)
}

fn eq_bars(animate: bool, color: Rgba) -> impl IntoElement {
    let mut bars = div().flex().items_end().gap(EQ_BAR_GAP).h(EQ_BAR_MAX_H);
    for (i, base) in EQ_BAR_HEIGHTS.iter().enumerate() {
        let base = *base;
        let bar = div().w(EQ_BAR_W).h(px(base)).rounded(px(1.0)).bg(color);
        let child: AnyElement = if animate {
            bar.with_animation(
                ("tts-eq-bar", i),
                Animation::new(Duration::from_secs_f32(0.5 + i as f32 * 0.15))
                    .repeat()
                    .with_easing(bounce(ease_in_out))
                    .with_max_fps(EQ_BAR_FPS),
                move |el, delta| el.h(px(base * (EQ_MIN_SCALE + (1.0 - EQ_MIN_SCALE) * delta))),
            )
            .into_any_element()
        } else {
            bar.into_any_element()
        };
        bars = bars.child(child);
    }
    bars
}

fn resolve_now_voice(engine_id: &str, voice_id: &str, voices: &[TtsVoice]) -> Option<SharedString> {
    if engine_id.is_empty() && voice_id.is_empty() {
        return None;
    }
    let name = voices
        .iter()
        .find(|v| v.id.0 == voice_id)
        .map(|v| v.name.clone())
        .unwrap_or_else(|| voice_id.to_owned());
    Some(format!("{} \u{b7} {}", engine_display_name(engine_id), name).into())
}

fn now_speaking_panel(
    now: Option<&NowSpeaking>,
    now_voice: Option<SharedString>,
    last_drop: Option<&str>,
    manual_paused: bool,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    let animate = now.is_some() && !manual_paused;
    let bar_color = if animate {
        palette.success
    } else {
        palette.text_faint
    };
    let label = div()
        .flex()
        .items_center()
        .gap(scaled(NOW_LABEL_GAP, density))
        .child(section_label(tr!("tts_dash_now_speaking_header"), palette))
        .child(eq_bars(animate, bar_color));

    let body = match now {
        Some(ns) => {
            let tile = div()
                .w(NOW_TILE)
                .h(NOW_TILE)
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .rounded(NOW_TILE_RADIUS)
                .bg(palette.surface_overlay)
                .child(icon(Icon::Message2Share, NOW_TILE_GLYPH, palette.brand));

            let mut name_row = div()
                .flex()
                .items_center()
                .gap(spacing(Spacing::Xs, density))
                .mb(NOW_NAME_MB)
                .child(
                    div()
                        .font_family(body_family())
                        .font_weight(FontWeight::MEDIUM)
                        .text_size(NOW_NAME_FONT)
                        .text_color(hash_accent(&ns.viewer_name, palette))
                        .child(ns.viewer_name.clone()),
                );
            if let Some(voice) = now_voice {
                name_row = name_row.child(badge(
                    palette.surface_overlay,
                    palette.text_muted,
                    voice,
                    true,
                    NOW_PILL_FONT,
                ));
            }

            let message = div()
                .font_family(body_family())
                .text_size(NOW_MSG_FONT)
                .line_height(relative(NOW_MSG_LINE_HEIGHT))
                .text_color(palette.text_primary)
                .child(ns.text.clone());

            let frac = if ns.total_secs > 0 {
                (ns.elapsed_secs as f32 / ns.total_secs as f32).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let progress = div()
                .flex()
                .items_center()
                .gap(scaled(PROGRESS_GAP, density))
                .mt(PROGRESS_MT)
                .child(
                    div()
                        .font_family(mono_family())
                        .text_size(PROGRESS_FONT)
                        .text_color(palette.text_muted)
                        .child(fmt_clock(u64::from(ns.elapsed_secs))),
                )
                .child(
                    div()
                        .flex_1()
                        .h(PROGRESS_BAR_H)
                        .rounded(PROGRESS_BAR_RADIUS)
                        .bg(palette.surface_overlay)
                        .child(
                            div()
                                .h_full()
                                .w(relative(frac))
                                .rounded(PROGRESS_BAR_RADIUS)
                                .bg(palette.success),
                        ),
                )
                .child(
                    div()
                        .font_family(mono_family())
                        .text_size(PROGRESS_FONT)
                        .text_color(palette.text_muted)
                        .child(fmt_clock(u64::from(ns.total_secs))),
                );

            let info = div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .child(name_row)
                .child(message)
                .child(progress);

            div()
                .flex()
                .items_start()
                .gap(spacing(Spacing::Sm, density))
                .child(tile)
                .child(info)
        }
        None => {
            let mut idle = div()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Xs, density))
                .child(
                    div()
                        .font_family(body_family())
                        .text_size(FONT_SM)
                        .text_color(palette.text_muted)
                        .child(tr!("tts_dash_no_speaking")),
                );
            if let Some(reason) = last_drop {
                idle = idle.child(
                    div()
                        .font_family(mono_family())
                        .text_size(FONT_XXS)
                        .text_color(palette.warning)
                        .child(tr!("tts_dash_last_drop", reason = reason)),
                );
            }
            idle
        }
    };

    div()
        .w_full()
        .flex()
        .flex_col()
        .child(section_heading(label, None, density))
        .child(
            div().mb(scaled(SECTION_GAP, density)).child(
                card(body, palette)
                    .padding(scaled(NOW_CARD_PAD, density))
                    .full_width(),
            ),
        )
        .into_any_element()
}

fn section_heading(
    label: impl IntoElement,
    right: Option<AnyElement>,
    density: Density,
) -> impl IntoElement {
    div()
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .mb(spacing(Spacing::Sm, density))
        .child(label)
        .children(right)
}

fn stat_row(
    label: impl Into<SharedString>,
    value: String,
    value_color: Rgba,
    palette: &ForgePalette,
    border_bottom: bool,
    density: Density,
) -> impl IntoElement {
    let mut row = div()
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .py(spacing(Spacing::Xs, density))
        .child(
            div()
                .flex_1()
                .font_family(body_family())
                .text_size(STAT_LABEL_FONT)
                .text_color(palette.text_muted)
                .child(label.into()),
        )
        .child(
            div()
                .font_family(body_family())
                .font_weight(FontWeight::MEDIUM)
                .text_size(STAT_VALUE_FONT)
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
    name: impl Into<SharedString>,
    meta: impl Into<SharedString>,
    status_color: Rgba,
    palette: &ForgePalette,
    density: Density,
) -> impl IntoElement {
    let name = name.into();
    let meta = meta.into();
    let body = div()
        .flex()
        .flex_col()
        .gap(ENGINE_NAME_MB)
        .child(
            div()
                .flex()
                .items_center()
                .child(
                    div()
                        .flex_1()
                        .font_family(body_family())
                        .font_weight(FontWeight::MEDIUM)
                        .text_size(ENGINE_NAME_FONT)
                        .text_color(palette.text_primary)
                        .child(name),
                )
                .child(status_dot(status_color, ENGINE_DOT)),
        )
        .child(
            div()
                .font_family(mono_family())
                .text_size(ENGINE_META_FONT)
                .text_color(palette.text_faint)
                .child(meta),
        );
    div().mb(spacing(Spacing::Xs, density)).child(
        card(body, palette)
            .padding_xy(scaled(ENGINE_PAD_V, density), scaled(ENGINE_PAD_H, density))
            .full_width(),
    )
}

fn test_speak_request(text: String, speaker_name: String) -> SpeakRequest {
    SpeakRequest {
        request_id: RequestId::new(),
        viewer_id: String::new(),
        viewer_name: speaker_name,
        text,
        priority: Priority::Normal,
        engine_override: None,
        voice_override: None,
        source_event_id: None,
        is_reward: false,
        target: None,
        message_emotes: EmoteTokenSet::default(),
    }
}

fn load_engine_roster(registry: Option<&Arc<RwLock<TtsRegistry>>>) -> Vec<EngineStatus> {
    let Some(registry) = registry else {
        return Vec::new();
    };
    let ids = registry
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .engine_ids();
    ids.into_iter()
        .map(|id| EngineStatus {
            name: engine_display_name(&id.0).to_owned(),
            meta: engine_kind(&id.0).to_owned(),
            warn: false,
            id: id.0,
        })
        .collect()
}

fn engine_kind(id: &str) -> &'static str {
    match id {
        forge_tts_core::ENGINE_ID_PIPER | forge_tts_core::ENGINE_ID_ESPEAK_NG => "local",
        forge_tts_core::ENGINE_ID_SAPI | forge_tts_core::ENGINE_ID_NSSPEECH => "system",
        _ => "cloud",
    }
}
