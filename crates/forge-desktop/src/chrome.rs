use gpui::{AppContext, Context, Entity};

use crate::awake_state::AwakeState;
use crate::event_loss::EventLoss;
use crate::footer::Footer;
use crate::integration_lifecycle::IntegrationLifecycle;
use crate::platforms::PlatformConnectivity;
use crate::runtime_status::RuntimeStatus;
use crate::screen::Screen;
use crate::shell::AppShell;
use crate::sidebar::SidebarNav;
use crate::titlebar::TitleBar;

pub struct Chrome {
    pub titlebar: Entity<TitleBar>,
    pub sidebar: Entity<SidebarNav>,
    pub footer: Entity<Footer>,
}

impl Chrome {
    pub fn new(
        status: Entity<RuntimeStatus>,
        connectivity: Entity<PlatformConnectivity>,
        event_loss: Entity<EventLoss>,
        awake: Entity<AwakeState>,
        lifecycle: Entity<IntegrationLifecycle>,
        current: Screen,
        cx: &mut Context<AppShell>,
    ) -> Self {
        let titlebar = cx.new(TitleBar::new);
        let sidebar = cx.new(|cx| SidebarNav::new(current, connectivity.clone(), lifecycle, cx));
        let footer = cx.new(|cx| Footer::new(status, connectivity, event_loss, awake, cx));
        Self {
            titlebar,
            sidebar,
            footer,
        }
    }
}
