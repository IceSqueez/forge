use std::collections::HashMap;
use std::mem::Discriminant;
use std::sync::Arc;

use forge_components::{FocusNextField, Toasts, toast_layer};
use forge_events::EventPublisher;
use forge_registry::TriggerRegistry;
use forge_runtime::dashboard::compute_stats;
use forge_storage::{CredentialsRepo, DataProvider, GlobalsRepo, ScriptRepo, SettingsRepo};
use forge_types::{IntegrationAvailability, IntegrationId};
use gpui::{
    AnyElement, AnyView, App, AppContext, AsyncApp, ClickEvent, Context, Entity, EventEmitter,
    FocusHandle, Window, div, prelude::*,
};

use crate::home_stats::HomeStats;

use crate::actions::{
    GoActions, GoChat, GoHome, GoSettings, GoTriggers, GoTwitch, NewAction, Quit, RunFocused,
    SHELL_CONTEXT, SaveFocused, ToggleEventFeed, ToggleLiveChat,
};
use crate::actions_screen::ScreenActionsView;
use crate::chat::ChatView;
use crate::chrome::Chrome;
use crate::clip_key_state::ClipKeyState;
use crate::detail_route::{DetailRoute, is_detail_of};
use crate::discord_screen::DiscordScreenView;
use crate::event_feed::EventFeedView;
use crate::first_run;
use crate::globals_view::GlobalsView;
use crate::home::HomeView;
use crate::hotkeys_screen::HotkeysScreenView;
use crate::integration_detail::{IntegrationDetail, ObsSignedOut, VTubeSignedOut};
use crate::integration_disabled::{DisabledLaunch, IntegrationDisabledView};
use crate::integration_failed::{IntegrationFailedView, LiveDetailRequested};
use crate::integration_lifecycle::IntegrationLifecycle;
use crate::integration_switch::IntegrationSwitch;
use crate::integrations::{obs_builtin_object, vtube_builtin_object};
use crate::integrations_hub::{HubLaunch, IntegrationsHubView};
use crate::midi_screen::MidiScreenView;
use crate::navigation::{NavRequested, Navigator, Screen};
use crate::navigation_guard::{LeaveDialogHandlers, LeaveTarget, NavigationGuard, leave_dialog};
use crate::obs_connect::ObsConnectView;
use crate::obs_credentials_form::ObsConnected;
use crate::overlays_screen::{OverlaysLaunch, OverlaysView};
use crate::pack_import_live::SetupLiveHandles;
use crate::pack_services::PackServices;
use crate::queues::QueuesView;
use crate::restore_close::{RestoreClose, RestoreCloseEvent, RestoreRoute};
use crate::runtime_handles::RuntimeHandles;
use crate::runtime_status::RuntimeStatus;
use crate::scheduled_runs_section::ScheduledRunsView;
use crate::screen_shortcuts::{
    ScreenShortcuts, ScreenShortcutsHandle, ToggleReturn, toggle_destination,
};
use crate::script_editor::ScriptEditorView;
use crate::server_console::ServerConsoleView;
use crate::settings::SettingsView;
use crate::soundboard::SoundboardView;
use crate::topics::Topics;
use crate::transfer_flows::{TransferFlows, TransferRoute};
use crate::triggers_screen::TriggersRegistryView;
use crate::tts::TtsView;
use crate::unavailable_builtin::unavailable_builtin;
use crate::unsaved_work::{UnsavedWork, UnsavedWorkHandle};
use crate::update_flow::{CloseDecision, UpdateFlowEvent};
use crate::vtube_connect::VTubeConnectView;
use crate::vtube_connect_form::VTubeConnected;
use forge_components::{ActivePresentation, Presentation};
use forge_hotkey::{HotkeyReconciler, HotkeySyncedTriggerRepo};
use forge_runtime::{LifecycleState, LifecycleStates};

fn settled_kinds(states: &LifecycleStates) -> HashMap<IntegrationId, Discriminant<LifecycleState>> {
    states
        .iter()
        .filter(|(_, state)| state.is_settled())
        .map(|(id, state)| (id.clone(), std::mem::discriminant(state)))
        .collect()
}
const OBS_BUILTIN_ID: &str = "obs";
const VTUBE_BUILTIN_ID: &str = "vtube";
const MIDI_BUILTIN_ID: &str = "midi";
const HOTKEY_BUILTIN_ID: &str = "hotkey";
const DISCORD_BUILTIN_ID: &str = "discord";

struct Router {
    screen: Screen,
    content: AnyView,
    guard: NavigationGuard,
    shortcuts: Option<ScreenShortcutsHandle>,
    toggle_return: Option<ToggleReturn>,
}

struct RoutedContent {
    view: AnyView,
    work: Option<UnsavedWorkHandle>,
    shortcuts: Option<ScreenShortcutsHandle>,
}

impl From<AnyView> for RoutedContent {
    fn from(view: AnyView) -> Self {
        Self {
            view,
            work: None,
            shortcuts: None,
        }
    }
}

impl RoutedContent {
    fn with_shortcuts<V: ScreenShortcuts>(mut self, view: &Entity<V>) -> Self {
        self.shortcuts = Some(ScreenShortcutsHandle::new(view.clone()));
        self
    }
}

impl<V: Render> From<Entity<V>> for RoutedContent {
    fn from(view: Entity<V>) -> Self {
        AnyView::from(view).into()
    }
}

pub struct AppShell {
    router: Router,
    chrome: Chrome,
    focus: FocusHandle,
    topics: Topics,
    handles: Arc<RuntimeHandles>,
}

