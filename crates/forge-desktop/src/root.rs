use std::sync::Arc;
use std::time::{Duration, Instant};

use forge_awake::StatusWatch;
use forge_components::{
    Density, FONT_LG, FONT_SM, ForgePalette, Icon, Radius, Spacing, ToastKind, Toasts, body_family,
    card, icon, mono_family, primary_button, radius, secondary_button, spacing, toast_layer, tr,
};
use forge_platform_core::{CONNECTION_STATE_CHANGED_KIND, PlatformEndpoints};
use forge_runtime::{EventSubscription, LiveViewerAggregatorHandle, LossWatch, QueueDepthWatch};
use forge_storage::CredentialsKeyLoss;
use forge_storage::paths;
use futures_util::StreamExt as _;
use gpui::{
    AnyElement, App, AppContext, AsyncApp, Context, Entity, Window, WindowHandle, div, prelude::*,
};

use crate::async_bridge::{BridgeFlow, drain_subscription};
use crate::awake_state::AwakeState;
use crate::boot::{BootFailure, build_runtime};
use crate::chat_feed::{ChatFeed, ChatMessage, DEFAULT_DISPLAY_LIMIT};
use crate::chat_feed_bridge::ChatFeedBridge;
use crate::event_log::EventLog;
use crate::event_loss::{EventLoss, UI_EVENTS};
use crate::globals::Globals;
use crate::home_stats::{HomeStats, Integration};
use crate::integration_lifecycle::IntegrationLifecycle;
use crate::log_tail::LogTail;
use crate::platforms::PlatformConnectivity;
use crate::problem_report::ProblemReport;
use crate::queue_health::QueueHealth;
use crate::runtime_handles::RuntimeHandles;
use crate::runtime_status::RuntimeStatus;
use crate::screen::Screen;
use crate::shell::AppShell;
use crate::speak_state::SpeakState;
use crate::system_info::WindowFacts;
use crate::topics::Topics;
use crate::twitch_bot_presence::{TwitchBotPresence, start_twitch_bot_seed};
use crate::update_flow::UpdateFlow;
use crate::window_presence::PresenceGate;
use forge_components::PushToast;
use forge_components::{ActivePresentation, Presentation};
use forge_speak_queue::{SpeakError, SpeakEventStream};

const LOSS_REPAINT_INTERVAL: Duration = Duration::from_millis(250);
const QUEUE_DEPTH_REPAINT_INTERVAL: Duration = Duration::from_millis(250);
const UPTIME_TICK: Duration = Duration::from_secs(1);

enum BootState {
    Booting,
    Ready { shell: Entity<AppShell> },
    Failed(BootFailure),
}

pub struct RootView {
    state: BootState,
    rt_handle: tokio::runtime::Handle,
    log_tail: LogTail,
    endpoints: PlatformEndpoints,
    initial_screen: Screen,
    window: Option<WindowHandle<RootView>>,
}

impl RootView {
    pub fn new(
        rt_handle: tokio::runtime::Handle,
        log_tail: LogTail,
        endpoints: PlatformEndpoints,
        initial_screen: Screen,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe_global::<Presentation>(|_, cx| cx.notify())
            .detach();
        cx.observe_global::<Toasts>(|root, cx| {
            if matches!(root.state, BootState::Failed(_)) {
                cx.notify();
            }
        })
        .detach();
        Self {
            state: BootState::Booting,
            rt_handle,
            log_tail,
            endpoints,
            initial_screen,
            window: None,
        }
    }

    pub fn set_window(&mut self, window: WindowHandle<RootView>) {
        self.window = Some(window);
    }

    fn mark_ready(&mut self, shell: Entity<AppShell>) {
        self.state = BootState::Ready { shell };
    }

    fn mark_failed(&mut self, failure: BootFailure) {
        self.state = BootState::Failed(failure);
    }

    fn retry(&mut self, cx: &mut Context<Self>) {
        let Some(window) = self.window else {
            return;
        };
        let rt_handle = self.rt_handle.clone();
        let log_tail = self.log_tail.clone();
        let endpoints = self.endpoints.clone();
        let initial_screen = self.initial_screen.clone();
        self.state = BootState::Booting;
        cx.notify();
        run_boot(
            rt_handle,
            log_tail,
            endpoints,
            initial_screen,
            None,
            window,
            cx,
        );
    }
}

