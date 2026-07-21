use forge_components::{
    BORDER_THIN, ConfirmTone, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_SM, FONT_XS,
    FONT_XXS, ForgePalette, Icon, InputEvent, OverlayPosition, Radius, ResizeEdge, ResizeRange,
    Spacing, TextInput, badge, confirm_modal, empty_state, fmt_clock, hash_accent, icon,
    install_resize, overlay, radius, slider, spacing, status_dot, tooltip_builder, tr,
};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use forge_speak_queue::{Priority, RequestId, SpeakCommand, SpeakQueueHandle, SpeakRequest};
use forge_storage::SettingsRepo;
use forge_tts_core::{TtsRegistry, TtsVoice};
use gpui::{
    Animation, AnimationExt, AnyElement, ClickEvent, Context, Entity, FontWeight, Pixels, Rgba,
    SharedString, Subscription, Window, bounce, div, ease_in_out, prelude::*, px, relative,
};

use crate::presentation::ActivePresentation;
use crate::speak_state::{NowSpeaking, QueueItem, SessionStats, SpeakState};

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
const VOLUME_GLYPH: Pixels = px(14.0);

const NOW_HEADER_MB: Pixels = px(8.0);
const NOW_ROW_GAP: Pixels = px(10.0);
const NOW_TILE: Pixels = px(32.0);
const NOW_TILE_RADIUS: Pixels = px(8.0);
const NOW_TILE_GLYPH: Pixels = px(16.0);
const NOW_NAME_GAP: Pixels = px(6.0);
const NOW_NAME_MB: Pixels = px(2.0);
const NOW_NAME_FONT: Pixels = px(12.5);
const NOW_PILL_FONT: Pixels = px(9.5);
const NOW_MSG_FONT: Pixels = px(12.0);
const NOW_MSG_LINE_HEIGHT: f32 = 1.45;
const PROGRESS_GAP: Pixels = px(8.0);
const PROGRESS_MT: Pixels = px(7.0);
const PROGRESS_FONT: Pixels = px(10.0);
const PROGRESS_BAR_H: Pixels = px(3.0);
const PROGRESS_BAR_RADIUS: Pixels = px(2.0);

const QUEUE_ROW_PAD_V: Pixels = px(9.0);
const QUEUE_ROW_GAP: Pixels = px(10.0);
const QUEUE_INDEX_FONT: Pixels = px(11.0);
const QUEUE_GRIP_GLYPH: Pixels = px(13.0);
const QUEUE_NAME_GAP: Pixels = px(6.0);
const QUEUE_NAME_FONT: Pixels = px(12.0);
const QUEUE_PREVIEW_FONT: Pixels = px(10.0);
const QUEUE_MSG_FONT: Pixels = px(11.0);
const QUEUE_DUR_FONT: Pixels = px(10.0);
const QUEUE_ACTION_GLYPH: Pixels = px(13.0);

const TOOLBAR_PAD_V: Pixels = px(9.0);
const BTN_PAD_V: Pixels = px(5.0);
const SOLID_BTN_PAD_H: Pixels = px(12.0);
const GHOST_BTN_PAD_H: Pixels = px(11.0);
const BTN_GAP: Pixels = px(5.0);
const STRIP_GAP: Pixels = px(8.0);
const VOL_GAP: Pixels = px(7.0);
const VOL_PCT_FONT: Pixels = px(11.0);
const VOL_PCT_W: Pixels = px(32.0);
const TEST_PAD_V: Pixels = px(4.0);
const TEST_PAD_H: Pixels = px(10.0);
const TEST_GAP: Pixels = px(6.0);
const TEST_ICON: Pixels = px(12.0);
const TEST_FONT: Pixels = px(11.5);
const BTN_FONT: Pixels = px(12.0);
const NOW_PAD_V: Pixels = px(14.0);
const NOW_LABEL_FONT: Pixels = px(10.0);
const HEADER_PAD_V: Pixels = px(9.0);
const COUNT_BADGE_FONT: Pixels = px(10.0);
const RAIL_PAD: Pixels = px(14.0);
const RAIL_LABEL_FONT: Pixels = px(10.0);
const RAIL_LABEL_MB: Pixels = px(8.0);
const RAIL_SECTION_MT: Pixels = px(16.0);
const STAT_PAD_V: Pixels = px(5.0);
const STAT_LABEL_FONT: Pixels = px(11.5);
const STAT_VALUE_FONT: Pixels = px(13.0);
const ENGINE_PAD_V: Pixels = px(9.0);
const ENGINE_PAD_H: Pixels = px(11.0);
const ENGINE_NAME_FONT: Pixels = px(11.5);
const ENGINE_META_FONT: Pixels = px(10.0);
const ENGINE_NAME_MB: Pixels = px(3.0);

