use std::sync::Arc;
use std::time::{Duration, Instant};

use forge_awake::StatusWatch;
use forge_components::{
    Density, FONT_LG, FONT_SM, FONT_XXS, ForgePalette, Icon, Radius, Spacing, ToastKind, Toasts,
    card, icon, primary_button, radius, secondary_button, spacing, toast_layer, tr,
};
use forge_events::EventSubscription;
use forge_platform_core::PlatformEndpoints;
use forge_runtime::{LiveViewerAggregatorHandle, LossWatch, QueueDepthWatch};
use forge_storage::CredentialsKeyLoss;
use futures_util::StreamExt as _;
use gpui::{
    AnyElement, App, AppContext, AsyncApp, Context, Entity, Window, WindowHandle, div, prelude::*,
};

use crate::async_bridge::{BridgeFlow, drain_subscription};
use crate::awake_state::AwakeState;
use crate::boot::{BootFailure, RuntimeInputs, build_runtime};
use crate::chat_feed::{ChatFeed, ChatMessage, DEFAULT_DISPLAY_LIMIT};
use crate::chat_feed_bridge::ChatFeedBridge;
use crate::event_categories::EventCategories;
use crate::event_log::EventLog;
use crate::event_loss::{EventLoss, UI_EVENTS};
use crate::globals::Globals;
use crate::home_stats::HomeStats;
use crate::integration_lifecycle::IntegrationLifecycle;
use crate::local_tts_engines::discover_local_tts_engines;
use crate::log_tail::LogTail;
use crate::navigation::{Navigator, Screen};
use crate::overlay_build_failure::OverlayBuildFailure;
use crate::platforms::PlatformConnectivity;
use crate::problem_report::ProblemReport;
use crate::queue_health::QueueHealth;
use crate::repo_failure::RepoFailure;
use crate::runtime_event::{self, RuntimeEvent};
use crate::runtime_handles::RuntimeHandles;
use crate::runtime_status::RuntimeStatus;
use crate::shell::AppShell;
use crate::speak_state::SpeakState;
use crate::system_info::WindowFacts;
use crate::topics::Topics;
use crate::twitch_bot_presence::{TwitchBotPresence, start_twitch_bot_seed};
use crate::update_flow::{GithubArtifactDownloader, UpdateFlow};
use crate::window_presence::PresenceGate;
use forge_components::PushToast;
use forge_components::{ActivePresentation, Presentation};
use forge_host::boot_warnings::BootWarnings;
use forge_host::platform_identities::PlatformIdentities;
use forge_speak_queue::{SpeakError, SpeakEventStream};

const LOSS_REPAINT_INTERVAL: Duration = Duration::from_millis(250);
const QUEUE_DEPTH_REPAINT_INTERVAL: Duration = Duration::from_millis(250);
const UPTIME_TICK: Duration = Duration::from_secs(1);

enum BootState {
    Booting,
    Ready { shell: Entity<AppShell> },
    Failed(BootFailure),
}

#[derive(Clone)]
pub(crate) struct BootInputs {
    pub(crate) rt_handle: tokio::runtime::Handle,
    pub(crate) log_tail: LogTail,
    pub(crate) endpoints: PlatformEndpoints,
    pub(crate) initial_screen: Screen,
}

pub(crate) struct RootView {
    state: BootState,
    inputs: BootInputs,
    window: Option<WindowHandle<RootView>>,
}

impl RootView {
    pub(crate) fn new(inputs: BootInputs, cx: &mut Context<Self>) -> Self {
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
            inputs,
            window: None,
        }
    }

    pub(crate) fn set_window(&mut self, window: WindowHandle<RootView>) {
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
        let inputs = self.inputs.clone();
        self.state = BootState::Booting;
        cx.notify();
        run_boot(inputs, None, window, cx);
    }
}

#[derive(Clone)]
struct BootEntities {
    status: Entity<RuntimeStatus>,
    chat_feed: Entity<ChatFeed>,
    home_stats: Entity<HomeStats>,
    event_log: Entity<EventLog>,
    globals: Entity<Globals>,
    platforms: Entity<PlatformConnectivity>,
    speak: Entity<SpeakState>,
    queue_health: Entity<QueueHealth>,
    event_loss: Entity<EventLoss>,
    awake: Entity<AwakeState>,
    twitch_bot: Entity<TwitchBotPresence>,
}

