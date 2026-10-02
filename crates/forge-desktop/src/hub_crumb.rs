use std::mem::discriminant;

use forge_components::{BreadcrumbCrumb, tr};
use forge_platform_core::IntegrationCategory;
use gpui::{ClickEvent, Context, ElementId, EventEmitter};

use crate::integration_catalog::core_features;
use crate::screen::Screen;
use crate::sidebar::NavRequested;

const WHOLE_HUB_ID: &str = "hub-crumb";

pub fn core_category(screen: &Screen) -> Option<IntegrationCategory> {
    core_features()
        .into_iter()
        .find(|feature| discriminant(&feature.screen) == discriminant(screen))
        .map(|feature| feature.category)
}

pub fn hub_crumb<V: EventEmitter<NavRequested>>(
    category: Option<IntegrationCategory>,
    cx: &mut Context<V>,
) -> BreadcrumbCrumb {
    let (label, id) = match category {
        Some(category) => (
            tr!(category.label_key()),
            ElementId::Name(format!("{WHOLE_HUB_ID}-{}", category.key()).into()),
        ),
        None => (
            tr!("integrations_breadcrumb"),
            ElementId::Name(WHOLE_HUB_ID.into()),
        ),
    };
    BreadcrumbCrumb::link(
        label,
        id,
        cx.listener(move |_, _: &ClickEvent, _, cx| {
            cx.emit(NavRequested(Screen::Integrations(category)));
        }),
    )
}