struct TtsRailResizeDrag;

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
    test_input: Entity<TextInput>,
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
                InputEvent::Cancelled => {}
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
            test_input,
            _test_sub: test_sub,
            _speak_obs: speak_obs,
        }
    }

    fn dispatch(&self, cmd: SpeakCommand) {
        let Some(handle) = self.speak.clone() else {
            eprintln!("forge-desktop: TTS command dropped - speak queue unavailable");
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

    fn play_now(&mut self, request_id: RequestId, _cx: &mut Context<Self>) {
        self.dispatch(SpeakCommand::PlayNow(request_id));
    }

    fn remove_queued(&mut self, request_id: RequestId, _cx: &mut Context<Self>) {
        self.dispatch(SpeakCommand::RemoveQueued(request_id));
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
        self.rt_handle.spawn(async move {
            if let Err(e) = forge_storage::set_master_volume(settings.as_ref(), volume).await {
                eprintln!("forge-desktop: persist master volume failed: {e}");
            }
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
        paused: bool,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (pause_label, pause_glyph, btn_bg) = if paused {
            (
                tr!("tts_dash_resume_btn"),
                Icon::PlayerPlay,
                palette.success,
            )
        } else {
            (tr!("tts_dash_pause_btn"), Icon::PlayerPause, palette.random)
        };
        let pause_btn = div()
            .id("tts-pause")
            .flex()
            .items_center()
            .gap(BTN_GAP)
            .py(BTN_PAD_V)
            .px(SOLID_BTN_PAD_H)
            .rounded(radius(Radius::Sm))
            .bg(btn_bg)
            .cursor_pointer()
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_pause(cx)))
            .child(icon(pause_glyph, PAUSE_GLYPH, palette.shell))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
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
        );
        let stop_btn = self.ghost_strip_button(
            "tts-stop",
            Icon::PlayerStop,
            palette.random,
            palette.random,
            tr!("tts_dash_stop_all_btn"),
            palette,
            cx.listener(|this, _: &ClickEvent, _, cx| this.arm_stop_all(cx)),
        );

        let divider = div()
            .w(px(1.0))
            .h(px(16.0))
            .mx(px(4.0))
            .bg(palette.border_regular);

        let vol_pct = (self.volume * 100.0).round() as u32;
        let vol_text = div()
            .w(VOL_PCT_W)
            .font_family(DEFAULT_MONO_FAMILY)
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
            .gap(VOL_GAP)
            .child(icon(Icon::Volume, VOLUME_GLYPH, palette.text_muted))
            .child(vol_slider)
            .child(vol_text);

        let left = div()
            .flex()
            .items_center()
            .gap(STRIP_GAP)
            .child(pause_btn)
            .child(skip_btn)
            .child(stop_btn)
            .child(divider)
            .child(volume_row);

        let test_field = div()
            .flex()
            .items_center()
            .gap(TEST_GAP)
            .py(TEST_PAD_V)
            .px(TEST_PAD_H)
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
            .gap(BTN_GAP)
            .py(BTN_PAD_V)
            .px(SOLID_BTN_PAD_H)
            .rounded(radius(Radius::Sm))
            .bg(palette.brand)
            .cursor_pointer()
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.speak_test(cx)))
            .child(icon(Icon::PlayerPlayFilled, SPEAK_BTN_GLYPH, palette.shell))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(BTN_FONT)
                    .text_color(palette.shell)
                    .child(tr!("tts_dash_speak_btn")),
            );
        let right = div()
            .flex()
            .items_center()
            .gap(STRIP_GAP)
            .child(test_field)
            .child(speak_btn);

        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .py(TOOLBAR_PAD_V)
            .px(spacing(Spacing::Md, Density::Cozy))
            .bg(palette.elevated)
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(left)
            .child(right)
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn ghost_strip_button(
        &self,
        id: &'static str,
        glyph: Icon,
        text_color: Rgba,
        hover_text: Rgba,
        label: impl Into<SharedString>,
        palette: &ForgePalette,
        handler: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
    ) -> AnyElement {
        let hover_border = palette.border_input;
        div()
            .id(id)
            .flex()
            .items_center()
            .gap(BTN_GAP)
            .py(BTN_PAD_V)
            .px(GHOST_BTN_PAD_H)
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
                    .font_family(DEFAULT_BODY_FAMILY)
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
        let latency = stats
            .avg_latency_ms
            .map(|ms| format!("{ms}ms"))
            .unwrap_or_else(|| "-".to_owned());

        let mut content = div()
            .flex()
            .flex_col()
            .child(rail_header(
                tr!("tts_dash_session_header"),
                px(0.0),
                palette,
            ))
            .child(stat_row(
                tr!("tts_dash_stat_spoken"),
                stats.spoken.to_string(),
                palette.brand,
                palette,
                true,
            ))
            .child(stat_row(
                tr!("tts_dash_stat_skipped"),
                stats.skipped.to_string(),
                palette.warning,
                palette,
                true,
            ))
            .child(stat_row(
                tr!("tts_dash_stat_filtered"),
                stats.filtered.to_string(),
                palette.random,
                palette,
                true,
            ))
            .child(stat_row(
                tr!("tts_dash_stat_avg_latency"),
                latency,
                palette.success,
                palette,
                false,
            ))
            .child(rail_header(
                tr!("tts_dash_engines_header"),
                RAIL_SECTION_MT,
                palette,
            ));

        if self.engines.is_empty() {
            content = content.child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
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
            content = content.child(engine_card(eng.name.clone(), meta, color, palette));
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
                    .p(RAIL_PAD)
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
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(FONT_XS)
                    .text_color(palette.text_primary)
                    .child(tr!("tts_dash_queue_header")),
            )
            .child(count_badge);
        let mut header = div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .py(HEADER_PAD_V)
            .px(spacing(Spacing::Md, density))
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(title);
        if !queue.is_empty() {
            let total: u32 = queue.iter().map(|item| item.duration_secs).sum();
            header = header.child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(tr!("tts_dash_queue_total", secs = total as i64)),
            );
        }

        let list: AnyElement = if queue.is_empty() {
            empty_state(tr!("tts_dash_queue_empty"), palette)
                .density(density)
                .into_any_element()
        } else {
            let mut col = div().w_full().flex().flex_col();
            for (index, item) in queue.iter().enumerate() {
                col = col.child(self.queue_item_row(index, item, palette, density, cx));
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
        &self,
        index: usize,
        item: &QueueItem,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pos = div()
            .w(QUEUE_POS_W)
            .flex_shrink_0()
            .text_center()
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(QUEUE_INDEX_FONT)
            .text_color(palette.text_faint)
            .child(format!("{}", index + 1));

        let grip = icon(
            Icon::GripVertical,
            QUEUE_GRIP_GLYPH,
            palette.text_extreme_faint,
        );

        let mut name_row = div().flex().items_center().gap(QUEUE_NAME_GAP).child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
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
            name_row = name_row.child(badge(palette.warning, palette.shell, label, true, FONT_XS));
        }
        if !item.engine_voice.is_empty() {
            name_row = name_row.child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
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
            .gap(spacing(Spacing::Xxs, density))
            .child(name_row)
            .child(
                div()
                    .truncate()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(QUEUE_MSG_FONT)
                    .text_color(palette.text_muted)
                    .child(item.text.clone()),
            );

        let duration = div()
            .flex_shrink_0()
            .font_family(DEFAULT_MONO_FAMILY)
            .text_size(QUEUE_DUR_FONT)
            .text_color(palette.text_faint)
            .child(format!("~{}", fmt_clock(u64::from(item.duration_secs))));

        let play_id = item.request_id.clone();
        let play_btn = div()
            .id(("tts-q-play", index))
            .flex_shrink_0()
            .cursor_pointer()
            .tooltip(tooltip_builder(tr!("tts_dash_play_now"), palette))
            .on_click(
                cx.listener(move |this, _: &ClickEvent, _, cx| this.play_now(play_id.clone(), cx)),
            )
            .child(icon(Icon::PlayerPlay, QUEUE_ACTION_GLYPH, palette.success));

        let remove_id = item.request_id.clone();
        let remove_btn = div()
            .id(("tts-q-remove", index))
            .flex_shrink_0()
            .cursor_pointer()
            .tooltip(tooltip_builder(tr!("tts_dash_remove_queued"), palette))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.remove_queued(remove_id.clone(), cx)
            }))
            .child(icon(Icon::X, QUEUE_ACTION_GLYPH, palette.text_faint));

        div()
            .w_full()
            .flex()
            .items_center()
            .gap(QUEUE_ROW_GAP)
            .py(QUEUE_ROW_PAD_V)
            .px(spacing(Spacing::Md, density))
            .border_b(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(pos)
            .child(grip)
            .child(content)
            .child(duration)
            .child(play_btn)
            .child(remove_btn)
            .into_any_element()
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

        let control_strip = self.control_strip(paused, &palette, cx);
        let now_speaking = now_speaking_panel(
            now.as_ref(),
            now_voice,
            last_drop.as_deref(),
            paused,
            &palette,
            density,
        );
        let queue_section = self.queue_section(&queue, &palette, density, cx);
        let right_pane = self.right_pane(&stats, &palette, cx);

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
                    .with_easing(bounce(ease_in_out)),
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
    Some(format!("{} \u{b7} {}", engine_label(engine_id), name).into())
}

