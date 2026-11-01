use gpui::{AppContext, Context, Entity};

use crate::app::awake_state::AwakeState;
use crate::app::footer::Footer;
use crate::app::navigation::Screen;
use crate::app::runtime_status::RuntimeStatus;
use crate::app::shell::AppShell;
use crate::app::sidebar::SidebarNav;
use crate::app::titlebar::TitleBar;
use crate::audience::event_loss::EventLoss;
use crate::integrations::integration_lifecycle::IntegrationLifecycle;
use crate::integrations::platforms::PlatformConnectivity;

pub(crate) struct Chrome {
    pub titlebar: Entity<TitleBar>,
    pub sidebar: Entity<SidebarNav>,
    pub footer: Entity<Footer>,
}

impl Chrome {
    pub(crate) fn new(
        status: Entity<RuntimeStatus>,
        connectivity: Entity<PlatformConnectivity>,
        event_loss: Entity<EventLoss>,
        awake: Entity<AwakeState>,
        lifecycle: Entity<IntegrationLifecycle>,
        current: Screen,
        cx: &mut Context<AppShell>,
    ) -> Self {
        let titlebar = cx.new(TitleBar::new);
        let sidebar =
            cx.new(|cx| SidebarNav::new(current, connectivity.clone(), lifecycle.clone(), cx));
        let footer =
            cx.new(|cx| Footer::new(status, connectivity, event_loss, awake, lifecycle, cx));
        Self {
            titlebar,
            sidebar,
            footer,
        }
    }
}