impl BootEntities {
    fn new(cx: &mut App) -> Self {
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
        Self {
            status,
            chat_feed,
            home_stats,
            event_log,
            globals,
            platforms,
            speak,
            queue_health,
            event_loss,
            awake,
            twitch_bot,
        }
    }
}

struct BootCaptures {
    speak_events: Option<SpeakEventStream>,
    overlay_build_failure: Option<OverlayBuildFailure>,
    boot_warnings: BootWarnings,
    credentials_key_loss: Option<CredentialsKeyLoss>,
    bridge_sub: EventSubscription,
    chat_feed_bridge: ChatFeedBridge,
    loss_watch: LossWatch,
    depth_watch: QueueDepthWatch,
    obs_stats_watch: forge_obs::ObsStatsWatch,
    awake_watch: StatusWatch,
    event_categories: EventCategories,
}

impl BootCaptures {
    fn take(handles: &mut RuntimeHandles) -> Self {
        Self {
            speak_events: handles.speech.speak_events.take(),
            overlay_build_failure: handles.overlay_build_failure.take(),
            boot_warnings: handles.boot_warnings.clone(),
            credentials_key_loss: handles.credentials_key_loss,
            bridge_sub: handles.core.bus.subscribe_observer(UI_EVENTS),
            chat_feed_bridge: ChatFeedBridge::subscribe(&handles.core.bus, &handles.rt_handle),
            loss_watch: handles.core.bus.watch_loss(),
            depth_watch: handles.automation.scheduler.watch_depths(),
            obs_stats_watch: handles.integrations.obs_install_seed.stream_stats(),
            awake_watch: handles.stay_awake.watch(),
            event_categories: EventCategories::from_registry(&handles.automation.trigger_registry),
        }
    }
}

struct InstalledShell {
    update_flow: Entity<UpdateFlow>,
    problem_report: Entity<ProblemReport>,
    window_facts: WindowFacts,
    navigator: Navigator,
}