pub fn run_boot(
    rt_handle: tokio::runtime::Handle,
    log_tail: LogTail,
    endpoints: PlatformEndpoints,
    initial_screen: Screen,
    preopened: Option<Arc<dyn forge_storage::DataProvider>>,
    window: WindowHandle<RootView>,
    cx: &mut App,
) {
    let status = cx.new(|_| RuntimeStatus::new());
    let chat_feed = cx.new(|_| ChatFeed::new());
    let home_stats = cx.new(|_| HomeStats::new());
    let event_log = cx.new(|_| EventLog::new());
    let globals = cx.new(|_| Globals::empty());
    let platforms = cx.new(|_| PlatformConnectivity::new());
    mirror_connections_into_home(&platforms, home_stats.clone(), cx);
    let speak = cx.new(|_| SpeakState::new());
    let queue_health = cx.new(|_| QueueHealth::new());
    let event_loss = cx.new(|_| EventLoss::new());
    let awake = cx.new(|_| AwakeState::default());
    let twitch_bot = cx.new(|_| TwitchBotPresence::new());

    let (hotkey_host, hotkey_main_thread) = forge_hotkey::main_thread_channel();
    cx.spawn(async move |_| hotkey_host.run().await).detach();

    let (result_tx, result_rx) =
        tokio::sync::oneshot::channel::<Result<RuntimeHandles, BootFailure>>();
    rt_handle.spawn(async move {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the view may close before the result arrives"
        )]
        let _ =
            result_tx.send(build_runtime(log_tail, endpoints, hotkey_main_thread, preopened).await);
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
                let speak_events = handles.speak_events.take();
                let overlay_build_failure = handles.overlay_build_failure.take();
                let initial_screen = handles.first_run.opening_screen(initial_screen);
                let handles = Arc::new(handles);
                let bridge_sub = handles.bus.subscribe_observer(UI_EVENTS);
                let chat_feed_bridge = ChatFeedBridge::subscribe(&handles.bus, &handles.rt_handle);
                let loss_watch = handles.bus.watch_loss();
                let depth_watch = handles.scheduler.watch_depths();
                let event_loss_for_bridge = event_loss.clone();
                let awake_for_bridge = awake.clone();
                let awake_watch = handles.stay_awake.watch();
                let event_loss_for_chat = event_loss.clone();
                let handles_for_shell = Arc::clone(&handles);
                let status_for_clock = status.clone();
                let home_stats_for_bridge = home_stats.clone();
                let home_stats_for_viewers = home_stats.clone();
                let event_log_for_bridge = event_log.clone();
                let platforms_for_bridge = platforms.clone();
                let twitch_bot_for_bridge = twitch_bot.clone();
                let twitch_bot_for_seed = twitch_bot.clone();
                let twitch_bot_handle = handles.twitch_bot.clone();
                let rt_handle_for_bot_seed = handles.rt_handle.clone();
                let queue_health_for_bridge = queue_health.clone();
                let queue_health_for_depths = queue_health.clone();
                let speak_for_bridge = speak.clone();
                let live_viewers_handle = handles.live_viewers.clone();
                let notification_policy = handles.notification_policy.clone();
                let rt_handle_for_notices = handles.rt_handle.clone();
                let scheduled_backups_for_notices = handles.scheduled_backups.clone();
                let settings_for_backup_notices =
                    Arc::clone(&handles.backend) as Arc<dyn forge_storage::SettingsRepo>;
                let backend_for_shortcuts = Arc::clone(&handles.backend);
                let rt_handle_for_shortcuts = handles.rt_handle.clone();
                let chat_feed_for_history = chat_feed.clone();
                let backend_for_history = Arc::clone(&handles.backend);
                let rt_handle_for_history = handles.rt_handle.clone();
                let backend_for_updates = Arc::clone(&handles.backend);
                let rt_handle_for_updates = handles.rt_handle.clone();
                let mut flow_for_updates = None;
                let mut report_for_crashes = None;
                let mut window_facts = None;
                let credentials_key_loss = handles.credentials_key_loss;
                let applied = window.update(cx, |root, window, cx| {
                    crate::i18n::install_language(handles.startup_language);
                    cx.set_global(crate::presentation::ActiveLanguage(
                        handles.startup_language,
                    ));
                    forge_components::install_display_formats(
                        handles.startup_display_formats.resolve(),
                    );
                    cx.set_global(crate::presentation::ActiveDisplayFormats(
                        handles.startup_display_formats,
                    ));
                    if let Some(reason) = handles.server_unavailable.clone() {
                        cx.set_global(crate::server_unavailable::ServerUnavailable(reason.into()));
                    }
                    platforms.update(cx, |connectivity, cx| {
                        connectivity.seed_from_builtins(&handles.builtins);
                        cx.notify();
                    });
                    crate::donation_services::mirror_donation_connectivity(
                        &handles.donation_services,
                        &platforms,
                        cx,
                    );
                    let integration_lifecycle = cx
                        .new(|_| IntegrationLifecycle::new(handles.integrations.watch().current()));
                    let update_flow = cx.new(|_| {
                        UpdateFlow::new(
                            Arc::clone(&handles.backend),
                            handles.rt_handle.clone(),
                            paths::data_dir(),
                        )
                        .with_exclusivity(handles.transfer_exclusivity.clone())
                        .with_scheduled_backups(handles.scheduled_backups.clone())
                    });
                    flow_for_updates = Some(update_flow.clone());
                    let problem_report = cx.new(|_| {
                        ProblemReport::new(
                            Arc::clone(&handles.backend),
                            handles.builtins.clone(),
                            handles.rt_handle.clone(),
                        )
                    });
                    report_for_crashes = Some(problem_report.clone());
                    window_facts = Some(WindowFacts::read(window, cx));
                    let topics = Topics::new(
                        chat_feed,
                        home_stats,
                        event_log,
                        globals,
                        platforms,
                        speak,
                        queue_health,
                        event_loss,
                        awake,
                        integration_lifecycle,
                        update_flow,
                        problem_report,
                        twitch_bot,
                    );
                    let shell = cx.new(|cx| {
                        AppShell::new(
                            status,
                            topics,
                            handles_for_shell,
                            initial_screen,
                            window,
                            cx,
                        )
                    });
                    let mut shutdown =
                        Some(crate::shutdown::ShutdownHandles::from_handles(&handles));
                    let stay_awake = handles.stay_awake.clone();
                    let mut scheduled_backups = Some(handles.scheduled_backups.clone());
                    App::on_app_quit(cx, move |cx| {
                        stay_awake.release();
                        let taken = shutdown.take();
                        let backups = scheduled_backups.take();
                        let teardown = crate::shutdown::QuitTeardown::claim(cx);
                        let exit = crate::update_exit::ExitHandoff::claim_at_quit(cx);
                        async move {
                            let backups_stopped = async {
                                if let Some(backups) = backups {
                                    crate::shutdown::stop_scheduled_backups(backups).await;
                                }
                            };
                            let graceful = async {
                                if let Some(handles) = taken {
                                    handles.run_graceful().await;
                                }
                            };
                            tokio::join!(backups_stopped, graceful);
                            if let Some(exit) = exit {
                                exit.finish();
                            }
                            teardown.finish();
                        }
                    })
                    .detach();
                    root.mark_ready(shell);
                    cx.notify();
                });
                if let (Ok(()), Some(update_flow), Some(problem_report)) =
                    (applied, flow_for_updates, report_for_crashes)
                {
                    cx.update(crate::boot_restore::raise_boot_restore_toast);
                    if let Some(loss) = credentials_key_loss {
                        cx.update(|cx| raise_credentials_key_loss_toast(loss, cx));
                    }
                    if let Some(failure) = overlay_build_failure {
                        cx.update(|cx| failure.raise(cx));
                    }
                    seed_chat_history(
                        cx,
                        chat_feed_for_history.clone(),
                        backend_for_history,
                        rt_handle_for_history,
                    )
                    .await;
                    start_loss_bridge(cx, event_loss_for_bridge, loss_watch);
                    start_queue_depth_bridge(cx, queue_health_for_depths, depth_watch);
                    chat_feed_bridge.start(cx, chat_feed_for_history, event_loss_for_chat);
                    start_bridge(
                        cx,
                        home_stats_for_bridge,
                        event_log_for_bridge,
                        platforms_for_bridge,
                        queue_health_for_bridge,
                        twitch_bot_for_bridge,
                        bridge_sub,
                    );
                    if let Some(bot) = twitch_bot_handle {
                        start_twitch_bot_seed(
                            cx,
                            twitch_bot_for_seed,
                            bot,
                            &rt_handle_for_bot_seed,
                        );
                    }
                    start_uptime_clock(cx, status_for_clock);
                    start_awake_bridge(cx, awake_for_bridge, awake_watch);
                    start_live_viewers_bridge(cx, home_stats_for_viewers, live_viewers_handle);
                    crate::system_notices::start_system_notices(
                        cx,
                        &notification_policy,
                        Arc::new(forge_runtime::sub_action_runners::SystemNotifyPort),
                        rt_handle_for_notices.clone(),
                    );
                    crate::scheduled_backup_notices::start_scheduled_backup_notices(
                        cx,
                        &scheduled_backups_for_notices,
                        settings_for_backup_notices,
                        Arc::new(forge_runtime::sub_action_runners::SystemNotifyPort),
                        rt_handle_for_notices,
                    );
                    apply_persisted_shortcuts(cx, backend_for_shortcuts, rt_handle_for_shortcuts);
                    let boot_report = crate::update_report::start_update_report(
                        cx,
                        Arc::clone(&backend_for_updates),
                        rt_handle_for_updates.clone(),
                    );
                    crate::update_leftover::start_leftover_install_notice(
                        cx,
                        Arc::clone(&backend_for_updates),
                        rt_handle_for_updates.clone(),
                    );
                    crate::problem_report::start_crash_notice(
                        cx,
                        problem_report,
                        rt_handle_for_updates.clone(),
                    );
                    let rt_handle_for_snapshot = rt_handle_for_updates.clone();
                    crate::update_check::start_update_check(
                        cx,
                        backend_for_updates,
                        rt_handle_for_updates,
                        boot_report,
                        update_flow,
                    );
                    if let Some(window_facts) = window_facts {
                        cx.spawn(async move |_| {
                            crate::crash_hook::snapshot_system_info(
                                window_facts,
                                &rt_handle_for_snapshot,
                            )
                            .await;
                            #[cfg(debug_assertions)]
                            crate::crash_hook::panic_if_requested();
                        })
                        .detach();
                    }
                    if let Some(events) = speak_events {
                        start_speak_bridge(cx, speak_for_bridge, events);
                    }
                }
            }
            Err(failure) => {
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "the view may close before the result arrives"
                )]
                let _ = window.update(cx, |root, _window, cx| {
                    root.mark_failed(failure);
                    cx.notify();
                });
                cx.update(crate::boot_restore::raise_boot_restore_toast);
            }
        }
    })
    .detach();
}

