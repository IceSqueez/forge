use std::collections::HashMap;
use std::mem::Discriminant;
use std::sync::Arc;

use forge_components::{Density, FOOTER_HEIGHT, Spacing, spacing, toast_card};
use forge_events::EventPublisher;
use forge_registry::TriggerRegistry;
use forge_runtime::dashboard::compute_stats;
use forge_storage::{CredentialsRepo, DataProvider, GlobalsRepo, ScriptRepo, SettingsRepo};
use forge_types::{IntegrationAvailability, IntegrationId};
use gpui::{
    AnyElement, AnyView, App, AppContext, AsyncApp, Context, Entity, EventEmitter, FocusHandle,
    Window, deferred, div, prelude::*,
};

use crate::home_stats::HomeStats;

use crate::actions::{GoActions, GoChat, GoHome, GoSettings, GoTriggers, GoTwitch, SHELL_CONTEXT};
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
use crate::hotkey_sync::{HotkeyReconciler, HotkeySyncedTriggerRepo};
use crate::hotkeys_screen::HotkeysScreenView;
use crate::integration_detail::{IntegrationDetail, ObsSignedOut, VTubeSignedOut};
use crate::integration_disabled::{DisabledLaunch, IntegrationDisabledView};
use crate::integration_failed::{IntegrationFailedView, SignInRequested};
use crate::integration_lifecycle::IntegrationLifecycle;
use crate::integration_supervisor::{LifecycleState, LifecycleStates};
use crate::integration_switch::IntegrationSwitch;
use crate::integrations::{obs_builtin_object, vtube_builtin_object};
use crate::integrations_hub::{HubLaunch, IntegrationsHubView};
use crate::midi_screen::MidiScreenView;
use crate::obs_connect::ObsConnectView;
use crate::obs_credentials_form::ObsConnected;
use crate::overlays_screen::{OverlaysLaunch, OverlaysView};
use crate::presentation::{ActivePresentation, Presentation};
use crate::queues::QueuesView;
use crate::runtime_handles::RuntimeHandles;
use crate::runtime_status::RuntimeStatus;
use crate::scheduled_runs_section::ScheduledRunsView;
use crate::screen::Screen;
use crate::script_editor::ScriptEditorView;
use crate::server_console::ServerConsoleView;
use crate::settings::SettingsView;
use crate::sidebar::NavRequested;
use crate::soundboard::SoundboardView;
use crate::toasts::Toasts;
use crate::topics::Topics;
use crate::triggers_screen::TriggersRegistryView;
use crate::tts::TtsView;
use crate::unavailable_builtin::unavailable_builtin;
use crate::vtube_connect::VTubeConnectView;
use crate::vtube_connect_form::VTubeConnected;