pub(crate) fn run_boot(
    inputs: BootInputs,
    opening: Option<crate::boot::BackendOpening>,
    window: WindowHandle<RootView>,
    cx: &mut App,
) {
    let entities = BootEntities::new(cx);
    let BootInputs {
        rt_handle,
        log_tail,
        endpoints,
        initial_screen,
    } = inputs;

    let (hotkey_host, hotkey_main_thread) = forge_hotkey::main_thread_channel();
    cx.spawn(async move |_| hotkey_host.run().await).detach();

    let (result_tx, result_rx) =
        tokio::sync::oneshot::channel::<Result<RuntimeHandles, BootFailure>>();
    rt_handle.spawn(async move {
        let preopened = match opening {
            Some(opening) => opening.finished().await.map(Some),
            None => Ok(None),
        };
        let booted = match preopened {
            Ok(preopened) => {
                let inputs = RuntimeInputs {
                    log_tail,
                    endpoints,
                    hotkey_main_thread,
                    preopened,
                    audio_sinks: Arc::new(forge_audio::CpalSinkFactory),
                    local_tts_engines: discover_local_tts_engines().await,
                    platform_identities: PlatformIdentities::from_environment(),
                };
                build_runtime(inputs).await
            }
            Err(failure) => Err(failure),
        };
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the view may close before the result arrives"
        )]
        let _ = result_tx.send(booted);
    });

    cx.spawn(async move |cx| {
        let outcome = match result_rx.await {
            Ok(outcome) => outcome,
            Err(error) => Err(BootFailure::Retry {
                failure: RepoFailure::startup_stopped(error),
            }),
        };
        match outcome {
            Ok(handles) => finish_boot(handles, entities, initial_screen, window, cx).await,
            Err(failure) => {
                crate::boot_signal::boot_failed(&failure);
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

async fn finish_boot(
    mut handles: RuntimeHandles,
    entities: BootEntities,
    initial_screen: Screen,
    window: WindowHandle<RootView>,
    cx: &mut AsyncApp,
) {
    let captures = BootCaptures::take(&mut handles);
    let initial_screen = handles.first_run.opening_screen(initial_screen);
    let handles = Arc::new(handles);
    let shell_entities = entities.clone();
    let installed = window.update(cx, |root, window, cx| {
        install_shell(root, &handles, shell_entities, initial_screen, window, cx)
    });
    if let Ok(installed) = installed {
        crate::boot_signal::ready();
        start_boot_services(&handles, entities, captures, installed, cx).await;
    }
}

fn install_shell(
    root: &mut RootView,
    handles: &Arc<RuntimeHandles>,
    entities: BootEntities,
    initial_screen: Screen,
    window: &mut Window,
    cx: &mut Context<RootView>,
) -> InstalledShell {
    let BootEntities {
        status,
        chat_feed,
        home_stats,
        event_log,
        globals,
        platforms,
        speak,
        queue_health,
        event_loss,
        awake,
        twitch_bot,
    } = entities;
    crate::i18n::install_process_language(handles.startup_language);
    cx.set_global(crate::presentation::ActiveLanguage(
        handles.startup_language,
    ));
    forge_components::install_process_display_formats(handles.startup_display_formats.resolve());
    cx.set_global(crate::presentation::ActiveDisplayFormats(
        handles.startup_display_formats,
    ));
    crate::message_catalogs::report_catalog_problems(cx);
    if let Some(reason) = handles.server_unavailable.clone() {
        cx.set_global(crate::server_unavailable::ServerUnavailable(reason));
    }
    platforms.update(cx, |connectivity, cx| {
        connectivity.seed_from_builtins(&handles.integrations.builtins);
        cx.notify();
    });
    crate::donation_services::mirror_donation_connectivity(
        &handles.integrations.donation_services,
        &platforms,
        cx,
    );
    let integration_lifecycle =
        cx.new(|_| IntegrationLifecycle::new(handles.integrations.supervisor.watch().current()));
    let update_flow = cx.new(|_| {
        UpdateFlow::new(
            Arc::clone(&handles.backend),
            Arc::new(GithubArtifactDownloader),
            handles.rt_handle.clone(),
            crate::app_paths::app_paths().data_dir(),
        )
        .with_exclusivity(handles.core.transfer_exclusivity.clone())
        .with_scheduled_backups(handles.core.scheduled_backups.clone())
    });
    let problem_report = cx.new(|_| {
        ProblemReport::new(
            Arc::clone(&handles.backend),
            handles.integrations.builtins.clone(),
            handles.rt_handle.clone(),
        )
    });
    let window_facts = WindowFacts::read(window, cx);
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
        update_flow.clone(),
        problem_report.clone(),
        twitch_bot,
    );
    let shell = cx.new(|cx| {
        AppShell::new(
            status,
            topics,
            Arc::clone(handles),
            initial_screen,
            window,
            cx,
        )
    });
    AppShell::route_quit(&shell, cx);
    let navigator = AppShell::navigator(&shell);
    let mut ordered_stop = Some(handles.stop.clone());
    let stay_awake = handles.stay_awake.clone();
    App::on_app_quit(cx, move |cx| {
        stay_awake.release();
        let taken = ordered_stop.take();
        let teardown = crate::shutdown::QuitTeardown::claim(cx);
        let exit = crate::update_exit::ExitHandoff::claim_at_quit(cx);
        async move {
            if let Some(stop) = taken {
                stop.run().await;
            }
            if let Some(exit) = exit {
                exit.finish();
            }
            teardown.finish();
        }
    })
    .detach();
    root.mark_ready(shell);
    cx.notify();
    InstalledShell {
        update_flow,
        problem_report,
        window_facts,
        navigator,
    }
}

async fn start_boot_services(
    handles: &Arc<RuntimeHandles>,
    entities: BootEntities,
    captures: BootCaptures,
    installed: InstalledShell,
    cx: &mut AsyncApp,
) {
    let BootEntities {
        status,
        chat_feed,
        home_stats,
        event_log,
        globals: _,
        platforms,
        speak,
        queue_health,
        event_loss,
        awake,
        twitch_bot,
    } = entities;
    let BootCaptures {
        speak_events,
        overlay_build_failure,
        boot_warnings,
        credentials_key_loss,
        bridge_sub,
        chat_feed_bridge,
        loss_watch,
        depth_watch,
        obs_stats_watch,
        awake_watch,
        event_categories,
    } = captures;
    let InstalledShell {
        update_flow,
        problem_report,
        window_facts,
        navigator,
    } = installed;

    cx.update(crate::boot_restore::raise_boot_restore_toast);
    if let Some(loss) = credentials_key_loss {
        cx.update(|cx| raise_credentials_key_loss_toast(loss, cx));
    }
    if let Some(failure) = overlay_build_failure {
        cx.update(|cx| failure.raise(cx));
    }
    let warnings_navigator = navigator.clone();
    cx.update(|cx| {
        crate::boot_warnings::raise_boot_warnings(&boot_warnings, warnings_navigator, cx);
    });
    seed_chat_history(
        cx,
        chat_feed.clone(),
        Arc::clone(&handles.backend),
        handles.rt_handle.clone(),
    )
    .await;
    start_loss_bridge(cx, event_loss.clone(), loss_watch);
    start_queue_depth_bridge(cx, queue_health.clone(), depth_watch);
    start_obs_stats_bridge(cx, home_stats.clone(), obs_stats_watch);
    chat_feed_bridge.start(cx, chat_feed, event_loss);
    event_log.update(cx, |log, _| log.set_categories(event_categories));
    start_bridge(
        cx,
        home_stats.clone(),
        event_log,
        platforms,
        queue_health,
        twitch_bot.clone(),
        bridge_sub,
    );
    if let Some(bot) = handles.integrations.twitch_bot.clone() {
        start_twitch_bot_seed(cx, twitch_bot, bot, &handles.rt_handle);
    }
    start_uptime_clock(cx, status);
    start_awake_bridge(cx, awake, awake_watch);
    start_live_viewers_bridge(cx, home_stats, handles.automation.live_viewers.clone());
    start_boot_notices(handles, navigator, cx);
    apply_persisted_shortcuts(cx, Arc::clone(&handles.backend), handles.rt_handle.clone());
    start_update_services(handles, update_flow, problem_report, window_facts, cx);
    if let Some(events) = speak_events {
        start_speak_bridge(cx, speak, events);
    }
}

fn start_boot_notices(handles: &Arc<RuntimeHandles>, navigator: Navigator, cx: &mut AsyncApp) {
    crate::system_notices::start_system_notices(
        cx,
        &handles.core.notification_policy,
        Arc::new(forge_host::os_adapters::SystemNotifyPort),
        handles.rt_handle.clone(),
    );
    crate::scheduled_backup_notices::start_scheduled_backup_notices(
        cx,
        &handles.core.scheduled_backups,
        Arc::clone(&handles.backend) as Arc<dyn forge_storage::SettingsRepo>,
        Arc::new(forge_host::os_adapters::SystemNotifyPort),
        handles.rt_handle.clone(),
        navigator,
    );
}

fn start_update_services(
    handles: &Arc<RuntimeHandles>,
    update_flow: Entity<UpdateFlow>,
    problem_report: Entity<ProblemReport>,
    window_facts: WindowFacts,
    cx: &mut AsyncApp,
) {
    let boot_report = crate::update_report::start_update_report(
        cx,
        Arc::clone(&handles.backend),
        handles.rt_handle.clone(),
    );
    crate::update_leftover::start_leftover_install_notice(
        cx,
        Arc::clone(&handles.backend),
        handles.rt_handle.clone(),
    );
    crate::problem_report::start_crash_notice(cx, problem_report, handles.rt_handle.clone());
    crate::update_check::start_update_check(
        cx,
        Arc::clone(&handles.backend),
        handles.rt_handle.clone(),
        boot_report,
        update_flow,
    );
    let rt_handle_for_snapshot = handles.rt_handle.clone();
    cx.spawn(async move |_| {
        crate::crash_hook::snapshot_system_info(window_facts, &rt_handle_for_snapshot).await;
        #[cfg(debug_assertions)]
        crate::crash_hook::panic_if_requested();
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
    let reply = crate::async_bridge::on_runtime(&rt_handle, async move {
        let settings = Arc::clone(&backend) as Arc<dyn forge_storage::SettingsRepo>;
        let limit = forge_storage::chat_history_display_limit(settings.as_ref())
            .await
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "chat history limit unreadable; the default applies");
                DEFAULT_DISPLAY_LIMIT
            });
        let rows = backend
            .chat_history_repo()
            .list_recent(limit as usize)
            .await
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "chat history unreadable; the feed starts empty");
                Vec::new()
            });
        (limit, rows)
    });
    let Ok((limit, mut rows)) = reply.await else {
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
            let decoded: Vec<Option<RuntimeEvent>> =
                batch.iter().map(runtime_event::decode).collect();

            platforms.update(cx, |connectivity, cx| {
                let mut changed = false;
                for typed in decoded.iter().flatten() {
                    changed |= connectivity.apply_runtime_event(typed);
                }
                if changed {
                    cx.notify();
                }
            });

            event_log.update(cx, |log, cx| {
                for (event, typed) in batch.iter().zip(&decoded) {
                    log.record(event, typed.clone());
                }
                if !batch.is_empty() {
                    cx.notify();
                }
            });

            home_stats.update(cx, |stats, cx| {
                let mut changed = false;
                for (event, typed) in batch.iter().zip(&decoded) {
                    changed |= stats.record_event(event, typed.as_ref());
                    if matches!(typed, Some(RuntimeEvent::ActionDone(_))) {
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
                for typed in decoded.iter().flatten() {
                    if let RuntimeEvent::TwitchBotStatus(status) = typed {
                        changed |= presence.apply_status_changed(status);
                    }
                }
                if changed {
                    cx.notify();
                }
            });

            cx.update(|cx| {
                for (event, typed) in batch.iter().zip(&decoded) {
                    match typed {
                        Some(RuntimeEvent::ClipPlayback(playback)) => {
                            crate::clip_playback::report_clip_playback(playback, cx);
                        }
                        Some(RuntimeEvent::OverlaySpeechFailed) => {
                            crate::overlay_speech_failure::report_overlay_speech_failure(event, cx);
                        }
                        _ => {}
                    }
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
            let running = watch.running();
            health.update(cx, |health, cx| {
                let depths_changed = health.apply_depths(depths);
                let running_changed = health.apply_running(running);
                if depths_changed || running_changed {
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

fn start_obs_stats_bridge(
    cx: &mut AsyncApp,
    home_stats: Entity<HomeStats>,
    mut watch: forge_obs::ObsStatsWatch,
) {
    cx.spawn(async move |cx| {
        let mut stats = watch.current();
        loop {
            home_stats.update(cx, |home, cx| {
                home.set_obs_stats(stats, time::OffsetDateTime::now_utc());
                cx.notify();
            });
            match watch.changed().await {
                Some(next) => stats = next,
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
        let reply = crate::async_bridge::on_runtime(&rt_handle, async move {
crate::shortcut_overrides::load_overrides(repo).await
        });
        let overrides = match reply.await {
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
            BootState::Booting => return splash(tr!("boot_starting"), &palette, density, cx),
            BootState::Ready { shell } => {
                return div().size_full().child(shell.clone()).into_any_element();
            }
            BootState::Failed(BootFailure::UpgradeRequired { expected, found }) => {
                upgrade_screen(*expected, *found, &palette, density, cx)
            }
            BootState::Failed(BootFailure::PreBaseline { found }) => {
                pre_baseline_screen(*found, &palette, density, cx)
            }
            BootState::Failed(BootFailure::Retry { failure }) => {
                retry_screen(&failure.text(), cx.entity(), &palette, density, cx)
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

pub(crate) struct RestoreWaitView;

impl Render for RestoreWaitView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();
        splash(tr!("boot_finishing_restore"), &palette, density, cx)
    }
}

fn splash(status: String, palette: &ForgePalette, density: Density, cx: &App) -> AnyElement {
    let column = div()
        .flex()
        .flex_col()
        .items_center()
        .gap(spacing(Spacing::Sm, density))
        .child(
            div()
                .font_family(cx.body_family())
                .text_size(FONT_LG)
                .text_color(palette.brand)
                .child("forge"),
        )
        .child(
            div()
                .font_family(cx.body_family())
                .text_size(FONT_SM)
                .text_color(palette.text_muted)
                .child(status),
        );
    centered(column, palette, density)
}

fn upgrade_screen(
    expected: u32,
    found: u32,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> AnyElement {
    let body = div()
        .flex()
        .flex_col()
        .items_center()
        .gap(spacing(Spacing::Md, density))
        .child(icon(Icon::AlertTriangle, FONT_LG, palette.warning))
        .child(
            div()
                .font_family(cx.body_family())
                .text_size(FONT_LG)
                .text_color(palette.text_primary)
                .child(tr!("boot_upgrade_title")),
        )
        .child(
            div()
                .font_family(cx.body_family())
                .text_size(FONT_SM)
                .text_color(palette.text_secondary)
                .child(tr!("boot_upgrade_body", found = found, expected = expected)),
        )
        .child(
            div()
                .font_family(cx.body_family())
                .text_size(FONT_SM)
                .text_color(palette.text_muted)
                .child(tr!("boot_upgrade_reassure")),
        );
    centered(card(body, palette), palette, density)
}

fn pre_baseline_screen(
    found: u32,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> AnyElement {
    let actions = div()
        .flex()
        .gap(spacing(Spacing::Sm, density))
        .child(
            secondary_button(tr!("boot_pre_baseline_open_folder"), palette).on_click(
                "boot-open-data-folder",
                |_, _window, cx: &mut App| {
                    cx.reveal_path(&crate::app_paths::app_paths().data_dir())
                },
            ),
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
                .font_family(cx.body_family())
                .text_size(FONT_LG)
                .text_color(palette.text_primary)
                .child(tr!("boot_pre_baseline_title")),
        )
        .child(
            div()
                .font_family(cx.body_family())
                .text_size(FONT_SM)
                .text_color(palette.text_secondary)
                .child(tr!(
                    "boot_pre_baseline_body",
                    found = found,
                    release = forge_storage::LAST_PRE_BASELINE_RELEASE
                )),
        )
        .child(
            div()
                .font_family(cx.body_family())
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
    cx: &App,
) -> AnyElement {
    let detail = div()
        .w_full()
        .p(spacing(Spacing::Sm, density))
        .rounded(radius(Radius::Sm))
        .bg(palette.surface_overlay)
        .font_family(cx.mono_family())
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
                .font_family(cx.body_family())
                .text_size(FONT_LG)
                .text_color(palette.text_primary)
                .child(tr!("boot_failure_title")),
        )
        .child(
            div()
                .font_family(cx.body_family())
                .text_size(FONT_SM)
                .text_color(palette.text_secondary)
                .child(tr!("boot_failure_cause")),
        )
        .child(
            div()
                .w_full()
                .font_family(cx.body_family())
                .text_size(FONT_XXS)
                .text_color(palette.text_muted)
                .child(tr!("boot_failure_details")),
        )
        .child(detail)
        .child(
            div()
                .font_family(cx.body_family())
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
    use crate::home_stats::Integration;
    use crate::test_support::{
        held_queue_scheduler, hold_one_dispatch, runtime, test_backend, writes_to,
    };

    fn queue(id: QueueId) -> Queue {
        Queue {
            id,
            name: "Default".to_owned(),
            description: String::new(),
            concurrency: std::num::NonZeroU32::MIN,
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
            connectivity.seed_from_builtins(&forge_host::integrations::BuiltinRegistry::default());
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
    fn a_pre_baseline_failure_renders_without_panicking_or_leaving_the_failed_state(
        cx: &mut TestAppContext,
    ) {
        crate::test_support::install_presentation(cx);
        cx.update(|cx| cx.set_global(Toasts::new()));
        let rt = runtime();
        let handle = rt.handle().clone();
        let (view, vcx) = cx.add_window_view(move |_window, cx| {
            RootView::new(
                BootInputs {
                    rt_handle: handle,
                    log_tail: LogTail::new(),
                    endpoints: PlatformEndpoints::default(),
                    initial_screen: Screen::Home,
                },
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

    #[gpui::test]
    fn boot_with_an_undecodable_stored_shortcut_value_leaves_it_unwritten(cx: &mut TestAppContext) {
        let key = crate::shortcut_overrides::KEYBOARD_SHORTCUTS.name();
        let rt = runtime();
        let (backend, mut writes) = test_backend();
        backend.seed_raw(&crate::shortcut_overrides::KEYBOARD_SHORTCUTS, "not json");

        apply_persisted_shortcuts(
            &mut cx.to_async(),
            backend as Arc<dyn forge_storage::DataProvider>,
            rt.handle().clone(),
        );
        crate::test_support::settle_runtime_work(cx, &rt);

        assert_eq!(writes_to(&mut writes, key), Vec::<String>::new());
    }
}
