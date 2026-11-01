use super::viewer_drawer::ViewerDrawer;
use super::{ChatView, PlatformFilter};
use crate::home_stats::Integration;
use crate::integration_lifecycle::IntegrationLifecycle;
use forge_runtime::LifecycleState;
use forge_types::PlatformId;
use gpui::{App, Context, Entity};
use std::sync::Arc;

pub(crate) fn platform_integration(platform: PlatformId) -> Integration {
    match platform {
        PlatformId::Twitch => Integration::Twitch,
        PlatformId::YouTube => Integration::YouTube,
        PlatformId::Kick => Integration::Kick,
    }
}

pub(crate) fn platform_switched_on(
    lifecycle: Option<&IntegrationLifecycle>,
    platform: PlatformId,
) -> bool {
    lifecycle.is_none_or(|lifecycle| lifecycle.is_on(&platform_integration(platform).builtin_id()))
}

pub(crate) fn platform_is_running(
    lifecycle: Option<&IntegrationLifecycle>,
    platform: PlatformId,
) -> bool {
    lifecycle.is_none_or(|lifecycle| {
        lifecycle.state_of(&platform_integration(platform).builtin_id()) == LifecycleState::Running
    })
}

impl ChatView {
    #[must_use]
    pub(crate) fn with_lifecycle(
        mut self,
        lifecycle: Entity<IntegrationLifecycle>,
        cx: &mut Context<Self>,
    ) -> Self {
        self.subscriptions._lifecycle_obs = Some(cx.observe(&lifecycle, |this, _, cx| {
            this.on_lifecycle_changed(cx);
        }));
        self.composer.update(cx, |composer, cx| {
            composer.set_lifecycle(lifecycle.clone(), cx);
        });
        self.drawer
            .update(cx, |drawer, _| drawer.set_lifecycle(lifecycle.clone()));
        self.lifecycle = Some(lifecycle);
        self.on_lifecycle_changed(cx);
        self
    }

    #[must_use]
    pub(crate) fn with_chat_bus(
        self,
        bus: Arc<forge_runtime::EventBus>,
        cx: &mut Context<Self>,
    ) -> Self {
        self.composer
            .update(cx, |composer, cx| composer.set_bus(bus, cx));
        self
    }

    #[must_use]
    pub(crate) fn with_twitch_bot(
        self,
        presence: Entity<crate::twitch_bot_presence::TwitchBotPresence>,
        cx: &mut Context<Self>,
    ) -> Self {
        self.composer
            .update(cx, |composer, cx| composer.set_twitch_bot(presence, cx));
        self
    }

    pub(crate) fn platform_enabled(&self, platform: PlatformId, cx: &App) -> bool {
        platform_switched_on(self.lifecycle.as_ref().map(|l| l.read(cx)), platform)
    }

    pub(crate) fn filter_offered(&self, filter: PlatformFilter, cx: &App) -> bool {
        match filter {
            PlatformFilter::All => true,
            PlatformFilter::Single(platform) => self.platform_enabled(platform, cx),
        }
    }

    fn on_lifecycle_changed(&mut self, cx: &mut Context<Self>) {
        let filter = self.timeline.read(cx).platform_filter;
        if !self.filter_offered(filter, cx) {
            self.timeline.update(cx, |timeline, cx| {
                timeline.set_platform_filter(PlatformFilter::All, cx)
            });
        }
        self.drawer
            .update(cx, |drawer, cx| drawer.request_selected_follow(cx));
        cx.notify();
    }
}

impl ViewerDrawer {
    pub(super) fn platform_running(&self, platform: PlatformId, cx: &App) -> bool {
        platform_is_running(self.lifecycle.as_ref().map(|l| l.read(cx)), platform)
    }
}
