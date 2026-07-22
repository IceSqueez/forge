use gpui::{
    AnyElement, App, ClickEvent, ElementId, InteractiveElement, IntoElement, ParentElement, Pixels,
    RenderOnce, Rgba, SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::icons::{Icon, icon};
use crate::palette::ForgePalette;
use crate::tokens::{BORDER_THIN, DEFAULT_BODY_FAMILY, Density, FONT_SM, Spacing, spacing};

type CrumbClick = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

const HOME_ICON_SIZE: Pixels = px(13.0);

const SEPARATOR_ICON_SIZE: Pixels = px(11.0);

pub struct BreadcrumbCrumb {
    label: SharedString,
    id: Option<ElementId>,
    on_click: Option<CrumbClick>,
}

impl BreadcrumbCrumb {
    pub fn leaf(label: impl Into<SharedString>) -> Self {
        BreadcrumbCrumb {
            label: label.into(),
            id: None,
            on_click: None,
        }
    }

    pub fn link(
        label: impl Into<SharedString>,
        id: impl Into<ElementId>,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        BreadcrumbCrumb {
            label: label.into(),
            id: Some(id.into()),
            on_click: Some(Box::new(handler)),
        }
    }
}

#[derive(IntoElement)]
pub struct Breadcrumb {
    crumbs: Vec<BreadcrumbCrumb>,
    right: Option<AnyElement>,
    density: Density,
    shell: Rgba,
    border: Rgba,
    faint: Rgba,
    current: Rgba,
    ancestor: Rgba,
}

/// The trail is ordered root-first, current-location last (ink is position-based on that order).
pub fn breadcrumb(crumbs: Vec<BreadcrumbCrumb>, palette: &ForgePalette) -> Breadcrumb {
    Breadcrumb {
        crumbs,
        right: None,
        density: Density::default(),
        shell: palette.shell,
        border: palette.border_regular,
        faint: palette.text_faint,
        current: palette.text_primary,
        ancestor: palette.text_muted,
    }
}

impl Breadcrumb {
    #[must_use]
    pub fn right(mut self, el: impl IntoElement) -> Self {
        self.right = Some(el.into_any_element());
        self
    }

    #[must_use]
    pub fn density(mut self, density: Density) -> Self {
        self.density = density;
        self
    }
}

impl RenderOnce for Breadcrumb {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let d = self.density;
        let shell = self.shell;
        let border = self.border;
        let faint = self.faint;
        let current = self.current;
        let ancestor = self.ancestor;
        let crumbs = self.crumbs;
        let right = self.right;

        let gap = spacing(Spacing::Xs, d);
        let last = crumbs.len().saturating_sub(1);

        let mut trail =
            div()
                .flex()
                .items_center()
                .gap(gap)
                .child(icon(Icon::Home, HOME_ICON_SIZE, faint));

        for (i, crumb) in crumbs.into_iter().enumerate() {
            let ink = if i == last { current } else { ancestor };

            trail = trail.child(icon(Icon::ChevronRight, SEPARATOR_ICON_SIZE, faint));

            let label = div()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
                .text_color(ink);

            let label_el = match (crumb.id, crumb.on_click) {
                (Some(id), Some(handler)) => label
                    .id(id)
                    .cursor_pointer()
                    .hover(move |s| s.text_color(current))
                    .on_click(handler)
                    .child(crumb.label)
                    .into_any_element(),
                _ => label.child(crumb.label).into_any_element(),
            };

            trail = trail.child(label_el);
        }

        let mut inner = div().w_full().flex().items_center().child(trail);
        if let Some(right) = right {
            inner = inner.justify_between().child(right);
        }

        div()
            .w_full()
            .py(spacing(Spacing::Sm, d))
            .px(spacing(Spacing::Md, d))
            .border(BORDER_THIN)
            .border_color(border)
            .bg(shell)
            .child(inner)
    }
}