fn raise_credentials_key_loss_toast(loss: CredentialsKeyLoss, cx: &mut App) {
    let count = i64::try_from(loss.stranded).unwrap_or(i64::MAX);
    cx.push_toast_full(
        ToastKind::Error,
        tr!("credentials_key_loss_toast", count = count),
        None,
        None,
        Duration::ZERO,
    );
}

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
            .unwrap_or(DEFAULT_DISPLAY_LIMIT);
        let rows = backend
            .chat_history_repo()
            .list_recent(limit as usize)
            .await
            .unwrap_or_default();
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the view may close before the result arrives"
        )]
        let _ = tx.send((limit, rows));
    });
    let Ok((limit, mut rows)) = rx.await else {
        return;
    };
    rows.reverse();
    let messages: Vec<ChatMessage> = rows.iter().map(ChatMessage::from_row).collect();
    chat_feed.update(cx, |feed, cx| {
        feed.set_capacity(limit as usize);
        feed.seed(messages);
        cx.notify();
    });
}

fn mirror_connections_into_home(
    platforms: &Entity<PlatformConnectivity>,
    home_stats: Entity<HomeStats>,
    cx: &mut App,
) {
    cx.observe(platforms, move |platforms, cx| {
        let connections = platforms.read(cx).connections().to_vec();
        home_stats.update(cx, |stats, cx| {
            if stats.set_connections(&connections) {
                cx.notify();
            }
        });
    })
    .detach();
}