impl AppShell {
    pub fn new(
        status: Entity<RuntimeStatus>,
        topics: Topics,
        handles: Arc<RuntimeHandles>,
        initial_screen: Screen,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let transfers = cx.new(|cx| {
            TransferFlows::new(
                topics.home_stats.clone(),
                PackServices::of(&handles),
                forge_paths::data_dir(),
                cx,
            )
            .with_setup_live(SetupLiveHandles::of(&handles), cx)
            .with_shared_exclusivity(
                handles.transfer_exclusivity.clone(),
                handles.scheduled_backups.clone(),
            )
            .watching_scheduled(cx)
        });
        cx.set_global(TransferRoute(transfers));
        let screen = initial_screen;
        let RoutedContent {
            view: content,
            work,
            shortcuts,
        } = Self::content_for(&screen, &topics, &handles, cx);
        let mut guard = NavigationGuard::new(window.window_handle());
        guard.watch(work);
        let focus = cx.focus_handle();
        let chrome = Chrome::new(
            status,
            topics.platforms.clone(),
            topics.event_loss.clone(),
            topics.awake.clone(),
            topics.integration_lifecycle.clone(),
            screen.clone(),
            cx,
        );

        Self::follow_nav(&chrome.sidebar, cx);

        cx.observe_global::<Presentation>(|_, cx| cx.notify())
            .detach();

        cx.observe_global::<Toasts>(|_, cx| cx.notify()).detach();

        cx.subscribe(
            &topics.update_flow,
            |this, _flow, event: &UpdateFlowEvent, cx| match event {
                UpdateFlowEvent::RestartRequested => this.request_close(cx),
                UpdateFlowEvent::CloseApproved => this.router.guard.close_window(cx),
            },
        )
        .detach();

        let restore_close = cx.new(|_| {
            RestoreClose::new(
                topics.update_flow.clone(),
                forge_paths::data_dir(),
                handles.rt_handle.clone(),
            )
            .with_scheduled_backups(handles.scheduled_backups.clone())
        });
        cx.subscribe(
            &restore_close,
            |this, _close, event: &RestoreCloseEvent, cx| match event {
                RestoreCloseEvent::CloseRequested => this.request_close(cx),
            },
        )
        .detach();
        cx.set_global(RestoreRoute(restore_close));

        Self::spawn_lifecycle_bridge(&handles, topics.integration_lifecycle.clone(), cx);

        window.focus(&focus, cx);
        cx.on_focus_lost(window, Self::restore_focus).detach();

        let shell = cx.weak_entity();
        window.on_window_should_close(cx, move |_window, cx| {
            shell
                .update(cx, |shell, cx| shell.allow_window_close(cx))
                .unwrap_or(true)
        });

        Self {
            router: Router {
                screen,
                content,
                guard,
                shortcuts,
                toggle_return: None,
            },
            chrome,
            focus,
            topics,
            handles,
        }
    }

