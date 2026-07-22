use std::sync::Arc;
use std::time::{Duration, Instant};

use forge_components::{
    DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_LG, FONT_SM, ForgePalette, Icon,
    Radius, Spacing, card, icon, primary_button, radius, spacing, tr,
};
use forge_platform_core::CONNECTION_STATE_CHANGED_KIND;
use forge_runtime::{EventBus, LiveViewerAggregatorHandle};
use forge_types::{ChatModerationAction, ChatModerationPayload};
use futures_util::StreamExt as _;
use gpui::{
    AnyElement, App, AppContext, AsyncApp, Context, Entity, Window, WindowHandle, div, prelude::*,
};

use crate::async_bridge::{BridgeFlow, drain_events};
use crate::boot::{BootFailure, build_runtime};
use crate::chat_feed::{ChatFeed, ChatMessage, chat_source, platform_of};
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
                // Not `Clone` - take the sole subscription out so the bridge below owns
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
                let backend_for_shortcuts = Arc::clone(&handles.backend);
                let rt_handle_for_shortcuts = handles.rt_handle.clone();
                let chat_feed_for_history = chat_feed.clone();
                let backend_for_history = Arc::clone(&handles.backend);
                let rt_handle_for_history = handles.rt_handle.clone();
                let applied = window.update(cx, |root, window, cx| {
                    // Render-thread install: the fluent bundle is thread-local and must be set
                    // before the shell's first render resolves any translated string.
                    crate::i18n::install_language(handles.startup_language);
                    cx.set_global(crate::presentation::ActiveLanguage(
                        handles.startup_language,
                    ));
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
                    seed_chat_history(
                        cx,
                        chat_feed_for_history,
                        backend_for_history,
                        rt_handle_for_history,
                    )
                    .await;
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
                    apply_persisted_shortcuts(cx, backend_for_shortcuts, rt_handle_for_shortcuts);
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

const DEFAULT_CHAT_HISTORY_DISPLAY_LIMIT: u32 = 500;

async fn seed_chat_history(
    cx: &mut AsyncApp,
    chat_feed: Entity<ChatFeed>,
    backend: Arc<dyn forge_storage::DataProvider>,
    rt_handle: tokio::runtime::Handle,
) {
    let (tx, rx) = tokio::sync::oneshot::channel();
    rt_handle.spawn(async move {
        let settings = Arc::clone(&backend) as Arc<dyn forge_storage::SettingsRepo>;
        let limit = forge_storage::chat_history_display_limit(settings.as_ref())
            .await
            .unwrap_or(DEFAULT_CHAT_HISTORY_DISPLAY_LIMIT);
        let rows = backend
            .chat_history_repo()
            .list_recent(limit as usize)
            .await
            .unwrap_or_default();
        let _ = tx.send(rows);
    });
    let Ok(mut rows) = rx.await else {
        return;
    };
    // Repo yields newest-first; the feed is oldest-first.
    rows.reverse();
    let messages: Vec<ChatMessage> = rows.iter().map(ChatMessage::from_row).collect();
    chat_feed.update(cx, |feed, cx| {
        feed.seed(messages);
        cx.notify();
    });
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
        drain_events(&bus, cx, move |batch, cx| {
            platforms.update(cx, |connectivity, cx| {
                let mut changed = false;
                for event in batch {
                    if event.kind == CONNECTION_STATE_CHANGED_KIND
                        && let Some(integ) = event
                            .payload
                            .get("platform_id")
                            .and_then(|v| v.as_str())
                            .and_then(Integration::from_id)
                    {
                        let connected = event.payload.get("state").and_then(|v| v.as_str())
                            == Some("connected");
                        changed |= connectivity.set_connected(integ, connected);
                    }
                }
                if changed {
                    cx.notify();
                }
            });

            event_log.update(cx, |log, cx| {
                let mut pushed = false;
                for event in batch {
                    if let Some(item) = EventLog::item_from_event(event) {
                        log.push(item);
                        pushed = true;
                    }
                }
                if pushed {
                    cx.notify();
                }
            });

            home_stats.update(cx, |stats, cx| {
                let mut changed = false;
                for event in batch {
                    changed |= stats.record_event(event);
                    if event.kind == "action.done" {
                        stats.record_action_done();
                        changed = true;
                    }
                }
                if changed {
                    cx.notify();
                }
            });

            chat_feed.update(cx, |feed, cx| {
                let mut changed = false;
                for event in batch {
                    if let Some(message) = ChatFeed::message_from_event(event) {
                        feed.push(message);
                        changed = true;
                    }
                    if let Some(value) = event.payload.get(ChatModerationPayload::KEY)
                        && let Ok(payload) =
                            serde_json::from_value::<ChatModerationPayload>(value.clone())
                        && let Some(platform) = chat_source(event.source).map(platform_of)
                    {
                        match payload.action {
                            ChatModerationAction::DeleteMessage { message_id } => {
                                feed.mark_deleted(&message_id)
                            }
                            ChatModerationAction::RemoveUser { user_name, .. } => {
                                feed.mark_user(platform, &user_name)
                            }
                            ChatModerationAction::ClearChat => feed.clear_platform(platform),
                        }
                        changed = true;
                    }
                    if event.kind == "command.matched"
                        && let Some(caused_by) = event.caused_by
                        && let Some(command) = event.payload.get("command").and_then(|v| v.as_str())
                    {
                        feed.mark_command(caused_by, command);
                        changed = true;
                    }
                    if event.kind == "action.start"
                        && let Some(caused_by) = event.caused_by
                        && let Some(action_name) =
                            event.payload.get("action_name").and_then(|v| v.as_str())
                    {
                        feed.set_triggered(caused_by, action_name);
                        changed = true;
                    }
                }
                if changed {
                    cx.notify();
                }
            });

            queue_health.update(cx, |health, cx| {
                let mut changed = false;
                for event in batch {
                    changed |= health.apply_event(event);
                }
                if changed {
                    cx.notify();
                }
            });

            BridgeFlow::Continue
        })
        .await;
    })
    .detach();
}

