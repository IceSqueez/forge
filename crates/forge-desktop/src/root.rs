use std::sync::Arc;
use std::time::{Duration, Instant};

use forge_components::{
    DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_LG, FONT_SM, ForgePalette, Icon,
    Radius, Spacing, card, icon, primary_button, radius, spacing,
};
use forge_events::EventsError;
use forge_platform_core::CONNECTION_STATE_CHANGED_KIND;
use forge_runtime::{EventBus, LiveViewerAggregatorHandle};
use futures_util::StreamExt as _;
use gpui::{
    AnyElement, App, AppContext, AsyncApp, Context, Entity, Window, WindowHandle, div, prelude::*,
};

use crate::boot::{BootFailure, build_runtime};
use crate::chat_feed::ChatFeed;
use crate::event_log::EventLog;
use crate::globals::Globals;
use crate::home_stats::{HomeStats, Integration};
use crate::platforms::PlatformConnectivity;
use crate::presentation::{ActivePresentation, Presentation};
use crate::queue_health::QueueHealth;
use crate::runtime_handles::RuntimeHandles;
use crate::runtime_status::RuntimeStatus;
use crate::shell::AppShell;
use crate::speak_state::SpeakState;
use crate::topics::Topics;
use forge_speak_queue::SpeakEventStream;

enum BootState {
    Booting,
    Ready {
        shell: Entity<AppShell>,
        // Held (unread) to keep the runtime's tasks alive for the app's lifetime; the
        // shell holds a second `Arc` clone through which screens reach the runtime.
        #[allow(dead_code)]
        handles: Arc<RuntimeHandles>,
    },
    Failed(BootFailure),
}

pub struct RootView {
    state: BootState,
    rt_handle: tokio::runtime::Handle,
    window: Option<WindowHandle<RootView>>,
}

impl RootView {
    pub fn new(rt_handle: tokio::runtime::Handle, cx: &mut Context<Self>) -> Self {
        cx.observe_global::<Presentation>(|_, cx| cx.notify())
            .detach();
        Self {
            state: BootState::Booting,
            rt_handle,
            window: None,
        }
    }

    pub fn set_window(&mut self, window: WindowHandle<RootView>) {
        self.window = Some(window);
    }

    fn mark_ready(&mut self, shell: Entity<AppShell>, handles: Arc<RuntimeHandles>) {
        self.state = BootState::Ready { shell, handles };
    }

    fn mark_failed(&mut self, failure: BootFailure) {
        self.state = BootState::Failed(failure);
    }

    fn retry(&mut self, cx: &mut Context<Self>) {
        let Some(window) = self.window else {
            return;
        };
        let rt_handle = self.rt_handle.clone();
        self.state = BootState::Booting;
        cx.notify();
        run_boot(rt_handle, window, cx);
    }
}

