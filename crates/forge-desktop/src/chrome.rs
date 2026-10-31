use gpui::{AppContext, Context, Entity};

use crate::awake_state::AwakeState;
use crate::event_loss::EventLoss;
use crate::footer::Footer;
use crate::integration_lifecycle::IntegrationLifecycle;
use crate::navigation::Screen;
use crate::platforms::PlatformConnectivity;
use crate::runtime_status::RuntimeStatus;
use crate::shell::AppShell;
use crate::sidebar::SidebarNav;
use crate::titlebar::TitleBar;

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