fn start_uptime_clock(cx: &mut AsyncApp, status: Entity<RuntimeStatus>) {
    cx.spawn(async move |cx| {
        loop {
            cx.background_executor().timer(Duration::from_secs(1)).await;
            status.update(cx, |status, cx| {
                status.refresh(Instant::now());
                cx.notify();
            });
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
            home_stats.update(cx, |stats, cx| {
                if stats.set_live_viewers(count) {
                    cx.notify();
                }
            });
        }
    })
    .detach();
}

fn apply_persisted_shortcuts(
    cx: &mut AsyncApp,
    backend: Arc<dyn forge_storage::DataProvider>,
    rt_handle: tokio::runtime::Handle,
) {
    cx.spawn(async move |cx| {
        let repo = backend as Arc<dyn forge_storage::SettingsRepo>;
        let (tx, rx) = tokio::sync::oneshot::channel();
        rt_handle.spawn(async move {
            let raw = repo
                .get_string(forge_storage::reserved_keys::KEYBOARD_SHORTCUTS)
                .await;
            let _ = tx.send(raw);
        });
        let Ok(Ok(Some(raw))) = rx.await else {
            return;
        };
        let overrides = crate::actions::parse_stored_overrides(&raw);
        if overrides.is_empty() {
            return;
        }
        cx.update(|cx| crate::actions::reapply_key_bindings(cx, &overrides));
    })
    .detach();
}

fn start_speak_bridge(cx: &mut AsyncApp, speak: Entity<SpeakState>, mut events: SpeakEventStream) {
    cx.spawn(async move |cx| {
        while let Ok(event) = events.recv().await {
            speak.update(cx, |state, cx| {
                if state.apply_event(event) {
                    cx.notify();
                }
            });
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
                .child(tr!("boot_starting")),
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
                .child(tr!("boot_upgrade_title")),
        )
        .child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
                .text_color(palette.text_secondary)
                .child(tr!(
                    "boot_upgrade_body",
                    found = found as i64,
                    expected = expected as i64
                )),
        )
        .child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
                .text_color(palette.text_muted)
                .child(tr!("boot_upgrade_reassure")),
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

    let retry = primary_button(tr!("boot_retry"), palette).on_click(
        "boot-retry",
        move |_, _window, cx: &mut App| {
            root.update(cx, |root, cx| root.retry(cx));
        },
    );

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
                .child(tr!("boot_failure_title")),
        )
        .child(detail)
        .child(
            div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
                .text_color(palette.text_muted)
                .child(tr!("boot_failure_reassure")),
        )
        .child(retry);
    centered(card(body, palette), palette, density)
}