pub fn run_boot(rt_handle: tokio::runtime::Handle, window: WindowHandle<RootView>, cx: &mut App) {
    let status = cx.new(|_| RuntimeStatus::new());
    let chat_feed = cx.new(|_| ChatFeed::new());
    let home_stats = cx.new(|_| HomeStats::new());
    let event_log = cx.new(|_| EventLog::new());
    let globals = cx.new(|_| Globals::empty());
    let platforms = cx.new(|_| PlatformConnectivity::new());
    let speak = cx.new(|_| SpeakState::new());
    let queue_health = cx.new(|_| QueueHealth::new());

    let (result_tx, result_rx) =
        tokio::sync::oneshot::channel::<Result<RuntimeHandles, BootFailure>>();
    rt_handle.spawn(async move {
        let _ = result_tx.send(build_runtime().await);
    });

    cx.spawn(async move |cx| {
        let outcome = match result_rx.await {
            Ok(outcome) => outcome,
            Err(_) => Err(BootFailure::Retry {
                reason: "runtime construction task ended before reporting a result".to_owned(),
            }),
        };
        match outcome {
            Ok(mut handles) => {
                // Not `Clone` — take the sole subscription out so the bridge below owns
                // the only drain.
                let speak_events = handles.speak_events.take();
                let handles = Arc::new(handles);
                let bus = Arc::clone(&handles.bus);
                let handles_for_shell = Arc::clone(&handles);
                let status_for_clock = status.clone();
                let chat_feed_for_bridge = chat_feed.clone();
                let home_stats_for_bridge = home_stats.clone();
                let home_stats_for_viewers = home_stats.clone();
                let event_log_for_bridge = event_log.clone();
                let platforms_for_bridge = platforms.clone();
                let queue_health_for_bridge = queue_health.clone();
                let speak_for_bridge = speak.clone();
                let live_viewers_handle = handles.live_viewers.clone();
                let applied = window.update(cx, |root, window, cx| {
                    platforms.update(cx, |connectivity, cx| {
                        connectivity.seed_from_builtins(&handles.builtins);
                        cx.notify();
                    });
                    let topics = Topics::new(
                        chat_feed,
                        home_stats,
                        event_log,
                        globals,
                        platforms,
                        speak,
                        queue_health,
                    );
                    let shell =
                        cx.new(|cx| AppShell::new(status, topics, handles_for_shell, window, cx));
                    root.mark_ready(shell, handles);
                    cx.notify();
                });
                if applied.is_ok() {
                    start_bridge(
                        cx,
                        chat_feed_for_bridge,
                        home_stats_for_bridge,
                        event_log_for_bridge,
                        platforms_for_bridge,
                        queue_health_for_bridge,
                        bus,
                    );
                    start_uptime_clock(cx, status_for_clock);
                    start_live_viewers_bridge(cx, home_stats_for_viewers, live_viewers_handle);
                    if let Some(events) = speak_events {
                        start_speak_bridge(cx, speak_for_bridge, events);
                    }
                }
            }
            Err(failure) => {
                let _ = window.update(cx, |root, _window, cx| {
                    root.mark_failed(failure);
                    cx.notify();
                });
            }
        }
    })
    .detach();
}

fn start_bridge(
    cx: &mut AsyncApp,
    chat_feed: Entity<ChatFeed>,
    home_stats: Entity<HomeStats>,
    event_log: Entity<EventLog>,
    platforms: Entity<PlatformConnectivity>,
    queue_health: Entity<QueueHealth>,
    bus: Arc<EventBus>,
) {
    cx.spawn(async move |cx| {
        let mut subscription = bus.subscribe();
        loop {
            match subscription.recv().await {
                Ok(event) => {
                    if event.kind == CONNECTION_STATE_CHANGED_KIND
                        && let Some(integ) = event
                            .payload
                            .get("platform_id")
                            .and_then(|v| v.as_str())
                            .and_then(Integration::from_id)
                    {
                        let connected = event.payload.get("state").and_then(|v| v.as_str())
                            == Some("connected");
                        if platforms
                            .update(cx, |connectivity, cx| {
                                if connectivity.set_connected(integ, connected) {
                                    cx.notify();
                                }
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                    if let Some(item) = EventLog::item_from_event(&event)
                        && event_log
                            .update(cx, |log, cx| {
                                log.push(item);
                                cx.notify();
                            })
                            .is_err()
                    {
                        break;
                    }
                    let is_action_done = event.kind == "action.done";
                    if home_stats
                        .update(cx, |stats, cx| {
                            let mut changed = stats.record_event(&event);
                            if is_action_done {
                                stats.record_action_done();
                                changed = true;
                            }
                            if changed {
                                cx.notify();
                            }
                        })
                        .is_err()
                    {
                        break;
                    }
                    if let Some(message) = ChatFeed::message_from_event(&event)
                        && chat_feed
                            .update(cx, |feed, cx| {
                                feed.push(message);
                                cx.notify();
                            })
                            .is_err()
                    {
                        break;
                    }
                    if queue_health
                        .update(cx, |health, cx| {
                            if health.apply_event(&event) {
                                cx.notify();
                            }
                        })
                        .is_err()
                    {
                        break;
                    }
                }
                Err(EventsError::LaggingReceiver) => {}
                Err(_) => break,
            }
        }
    })
    .detach();
}

fn start_uptime_clock(cx: &mut AsyncApp, status: Entity<RuntimeStatus>) {
    cx.spawn(async move |cx| {
        loop {
            cx.background_executor().timer(Duration::from_secs(1)).await;
            if status
                .update(cx, |status, cx| {
                    status.refresh(Instant::now());
                    cx.notify();
                })
                .is_err()
            {
                break;
            }
        }
    })
    .detach();
}

fn start_live_viewers_bridge(
    cx: &mut AsyncApp,
    home_stats: Entity<HomeStats>,
    handle: LiveViewerAggregatorHandle,
) {
    cx.spawn(async move |cx| {
        let mut stream = std::pin::pin!(handle.subscribe());
        while let Some(count) = stream.next().await {
            if home_stats
                .update(cx, |stats, cx| {
                    if stats.set_live_viewers(count) {
                        cx.notify();
                    }
                })
                .is_err()
            {
                break;
            }
        }
    })
    .detach();
}

fn start_speak_bridge(cx: &mut AsyncApp, speak: Entity<SpeakState>, mut events: SpeakEventStream) {
    cx.spawn(async move |cx| {
        while let Ok(event) = events.recv().await {
            if speak
                .update(cx, |state, cx| {
                    if state.apply_event(event) {
                        cx.notify();
                    }
                })
                .is_err()
            {
                break;
            }
        }
    })
    .detach();
}

impl Render for RootView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();
        match &self.state {
            BootState::Booting => splash(&palette, density),
            BootState::Ready { shell, .. } => {
                div().size_full().child(shell.clone()).into_any_element()
            }
            BootState::Failed(BootFailure::UpgradeRequired { expected, found }) => {
                upgrade_screen(*expected, *found, &palette, density)
            }
            BootState::Failed(BootFailure::Retry { reason }) => {
                retry_screen(reason, cx.entity(), &palette, density)
            }
        }
    }
}

fn centered(child: impl IntoElement, palette: &ForgePalette, density: Density) -> AnyElement {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(palette.base)
        .p(spacing(Spacing::Lg, density))
        .child(child)
        .into_any_element()
}

fn splash(palette: &ForgePalette, density: Density) -> AnyElement {
    let column = div()
        .flex()
        .flex_col()
        .items_center()
        .gap(spacing(Spacing::Sm, density))
        .child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_LG)
                .text_color(palette.brand)
                .child("forge"),
        )
        .child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
                .text_color(palette.text_muted)
                .child("Starting…"),
        );
    centered(column, palette, density)
}

