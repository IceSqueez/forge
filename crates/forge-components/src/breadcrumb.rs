use gpui::{
    AnyElement, App, ClickEvent, ElementId, InteractiveElement, IntoElement, ParentElement, Pixels,
    RenderOnce, Rgba, SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::icons::{Icon, icon};
use crate::palette::ForgePalette;
use crate::tokens::{BORDER_THIN, DEFAULT_BODY_FAMILY, Density, FONT_SM, Spacing, spacing};

/// Boxed click handler carried by a pressable (ancestor) crumb. gpui passes the
/// click event plus the window and app contexts, through which the caller reaches
/// its entity — same shape as the button and row-card families.
type CrumbClick = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// Home glyph size leading the trail. The source pins this at a literal 13px, off
/// the `FONT_*` scale, so it is carried as a literal rather than snapped onto a
/// token — mirroring the off-scale glyph sizes elsewhere in the kit.
const HOME_ICON_SIZE: Pixels = px(13.0);

/// The `>` separator glyph size between crumbs. The source pins this at a literal
/// 11px (a step below the 13px home glyph so the chevrons read lighter than the
/// anchor), off the `FONT_*` scale, so it is carried as a literal.
const SEPARATOR_ICON_SIZE: Pixels = px(11.0);

/// One segment of a breadcrumb trail: a label plus, for a clickable ancestor, the
/// id and handler that navigate back to it.
///
/// Build a non-clickable current-location segment with [`BreadcrumbCrumb::leaf`]
/// and a pressable ancestor segment with [`BreadcrumbCrumb::link`]. Clickability is
/// independent of ink: the trail inks the last segment as the current location and
/// every earlier segment as an ancestor regardless of whether a handler is attached
/// (see [`breadcrumb`]).
pub struct BreadcrumbCrumb {
    label: SharedString,
    id: Option<ElementId>,
    on_click: Option<CrumbClick>,
}

impl BreadcrumbCrumb {
    /// A non-clickable segment — the current location, the tail of the trail. It
    /// renders as plain text with no pointer affordance.
    pub fn leaf(label: impl Into<SharedString>) -> Self {
        BreadcrumbCrumb {
            label: label.into(),
            id: None,
            on_click: None,
        }
    }

    /// A clickable ancestor segment that navigates when pressed. gpui needs a stable
    /// [`ElementId`] to promote the segment to a stateful, clickable element, so the
    /// caller supplies one alongside the handler (which mutates its own entity via
    /// the passed `cx`).
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

/// A full-width breadcrumb chrome bar: a `home` anchor, then each crumb preceded by
/// a `>` separator, with an optional right-aligned control cluster.
///
/// Built via [`breadcrumb`]. The trail inks the last crumb as the current location
/// (`text_primary`) and every earlier crumb as an ancestor (`text_muted`); an
/// ancestor carrying a handler brightens to `text_primary` under the pointer. The
/// home anchor and every separator ink `text_faint`. The bar fills a `shell`
/// background under a thin `border_regular` rule with square corners. Ink and
/// surface colors resolve from a [`ForgePalette`] up front, so the built value
/// carries no palette borrow (same discipline as the button and card families).
#[derive(IntoElement)]
pub struct Breadcrumb {
    crumbs: Vec<BreadcrumbCrumb>,
    right: Option<AnyElement>,
    density: Density,
    shell: Rgba,
    border: Rgba,
    /// Home anchor and separator ink.
    faint: Rgba,
    /// Current-location (last) crumb ink, and the hover ink an ancestor brightens to.
    current: Rgba,
    /// Ancestor (non-last) crumb rest ink.
    ancestor: Rgba,
}

/// Start a breadcrumb bar from an ordered trail (root first, current location
/// last). Defaults: no right cluster, `Density::Cozy` (which reproduces the
/// source's fixed inset exactly). Colors resolve from `palette` up front.
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
    /// Pins a control cluster to the bar's right edge (a search field, filter chips,
    /// an action button). The crumb trail stays left; the cluster is pushed right
    /// with the free space between them.
    #[must_use]
    pub fn right(mut self, el: impl IntoElement) -> Self {
        self.right = Some(el.into_any_element());
        self
    }

    /// Overrides the density used to scale the bar's padding and inter-crumb gap. A
    /// bare [`breadcrumb`] resolves these at `Density::Cozy`, reproducing the
    /// source's fixed inset exactly.
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

        // The trail: the home anchor, then for every crumb a `>` separator followed
        // by its label. The separator sits before every crumb (including the first,
        // right after the anchor). All cells share one `Spacing::Xs` gap.
        let mut trail =
            div()
                .flex()
                .items_center()
                .gap(gap)
                .child(icon(Icon::Home, HOME_ICON_SIZE, faint));

        for (i, crumb) in crumbs.into_iter().enumerate() {
            // Ink is position-based: the last crumb is the current location, every
            // earlier crumb an ancestor. Clickability is independent of ink.
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
                    // A pressable ancestor brightens to the current-location ink
                    // under the pointer; the plain leaf never hovers.
                    .hover(move |s| s.text_color(current))
                    .on_click(handler)
                    .child(crumb.label)
                    .into_any_element(),
                _ => label.child(crumb.label).into_any_element(),
            };

            trail = trail.child(label_el);
        }

        // The trail stays left; a right cluster is pushed to the far edge with the
        // free space between them.
        let mut inner = div().w_full().flex().items_center().child(trail);
        if let Some(right) = right {
            inner = inner.justify_between().child(right);
        }

        // Square-cornered chrome bar: shell fill under a thin border rule.
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
