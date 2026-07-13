use forge_platform_core::BuiltinId;
use gpui::{AnyView, AppContext, Context, Entity, FocusHandle, Window, div, prelude::*};

use crate::actions::{GoActions, GoChat, GoHome, GoSettings, GoTriggers, GoTwitch, SHELL_CONTEXT};
use crate::chat::ChatView;
use crate::chrome::Chrome;
use crate::event_feed::EventFeedView;
use crate::globals_view::GlobalsView;
use crate::home::HomeView;
use crate::integration_detail::IntegrationDetail;
use crate::platforms::PlatformsView;
use crate::presentation::{ActivePresentation, Presentation};
use crate::queues::QueuesView;
use crate::runtime_status::RuntimeStatus;
use crate::screen::Screen;
use crate::screen_stub::ScreenStub;
use crate::server_console::ServerConsoleView;
use crate::settings::SettingsView;
use crate::sidebar::NavRequested;
use crate::soundboard::SoundboardView;
use crate::stream_apps::StreamAppsView;
use crate::topics::Topics;
use crate::tts::TtsView;

/// Root shell view-entity. Holds the router discriminant, the single active-screen
/// child entity, the chrome bundle (title bar / sidebar / footer child entities),
/// its own focus handle, and the bridge-topics bundle — five top-level fields,
/// within the ≤5 budget. It owns no screen-internal or domain state; the routed
/// screen is a separate view-entity swapped on navigation, and the runtime→UI topic
/// caches live behind the `topics` bundle, handed to whichever screen consumes them.
pub struct AppShell {
    screen: Screen,
    content: AnyView,
    chrome: Chrome,
    focus: FocusHandle,
    /// The runtime→UI bridge topic caches (chat feed, home stats, …). Grouping them
    /// behind one field — as [`Chrome`] groups the chrome children — keeps the root
    /// within its ≤5-field budget while each topic persists across navigation. The
    /// fifth and last root field.
    topics: Topics,
}

impl AppShell {
    pub fn new(
        status: Entity<RuntimeStatus>,
        topics: Topics,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let screen = Screen::Home;
        let content = Self::content_for(&screen, &topics, cx);
        let focus = cx.focus_handle();
        let chrome = Chrome::new(status, screen.clone(), cx);

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

        window.focus(&focus);

        Self {
            screen,
            content,
            chrome,
            focus,
            topics,
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
    fn content_for(screen: &Screen, topics: &Topics, cx: &mut Context<Self>) -> AnyView {
        match screen {
            Screen::Home => {
                let home = cx.new(|cx| HomeView::new(topics.home_stats.clone(), cx));
                cx.subscribe(&home, |this, _home, event: &NavRequested, cx| {
                    this.navigate(event.0.clone(), cx);
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
            Screen::Globals => cx
                .new(|cx| GlobalsView::new(topics.globals.clone(), cx))
                .into(),
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
                let id = id.clone();
                let detail = cx.new(|cx| IntegrationDetail::new(id, cx));
                cx.subscribe(&detail, |this, _view, event: &NavRequested, cx| {
                    this.navigate(event.0.clone(), cx);
                })
                .detach();
                detail.into()
            }
            Screen::Settings => cx.new(SettingsView::new).into(),
            Screen::Queues => cx.new(QueuesView::new).into(),
            Screen::Soundboard => cx.new(SoundboardView::new).into(),
            Screen::Tts => cx.new(TtsView::new).into(),
            Screen::Server => cx.new(ServerConsoleView::new).into(),
            other => cx.new(|_| ScreenStub::new(other.clone())).into(),
        }
    }

    /// Routes to `screen`: swaps the active-screen child and pushes the confirmed
    /// selection back into the sidebar so its highlight tracks the single source of
    /// truth (this root's `screen`). A no-op when already there.
    fn navigate(&mut self, screen: Screen, cx: &mut Context<Self>) {
        if self.screen == screen {
            return;
        }
        self.content = Self::content_for(&screen, &self.topics, cx);
        self.chrome.sidebar.update(cx, |sidebar, cx| {
            sidebar.set_current(screen.clone());
            cx.notify();
        });
        self.screen = screen;
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
                    .child(self.content.clone()),
            );

        div()
            .key_context(SHELL_CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::go_home))
            .on_action(cx.listener(Self::go_chat))
            .on_action(cx.listener(Self::go_actions))
            .on_action(cx.listener(Self::go_triggers))
            .on_action(cx.listener(Self::go_twitch))
            .on_action(cx.listener(Self::go_settings))
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(self.chrome.titlebar.clone())
            .child(body)
            .child(self.chrome.footer.clone())
    }
}