fn start_bridge(
    cx: &mut AsyncApp,
    home_stats: Entity<HomeStats>,
    event_log: Entity<EventLog>,
    platforms: Entity<PlatformConnectivity>,
    queue_health: Entity<QueueHealth>,
    twitch_bot: Entity<TwitchBotPresence>,
    sub: EventSubscription,
) {
    cx.spawn(async move |cx| {
        drain_subscription(sub, cx, move |batch, cx| {
            platforms.update(cx, |connectivity, cx| {
                let mut changed = false;
                for event in batch {
                    changed |= connectivity.apply_obs_event(event);
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

            queue_health.update(cx, |health, cx| {
                let mut changed = false;
                for event in batch {
                    changed |= health.apply_event(event);
                }
                if changed {
                    cx.notify();
                }
            });

            twitch_bot.update(cx, |presence, cx| {
                let mut changed = false;
                for event in batch {
                    changed |= presence.apply_event(event);
                }
                if changed {
                    cx.notify();
                }
            });

            cx.update(|cx| {
                for event in batch {
                    crate::clip_playback::report_clip_playback(event, cx);
                    crate::overlay_speech_failure::report_overlay_speech_failure(event, cx);
                }
            });

            BridgeFlow::Continue
        })
        .await;
    })
    .detach();
}

fn start_loss_bridge(cx: &mut AsyncApp, loss: Entity<EventLoss>, mut watch: LossWatch) {
    cx.spawn(async move |cx| {
        let mut report = watch.current();
        loop {
            loss.update(cx, |loss, cx| {
                if loss.apply_report(report) {
                    cx.notify();
                }
            });
            cx.background_executor().timer(LOSS_REPAINT_INTERVAL).await;
            match watch.changed().await {
                Some(next) => report = next,
                None => break,
            }
        }
    })
    .detach();
}

fn start_queue_depth_bridge(
    cx: &mut AsyncApp,
    health: Entity<QueueHealth>,
    mut watch: QueueDepthWatch,
) {
    cx.spawn(async move |cx| {
        let mut depths = watch.current();
        loop {
            health.update(cx, |health, cx| {
                if health.apply_depths(depths) {
                    cx.notify();
                }
            });
            cx.background_executor()
                .timer(QUEUE_DEPTH_REPAINT_INTERVAL)
                .await;
            match watch.changed().await {
                Some(next) => depths = next,
                None => break,
            }
        }
    })
    .detach();
}

fn start_awake_bridge(cx: &mut AsyncApp, awake: Entity<AwakeState>, mut watch: StatusWatch) {
    cx.spawn(async move |cx| {
        let mut status = watch.current();
        loop {
            awake.update(cx, |awake, cx| {
                if awake.apply(status) {
                    cx.notify();
                }
            });
            match watch.changed().await {
                Ok(next) => status = next,
                Err(_) => break,
            }
        }
    })
    .detach();
}

fn start_uptime_clock(cx: &mut AsyncApp, status: Entity<RuntimeStatus>) {
    let mut presence = cx.update(|cx| PresenceGate::of(cx));
    cx.spawn(async move |cx| {
        loop {
            cx.background_executor().timer(UPTIME_TICK).await;
            presence.until_visible().await;
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
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the view may close before the result arrives"
            )]
            let _ = tx.send(crate::shortcut_overrides::load_overrides(repo).await);
        });
        let overrides = match rx.await {
            Ok(Ok(overrides)) => overrides,
            Ok(Err(e)) => {
                tracing::warn!(error = %e, "stored keyboard shortcuts unreadable; defaults stay active");
                return;
            }
            Err(_) => return,
        };
        if overrides.is_empty() {
            return;
        }
        cx.update(|cx| overrides.apply(cx));
    })
    .detach();
}

