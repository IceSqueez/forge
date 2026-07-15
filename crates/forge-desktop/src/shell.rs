use std::sync::Arc;

use forge_components::{Density, FOOTER_HEIGHT, Spacing, spacing, toast_card};
use forge_platform_core::BuiltinId;
use forge_runtime::dashboard::compute_stats;
use forge_storage::{CredentialsRepo, DataProvider, GlobalsRepo};
use gpui::{
    AnyElement, AnyView, App, AppContext, AsyncApp, Context, Entity, FocusHandle, Window, deferred,
    div, prelude::*,
};

use crate::home_stats::HomeStats;

use crate::actions::{GoActions, GoChat, GoHome, GoSettings, GoTriggers, GoTwitch, SHELL_CONTEXT};
use crate::actions_screen::ScreenActionsView;
use crate::chat::ChatView;
use crate::chrome::Chrome;
use crate::event_feed::EventFeedView;
use crate::globals_view::GlobalsView;
use crate::home::HomeView;
use crate::integration_detail::IntegrationDetail;
use crate::integration_seed;
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

/// Draw priority for the deferred toast host. One above the overlay priority so a
/// transient notification floats over an open modal rather than behind it.
const TOAST_PRIORITY: usize = 2;

/// The active-screen router pair: the current [`Screen`] discriminant and the single
/// child view-entity rendering it (erased to [`AnyView`]). Bundling the two into one
/// field keeps [`AppShell`] within its ≤5 top-level field budget alongside the three
/// aggregation bundles (chrome / topics / handles) and the focus handle.
struct Router {
    screen: Screen,
    content: AnyView,
}

/// Root shell view-entity. Holds the active-screen router pair, the chrome bundle
/// (title bar / sidebar / footer child entities), its own focus handle, the bridge-
/// topics bundle, and the inbound runtime handle bundle — five top-level fields,
/// within the ≤5 budget. It owns no screen-internal or domain state; the routed
/// screen is a separate view-entity swapped on navigation, the runtime→UI topic
/// caches live behind the `topics` bundle, and the runtime command/read handles live
/// behind the `handles` bundle — each handed to whichever screen consumes them.
pub struct AppShell {
    router: Router,
    chrome: Chrome,
    focus: FocusHandle,
    /// The runtime→UI bridge topic caches (chat feed, home stats, …). Grouping them
    /// behind one field — as [`Chrome`] groups the chrome children — keeps the root
    /// within its ≤5-field budget while each topic persists across navigation.
    topics: Topics,
    /// The inbound grouping of the runtime's command/read handles (including the live
    /// `builtins` trait-object map). A shared `Arc` — the window root holds the other
    /// clone to keep the runtime alive — from which `content_for` hands each screen the
    /// handle(s) it consumes.
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

        // The sidebar voices navigation intent; the root is the sole router owner.
        cx.subscribe(
            &chrome.sidebar,
            |this, _sidebar, event: &NavRequested, cx| {
                this.navigate(event.0.clone(), cx);
            },
        )
        .detach();

        // Repaint when the presentation global (theme / density) is replaced.
        cx.observe_global::<Presentation>(|_, cx| cx.notify())
            .detach();

        // Repaint the toast host whenever a toast is pushed or dismissed.
        cx.observe_global::<Toasts>(|_, cx| cx.notify()).detach();

        window.focus(&focus);