fn now_speaking_panel(
    now: Option<&NowSpeaking>,
    now_voice: Option<SharedString>,
    last_drop: Option<&str>,
    paused: bool,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    let animate = now.is_some() && !paused;
    let bar_color = if animate {
        palette.success
    } else {
        palette.text_faint
    };
    let header = div()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xs, density))
        .mb(NOW_HEADER_MB)
        .child(
            div()
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(NOW_LABEL_FONT)
                .text_color(palette.text_muted)
                .child(tr!("tts_dash_now_speaking_header")),
        )
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
                .gap(NOW_NAME_GAP)
                .mb(NOW_NAME_MB)
                .child(
                    div()
                        .font_family(DEFAULT_BODY_FAMILY)
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
                .font_family(DEFAULT_BODY_FAMILY)
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
                .gap(PROGRESS_GAP)
                .mt(PROGRESS_MT)
                .child(
                    div()
                        .font_family(DEFAULT_MONO_FAMILY)
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
                        .font_family(DEFAULT_MONO_FAMILY)
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

            div().flex().flex_col().child(header).child(
                div()
                    .flex()
                    .items_start()
                    .gap(NOW_ROW_GAP)
                    .child(tile)
                    .child(info),
            )
        }
        None => {
            let mut idle = div()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Xs, density))
                .child(header)
                .child(
                    div()
                        .font_family(DEFAULT_BODY_FAMILY)
                        .text_size(FONT_SM)
                        .text_color(palette.text_muted)
                        .child(tr!("tts_dash_no_speaking")),
                );
            if let Some(reason) = last_drop {
                idle = idle.child(
                    div()
                        .font_family(DEFAULT_MONO_FAMILY)
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
        .py(NOW_PAD_V)
        .px(spacing(Spacing::Md, density))
        .bg(palette.elevated)
        .border_b(BORDER_THIN)
        .border_color(palette.border_regular)
        .child(body)
        .into_any_element()
}

fn rail_header(
    label: impl Into<SharedString>,
    margin_top: Pixels,
    palette: &ForgePalette,
) -> impl IntoElement {
    div()
        .mt(margin_top)
        .mb(RAIL_LABEL_MB)
        .font_family(DEFAULT_MONO_FAMILY)
        .text_size(RAIL_LABEL_FONT)
        .text_color(palette.text_muted)
        .child(label.into())
}

fn stat_row(
    label: impl Into<SharedString>,
    value: String,
    value_color: Rgba,
    palette: &ForgePalette,
    border_bottom: bool,
) -> impl IntoElement {
    let mut row = div()
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .py(STAT_PAD_V)
        .child(
            div()
                .flex_1()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(STAT_LABEL_FONT)
                .text_color(palette.text_muted)
                .child(label.into()),
        )
        .child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
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
                        .font_family(DEFAULT_BODY_FAMILY)
                        .font_weight(FontWeight::MEDIUM)
                        .text_size(ENGINE_NAME_FONT)
                        .text_color(palette.text_primary)
                        .child(name),
                )
                .child(status_dot(status_color, ENGINE_DOT)),
        )
        .child(
            div()
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(ENGINE_META_FONT)
                .text_color(palette.text_faint)
                .child(meta),
        );
    div().mb(px(6.0)).child(
        div()
            .py(ENGINE_PAD_V)
            .px(ENGINE_PAD_H)
            .rounded(radius(Radius::Md))
            .bg(palette.elevated)
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .child(body),
    )
}

fn test_speak_request(text: String, speaker_name: String) -> SpeakRequest {
    SpeakRequest {
        request_id: RequestId::new(),
        viewer_id: String::new(),
        viewer_name: speaker_name,
        text,
        priority: Priority::Normal,
        alias_override: None,
        engine_override: None,
        voice_override: None,
        source_event_id: forge_types::EventId::new(),
        is_reward: false,
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
            name: engine_label(&id.0),
            meta: engine_kind(&id.0).to_owned(),
            warn: false,
            id: id.0,
        })
        .collect()
}

fn engine_label(id: &str) -> String {
    match id {
        "piper" => "Piper",
        "espeak-ng" => "eSpeak-NG",
        "sapi" => "Microsoft SAPI 5",
        "nsspeech" => "Apple AVSpeech",
        "azure" => "Azure Speech",
        "elevenlabs" => "ElevenLabs",
        "openai" => "OpenAI TTS",
        "polly" => "Amazon Polly",
        other => return other.to_owned(),
    }
    .to_owned()
}

fn engine_kind(id: &str) -> &'static str {
    match id {
        "piper" | "espeak-ng" => "local",
        "sapi" | "nsspeech" => "system",
        _ => "cloud",
    }
}