fn start_speak_bridge(cx: &mut AsyncApp, speak: Entity<SpeakState>, mut events: SpeakEventStream) {
    cx.spawn(async move |cx| {
        loop {
            match events.recv().await {
                Ok(event) => {
                    speak.update(cx, |state, cx| {
                        if state.apply_event(event) {
                            cx.notify();
                        }
                    });
                }
                Err(SpeakError::LaggingReceiver) => {
                    tracing::warn!("speak event bridge lagged; dropped events");
                }
                Err(SpeakError::ActorGone) => break,
            }
        }
    })
    .detach();
}

impl Render for RootView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();
        let failure_screen = match &self.state {
            BootState::Booting => return splash(&palette, density),
            BootState::Ready { shell } => {
                return div().size_full().child(shell.clone()).into_any_element();
            }
            BootState::Failed(BootFailure::UpgradeRequired { expected, found }) => {
                upgrade_screen(*expected, *found, &palette, density)
            }
            BootState::Failed(BootFailure::PreBaseline { found }) => {
                pre_baseline_screen(*found, &palette, density)
            }
            BootState::Failed(BootFailure::Retry { reason }) => {
                retry_screen(reason, cx.entity(), &palette, density)
            }
        };
        div()
            .size_full()
            .relative()
            .child(failure_screen)
            .children(toast_layer(&palette, cx))
            .into_any_element()
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
                .font_family(body_family())
                .text_size(FONT_LG)
                .text_color(palette.brand)
                .child("forge"),
        )
        .child(
            div()
                .font_family(body_family())
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
                .font_family(body_family())
                .text_size(FONT_LG)
                .text_color(palette.text_primary)
                .child(tr!("boot_upgrade_title")),
        )
        .child(
            div()
                .font_family(body_family())
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
                .font_family(body_family())
                .text_size(FONT_SM)
                .text_color(palette.text_muted)
                .child(tr!("boot_upgrade_reassure")),
        );
    centered(card(body, palette), palette, density)
}

