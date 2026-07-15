use gpui::{AppContext, Context, Entity};

use crate::footer::Footer;
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
        current: Screen,
        cx: &mut Context<AppShell>,
    ) -> Self {
        let titlebar = cx.new(TitleBar::new);
        let sidebar = cx.new(|cx| SidebarNav::new(current, connectivity.clone(), cx));
        let footer = cx.new(|cx| Footer::new(status, connectivity, cx));
        Self {
            titlebar,
            sidebar,
            footer,
        }
    }
}
