mod now_panel;
mod panes;

use now_panel::{
    engine_card, now_speaking_panel, resolve_now_voice, section_heading, stat_row, strip_divider,
};

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
    ForgePalette, Icon, InputEvent, KeyActivated, OverlayPosition, Radius, ResizeEdge, ResizeRange,
    Spacing, TextInput, badge, body_family, card, chip, confirm_modal, empty_state, fmt_clock,
    hash_accent, icon, install_resize, mono_family, overlay, radius, scaled, section_label, slider,
    spacing, status_dot, tooltip_builder, tr, with_alpha,
};
use std::sync::Arc;
use std::time::Duration;

use forge_components::OverflowRevealingFocus;
use forge_speak_queue::{
    EmoteTokenSet, Priority, RequestId, SpeakCommand, SpeakQueueHandle, SpeakRequest,
};
use forge_speech_types::TtsVoice;
use forge_storage::SettingsRepo;
use forge_tts_core::engine_display_name;
use gpui::{
    Animation, AnimationExt, AnyElement, ClickEvent, Context, CursorStyle, Entity, FontWeight,
    Pixels, Rgba, SharedString, Subscription, Window, bounce, div, ease_in_out, prelude::*, px,
    relative,
};

use crate::async_bridge;
use crate::speak_state::{NowSpeaking, QueueItem, SessionStats, SpeakState};
use crate::tts_engine_roster::{RosterEngine, engine_class_label, load_engine_roster};
use forge_components::ActivePresentation;
use forge_components::{PushToast, ToastKind};

const NO_LATENCY_READING: &str = "-";
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
const EQ_BAR_RADIUS: Pixels = px(1.0);
const EQ_BAR_BASE_PERIOD_SECS: f32 = 0.5;
const EQ_BAR_PERIOD_STEP_SECS: f32 = 0.15;
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

pub(crate) struct TtsDashboardView {
    speak_state: Entity<SpeakState>,
    speak: Option<SpeakQueueHandle>,
    settings: Arc<dyn SettingsRepo>,
    rt_handle: tokio::runtime::Handle,
    volume: f32,
    rail_width: Pixels,
    engines: Vec<RosterEngine>,
    pending_stop_all: bool,
    dragged_row: Option<RequestId>,
    test_input: Entity<TextInput>,
    volume_debounce: async_bridge::Debounced,
    _test_sub: Subscription,
    _speak_obs: Subscription,
}