    fn restore_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let target = window
            .focus_lost_restore_target(cx)
            .unwrap_or_else(|| self.focus.clone());
        window.focus(&target, cx);
    }

    fn content_for(
        screen: &Screen,
        topics: &Topics,
        handles: &Arc<RuntimeHandles>,
        cx: &mut Context<Self>,
    ) -> RoutedContent {
        match screen {
            Screen::Home => {
                let home_pack = PackServices::of(handles);
                let home_lifecycle = topics.integration_lifecycle.clone();
                let home = cx.new(|cx| {
                    HomeView::new(
                        topics.home_stats.clone(),
                        topics.event_loss.clone(),
                        home_pack,
                        cx,
                    )
                    .with_lifecycle(home_lifecycle, cx)
                });
                Self::follow_nav(&home, cx);
                let home_stats = topics.home_stats.clone();
                let backend = Arc::clone(&handles.backend);
                let stats_registry = Arc::clone(&handles.trigger_registry);
                let rt_handle = handles.rt_handle.clone();
                cx.spawn(async move |_shell, cx| {
                    refresh_dashboard_stats(home_stats, backend, stats_registry, rt_handle, cx)
                        .await;
                })
                .detach();
                home.into()
            }
            Screen::Chat => {
                let palette = cx.palette();
                let rt_handle = handles.rt_handle.clone();
                let viewer_repo = handles.backend.viewer_repo();
                let chat_history_repo = handles.backend.chat_history_repo();
                let action_engine = handles.action_engine.clone();
                let voice_alias_repo = handles.backend.voice_alias_repo();
                let speak = handles.speak.clone();
                let bot_accounts = handles.bot_accounts.clone();
                let bus = Arc::clone(&handles.bus);
                let builtins = handles.builtins.clone();
                cx.new(|cx| {
                    ChatView::new(
                        topics.chat_feed.clone(),
                        topics.home_stats.clone(),
                        rt_handle,
                        viewer_repo,
                        chat_history_repo,
                        action_engine,
                        voice_alias_repo,
                        speak,
                        bot_accounts,
                        palette,
                        cx,
                    )
                    .with_follow_lookups(builtins.clone())
                    .with_ban_list(builtins, Arc::clone(&bus))
                    .with_lifecycle(topics.integration_lifecycle.clone(), cx)
                    .with_chat_bus(bus, cx)
                    .with_twitch_bot(topics.twitch_bot.clone(), cx)
                })
                .into()
            }
            Screen::EventFeed => {
                let rt_handle = handles.rt_handle.clone();
                let bus = Arc::clone(&handles.bus);
                cx.new(|cx| EventFeedView::new(topics.event_log.clone(), rt_handle, bus, cx))
                    .into()
            }
            Screen::Globals => {
                let globals = topics.globals.clone();
                let backend: Arc<dyn GlobalsRepo> =
                    Arc::clone(&handles.backend) as Arc<dyn GlobalsRepo>;
                let rt_handle = handles.rt_handle.clone();
                let bus = Arc::clone(&handles.bus);
                cx.new(|cx| {
                    let mut view = GlobalsView::new(globals, backend, rt_handle, cx);
                    view.follow_runtime_writes(bus, cx);
                    view
                })
                .into()
            }
            Screen::Welcome => {
                let lifecycle = topics.integration_lifecycle.clone();
                let connectivity = topics.platforms.clone();
                let launch = Self::hub_launch(handles);
                let hub = cx.new(|cx| {
                    IntegrationsHubView::new(None, lifecycle, connectivity, launch, cx).welcome()
                });
                Self::follow_nav(&hub, cx);
                Self::follow_live_detail(&hub, cx);
                hub.into()
            }
            Screen::Integrations(focus) => {
                let launch = Self::hub_launch(handles);
                let lifecycle = topics.integration_lifecycle.clone();
                let connectivity = topics.platforms.clone();
                let focus = *focus;
                let hub = cx
                    .new(|cx| IntegrationsHubView::new(focus, lifecycle, connectivity, launch, cx));
                Self::follow_nav(&hub, cx);
                Self::follow_live_detail(&hub, cx);
                hub.into()
            }
            Screen::BuiltinDetail(id) => match Self::detail_route(id, topics, cx) {
                DetailRoute::Disabled => Self::disabled_screen(id, topics, handles, cx).into(),
                DetailRoute::Failed => Self::failed_screen(id, topics, handles, cx).into(),
                DetailRoute::Live => Self::builtin_detail_screen(id, topics, handles, cx).into(),
            },
            Screen::Settings(preselect) => {
                let handles = Arc::clone(handles);
                let preselect = *preselect;
                let awake = topics.awake.clone();
                let update_flow = topics.update_flow.clone();
                let problem_report = topics.problem_report.clone();
                cx.new(|cx| {
                    SettingsView::new(handles, awake, update_flow, problem_report, preselect, cx)
                })
                .into()
            }
            Screen::Queues => {
                let queue_health = topics.queue_health.clone();
                let scheduler = handles.scheduler.clone();
                let queue_repo = handles.backend.queue_repo();
                let action_repo = handles.backend.action_repo();
                let rt_handle = handles.rt_handle.clone();
                let scheduled = Self::scheduled_runs_section(handles, cx);
                let view = cx.new(|cx| {
                    QueuesView::new(
                        queue_health,
                        scheduler,
                        queue_repo,
                        action_repo,
                        rt_handle,
                        cx,
                    )
                    .with_scheduled(scheduled, cx)
                });
                Self::routed(view, cx).into()
            }
            Screen::Soundboard => {
                let player = handles.soundboard_player.clone();
                let settings_repo =
                    Arc::clone(&handles.backend) as Arc<dyn forge_storage::SettingsRepo>;
                let rt_handle = handles.rt_handle.clone();
                let bus = Arc::clone(&handles.bus);
                let keys = ClipKeyState::new(
                    handles.hotkey_reconciler.clone(),
                    Arc::clone(&handles.backend),
                );
                let switch = Self::integration_switch(topics, handles);
                let view = cx.new(|cx| {
                    SoundboardView::new(player, settings_repo, rt_handle, bus, keys, cx)
                        .with_integration_switch(switch, cx)
                });
                Self::routed(view, cx).into()
            }
            Screen::Tts(preselect) => {
                let preselect = *preselect;
                let speak_state = topics.speak.clone();
                let speak = handles.speak.clone();
                let backend = Arc::clone(&handles.backend);
                let rt_handle = handles.rt_handle.clone();
                let pipeline_config = handles.pipeline_config.clone();
                let bot_accounts = handles.bot_accounts.clone();
                let tts_registry = handles.tts_registry.clone();
                let cloud_tts_gateways = handles.cloud_tts_gateways.clone();
                let view = cx.new(|cx| {
                    TtsView::new(
                        speak_state,
                        speak,
                        backend,
                        rt_handle,
                        pipeline_config,
                        bot_accounts,
                        tts_registry,
                        cloud_tts_gateways,
                        preselect,
                        cx,
                    )
                });
                Self::routed(view, cx).into()
            }
            Screen::Overlays => {
                let launch = OverlaysLaunch {
                    repo: handles.backend.overlay_repo(),
                    server: handles.server.clone(),
                    rt_handle: handles.rt_handle.clone(),
                    kinds: Arc::clone(&handles.overlay_kinds),
                    service: handles.overlays.clone(),
                    samples: Arc::clone(&handles.overlay_samples),
                    library: Arc::clone(handles.soundboard_player.library()),
                    media: handles.backend.media_repo(),
                    settings_repo: Arc::clone(&handles.backend) as Arc<dyn SettingsRepo>,
                    audio_router: Arc::clone(&handles.audio_router),
                    speak: handles.speak.clone(),
                    actions: Arc::clone(&handles.overlay_actions),
                    triggers: handles.trigger_registry.clone(),
                    sub_actions: handles.sub_action_registry.clone(),
                    scheduler: handles.scheduler.clone(),
                    latest_values: handles.latest_values.clone(),
                    bus: Arc::clone(&handles.bus),
                };
                let view = cx.new(|cx| OverlaysView::new(launch, cx));
                Self::guarded(view.clone(), cx).with_shortcuts(&view)
            }
            Screen::Server => {
                let server = handles.server.clone();
                let rt_handle = handles.rt_handle.clone();
                let credentials: Arc<dyn CredentialsRepo> =
                    Arc::clone(&handles.backend) as Arc<dyn CredentialsRepo>;
                let settings: Arc<dyn SettingsRepo> =
                    Arc::clone(&handles.backend) as Arc<dyn SettingsRepo>;
                let console = cx
                    .new(|cx| ServerConsoleView::new(server, rt_handle, credentials, settings, cx));
                Self::follow_nav(&console, cx);
                console.into()
            }
            Screen::Actions(preselect) => {
                let preselect = *preselect;
                let action_repo = handles.backend.action_repo();
                let queue_repo = handles.backend.queue_repo();
                let actions_service = Arc::new(forge_runtime::actions::ActionsService::new(
                    handles.backend.action_repo(),
                    handles.backend.queue_repo(),
                    handles.backend.history_repo(),
                    handles.backend.trigger_instance_repo(),
                ));
                let trigger_instance_repo = HotkeySyncedTriggerRepo::wrap(
                    handles.backend.trigger_instance_repo(),
                    handles.hotkey_reconciler.clone(),
                );
                let script_repo = Arc::clone(&handles.backend) as Arc<dyn ScriptRepo>;
                let clip_library = Arc::clone(handles.soundboard_player.library());
                let globals_repo = Arc::clone(&handles.backend) as Arc<dyn GlobalsRepo>;
                let settings_repo = Arc::clone(&handles.backend) as Arc<dyn SettingsRepo>;
                let overlay_repo = handles.backend.overlay_repo();
                let overlay_kinds = Arc::clone(&handles.overlay_kinds);
                let tts_registry = handles.tts_registry.clone();
                let speak = handles.speak.clone();
                let sub_action_registry = handles.sub_action_registry.clone();
                let trigger_registry = handles.trigger_registry.clone();
                let rt_handle = handles.rt_handle.clone();
                let bus = Arc::clone(&handles.bus);
                let scheduler = handles.scheduler.clone();
                let builtins = handles.builtins.clone();
                let scheduled_run_repo = handles.backend.scheduled_run_repo();
                let switch = Self::integration_switch(topics, handles);
                let view = cx.new(|cx| {
                    ScreenActionsView::new(
                        action_repo,
                        queue_repo,
                        actions_service,
                        trigger_instance_repo,
                        script_repo,
                        clip_library,
                        globals_repo,
                        settings_repo,
                        overlay_repo,
                        overlay_kinds,
                        tts_registry,
                        speak,
                        sub_action_registry,
                        trigger_registry,
                        rt_handle,
                        bus,
                        scheduler,
                        preselect,
                        cx,
                    )
                    .with_builtins(builtins)
                    .with_scheduled_runs(scheduled_run_repo)
                    .with_pack_export(cx)
                    .with_integration_switch(switch, cx)
                });
                Self::follow_nav(&view, cx);
                RoutedContent::from(view.clone()).with_shortcuts(&view)
            }
            Screen::Triggers(preselect) => {
                let repo = HotkeySyncedTriggerRepo::wrap(
                    handles.backend.trigger_instance_repo(),
                    handles.hotkey_reconciler.clone(),
                );
                let action_repo = handles.backend.action_repo();
                let registry = handles.trigger_registry.clone();
                let settings_repo = Arc::clone(&handles.backend) as Arc<dyn SettingsRepo>;
                let rt_handle = handles.rt_handle.clone();
                let preselect = *preselect;
                let builtins = handles.builtins.clone();
                let bus = Arc::clone(&handles.bus);
                let switch = Self::integration_switch(topics, handles);
                let view = cx.new(|cx| {
                    TriggersRegistryView::new(
                        repo,
                        action_repo,
                        registry,
                        settings_repo,
                        rt_handle,
                        preselect,
                        cx,
                    )
                    .with_builtins(builtins)
                    .with_event_bus(bus)
                    .with_integration_switch(switch, cx)
                });
                Self::follow_nav(&view, cx);
                RoutedContent::from(view.clone()).with_shortcuts(&view)
            }
            Screen::Scripts => {
                let backend = handles.backend.clone();
                let script_registry = handles.script_registry.clone();
                let bus = handles.bus.clone();
                let integrations: Arc<dyn IntegrationAvailability> =
                    Arc::new(handles.action_engine.integration_gate().clone());
                let rt_handle = handles.rt_handle.clone();
                let editor = cx.new(|cx| {
                    ScriptEditorView::new(
                        backend,
                        script_registry,
                        bus,
                        integrations,
                        rt_handle,
                        cx,
                    )
                });
                Self::guarded(editor.clone(), cx).with_shortcuts(&editor)
            }
        }
    }

    fn follow_live_detail<V: EventEmitter<LiveDetailRequested>>(
        view: &Entity<V>,
        cx: &mut Context<Self>,
    ) {
        cx.subscribe(view, |this, _view, event: &LiveDetailRequested, cx| {
            this.navigate(Screen::BuiltinDetail(event.0.clone()), cx);
            this.show_detail_despite_failure(&event.0, cx);
        })
        .detach();
    }

    fn follow_nav<V: EventEmitter<NavRequested>>(view: &Entity<V>, cx: &mut Context<Self>) {
        cx.subscribe(view, |this, _view, event: &NavRequested, cx| {
            this.navigate(event.0.clone(), cx);
        })
        .detach();
    }

    fn routed<V: Render + EventEmitter<NavRequested>>(
        view: Entity<V>,
        cx: &mut Context<Self>,
    ) -> AnyView {
        Self::follow_nav(&view, cx);
        view.into()
    }

    fn guarded<V: Render + EventEmitter<NavRequested> + UnsavedWork>(
        view: Entity<V>,
        cx: &mut Context<Self>,
    ) -> RoutedContent {
        cx.observe(&view, |this, _view, cx| this.settle_pending_leave(cx))
            .detach();
        let work = UnsavedWorkHandle::new(view.clone());
        RoutedContent {
            view: Self::routed(view, cx),
            work: Some(work),
            shortcuts: None,
        }
    }

    fn scheduled_runs_section(
        handles: &Arc<RuntimeHandles>,
        cx: &mut Context<Self>,
    ) -> Entity<ScheduledRunsView> {
        let repo = handles.backend.scheduled_run_repo();
        let changes = handles.backend.scheduled_run_revision().subscribe();
        let catalog_changes = handles.backend.catalog_revision().subscribe();
        let action_repo = handles.backend.action_repo();
        let runs = handles.scheduled_runs.clone();
        let rt_handle = handles.rt_handle.clone();
        cx.new(|cx| {
            ScheduledRunsView::new(
                repo,
                changes,
                catalog_changes,
                action_repo,
                runs,
                rt_handle,
                cx,
            )
        })
    }

    fn integration_switch(topics: &Topics, handles: &Arc<RuntimeHandles>) -> IntegrationSwitch {
        IntegrationSwitch::new(
            topics.integration_lifecycle.clone(),
            handles.integrations.clone(),
        )
    }

    fn hub_launch(handles: &Arc<RuntimeHandles>) -> HubLaunch {
        HubLaunch {
            supervisor: handles.integrations.clone(),
            builtins: handles.builtins.clone(),
            backend: Arc::clone(&handles.backend),
            sub_actions: Arc::clone(&handles.sub_action_registry),
            triggers: Arc::clone(&handles.trigger_registry),
            rt_handle: handles.rt_handle.clone(),
            obs_install_seed: handles.obs_install_seed.clone(),
            vtube_install_seed: handles.vtube_install_seed.clone(),
        }
    }

    fn detail_route(id: &IntegrationId, topics: &Topics, cx: &Context<Self>) -> DetailRoute {
        DetailRoute::resolve(topics.integration_lifecycle.read(cx), id)
    }

    fn builtin_detail_screen(
        id: &IntegrationId,
        topics: &Topics,
        handles: &Arc<RuntimeHandles>,
        cx: &mut Context<Self>,
    ) -> AnyView {
        let builtin = match id.as_str() {
            OBS_BUILTIN_ID => match handles.obs_install_seed.live() {
                Some(client) => Some(obs_builtin_object(client)),
                None => return Self::obs_connect_screen(handles, cx),
            },
            VTUBE_BUILTIN_ID => match handles.vtube_install_seed.live() {
                Some(client) => Some(vtube_builtin_object(client)),
                None => return Self::vtube_connect_screen(handles, cx),
            },
            MIDI_BUILTIN_ID => return Self::midi_screen(handles, cx),
            HOTKEY_BUILTIN_ID => match handles.hotkey_reconciler.clone() {
                Some(reconciler) => return Self::hotkeys_screen(handles, reconciler, cx),
                None => handles.builtins.get(id),
            },
            DISCORD_BUILTIN_ID => match handles.discord_client.clone() {
                Some(client) => return Self::discord_screen(handles, client, cx),
                None => handles.builtins.get(id),
            },
            _ => handles.builtins.get(id),
        };
        let object = builtin.unwrap_or_else(|| unavailable_builtin(id));

        let connectivity = topics.platforms.clone();
        let credentials = Arc::clone(&handles.backend) as Arc<dyn CredentialsRepo>;
        let settings = Arc::clone(&handles.backend) as Arc<dyn SettingsRepo>;
        let history = handles.backend.history_repo();
        let trigger_registry = handles.trigger_registry.clone();
        let bus = Arc::clone(&handles.bus) as Arc<dyn EventPublisher>;
        let event_bus = Arc::clone(&handles.bus);
        let rt_handle = handles.rt_handle.clone();
        let action_engine = handles.action_engine.clone();
        let live_viewers = handles.live_viewers.clone();
        let twitch_slot = handles
            .integrations
            .slot(&forge_platform_twitch::TWITCH_INTEGRATION.id);
        let kick_slot = handles
            .integrations
            .slot(&forge_platform_kick::KICK_INTEGRATION.id);
        let youtube_slot = handles
            .integrations
            .slot(&forge_platform_youtube::YOUTUBE_INTEGRATION.id);
        let obs_install_seed = handles.obs_install_seed.clone();
        let vtube_install_seed = handles.vtube_install_seed.clone();
        let detail = cx.new(|cx| {
            IntegrationDetail::new(
                object,
                rt_handle,
                action_engine,
                credentials,
                settings,
                history,
                trigger_registry,
                bus,
                event_bus,
                live_viewers,
                twitch_slot,
                kick_slot,
                youtube_slot,
                obs_install_seed,
                vtube_install_seed,
                connectivity,
                cx,
            )
        });
        if let Some(service) = handles.donation_services.service(id) {
            detail.update(cx, |detail, cx| {
                detail.attach_donation_settings(service, cx)
            });
        }
        if let Some(sign_in) = handles.twitch_sign_in.clone() {
            detail.update(cx, |detail, _| detail.attach_twitch_sign_in(sign_in));
        }
        if let Some(sign_in) = handles.youtube_sign_in.clone() {
            detail.update(cx, |detail, _| detail.attach_youtube_sign_in(sign_in));
        }
        if let Some(sign_in) = handles.kick_sign_in.clone() {
            detail.update(cx, |detail, _| detail.attach_kick_sign_in(sign_in));
        }
        let platform_credentials = handles.platform_credentials.clone();
        detail.update(cx, |detail, _| {
            detail.attach_platform_credentials(platform_credentials)
        });
        if let Some(bot) = handles.twitch_bot.clone() {
            let presence = topics.twitch_bot.clone();
            detail.update(cx, |detail, cx| detail.attach_twitch_bot(bot, presence, cx));
        }
        Self::follow_nav(&detail, cx);
        cx.subscribe(&detail, |this, _view, _: &ObsConnected, cx| {
            this.enable_integration(&forge_obs::OBS_INTEGRATION.id);
            this.rebuild_current(cx);
        })
        .detach();
        cx.subscribe(&detail, |this, _view, _: &ObsSignedOut, cx| {
            this.rebuild_current(cx);
        })
        .detach();
        cx.subscribe(&detail, |this, _view, _: &VTubeSignedOut, cx| {
            this.rebuild_current(cx);
        })
        .detach();
        detail.into()
    }

    fn failed_screen(
        id: &IntegrationId,
        topics: &Topics,
        handles: &Arc<RuntimeHandles>,
        cx: &mut Context<Self>,
    ) -> AnyView {
        let lifecycle = topics.integration_lifecycle.clone();
        let slot = handles.integrations.slot(id);
        let rt_handle = handles.rt_handle.clone();
        let id = id.clone();
        let view =
            cx.new(|cx| IntegrationFailedView::new(id.clone(), lifecycle, slot, rt_handle, cx));
        Self::follow_nav(&view, cx);
        Self::follow_live_detail(&view, cx);
        view.into()
    }

    fn disabled_screen(
        id: &IntegrationId,
        topics: &Topics,
        handles: &Arc<RuntimeHandles>,
        cx: &mut Context<Self>,
    ) -> AnyView {
        let launch = DisabledLaunch {
            slot: handles.integrations.slot(id),
            backend: Arc::clone(&handles.backend),
            sub_actions: Arc::clone(&handles.sub_action_registry),
            triggers: Arc::clone(&handles.trigger_registry),
            rt_handle: handles.rt_handle.clone(),
        };
        let lifecycle = topics.integration_lifecycle.clone();
        let id = id.clone();
        let view = cx.new(|cx| IntegrationDisabledView::new(id, lifecycle, launch, cx));
        Self::follow_nav(&view, cx);
        view.into()
    }

    fn obs_connect_screen(handles: &Arc<RuntimeHandles>, cx: &mut Context<Self>) -> AnyView {
        let credentials = Arc::clone(&handles.backend) as Arc<dyn CredentialsRepo>;
        let settings = Arc::clone(&handles.backend) as Arc<dyn SettingsRepo>;
        let bus = Arc::clone(&handles.bus) as Arc<dyn EventPublisher>;
        let rt_handle = handles.rt_handle.clone();
        let seed = handles.obs_install_seed.clone();
        let connect =
            cx.new(|cx| ObsConnectView::new(rt_handle, credentials, settings, bus, seed, cx));
        Self::follow_nav(&connect, cx);
        cx.subscribe(&connect, |this, _view, _: &ObsConnected, cx| {
            this.enable_integration(&forge_obs::OBS_INTEGRATION.id);
            this.rebuild_current(cx);
        })
        .detach();
        connect.into()
    }

    fn hotkeys_screen(
        handles: &Arc<RuntimeHandles>,
        reconciler: Arc<HotkeyReconciler>,
        cx: &mut Context<Self>,
    ) -> AnyView {
        let backend = Arc::clone(&handles.backend);
        let settings = Arc::clone(&handles.backend) as Arc<dyn SettingsRepo>;
        let bus = Arc::clone(&handles.bus);
        let rt_handle = handles.rt_handle.clone();
        let engine = handles
            .integrations
            .slot(&forge_hotkey::HOTKEY_INTEGRATION.id);
        let view = cx.new(|cx| {
            HotkeysScreenView::new(reconciler, engine, backend, settings, bus, rt_handle, cx)
        });
        Self::routed(view, cx)
    }

    fn discord_screen(
        handles: &Arc<RuntimeHandles>,
        client: Arc<forge_discord::DiscordClient>,
        cx: &mut Context<Self>,
    ) -> AnyView {
        let action_repo = handles.backend.action_repo();
        let bus = Arc::clone(&handles.bus);
        let rt_handle = handles.rt_handle.clone();
        let integration = handles
            .integrations
            .slot(&forge_discord::DISCORD_INTEGRATION.id);
        let view = cx
            .new(|cx| DiscordScreenView::new(client, integration, action_repo, bus, rt_handle, cx));
        Self::routed(view, cx)
    }

    fn midi_screen(handles: &Arc<RuntimeHandles>, cx: &mut Context<Self>) -> AnyView {
        let sink = Arc::clone(&handles.midi_sink);
        let integration = handles.integrations.slot(&forge_midi::MIDI_INTEGRATION.id);
        let trigger_repo = handles.backend.trigger_instance_repo();
        let action_repo = handles.backend.action_repo();
        let settings = Arc::clone(&handles.backend) as Arc<dyn SettingsRepo>;
        let bus = Arc::clone(&handles.bus);
        let rt_handle = handles.rt_handle.clone();
        let view = cx.new(|cx| {
            MidiScreenView::new(
                sink,
                integration,
                trigger_repo,
                action_repo,
                settings,
                bus,
                rt_handle,
                cx,
            )
        });
        Self::routed(view, cx)
    }

    fn vtube_connect_screen(handles: &Arc<RuntimeHandles>, cx: &mut Context<Self>) -> AnyView {
        let credentials = Arc::clone(&handles.backend) as Arc<dyn CredentialsRepo>;
        let settings = Arc::clone(&handles.backend) as Arc<dyn SettingsRepo>;
        let bus = Arc::clone(&handles.bus) as Arc<dyn EventPublisher>;
        let rt_handle = handles.rt_handle.clone();
        let seed = handles.vtube_install_seed.clone();
        let connect =
            cx.new(|cx| VTubeConnectView::new(rt_handle, credentials, settings, bus, seed, cx));
        Self::follow_nav(&connect, cx);
        cx.subscribe(&connect, |this, _view, _: &VTubeConnected, cx| {
            this.enable_integration(&forge_vtube::VTUBE_INTEGRATION.id);
            this.rebuild_current(cx);
        })
        .detach();
        connect.into()
    }

    fn enable_integration(&self, id: &IntegrationId) {
        if let Some(slot) = self.handles.integrations.slot(id) {
            slot.request_enable();
        }
    }

    fn spawn_lifecycle_bridge(
        handles: &Arc<RuntimeHandles>,
        topic: Entity<IntegrationLifecycle>,
        cx: &mut Context<Self>,
    ) {
        let mut lifecycle = handles.integrations.watch();
        let mut settled = settled_kinds(&lifecycle.current());
        cx.spawn(async move |this, cx| {
            while let Some(states) = lifecycle.changed().await {
                topic.update(cx, |topic, cx| {
                    if topic.replace(states.clone()) {
                        cx.notify();
                    }
                });
                let next = settled_kinds(&states);
                let changed: Vec<IntegrationId> = next
                    .iter()
                    .filter(|(id, kind)| settled.get(*id) != Some(*kind))
                    .map(|(id, _)| id.clone())
                    .collect();
                settled.extend(next);
                if changed.is_empty() {
                    continue;
                }
                if this
                    .update(cx, |shell, cx| shell.on_lifecycle_settled(&changed, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    fn on_lifecycle_settled(&mut self, changed: &[IntegrationId], cx: &mut Context<Self>) {
        if let Screen::BuiltinDetail(id) = &self.router.screen
            && changed.contains(id)
        {
            self.rebuild_current(cx);
        }
    }

    fn show_detail_despite_failure(&mut self, id: &IntegrationId, cx: &mut Context<Self>) {
        if !is_detail_of(&self.router.screen, id) {
            return;
        }
        let content = Self::builtin_detail_screen(id, &self.topics, &self.handles, cx);
        self.show(content.into());
        cx.notify();
    }

    fn rebuild_current(&mut self, cx: &mut Context<Self>) {
        let screen = self.router.screen.clone();
        let content = Self::content_for(&screen, &self.topics, &self.handles, cx);
        self.show(content);
        cx.notify();
    }

    fn show(&mut self, content: RoutedContent) {
        self.router.content = content.view;
        self.router.guard.watch(content.work);
        self.router.shortcuts = content.shortcuts;
    }

    pub fn navigator(shell: &Entity<Self>) -> Navigator {
        let shell = shell.downgrade();
        Navigator::new(move |screen, cx| {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "the shell may close before the request arrives"
            )]
            let _ = shell.update(cx, |shell, cx| shell.navigate(screen, cx));
        })
    }

    pub fn route_quit(shell: &Entity<Self>, cx: &mut App) {
        let shell = shell.downgrade();
        cx.on_action(move |_: &Quit, cx| {
            if shell
                .update(cx, |shell, cx| shell.request_close(cx))
                .is_err()
            {
                cx.quit();
            }
        });
    }

    fn navigate(&mut self, screen: Screen, cx: &mut Context<Self>) {
        if self.router.screen == screen {
            return;
        }
        if self.router.guard.blocks(cx) {
            self.router.guard.ask(LeaveTarget::Screen(screen));
            cx.notify();
            return;
        }
        self.enter(screen, cx);
    }

    fn allow_window_close(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.router.guard.blocks(cx) {
            return self.update_close_decision(cx) == CloseDecision::Proceed;
        }
        self.router.guard.ask(LeaveTarget::CloseWindow);
        cx.notify();
        false
    }

    fn request_close(&mut self, cx: &mut Context<Self>) {
        if self.router.guard.blocks(cx) {
            self.router.guard.ask(LeaveTarget::CloseWindow);
            cx.notify();
            return;
        }
        self.close_window_now(cx);
    }

    fn close_window_now(&mut self, cx: &mut Context<Self>) {
        if self.update_close_decision(cx) == CloseDecision::Proceed {
            self.router.guard.close_window(cx);
        }
    }

    fn update_close_decision(&mut self, cx: &mut Context<Self>) -> CloseDecision {
        let decision = self
            .topics
            .update_flow
            .update(cx, |flow, cx| flow.close_decision(cx));
        if decision == CloseDecision::Hold
            && let Some(restore_close) = RestoreClose::route(cx)
        {
            restore_close.update(cx, |close, cx| close.yield_to_update(cx));
        }
        decision
    }

    fn leave_to(&mut self, target: LeaveTarget, cx: &mut Context<Self>) {
        match target {
            LeaveTarget::Screen(screen) => self.enter(screen, cx),
            LeaveTarget::ToggledScreen(recorded) => {
                self.enter(recorded.toggled.clone(), cx);
                self.router.toggle_return = Some(recorded);
            }
            LeaveTarget::CloseWindow => self.close_window_now(cx),
        }
    }

    fn cancel_leave(&mut self, cx: &mut Context<Self>) {
        self.router.guard.cancel();
        self.topics
            .update_flow
            .update(cx, |flow, cx| flow.abort_restart(cx));
        if let Some(restore_close) = RestoreClose::route(cx) {
            restore_close.update(cx, |close, cx| close.abort(cx));
        }
        cx.notify();
    }

    fn discard_and_leave(&mut self, cx: &mut Context<Self>) {
        if let Some(target) = self.router.guard.discard(cx) {
            self.leave_to(target, cx);
        }
        cx.notify();
    }

    fn save_and_leave(&mut self, cx: &mut Context<Self>) {
        if let Some(target) = self.router.guard.save(cx) {
            self.leave_to(target, cx);
        }
        cx.notify();
    }

    fn settle_pending_leave(&mut self, cx: &mut Context<Self>) {
        if let Some(target) = self.router.guard.settle(cx) {
            self.leave_to(target, cx);
            cx.notify();
        }
    }

    fn enter(&mut self, screen: Screen, cx: &mut Context<Self>) {
        if self.router.screen == Screen::Welcome {
            first_run::spawn_record_completed(
                Arc::clone(&self.handles.backend) as Arc<dyn SettingsRepo>,
                &self.handles.rt_handle,
            );
        }
        let content = Self::content_for(&screen, &self.topics, &self.handles, cx);
        self.show(content);
        self.chrome.sidebar.update(cx, |sidebar, cx| {
            sidebar.set_current(screen.clone());
            cx.notify();
        });
        self.router.screen = screen;
        self.router.toggle_return = None;
        cx.notify();
    }

    fn go_home(&mut self, _: &GoHome, _: &mut Window, cx: &mut Context<Self>) {
        self.navigate(Screen::Home, cx);
    }

    fn go_chat(&mut self, _: &GoChat, _: &mut Window, cx: &mut Context<Self>) {
        self.navigate(Screen::Chat, cx);
    }

    fn go_actions(&mut self, _: &GoActions, _: &mut Window, cx: &mut Context<Self>) {
        self.navigate(Screen::Actions(None), cx);
    }

    fn go_triggers(&mut self, _: &GoTriggers, _: &mut Window, cx: &mut Context<Self>) {
        self.navigate(Screen::Triggers(None), cx);
    }

    fn go_twitch(&mut self, _: &GoTwitch, _: &mut Window, cx: &mut Context<Self>) {
        self.navigate(Screen::BuiltinDetail(IntegrationId::new("twitch")), cx);
    }

    fn go_settings(&mut self, _: &GoSettings, _: &mut Window, cx: &mut Context<Self>) {
        self.navigate(Screen::Settings(None), cx);
    }

    fn screen_shortcuts(&self) -> Option<ScreenShortcutsHandle> {
        if self.router.guard.is_asking() {
            return None;
        }
        self.router.shortcuts.clone()
    }

    fn save_focused(&mut self, _: &SaveFocused, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(shortcuts) = self.screen_shortcuts() {
            shortcuts.save(cx);
        }
    }

    fn run_focused(&mut self, _: &RunFocused, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(shortcuts) = self.screen_shortcuts() {
            shortcuts.run(cx);
        }
    }

    fn new_action(&mut self, _: &NewAction, window: &mut Window, cx: &mut Context<Self>) {
        if self.router.guard.is_asking() {
            return;
        }
        if !matches!(self.router.screen, Screen::Actions(_)) {
            self.navigate(Screen::Actions(None), cx);
        }
        if !matches!(self.router.screen, Screen::Actions(_)) {
            return;
        }
        if let Some(shortcuts) = self.router.shortcuts.clone() {
            shortcuts.new_action(window, cx);
        }
    }

    fn toggle_live_chat(&mut self, _: &ToggleLiveChat, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_screen(Screen::Chat, cx);
    }

    fn toggle_event_feed(&mut self, _: &ToggleEventFeed, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_screen(Screen::EventFeed, cx);
    }

    fn toggle_screen(&mut self, target: Screen, cx: &mut Context<Self>) {
        if self.router.guard.is_asking() {
            return;
        }
        let from = self.router.screen.clone();
        let Some(destination) =
            toggle_destination(&from, &target, self.router.toggle_return.as_ref())
        else {
            return;
        };
        self.navigate(destination.clone(), cx);
        if destination != target {
            return;
        }
        let recorded = ToggleReturn {
            toggled: target.clone(),
            back_to: from,
        };
        if self.router.screen == target {
            self.router.toggle_return = Some(recorded);
        } else if self.router.guard.is_asking() {
            self.router.guard.ask(LeaveTarget::ToggledScreen(recorded));
        }
    }

    fn focus_next_stop(&mut self, _: &FocusNextField, window: &mut Window, cx: &mut Context<Self>) {
        let before = window.focused(cx);
        window.focus_next(cx);
        if window.focused(cx) == before {
            window.focus(&self.focus, cx);
        }
    }

    fn leave_confirm(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.router.guard.is_asking() {
            return None;
        }
        let palette = cx.palette();
        let weak = cx.entity().downgrade();
        Some(leave_dialog(
            self.router.guard.work_name(cx),
            &palette,
            LeaveDialogHandlers {
                cancel: cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_leave(cx)),
                save: cx.listener(|this, _: &ClickEvent, _, cx| this.save_and_leave(cx)),
                discard: cx.listener(|this, _: &ClickEvent, _, cx| this.discard_and_leave(cx)),
                dismiss: move |_window: &mut Window, cx: &mut App| {
                    #[expect(
                        clippy::let_underscore_must_use,
                        reason = "the view may close before the result arrives"
                    )]
                    let _ = weak.update(cx, |this, cx| this.cancel_leave(cx));
                },
            },
        ))
    }
}