fn upgrade_screen(
    expected: u32,
    found: u32,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    let body = div()
        .flex()
        .flex_col()
        .items_center()
        .gap(spacing(Spacing::Md, density))
        .child(icon(Icon::AlertTriangle, FONT_LG, palette.warning))
        .child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_LG)
                .text_color(palette.text_primary)
                .child("Update required"),
        )
        .child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
                .text_color(palette.text_secondary)
                .child(format!(
                    "Your forge data uses schema version {found}, newer than this build's version {expected}. Update forge to the latest release to open it."
                )),
        )
        .child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
                .text_color(palette.text_muted)
                .child("Your data is safe and untouched."),
        );
    centered(card(body, palette), palette, density)
}

fn retry_screen(
    reason: &str,
    root: Entity<RootView>,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    let detail = div()
        .w_full()
        .p(spacing(Spacing::Sm, density))
        .rounded(radius(Radius::Sm))
        .bg(palette.surface_overlay)
        .font_family(DEFAULT_MONO_FAMILY)
        .text_size(FONT_SM)
        .text_color(palette.text_secondary)
        .child(reason.to_owned());

    let retry =
        primary_button("Retry", palette).on_click("boot-retry", move |_, _window, cx: &mut App| {
            root.update(cx, |root, cx| root.retry(cx));
        });

    let body = div()
        .flex()
        .flex_col()
        .items_center()
        .gap(spacing(Spacing::Md, density))
        .child(icon(Icon::AlertTriangle, FONT_LG, palette.warning))
        .child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_LG)
                .text_color(palette.text_primary)
                .child("Couldn't open your data"),
        )
        .child(detail)
        .child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
                .text_color(palette.text_muted)
                .child("Your data is safe. If this keeps happening, please report it."),
        )
        .child(retry);
    centered(card(body, palette), palette, density)
}