fn pre_baseline_screen(found: u32, palette: &ForgePalette, density: Density) -> AnyElement {
    let actions = div()
        .flex()
        .gap(spacing(Spacing::Sm, density))
        .child(
            secondary_button(tr!("boot_pre_baseline_open_folder"), palette)
                .on_click("boot-open-data-folder", |_, _window, cx: &mut App| {
                    cx.reveal_path(&paths::data_dir())
                }),
        )
        .child(
            primary_button(tr!("boot_pre_baseline_quit"), palette)
                .on_click("boot-quit", |_, _window, cx: &mut App| cx.quit()),
        );
    let body = div()
        .flex()
        .flex_col()
        .items_center()
        .gap(spacing(Spacing::Md, density))
        .child(icon(Icon::AlertTriangle, FONT_LG, palette.warning))
        .child(
            div()
                .font_family(body_family())
                .text_size(FONT_LG)
                .text_color(palette.text_primary)
                .child(tr!("boot_pre_baseline_title")),
        )
        .child(
            div()
                .font_family(body_family())
                .text_size(FONT_SM)
                .text_color(palette.text_secondary)
                .child(tr!(
                    "boot_pre_baseline_body",
                    found = found as i64,
                    release = forge_storage::LAST_PRE_BASELINE_RELEASE
                )),
        )
        .child(
            div()
                .font_family(body_family())
                .text_size(FONT_SM)
                .text_color(palette.text_muted)
                .child(tr!("boot_pre_baseline_reassure")),
        )
        .child(actions);
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
        .font_family(mono_family())
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
                .font_family(body_family())
                .text_size(FONT_LG)
                .text_color(palette.text_primary)
                .child(tr!("boot_failure_title")),
        )
        .child(detail)
        .child(
            div()
                .font_family(body_family())
                .text_size(FONT_SM)
                .text_color(palette.text_muted)
                .child(tr!("boot_failure_reassure")),
        )
        .child(retry);
    centered(card(body, palette), palette, density)
}

#[cfg(test)]
mod tests {
    use forge_types::{Queue, QueueId};
    use gpui::TestAppContext;

    use super::*;
    use crate::test_support::{
        held_queue_scheduler, hold_one_dispatch, pump, runtime, test_backend, writes_to,
    };

    fn queue(id: QueueId) -> Queue {
        Queue {
            id,
            name: "Default".to_owned(),
            description: String::new(),
            concurrency: 1,
        }
    }

    fn pending_in(
        health: &Entity<QueueHealth>,
        id: QueueId,
        cx: &mut TestAppContext,
    ) -> Option<usize> {
        health.read_with(cx, |health, _| health.depth(id).map(|depth| depth.pending))
    }

    fn mirrored(cx: &mut TestAppContext) -> (Entity<PlatformConnectivity>, Entity<HomeStats>) {
        let platforms = cx.new(|_| PlatformConnectivity::new());
        let home = cx.new(|_| HomeStats::new());
        cx.update(|cx| mirror_connections_into_home(&platforms, home.clone(), cx));
        (platforms, home)
    }

    fn home_connections(
        home: &Entity<HomeStats>,
        cx: &mut TestAppContext,
    ) -> Vec<(Integration, bool)> {
        home.read_with(cx, |home, _| home.enabled_connections(|_| true).connections)
    }

    #[gpui::test]
    fn a_platform_connection_change_reaches_the_home_strip_in_roster_order(
        cx: &mut TestAppContext,
    ) {
        let (platforms, home) = mirrored(cx);

        platforms.update(cx, |connectivity, cx| {
            connectivity.set_connected(Integration::Obs, true);
            cx.notify();
        });
        cx.run_until_parked();

        assert_eq!(
            home_connections(&home, cx),
            vec![
                (Integration::Twitch, false),
                (Integration::YouTube, false),
                (Integration::Kick, false),
                (Integration::Obs, true),
                (Integration::VTube, false),
            ]
        );
    }

