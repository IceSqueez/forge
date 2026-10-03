use super::*;
use crate::home_stats::Integration;
use crate::integration_lifecycle::IntegrationLifecycle;

pub(crate) fn platform_integration(platform: Platform) -> Integration {
    match platform {
        Platform::Twitch => Integration::Twitch,
        Platform::YouTube => Integration::YouTube,
        Platform::Kick => Integration::Kick,
    }
}

impl ChatView {
    #[must_use]
    pub fn with_lifecycle(
        mut self,
        lifecycle: Entity<IntegrationLifecycle>,
        cx: &mut Context<Self>,
    ) -> Self {
        self._lifecycle_obs = Some(cx.observe(&lifecycle, |this, _, cx| {
            this.on_lifecycle_changed(cx);
        }));
        self.composer.update(cx, |composer, cx| {
            composer.set_lifecycle(lifecycle.clone(), cx);
        });
        self.lifecycle = Some(lifecycle);
        self.on_lifecycle_changed(cx);
        self
    }

    #[must_use]
    pub fn with_chat_bus(self, bus: Arc<forge_runtime::EventBus>, cx: &mut Context<Self>) -> Self {
        self.composer
            .update(cx, |composer, cx| composer.set_bus(bus, cx));
        self
    }

    pub(crate) fn platform_enabled(&self, platform: Platform, cx: &App) -> bool {
        match &self.lifecycle {
            Some(lifecycle) => lifecycle
                .read(cx)
                .is_on(&platform_integration(platform).builtin_id()),
            None => true,
        }
    }

    pub(crate) fn filter_offered(&self, filter: PlatformFilter, cx: &App) -> bool {
        match filter {
            PlatformFilter::All => true,
            PlatformFilter::Single(platform) => self.platform_enabled(platform, cx),
        }
    }

    fn on_lifecycle_changed(&mut self, cx: &mut Context<Self>) {
        if !self.filter_offered(self.platform_filter, cx) {
            self.platform_filter = PlatformFilter::All;
            self.reset_chat_list(cx);
        }
        cx.notify();
    }
}
