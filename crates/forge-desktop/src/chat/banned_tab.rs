use std::sync::Arc;

use forge_components::{ForgePalette, Icon, segment, segmented, tr};
use forge_runtime::EventBus;
use gpui::{AnyElement, AppContext, ClickEvent, Context, Entity, IntoElement};

use super::ChatView;
use super::banned_panel::BannedPanel;
use crate::integrations::BuiltinRegistry;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ChatTab {
    #[default]
    Feed,
    Banned,
}

#[derive(Default)]
pub(crate) struct BannedHost {
    pub tab: ChatTab,
    builtins: BuiltinRegistry,
    bus: Option<Arc<EventBus>>,
    panel: Option<Entity<BannedPanel>>,
}

impl BannedHost {
    pub fn panel(&self) -> Option<&Entity<BannedPanel>> {
        self.panel.as_ref().filter(|_| self.tab == ChatTab::Banned)
    }
}

impl ChatView {
    #[must_use]
    pub fn with_ban_list(mut self, builtins: BuiltinRegistry, bus: Arc<EventBus>) -> Self {
        self.banned.builtins = builtins;
        self.banned.bus = Some(bus);
        self
    }

    pub(crate) fn set_tab(&mut self, tab: ChatTab, cx: &mut Context<Self>) {
        self.banned.tab = tab;
        if tab == ChatTab::Banned && self.banned.panel.is_none() {
            let builtins = self.banned.builtins.clone();
            let lifecycle = self.lifecycle.clone();
            let bus = self.banned.bus.clone();
            let rt_handle = self.rt_handle.clone();
            let action_engine = self.action_engine.clone();
            self.banned.panel = Some(cx.new(|cx| {
                BannedPanel::new(builtins, lifecycle, bus, rt_handle, action_engine, cx)
            }));
        }
        cx.notify();
    }

    pub(super) fn render_tabs(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let active = self.banned.tab;
        segmented(
            vec![
                segment(
                    "chat-tab-feed",
                    tr!("chat_tab_feed"),
                    active == ChatTab::Feed,
                    cx.listener(|this, _: &ClickEvent, _, cx| this.set_tab(ChatTab::Feed, cx)),
                )
                .icon(Icon::MessageCircle),
                segment(
                    "chat-tab-banned",
                    tr!("chat_tab_banned"),
                    active == ChatTab::Banned,
                    cx.listener(|this, _: &ClickEvent, _, cx| this.set_tab(ChatTab::Banned, cx)),
                )
                .icon(Icon::Ban),
            ],
            palette,
        )
        .into_any_element()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::sync::Arc;

    use forge_runtime::EventBus;
    use forge_storage::chat_history::MockChatHistoryRepo;
    use gpui::{Entity, TestAppContext};

    use super::ChatTab;
    use crate::chat::ChatView;
    use crate::chat::banned_panel::tests::{FakeBans, registry_with, settle};
    use crate::chat::tests::mount_gated;
    use crate::home_stats::Integration;
    use crate::test_support::{StubEventLog, runtime};

    fn mount_with_bans(
        cx: &mut TestAppContext,
        rt: &tokio::runtime::Runtime,
        source: &Arc<FakeBans>,
    ) -> Entity<ChatView> {
        let (_feed, view) = mount_gated(cx, rt, None, MockChatHistoryRepo::new());
        let builtins = registry_with(&[(Integration::Twitch, Some(Arc::clone(source)))]);
        view.update(cx, |view, _| {
            view.banned.builtins = builtins;
            view.banned.bus = Some(EventBus::new(Arc::new(StubEventLog)));
        });
        view
    }

    fn show(cx: &mut TestAppContext, view: &Entity<ChatView>, tab: ChatTab) {
        view.update(cx, |view, cx| view.set_tab(tab, cx));
    }

    fn panel_shown(cx: &mut TestAppContext, view: &Entity<ChatView>) -> bool {
        view.read_with(cx, |view, _| view.banned.panel().is_some())
    }

    #[gpui::test]
    fn the_ban_list_is_not_fetched_while_the_feed_tab_is_shown(cx: &mut TestAppContext) {
        let rt = runtime();
        let source = FakeBans::answering(Vec::new());
        let view = mount_with_bans(cx, &rt, &source);

        show(cx, &view, ChatTab::Feed);
        settle(cx, &rt);

        assert_eq!(source.calls(), 0);
        assert!(!panel_shown(cx, &view));
    }

    #[gpui::test]
    fn opening_the_banned_tab_lists_once_and_reopening_it_reuses_the_panel(
        cx: &mut TestAppContext,
    ) {
        let rt = runtime();
        let source = FakeBans::answering(Vec::new());
        let view = mount_with_bans(cx, &rt, &source);

        show(cx, &view, ChatTab::Banned);
        settle(cx, &rt);
        assert_eq!(source.calls(), 1);

        show(cx, &view, ChatTab::Feed);
        assert!(!panel_shown(cx, &view));
        show(cx, &view, ChatTab::Banned);
        settle(cx, &rt);

        assert!(panel_shown(cx, &view));
        assert_eq!(source.calls(), 1);
    }
}