    #[gpui::test]
    fn a_builtin_seed_reaches_the_home_strip(cx: &mut TestAppContext) {
        let (platforms, home) = mirrored(cx);
        platforms.update(cx, |connectivity, cx| {
            connectivity.set_connected(Integration::Twitch, true);
            cx.notify();
        });
        cx.run_until_parked();
        let before_seed =
            home.read_with(cx, |home, _| home.enabled_connections(|_| true).connected);

        platforms.update(cx, |connectivity, cx| {
            connectivity.seed_from_builtins(&crate::integrations::BuiltinRegistry::default());
            cx.notify();
        });
        cx.run_until_parked();
        let after_seed = home.read_with(cx, |home, _| home.enabled_connections(|_| true).connected);

        assert_eq!((before_seed, after_seed), (1, 0));
    }

    #[gpui::test]
    fn the_depth_bridge_carries_every_later_depth_into_queue_health(cx: &mut TestAppContext) {
        let rt = runtime();
        let id = QueueId::new();
        let scheduler = held_queue_scheduler(&rt, queue(id));
        hold_one_dispatch(&rt, &scheduler, id);
        let health = cx.new(|_| QueueHealth::new());

        start_queue_depth_bridge(&mut cx.to_async(), health.clone(), scheduler.watch_depths());
        cx.run_until_parked();
        let at_start = pending_in(&health, id, cx);
        hold_one_dispatch(&rt, &scheduler, id);
        cx.executor().advance_clock(QUEUE_DEPTH_REPAINT_INTERVAL);
        cx.run_until_parked();

        assert_eq!((at_start, pending_in(&health, id, cx)), (Some(1), Some(2)));
    }
    const FAILURE_WINDOW_W: f32 = 1000.0;
    const FAILURE_WINDOW_H: f32 = 700.0;

    #[gpui::test]
    fn a_pre_baseline_failure_renders_its_screen_and_keeps_the_failed_state(
        cx: &mut TestAppContext,
    ) {
        crate::test_support::install_presentation(cx);
        cx.update(|cx| cx.set_global(Toasts::new()));
        let rt = runtime();
        let handle = rt.handle().clone();
        let (view, vcx) = cx.add_window_view(move |_window, cx| {
            RootView::new(
                handle,
                LogTail::new(),
                PlatformEndpoints::default(),
                Screen::Home,
                cx,
            )
        });
        view.update(vcx, |root, cx| {
            root.mark_failed(BootFailure::PreBaseline { found: 7 });
            cx.notify();
        });

        vcx.simulate_resize(gpui::size(
            gpui::px(FAILURE_WINDOW_W),
            gpui::px(FAILURE_WINDOW_H),
        ));
        vcx.run_until_parked();

        view.read_with(vcx, |root, _| {
            assert!(matches!(
                root.state,
                BootState::Failed(BootFailure::PreBaseline { found: 7 })
            ));
        });
    }

    #[test]
    fn the_pre_baseline_body_names_the_release_to_open_and_the_found_version_in_every_language() {
        for language in [forge_storage::Language::En, forge_storage::Language::Uk] {
            crate::i18n::install_language(language);

            let body = tr!(
                "boot_pre_baseline_body",
                found = 7_i64,
                release = forge_storage::LAST_PRE_BASELINE_RELEASE
            );

            assert!(
                body.contains(forge_storage::LAST_PRE_BASELINE_RELEASE),
                "{language:?}: {body}"
            );
            assert!(body.contains('7'), "{language:?}: {body}");
        }
    }

    const BOOT_SETTLE_ROUNDS: usize = 3;

    #[gpui::test]
    fn boot_with_an_undecodable_stored_shortcut_value_leaves_it_unwritten(cx: &mut TestAppContext) {
        let key = forge_storage::reserved_keys::KEYBOARD_SHORTCUTS;
        let rt = runtime();
        let (backend, mut writes) = test_backend();
        backend.seed(key, "not json");

        apply_persisted_shortcuts(
            &mut cx.to_async(),
            backend as Arc<dyn forge_storage::DataProvider>,
            rt.handle().clone(),
        );
        for _ in 0..BOOT_SETTLE_ROUNDS {
            pump(&rt);
            cx.run_until_parked();
        }

        assert_eq!(writes_to(&mut writes, key), Vec::<String>::new());
    }
}