const DASHBOARD_RUN_WINDOW: time::Duration = time::Duration::hours(24);

pub(crate) async fn refresh_dashboard_stats(
    home_stats: Entity<HomeStats>,
    backend: Arc<dyn DataProvider>,
    trigger_registry: Arc<TriggerRegistry>,
    rt_handle: tokio::runtime::Handle,
    cx: &mut AsyncApp,
) {
    let actions = backend.action_repo();
    let globals: Arc<dyn GlobalsRepo> = Arc::clone(&backend) as Arc<dyn GlobalsRepo>;
    let history = backend.history_repo();
    let triggers = backend.trigger_instance_repo();
    let (tx, rx) = tokio::sync::oneshot::channel();
    rt_handle.spawn(async move {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the view may close before the result arrives"
        )]
        let _ = tx.send(
            compute_stats(
                &*actions,
                &*globals,
                &*history,
                &*triggers,
                &trigger_registry,
                time::OffsetDateTime::now_utc() - DASHBOARD_RUN_WINDOW,
            )
            .await,
        );
    });
    match rx.await {
        Ok(Ok(stats)) => {
            home_stats.update(cx, |stats_topic, cx| {
                if stats_topic.set_stats(stats) {
                    cx.notify();
                }
            });
        }
        Ok(Err(err)) => {
            tracing::warn!(error = %err, "home dashboard stats load failed");
        }
        Err(_) => {}
    }
}