        Self {
            router: Router { screen, content },
            chrome,
            focus,
            topics,
            handles,
        }
    }

    /// Builds the active-screen child view for `screen`, erased to [`AnyView`] so
    /// the router holds one field across heterogeneous screen types. Home gets the
    /// real [`HomeView`] (fed the shared home-stats topic); Chat gets the real
    /// [`ChatView`] (fed the shared chat feed + the active palette); every other
    /// destination still routes to the placeholder until its screen lands.
    ///
    /// Screens that voice navigation intent do so through [`NavRequested`]; Home is
    /// wired here the same way the sidebar is — the shell subscribes and routes, so
    /// the active screen stays single-sourced on this root.
    ///
    /// A [`Screen::BuiltinDetail`] is fed the live `Builtin*` trait objects for its id
    /// from the runtime `handles`; an id absent from the map (no credentials, or bring-
    /// up failed) falls back to the static [`integration_seed`] so the detail still
    /// opens with a real visual frame.
    fn content_for(
        screen: &Screen,
        topics: &Topics,
        handles: &Arc<RuntimeHandles>,
        cx: &mut Context<Self>,
    ) -> AnyView {
        match screen {
            Screen::Home => {
                let home = cx.new(|cx| HomeView::new(topics.home_stats.clone(), cx));
                cx.subscribe(&home, |this, _home, event: &NavRequested, cx| {
                    this.navigate(event.0.clone(), cx);
                })
                .detach();
                // Refresh the at-a-glance readout every time Home is mounted: once at
                // shell construction (the initial Home screen) and again on each
                // navigation back to Home, matching the dashboard's mount-time pull
                // cadence. The pull is an async snapshot off the storage repos, applied
                // back into the shared home-stats topic.
                let home_stats = topics.home_stats.clone();
                let backend = Arc::clone(&handles.backend);
                cx.spawn(async move |_shell, cx| {
                    refresh_dashboard_stats(home_stats, backend, cx).await;
                })
                .detach();
                home.into()
            }
            Screen::Chat => {
                let palette = cx.palette();
                cx.new(|cx| ChatView::new(topics.chat_feed.clone(), palette, cx))
                    .into()
            }
            Screen::EventFeed => cx
                .new(|cx| EventFeedView::new(topics.event_log.clone(), cx))
                .into(),
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
                let connectivity = topics.platforms.clone();
                let detail = match handles.builtins.get(id) {
                    Some(obj) => {
                        let icon = obj.icon.clone();
                        let status = obj.status.clone();
                        let health = obj.health.clone();
                        let content = obj.content.clone();
                        let quick = obj.quick.clone();
                        let control = obj.control.clone();
                        let obs_client = obj.obs_client.clone();
                        let rt_handle = handles.rt_handle.clone();
                        let action_engine = handles.action_engine.clone();
                        cx.new(|cx| {
                            IntegrationDetail::new(
                                icon,
                                status,
                                health,
                                content,
                                quick,
                                control,
                                obs_client,
                                rt_handle,
                                action_engine,
                                connectivity,
                                cx,
                            )
                        })
                    }
                    None => {
                        let seed = integration_seed::seed(id);
                        let rt_handle = handles.rt_handle.clone();
                        let action_engine = handles.action_engine.clone();
                        cx.new(|cx| {
                            IntegrationDetail::new(
                                seed.icon,
                                seed.status,
                                seed.health,
                                seed.content,
                                seed.quick,
                                None,
                                None,
                                rt_handle,
                                action_engine,
                                connectivity,
                                cx,
                            )
                        })
                    }
                };
                cx.subscribe(&detail, |this, _view, event: &NavRequested, cx| {
                    this.navigate(event.0.clone(), cx);
                })
                .detach();
                detail.into()
            }
            Screen::Settings => cx.new(SettingsView::new).into(),
            Screen::Queues => {
                let queue_health = topics.queue_health.clone();
                let scheduler = handles.scheduler.clone();
                let bus = Arc::clone(&handles.bus);
                let queue_repo = handles.backend.queue_repo();
                let action_repo = handles.backend.action_repo();
                let rt_handle = handles.rt_handle.clone();
                cx.new(|cx| {
                    QueuesView::new(
                        queue_health,
                        scheduler,
                        bus,
                        queue_repo,
                        action_repo,
                        rt_handle,
                        cx,
                    )
                })
                .into()
            }
            Screen::Soundboard => cx.new(SoundboardView::new).into(),
            Screen::Tts => {
                let speak_state = topics.speak.clone();
                let speak = handles.speak.clone();
                let backend = Arc::clone(&handles.backend);
                let rt_handle = handles.rt_handle.clone();
                cx.new(|cx| TtsView::new(speak_state, speak, backend, rt_handle, cx))
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
            Screen::Actions => {
                let action_repo = handles.backend.action_repo();
                let queue_repo = handles.backend.queue_repo();
                let actions_service = Arc::new(forge_runtime::actions::ActionsService::new(
                    handles.backend.action_repo(),
                    handles.backend.queue_repo(),
                    handles.backend.history_repo(),
                    handles.backend.trigger_instance_repo(),
                    handles.backend.soundboard_clips_repo(),
                ));
                let sub_action_registry = handles.sub_action_registry.clone();
                let trigger_registry = handles.trigger_registry.clone();
                let rt_handle = handles.rt_handle.clone();
                let bus = Arc::clone(&handles.bus);
                cx.new(|cx| {
                    ScreenActionsView::new(
                        action_repo,
                        queue_repo,
                        actions_service,
                        sub_action_registry,
                        trigger_registry,
                        rt_handle,
                        bus,
                        cx,
                    )
                })
                .into()
            }
            Screen::Triggers => {
                let repo = handles.backend.trigger_instance_repo();
                let action_repo = handles.backend.action_repo();
                let registry = handles.trigger_registry.clone();
                let rt_handle = handles.rt_handle.clone();
                cx.new(|cx| TriggersRegistryView::new(repo, action_repo, registry, rt_handle, cx))
                    .into()
            }
            Screen::Scripts => {
                let editor = cx.new(ScriptEditorView::new);
                cx.subscribe(&editor, |this, _view, event: &NavRequested, cx| {
                    this.navigate(event.0.clone(), cx);
                })
                .detach();
                editor.into()
            }
        }
    }

    /// Routes to `screen`: swaps the active-screen child and pushes the confirmed
    /// selection back into the sidebar so its highlight tracks the single source of
    /// truth (this root's `screen`). A no-op when already there.
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
        self.navigate(Screen::Actions, cx);
    }

    fn go_triggers(&mut self, _: &GoTriggers, _: &mut Window, cx: &mut Context<Self>) {
        self.navigate(Screen::Triggers, cx);
    }

    fn go_twitch(&mut self, _: &GoTwitch, _: &mut Window, cx: &mut Context<Self>) {
        self.navigate(Screen::BuiltinDetail(BuiltinId::new("twitch")), cx);
    }

    fn go_settings(&mut self, _: &GoSettings, _: &mut Window, cx: &mut Context<Self>) {
        self.navigate(Screen::Settings, cx);
    }

    /// Builds the bottom-right toast host from the `Toasts` global, or `None` when the
    /// queue is empty. Each card's dismiss and action controls capture only the toast
    /// id and reach back into the global at click time, so this borrows the queue
    /// immutably while it renders. The host draws in a deferred pass above ordinary
    /// content (and any open overlay).
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
            let mut card = toast_card(data.kind, data.message.clone(), &palette).on_dismiss(
                ("toast-dismiss", id as usize),
                move |_, _, cx: &mut App| {
                    cx.global_mut::<Toasts>().dismiss(id);
                },
            );
            if let Some(glyph) = data.icon {
                card = card.icon(glyph);
            }
            if let Some(action) = &data.action {
                card = card.action(
                    ("toast-action", id as usize),
                    action.label.clone(),
                    move |_, window, cx: &mut App| {
                        // Taking the toast out both removes it and yields its owned
                        // callback, which then runs against the freed context.
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

/// Async-pull refresh of the Home at-a-glance readout. Reads the action/global counts
/// and the trailing-24h fired-runs total off the storage repos (all awaited, never
/// blocking the foreground executor), then folds the snapshot into the shared home-stats
/// topic and repaints Home only when a value moved. A load failure logs and leaves the
/// prior readout in place; a released topic entity makes the apply a no-op.
async fn refresh_dashboard_stats(
    home_stats: Entity<HomeStats>,
    backend: Arc<dyn DataProvider>,
    cx: &mut AsyncApp,
) {
    let actions = backend.action_repo();
    let globals: Arc<dyn GlobalsRepo> = Arc::clone(&backend) as Arc<dyn GlobalsRepo>;
    let history = backend.history_repo();
    match compute_stats(&*actions, &*globals, &*history).await {
        Ok(stats) => {
            let _ = home_stats.update(cx, |stats_topic, cx| {
                if stats_topic.set_stats(stats) {
                    cx.notify();
                }
            });
        }
        Err(err) => {
            eprintln!("forge-desktop: home dashboard stats load failed: {err}");
        }
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
            // Positioning context the bottom-right toast host anchors against.
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
