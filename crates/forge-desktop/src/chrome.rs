use gpui::{AppContext, Context, Entity};

use crate::footer::Footer;
use crate::runtime_status::RuntimeStatus;
use crate::screen::Screen;
use crate::shell::AppShell;
use crate::sidebar::SidebarNav;
use crate::titlebar::TitleBar;

/// Plain grouping of the three persistent chrome view-entities (title bar, left
/// nav rail, footer). It is a state-less handle bundle, not a view: the root
/// [`AppShell`] places each child in its own layout slot around the routed
/// content. Grouping them behind one field keeps the root within its ≤5 top-level
/// field budget while every chrome region stays a genuine child
/// view-entity.
pub struct Chrome {
    pub titlebar: Entity<TitleBar>,
    pub sidebar: Entity<SidebarNav>,
    pub footer: Entity<Footer>,
}

impl Chrome {
    pub fn new(status: Entity<RuntimeStatus>, current: Screen, cx: &mut Context<AppShell>) -> Self {
        let titlebar = cx.new(TitleBar::new);
        let sidebar = cx.new(|cx| SidebarNav::new(current, cx));
        let footer = cx.new(|cx| Footer::new(status, cx));
        Self {
            titlebar,
            sidebar,
            footer,
        }
    }
}