impl Render for AppShell {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();

        let welcome = self.router.screen == Screen::Welcome;
        let body = div()
            .w_full()
            .flex_1()
            .flex()
            .flex_row()
            .overflow_hidden()
            .when(!welcome, |body| body.child(self.chrome.sidebar.clone()))
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .overflow_hidden()
                    .child(self.router.content.clone()),
            );

        let root = div()
            .key_context(SHELL_CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::go_home))
            .on_action(cx.listener(Self::go_chat))
            .on_action(cx.listener(Self::go_actions))
            .on_action(cx.listener(Self::go_triggers))
            .on_action(cx.listener(Self::go_twitch))
            .on_action(cx.listener(Self::go_settings))
            .on_action(cx.listener(Self::save_focused))
            .on_action(cx.listener(Self::run_focused))
            .on_action(cx.listener(Self::new_action))
            .on_action(cx.listener(Self::toggle_live_chat))
            .on_action(cx.listener(Self::toggle_event_feed))
            .on_action(cx.listener(Self::focus_next_stop))
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(self.chrome.titlebar.clone())
            .child(body)
            .when(!welcome, |root| root.child(self.chrome.footer.clone()));

        root.child(self.topics.problem_report.clone())
            .children(self.leave_confirm(cx))
            .children(toast_layer(&cx.palette(), cx))
    }
}
