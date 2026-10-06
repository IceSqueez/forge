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
