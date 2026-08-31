use std::sync::Arc;

use forge_components::{Density, FOOTER_HEIGHT, Spacing, spacing, toast_card};
use forge_events::EventPublisher;
use forge_platform_core::BuiltinId;
use forge_registry::TriggerRegistry;
use forge_runtime::dashboard::compute_stats;
use forge_storage::{CredentialsRepo, DataProvider, GlobalsRepo, ScriptRepo, SettingsRepo};
use gpui::{
    AnyElement, AnyView, App, AppContext, AsyncApp, Context, Entity, FocusHandle, Window, deferred,
    div, prelude::*,
};

use crate::home_stats::HomeStats;

use crate::actions::{GoActions, GoChat, GoHome, GoSettings, GoTriggers, GoTwitch, SHELL_CONTEXT};
use crate::actions_screen::ScreenActionsView;
use crate::chat::ChatView;
use crate::chrome::Chrome;
use crate::discord_screen::DiscordScreenView;
use crate::event_feed::EventFeedView;
use crate::globals_view::GlobalsView;
use crate::home::HomeView;
use crate::hotkeys_screen::HotkeysScreenView;
use crate::integration_detail::{IntegrationDetail, ObsSignedOut, VTubeSignedOut};
use crate::integrations::{obs_builtin_object, vtube_builtin_object};
use crate::midi_screen::MidiScreenView;
use crate::obs_connect::ObsConnectView;
use crate::obs_credentials_form::ObsConnected;
use crate::overlays_screen::OverlaysView;
use crate::platforms::PlatformsView;
use crate::presentation::{ActivePresentation, Presentation};
use crate::queues::QueuesView;
use crate::runtime_handles::RuntimeHandles;
use crate::runtime_status::RuntimeStatus;
use crate::screen::Screen;
use crate::script_editor::ScriptEditorView;
use crate::server_console::ServerConsoleView;
use crate::settings::SettingsView;
use crate::sidebar::NavRequested;
use crate::soundboard::SoundboardView;
use crate::stream_apps::StreamAppsView;
use crate::toasts::Toasts;
use crate::topics::Topics;
use crate::triggers_screen::TriggersRegistryView;
use crate::tts::TtsView;
use crate::unavailable_builtin::unavailable_builtin;
use crate::vtube_connect::VTubeConnectView;
use crate::vtube_connect_form::VTubeConnected;

