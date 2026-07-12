use gpui::{AnyView, AppContext, Context, Entity, FocusHandle, Window, div, prelude::*};

use crate::actions::{GoActions, GoChat, GoHome, GoSettings, GoTriggers, GoTwitch, SHELL_CONTEXT};
use crate::chat::ChatView;
use crate::chat_feed::ChatFeed;
use crate::chrome::Chrome;
use crate::presentation::{ActivePresentation, Presentation};
use crate::runtime_status::RuntimeStatus;
use crate::screen::Screen;
use crate::screen_stub::ScreenStub;
use crate::sidebar::NavRequested;

/// Root shell view-entity. Holds the router discriminant, the single active-screen
/// child entity, the chrome bundle (title bar / sidebar / footer child entities),
/// and its own focus handle — four top-level fields, within the ≤5 budget. It owns
/// no screen-internal or domain state; the routed screen is a separate view-entity
/// swapped on navigation, and the runtime→UI topic entities live behind the chrome
/// children (the footer) or the boot bridge, never inlined here.
pub struct AppShell {
    screen: Screen,
    content: AnyView,
    chrome: Chrome,
    focus: FocusHandle,
    /// Chat-topic feed handle, injected into the [`ChatView`] on every route to
    /// Chat so the seed and any bridged messages persist across navigation. A
    /// runtime handle, not screen-internal state — the fifth and last root field.
    chat_feed: Entity<ChatFeed>,
}

impl AppShell {
    pub fn new(
        status: Entity<RuntimeStatus>,
        chat_feed: Entity<ChatFeed>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let screen = Screen::Home;
        let content = Self::content_for(screen, &chat_feed, cx);
        let focus = cx.focus_handle();
        let chrome = Chrome::new(status, screen, cx);

        // The sidebar voices navigation intent; the root is the sole router owner.
        cx.subscribe(
            &chrome.sidebar,
            |this, _sidebar, event: &NavRequested, cx| {
                this.navigate(event.0, cx);
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
            chat_feed,
        }
    }

    /// Builds the active-screen child view for `screen`, erased to [`AnyView`] so
    /// the router holds one field across heterogeneous screen types. Chat gets the
    /// real [`ChatView`] (fed the shared topic + the active palette); every other
    /// destination still routes to the placeholder until its screen lands.
    fn content_for(screen: Screen, feed: &Entity<ChatFeed>, cx: &mut Context<Self>) -> AnyView {
        match screen {
            Screen::Chat => {
                let palette = cx.palette();
                cx.new(|cx| ChatView::new(feed.clone(), palette, cx)).into()
            }
            _ => cx.new(|_| ScreenStub::new(screen)).into(),
        }
    }

    /// Routes to `screen`: swaps the active-screen child and pushes the confirmed
    /// selection back into the sidebar so its highlight tracks the single source of
    /// truth (this root's `screen`). A no-op when already there.
    fn navigate(&mut self, screen: Screen, cx: &mut Context<Self>) {
        if self.screen == screen {
            return;
        }
        self.screen = screen;
        self.content = Self::content_for(screen, &self.chat_feed, cx);
        self.chrome.sidebar.update(cx, |sidebar, cx| {
            sidebar.set_current(screen);
            cx.notify();
        });
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
        self.navigate(Screen::Twitch, cx);
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
