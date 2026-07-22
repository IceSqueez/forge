use gpui::{
    AnyElement, App, FontWeight, IntoElement, ParentElement, Pixels, RenderOnce, Rgba,
    SharedString, Styled, Window, div, px,
};

use crate::breadcrumb::{BreadcrumbCrumb, breadcrumb};
use crate::cards::toolbar_row;
use crate::palette::ForgePalette;
use crate::status::status_dot;
use crate::tokens::{DEFAULT_BODY_FAMILY, Density, FONT_XS, Spacing, spacing};

const HEADER_STAT_FS: Pixels = px(11.5);

const HEADER_STATUS_DOT: Pixels = px(7.0);

const HEADER_STATUS_GAP: Pixels = px(5.0);

pub struct HeaderStat {
    value: SharedString,
    value_color: Rgba,
    label: SharedString,
}

pub fn header_stat(
    value: impl Into<SharedString>,
    value_color: Rgba,
    label: impl Into<SharedString>,
) -> HeaderStat {
    HeaderStat {
        value: value.into(),
        value_color,
        label: label.into(),
    }
}

pub fn header_stats(stats: Vec<HeaderStat>, palette: &ForgePalette) -> impl IntoElement {
    let muted = palette.text_muted;
    let faint = palette.text_faint;
    let last = stats.len().saturating_sub(1);

    let mut row = div()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Sm, Density::Cozy));

    for (i, stat) in stats.into_iter().enumerate() {
        let pill = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, Density::Cozy))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(HEADER_STAT_FS)
                    .text_color(stat.value_color)
                    .child(stat.value),
            )
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(HEADER_STAT_FS)
                    .text_color(muted)
                    .child(stat.label),
            );

        row = row.child(pill);

        if i != last {
            row = row.child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(HEADER_STAT_FS)
                    .text_color(faint)
                    .child("\u{b7}"),
            );
        }
    }

    row
}

pub fn header_status(color: Rgba, label: impl Into<SharedString>) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(HEADER_STATUS_GAP)
        .text_size(FONT_XS)
        .text_color(color)
        .child(status_dot(color, HEADER_STATUS_DOT))
        .child(label.into())
}

#[derive(IntoElement)]
pub struct PageFrame {
    crumbs: Vec<BreadcrumbCrumb>,
    header_right: Option<AnyElement>,
    switcher: Option<AnyElement>,
    subheader_left: Option<AnyElement>,
    subheader_right: Option<AnyElement>,
    body: Option<AnyElement>,
    density: Density,
    palette: ForgePalette,
}

pub fn page_frame(crumbs: Vec<BreadcrumbCrumb>, palette: &ForgePalette) -> PageFrame {
    PageFrame {
        crumbs,
        header_right: None,
        switcher: None,
        subheader_left: None,
        subheader_right: None,
        body: None,
        density: Density::default(),
        palette: *palette,
    }
}

impl PageFrame {
    #[must_use]
    pub fn header_right(mut self, el: impl IntoElement) -> Self {
        self.header_right = Some(el.into_any_element());
        self
    }

    #[must_use]
    pub fn section_switcher(mut self, el: impl IntoElement) -> Self {
        self.switcher = Some(el.into_any_element());
        self
    }

    /// The screen must place its search field first in this slot; the frame docks it leftmost in tier 3 so search lands in the same pixel on every screen.
    #[must_use]
    pub fn subheader_left(mut self, el: impl IntoElement) -> Self {
        self.subheader_left = Some(el.into_any_element());
        self
    }

    #[must_use]
    pub fn subheader_right(mut self, el: impl IntoElement) -> Self {
        self.subheader_right = Some(el.into_any_element());
        self
    }

    #[must_use]
    pub fn body(mut self, el: impl IntoElement) -> Self {
        self.body = Some(el.into_any_element());
        self
    }

    #[must_use]
    pub fn density(mut self, density: Density) -> Self {
        self.density = density;
        self
    }
}

impl RenderOnce for PageFrame {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let palette = self.palette;
        let density = self.density;

        let mut band = breadcrumb(self.crumbs, &palette).density(density);
        if let Some(right) = self.header_right {
            band = band.right(right);
        }

        let mut root = div()
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(band);

        if let Some(switcher) = self.switcher {
            root = root.child(switcher);
        }

        // Tier 3 exists only when a subheader slot is filled; a tier-2-only screen renders the breadcrumb band and nothing beneath it.
        if self.subheader_left.is_some() || self.subheader_right.is_some() {
            let left = self
                .subheader_left
                .unwrap_or_else(|| div().into_any_element());
            let right = self
                .subheader_right
                .unwrap_or_else(|| div().into_any_element());
            root = root.child(toolbar_row(left, right).attached(&palette).density(density));
        }

        if let Some(body) = self.body {
            root = root.child(body);
        }

        root
    }
}