const TOAST_PRIORITY: usize = 2;
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
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let screen = Screen::Home;
        let content = Self::content_for(&screen, &topics, &handles, cx);
        let focus = cx.focus_handle();
        let chrome = Chrome::new(status, topics.platforms.clone(), screen.clone(), cx);

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

        window.focus(&focus, cx);

        Self {
            router: Router { screen, content },
            chrome,
            focus,
            topics,
            handles,
        }
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
                let home = cx.new(|cx| {
                    HomeView::new(
                        topics.home_stats.clone(),
                        home_backend,
                        home_registry,
                        home_rt,
                        cx,
                    )
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
                cx.new(|cx| {
                    ChatView::new(
                        topics.chat_feed.clone(),
                        topics.home_stats.clone(),
                        rt_handle,
                        viewer_repo,
                        action_engine,
                        voice_alias_repo,
                        speak,
                        palette,
                        cx,
                    )
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
            Screen::Platforms => {
                let platforms = cx.new(|cx| PlatformsView::new(topics.platforms.clone(), cx));
                cx.subscribe(&platforms, |this, _view, event: &NavRequested, cx| {
                    this.navigate(event.0.clone(), cx);
                })
                .detach();
                platforms.into()
            }
            Screen::StreamApps => {
                let apps = cx.new(|cx| StreamAppsView::new(topics.platforms.clone(), cx));
                cx.subscribe(&apps, |this, _view, event: &NavRequested, cx| {
                    this.navigate(event.0.clone(), cx);
                })
                .detach();
                apps.into()
            }
            Screen::BuiltinDetail(id) => {
                let builtin = match id.as_str() {
                    OBS_BUILTIN_ID => match handles.obs_install_seed.live() {
                        Some(client) => Some(obs_builtin_object(client)),
                        None => return Self::obs_connect_screen(handles, cx),
                    },
                    VTUBE_BUILTIN_ID => match handles.vtube_install_seed.live() {
                        Some(client) => Some(vtube_builtin_object(client)),
                        None => return Self::vtube_connect_screen(handles, cx),
                    },
                    MIDI_BUILTIN_ID => match handles.midi_client.clone() {
                        Some(client) => return Self::midi_screen(handles, client, cx),
                        None => handles.builtins.get(id),
                    },
                    HOTKEY_BUILTIN_ID => match handles.hotkey_client.clone() {
                        Some(client) => return Self::hotkeys_screen(handles, client, cx),
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
                let builtins = handles.builtins.clone();
                let twitch_install_seed = handles.twitch_install_seed.clone();
                let kick_install_seed = handles.kick_install_seed.clone();
                let youtube_install_seed = handles.youtube_install_seed.clone();
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
                        builtins,
                        twitch_install_seed,
                        kick_install_seed,
                        youtube_install_seed,
                        obs_install_seed,
                        vtube_install_seed,
                        connectivity,
                        cx,
                    )
                });
                cx.subscribe(&detail, |this, _view, event: &NavRequested, cx| {
                    this.navigate(event.0.clone(), cx);
                })
                .detach();
                cx.subscribe(&detail, |this, _view, _: &ObsConnected, cx| {
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
            Screen::Settings => {
                let handles = Arc::clone(handles);
                cx.new(|cx| SettingsView::new(handles, cx)).into()
            }
            Screen::Queues => {
                let queue_health = topics.queue_health.clone();
                let scheduler = handles.scheduler.clone();
                let queue_repo = handles.backend.queue_repo();
                let action_repo = handles.backend.action_repo();
                let rt_handle = handles.rt_handle.clone();
                cx.new(|cx| {
                    QueuesView::new(
                        queue_health,
                        scheduler,
                        queue_repo,
                        action_repo,
                        rt_handle,
                        cx,
                    )
                })
                .into()
            }
            Screen::Soundboard => {
                let player = handles.soundboard_player.clone();
                let clips_repo = handles.backend.soundboard_clips_repo();
                let settings_repo =
                    Arc::clone(&handles.backend) as Arc<dyn forge_storage::SettingsRepo>;
                let rt_handle = handles.rt_handle.clone();
                let bus = Arc::clone(&handles.bus);
                cx.new(|cx| {
                    SoundboardView::new(player, clips_repo, settings_repo, rt_handle, bus, cx)
                })
                .into()
            }
            Screen::Tts => {
                let speak_state = topics.speak.clone();
                let speak = handles.speak.clone();
                let backend = Arc::clone(&handles.backend);
                let rt_handle = handles.rt_handle.clone();
                let pipeline_config = handles.pipeline_config.clone();
                let tts_registry = handles.tts_registry.clone();
                cx.new(|cx| {
                    TtsView::new(
                        speak_state,
                        speak,
                        backend,
                        rt_handle,
                        pipeline_config,
                        tts_registry,
                        cx,
                    )
                })
                .into()
            }
            Screen::Overlays => {
                let repo = handles.backend.overlay_repo();
                let server = handles.server.clone();
                let rt_handle = handles.rt_handle.clone();
                let kinds = Arc::clone(&handles.overlay_kinds);
                let overlays = handles.overlays.clone();
                cx.new(|cx| OverlaysView::new(repo, server, rt_handle, kinds, overlays, cx))
                    .into()
            }
            Screen::Server => {
                let server = handles.server.clone();
                let rt_handle = handles.rt_handle.clone();
                let credentials: Arc<dyn CredentialsRepo> =
                    Arc::clone(&handles.backend) as Arc<dyn CredentialsRepo>;
                cx.new(|cx| ServerConsoleView::new(server, rt_handle, credentials, cx))
                    .into()
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
                let trigger_instance_repo = handles.backend.trigger_instance_repo();
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
                });
                cx.subscribe(&view, |this, _view, event: &NavRequested, cx| {
                    this.navigate(event.0.clone(), cx);
                })
                .detach();
                view.into()
            }
            Screen::Triggers(preselect) => {
                let repo = handles.backend.trigger_instance_repo();
                let action_repo = handles.backend.action_repo();
                let registry = handles.trigger_registry.clone();
                let settings_repo = Arc::clone(&handles.backend) as Arc<dyn SettingsRepo>;
                let rt_handle = handles.rt_handle.clone();
                let preselect = *preselect;
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
                let rt_handle = handles.rt_handle.clone();
                let editor = cx
                    .new(|cx| ScriptEditorView::new(backend, script_registry, bus, rt_handle, cx));
                cx.subscribe(&editor, |this, _view, event: &NavRequested, cx| {
                    this.navigate(event.0.clone(), cx);
                })
                .detach();
                editor.into()
            }
        }
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
            this.rebuild_current(cx);
        })
        .detach();
        connect.into()
    }

    fn hotkeys_screen(
        handles: &Arc<RuntimeHandles>,
        client: Arc<forge_hotkey::HotkeyClient>,
        cx: &mut Context<Self>,
    ) -> AnyView {
        let backend = Arc::clone(&handles.backend);
        let settings = Arc::clone(&handles.backend) as Arc<dyn SettingsRepo>;
        let bus = Arc::clone(&handles.bus);
        let rt_handle = handles.rt_handle.clone();
        cx.new(|cx| HotkeysScreenView::new(client, backend, settings, bus, rt_handle, cx))
            .into()
    }

    fn discord_screen(handles: &Arc<RuntimeHandles>, cx: &mut Context<Self>) -> AnyView {
        let client = Arc::clone(&handles.discord_client);
        let action_repo = handles.backend.action_repo();
        let bus = Arc::clone(&handles.bus);
        let rt_handle = handles.rt_handle.clone();
        cx.new(|cx| DiscordScreenView::new(client, action_repo, bus, rt_handle, cx))
            .into()
    }

    fn midi_screen(
        handles: &Arc<RuntimeHandles>,
        client: Arc<forge_midi::MidiClient>,
        cx: &mut Context<Self>,
    ) -> AnyView {
        let trigger_repo = handles.backend.trigger_instance_repo();
        let action_repo = handles.backend.action_repo();
        let settings = Arc::clone(&handles.backend) as Arc<dyn SettingsRepo>;
        let bus = Arc::clone(&handles.bus);
        let rt_handle = handles.rt_handle.clone();
        cx.new(|cx| {
            MidiScreenView::new(
                client,
                trigger_repo,
                action_repo,
                settings,
                bus,
                rt_handle,
                cx,
            )
        })
        .into()
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
            this.rebuild_current(cx);
        })
        .detach();
        connect.into()
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
        self.navigate(Screen::BuiltinDetail(BuiltinId::new("twitch")), cx);
    }

    fn go_settings(&mut self, _: &GoSettings, _: &mut Window, cx: &mut Context<Self>) {
        self.navigate(Screen::Settings, cx);
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

        let body = div()
            .w_full()
            .flex_1()
            .flex()
            .flex_row()
            .overflow_hidden()
            .child(self.chrome.sidebar.clone())
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
            .child(self.chrome.footer.clone());

        root.children(self.toast_host(cx))
    }
}