const TOAST_PRIORITY: usize = 2;

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
        let screen = initial_screen;
        let content = Self::content_for(&screen, &topics, &handles, cx);
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

        cx.subscribe(
            &chrome.sidebar,
            |this, _sidebar, event: &NavRequested, cx| {
                this.navigate(event.0.clone(), cx);
            },
        )
        .detach();

        cx.observe_global::<Presentation>(|_, cx| cx.notify())
            .detach();

        cx.observe_global::<Toasts>(|_, cx| cx.notify()).detach();

        Self::spawn_lifecycle_bridge(&handles, topics.integration_lifecycle.clone(), cx);

        window.focus(&focus, cx);
        cx.on_focus_lost(window, Self::restore_focus).detach();

        Self {
            router: Router { screen, content },
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
    ) -> AnyView {
        match screen {
            Screen::Home => {
                let home_backend = Arc::clone(&handles.backend);
                let home_registry = Arc::clone(&handles.trigger_registry);
                let home_rt = handles.rt_handle.clone();
                let home_lifecycle = topics.integration_lifecycle.clone();
                let home = cx.new(|cx| {
                    HomeView::new(
                        topics.home_stats.clone(),
                        topics.event_loss.clone(),
                        home_backend,
                        home_registry,
                        home_rt,
                        cx,
                    )
                    .with_lifecycle(home_lifecycle, cx)
                });
                cx.subscribe(&home, |this, _home, event: &NavRequested, cx| {
                    this.navigate(event.0.clone(), cx);
                })
                .detach();
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
                let action_engine = handles.action_engine.clone();
                let voice_alias_repo = handles.backend.voice_alias_repo();
                let speak = handles.speak.clone();
                let bot_accounts = handles.bot_accounts.clone();
                let bus = Arc::clone(&handles.bus);
                cx.new(|cx| {
                    ChatView::new(
                        topics.chat_feed.clone(),
                        topics.home_stats.clone(),
                        rt_handle,
                        viewer_repo,
                        action_engine,
                        voice_alias_repo,
                        speak,
                        bot_accounts,
                        palette,
                        cx,
                    )
                    .with_lifecycle(topics.integration_lifecycle.clone(), cx)
                    .with_chat_bus(bus, cx)
                })
                .into()
            }
            Screen::EventFeed => {
                let rt_handle = handles.rt_handle.clone();
                cx.new(|cx| EventFeedView::new(topics.event_log.clone(), rt_handle, cx))
                    .into()
            }
            Screen::Globals => {
                let globals = topics.globals.clone();
                let backend: Arc<dyn GlobalsRepo> =
                    Arc::clone(&handles.backend) as Arc<dyn GlobalsRepo>;
                let rt_handle = handles.rt_handle.clone();
                cx.new(|cx| GlobalsView::new(globals, backend, rt_handle, cx))
                    .into()
            }
            Screen::Welcome => {
                let lifecycle = topics.integration_lifecycle.clone();
                let connectivity = topics.platforms.clone();
                let launch = Self::hub_launch(handles);
                let hub = cx.new(|cx| {
                    IntegrationsHubView::new(None, lifecycle, connectivity, launch, cx).welcome()
                });
                cx.subscribe(&hub, |this, _view, event: &NavRequested, cx| {
                    this.navigate(event.0.clone(), cx);
                })
                .detach();
                hub.into()
            }
            Screen::Integrations(focus) => {
                let launch = Self::hub_launch(handles);
                let lifecycle = topics.integration_lifecycle.clone();
                let connectivity = topics.platforms.clone();
                let focus = *focus;
                let hub = cx
                    .new(|cx| IntegrationsHubView::new(focus, lifecycle, connectivity, launch, cx));
                cx.subscribe(&hub, |this, _view, event: &NavRequested, cx| {
                    this.navigate(event.0.clone(), cx);
                })
                .detach();
                hub.into()
            }
            Screen::BuiltinDetail(id) => match Self::detail_route(id, topics, cx) {
                DetailRoute::Disabled => Self::disabled_screen(id, topics, handles, cx),
                DetailRoute::Failed => Self::failed_screen(id, topics, handles, cx),
                DetailRoute::Live => Self::builtin_detail_screen(id, topics, handles, cx),
            },
            Screen::Settings(preselect) => {
                let handles = Arc::clone(handles);
                let preselect = *preselect;
                let awake = topics.awake.clone();
                cx.new(|cx| SettingsView::new(handles, awake, preselect, cx))
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
                Self::routed(view, cx)
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
                Self::routed(view, cx)
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
                let view = cx.new(|cx| {
                    TtsView::new(
                        speak_state,
                        speak,
                        backend,
                        rt_handle,
                        pipeline_config,
                        bot_accounts,
                        tts_registry,
                        preselect,
                        cx,
                    )
                });
                Self::routed(view, cx)
            }
            Screen::Overlays => {
                let launch = OverlaysLaunch {
                    repo: handles.backend.overlay_repo(),
                    server: handles.server.clone(),
                    rt_handle: handles.rt_handle.clone(),
                    kinds: Arc::clone(&handles.overlay_kinds),
                    service: handles.overlays.clone(),
                    library: Arc::clone(handles.soundboard_player.library()),
                    media: handles.backend.media_repo(),
                    settings_repo: Arc::clone(&handles.backend) as Arc<dyn SettingsRepo>,
                    audio_router: Arc::clone(&handles.audio_router),
                    actions: Arc::new(forge_runtime::actions::ActionsService::new(
                        handles.backend.action_repo(),
                        handles.backend.queue_repo(),
                        handles.backend.history_repo(),
                        handles.backend.trigger_instance_repo(),
                        handles.backend.soundboard_clips_repo(),
                    )),
                    triggers: handles.trigger_registry.clone(),
                    sub_actions: handles.sub_action_registry.clone(),
                    scheduler: handles.scheduler.clone(),
                    latest_values: handles.latest_values.clone(),
                    bus: Arc::clone(&handles.bus),
                };
                let view = cx.new(|cx| OverlaysView::new(launch, cx));
                cx.subscribe(&view, |this, _view, event: &NavRequested, cx| {
                    this.navigate(event.0.clone(), cx);
                })
                .detach();
                view.into()
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
                cx.subscribe(&console, |this, _view, event: &NavRequested, cx| {
                    this.navigate(event.0.clone(), cx);
                })
                .detach();
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
                    handles.backend.soundboard_clips_repo(),
                ));
                let trigger_instance_repo = HotkeySyncedTriggerRepo::wrap(
                    handles.backend.trigger_instance_repo(),
                    handles.hotkey_reconciler.clone(),
                );
                let script_repo = Arc::clone(&handles.backend) as Arc<dyn ScriptRepo>;
                let soundboard_repo = handles.backend.soundboard_clips_repo();
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
                        soundboard_repo,
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
                    .with_integration_switch(switch, cx)
                });
                cx.subscribe(&view, |this, _view, event: &NavRequested, cx| {
                    this.navigate(event.0.clone(), cx);
                })
                .detach();
                view.into()
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
                cx.subscribe(&view, |this, _view, event: &NavRequested, cx| {
                    this.navigate(event.0.clone(), cx);
                })
                .detach();
                view.into()
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
                cx.subscribe(&editor, |this, _view, event: &NavRequested, cx| {
                    this.navigate(event.0.clone(), cx);
                })
                .detach();
                editor.into()
            }
        }
    }

    fn routed<V: Render + EventEmitter<NavRequested>>(
        view: Entity<V>,
        cx: &mut Context<Self>,
    ) -> AnyView {
        cx.subscribe(&view, |this, _view, event: &NavRequested, cx| {
            this.navigate(event.0.clone(), cx);
        })
        .detach();
        view.into()
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
            DISCORD_BUILTIN_ID => return Self::discord_screen(handles, cx),
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
        cx.subscribe(&detail, |this, _view, event: &NavRequested, cx| {
            this.navigate(event.0.clone(), cx);
        })
        .detach();
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
        cx.subscribe(&view, |this, _view, event: &NavRequested, cx| {
            this.navigate(event.0.clone(), cx);
        })
        .detach();
        cx.subscribe(&view, move |this, _view, _: &SignInRequested, cx| {
            this.show_detail_despite_failure(&id, cx);
        })
        .detach();
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
        cx.subscribe(&view, |this, _view, event: &NavRequested, cx| {
            this.navigate(event.0.clone(), cx);
        })
        .detach();
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
        cx.subscribe(&connect, |this, _view, event: &NavRequested, cx| {
            this.navigate(event.0.clone(), cx);
        })
        .detach();
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

    fn discord_screen(handles: &Arc<RuntimeHandles>, cx: &mut Context<Self>) -> AnyView {
        let client = Arc::clone(&handles.discord_client);
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
        let event_bus = Arc::clone(&handles.bus);
        let rt_handle = handles.rt_handle.clone();
        let seed = handles.vtube_install_seed.clone();
        let connect = cx.new(|cx| {
            VTubeConnectView::new(rt_handle, credentials, settings, bus, event_bus, seed, cx)
        });
        cx.subscribe(&connect, |this, _view, event: &NavRequested, cx| {
            this.navigate(event.0.clone(), cx);
        })
        .detach();
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
        self.router.content = Self::builtin_detail_screen(id, &self.topics, &self.handles, cx);
        cx.notify();
    }

    fn rebuild_current(&mut self, cx: &mut Context<Self>) {
        let screen = self.router.screen.clone();
        self.router.content = Self::content_for(&screen, &self.topics, &self.handles, cx);
        cx.notify();
    }

    fn navigate(&mut self, screen: Screen, cx: &mut Context<Self>) {
        if self.router.screen == screen {
            return;
        }
        if self.router.screen == Screen::Welcome {
            first_run::spawn_record_completed(
                Arc::clone(&self.handles.backend) as Arc<dyn SettingsRepo>,
                &self.handles.rt_handle,
            );
        }
        self.router.content = Self::content_for(&screen, &self.topics, &self.handles, cx);
        self.chrome.sidebar.update(cx, |sidebar, cx| {
            sidebar.set_current(screen.clone());
            cx.notify();
        });
        self.router.screen = screen;
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

    fn toast_host(&self, cx: &App) -> Option<AnyElement> {
        let toasts = cx.global::<Toasts>();
        if toasts.items().is_empty() {
            return None;
        }
        let palette = cx.palette();

        let mut column = div()
            .flex()
            .flex_col()
            .items_end()
            .gap(spacing(Spacing::Sm, Density::Cozy));

        for data in toasts.items() {
            let id = data.id;
            let mut card = toast_card(
                ("toast-card", id as usize),
                data.kind,
                data.message.clone(),
                &palette,
            )
            .on_dismiss(("toast-dismiss", id as usize), move |_, _, cx: &mut App| {
                cx.global_mut::<Toasts>().dismiss(id);
            });
            if let Some(glyph) = data.icon {
                card = card.icon(glyph);
            }
            if let Some(action) = &data.action {
                card = card.action(
                    ("toast-action", id as usize),
                    action.label.clone(),
                    move |_, window, cx: &mut App| {
                        if let Some(data) = cx.global_mut::<Toasts>().take(id)
                            && let Some(action) = data.action
                        {
                            (action.on_action)(window, cx);
                        }
                    },
                );
            }
            column = column.child(card);
        }

        Some(
            deferred(
                div()
                    .absolute()
                    .right(spacing(Spacing::Md, Density::Cozy))
                    .bottom(FOOTER_HEIGHT + spacing(Spacing::Sm, Density::Cozy))
                    .child(column),
            )
            .with_priority(TOAST_PRIORITY)
            .into_any_element(),
        )
    }
}

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
        let _ = tx.send(
            compute_stats(
                &*actions,
                &*globals,
                &*history,
                &*triggers,
                &trigger_registry,
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
            eprintln!("forge-desktop: home dashboard stats load failed: {err}");
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
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(self.chrome.titlebar.clone())
            .child(body)
            .when(!welcome, |root| root.child(self.chrome.footer.clone()));

        root.children(self.toast_host(cx))
    }
}