impl TtsDashboardView {
    pub(crate) fn new(
        speak_state: Entity<SpeakState>,
        speak: Option<SpeakQueueHandle>,
        settings: Arc<dyn SettingsRepo>,
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
            engines: load_engine_roster(speak.as_ref()),
            speak,
            settings,
            rt_handle,
            volume,
            rail_width: RAIL_DEFAULT_W,
            pending_stop_all: false,
            dragged_row: None,
            test_input,
            volume_debounce: async_bridge::Debounced::new(async_bridge::SLIDER_PERSIST_DEBOUNCE),
            _test_sub: test_sub,
            _speak_obs: speak_obs,
        }
    }

    pub(crate) fn refresh_engine_roster(&mut self, cx: &mut Context<Self>) {
        self.engines = load_engine_roster(self.speak.as_ref());
        cx.notify();
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

    fn dispatch_confirmed(
        &self,
        cmd: SpeakCommand,
        accepted: impl FnOnce(&mut SpeakState) + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        let Some(handle) = self.speak.clone() else {
            cx.push_toast(ToastKind::Error, tr!("tts_dashboard_queue_unavailable"));
            return;
        };
        async_bridge::run_async(
            &self.rt_handle,
            async move { handle.send(cmd).await.map_err(|error| error.to_string()) },
            move |this, result, cx| match result {
                Ok(()) => this.speak_state.update(cx, |state, cx| {
                    accepted(state);
                    cx.notify();
                }),
                Err(error) => {
                    tracing::error!(%error, "TTS command dispatch failed");
                    cx.push_toast(ToastKind::Error, tr!("tts_dashboard_queue_unavailable"));
                }
            },
            cx,
        );
    }

    fn toggle_pause(&mut self, cx: &mut Context<Self>) {
        let paused = self.speak_state.read(cx).manual_paused();
        let cmd = if paused {
            SpeakCommand::Resume
        } else {
            SpeakCommand::Pause
        };
        self.dispatch_confirmed(cmd, move |state| state.set_manual_paused(!paused), cx);
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
        self.dispatch_confirmed(SpeakCommand::Clear, SpeakState::clear_all, cx);
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
}

impl Render for TtsDashboardView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let manual_paused = self.speak_state.read(cx).manual_paused();
        let gate_held = self.speak_state.read(cx).gate_held();
        let now = self.speak_state.read(cx).now_speaking_snapshot();
        let queue = self.speak_state.read(cx).queue_snapshot();
        let stats = self.speak_state.read(cx).stats_snapshot();
        let last_drop = self
            .speak_state
            .read(cx)
            .last_drop()
            .map(|reason| crate::user_text::resolve_user_text(reason).inline());

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
            now.as_deref(),
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
            .overflow_y_revealing_focus()
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

#[cfg(test)]
mod tests {
    use forge_speak_queue::{SpeakEvent, SpeakEventStream};
    use forge_storage::Language;
    use gpui::TestAppContext;

    use super::*;
    use crate::test_support::{
        LiveAliases, error_toasts, install_presentation, pump_until, runtime, spawn_speak_queue,
        test_backend,
    };

    struct Queue {
        handle: SpeakQueueHandle,
        _events: SpeakEventStream,
        _aliases: LiveAliases,
    }

    fn live_queue(rt: &tokio::runtime::Runtime) -> Queue {
        let (handle, _events, _aliases) = rt.block_on(async { spawn_speak_queue(Vec::new()) });
        Queue {
            handle,
            _events,
            _aliases,
        }
    }

    fn queue_that_refuses() -> Queue {
        let actor_runtime = runtime();
        live_queue(&actor_runtime)
    }

    struct Dashboard {
        view: Entity<TtsDashboardView>,
        state: Entity<SpeakState>,
    }

    fn dashboard(
        speak: Option<SpeakQueueHandle>,
        rt: &tokio::runtime::Runtime,
        cx: &mut TestAppContext,
    ) -> Dashboard {
        crate::i18n::install_language(Language::En);
        install_presentation(cx);
        cx.update(|cx| cx.set_global(forge_components::Toasts::new()));
        let (backend, _writes) = test_backend();
        let state = cx.new(|_| SpeakState::new());
        let shared = state.clone();
        let handle = rt.handle().clone();
        let view = cx.new(|cx| {
            TtsDashboardView::new(shared, speak, backend as Arc<dyn SettingsRepo>, handle, cx)
        });
        Dashboard { view, state }
    }

    impl Dashboard {
        fn paused(&self, cx: &mut TestAppContext) -> bool {
            self.state.read_with(cx, |state, _| state.manual_paused())
        }

        fn queued(&self, cx: &mut TestAppContext) -> usize {
            self.state
                .read_with(cx, |state, _| state.queue_snapshot().len())
        }

        fn seat_one_line(&self, cx: &mut TestAppContext) {
            self.state.update(cx, |state, _| {
                state.apply_event(SpeakEvent::Enqueued {
                    request_id: RequestId::new(),
                    queue_len: 1,
                    viewer_name: "viewer".to_owned(),
                    text: "hello".to_owned(),
                    is_high_priority: false,
                    voice_preview: "voice".to_owned(),
                    estimated_secs: 1,
                });
            });
        }

        fn wait_for_a_refusal(&self, rt: &tokio::runtime::Runtime, cx: &mut TestAppContext) {
            pump_until(rt, || {
                cx.run_until_parked();
                cx.update(|cx| error_toasts(cx)) == 1
            });
        }
    }

    #[gpui::test]
    fn pause_shows_only_once_the_queue_took_the_command(cx: &mut TestAppContext) {
        let rt = runtime();
        let queue = live_queue(&rt);
        let dashboard = dashboard(Some(queue.handle.clone()), &rt, cx);

        dashboard.view.update(cx, |view, cx| view.toggle_pause(cx));
        let paused_before_the_answer = dashboard.paused(cx);
        pump_until(&rt, || {
            cx.run_until_parked();
            dashboard.paused(cx)
        });

        assert!(!paused_before_the_answer);
    }

    #[gpui::test]
    fn a_refused_pause_leaves_the_state_and_reports_it(cx: &mut TestAppContext) {
        let rt = runtime();
        let queue = queue_that_refuses();
        let dashboard = dashboard(Some(queue.handle.clone()), &rt, cx);

        dashboard.view.update(cx, |view, cx| view.toggle_pause(cx));
        dashboard.wait_for_a_refusal(&rt, cx);

        assert!(!dashboard.paused(cx));
    }

    #[gpui::test]
    fn pause_with_no_queue_at_all_is_reported_and_changes_nothing(cx: &mut TestAppContext) {
        let rt = runtime();
        let dashboard = dashboard(None, &rt, cx);

        dashboard.view.update(cx, |view, cx| view.toggle_pause(cx));

        assert_eq!(
            (dashboard.paused(cx), cx.update(|cx| error_toasts(cx))),
            (false, 1)
        );
    }

    #[gpui::test]
    fn stop_all_empties_the_rows_only_once_the_queue_took_the_command(cx: &mut TestAppContext) {
        let rt = runtime();
        let queue = live_queue(&rt);
        let dashboard = dashboard(Some(queue.handle.clone()), &rt, cx);
        dashboard.seat_one_line(cx);

        dashboard
            .view
            .update(cx, |view, cx| view.confirm_stop_all(cx));
        let queued_before_the_answer = dashboard.queued(cx);
        pump_until(&rt, || {
            cx.run_until_parked();
            dashboard.queued(cx) == 0
        });

        assert_eq!(queued_before_the_answer, 1);
    }

    #[gpui::test]
    fn a_refused_stop_all_keeps_the_rows_and_reports_it(cx: &mut TestAppContext) {
        let rt = runtime();
        let queue = queue_that_refuses();
        let dashboard = dashboard(Some(queue.handle.clone()), &rt, cx);
        dashboard.seat_one_line(cx);

        dashboard
            .view
            .update(cx, |view, cx| view.confirm_stop_all(cx));
        dashboard.wait_for_a_refusal(&rt, cx);

        assert_eq!(dashboard.queued(cx), 1);
    }
}
