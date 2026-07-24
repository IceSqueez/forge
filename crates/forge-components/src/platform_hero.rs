use gpui::{
    AnyElement, App, IntoElement, ParentElement, RenderOnce, Rgba, SharedString, Styled, Window,
    div, px,
};

use crate::avatar::{AvatarTile, avatar_tile};
use crate::palette::ForgePalette;
use crate::tokens::{
    BORDER_THIN, Density, FONT_MD, FONT_XS, Radius, Spacing, body_family, radius, spacing,
};

#[derive(IntoElement)]
pub struct PlatformHero {
    tile: AvatarTile,
    name: SharedString,
    name_badges: Vec<AnyElement>,
    description: SharedString,
    right: Option<AnyElement>,
    density: Density,
    name_color: Rgba,
    description_color: Rgba,
    border: Rgba,
    background: Rgba,
}

pub fn platform_hero(
    letter: impl Into<SharedString>,
    brand: Rgba,
    name: impl Into<SharedString>,
    description: impl Into<SharedString>,
    palette: &ForgePalette,
) -> PlatformHero {
    PlatformHero {
        tile: avatar_tile(letter, brand, palette)
            .size(px(48.0))
            .corner(px(11.0))
            .font(px(24.0)),
        name: name.into(),
        name_badges: Vec::new(),
        description: description.into(),
        right: None,
        density: Density::Cozy,
        name_color: palette.text_primary,
        description_color: palette.text_muted,
        border: palette.border_regular,
        background: palette.elevated,
    }
}

impl PlatformHero {
    #[must_use]
    pub fn right(mut self, right: impl IntoElement) -> Self {
        self.right = Some(right.into_any_element());
        self
    }

    #[must_use]
    pub fn name_badges(mut self, badges: Vec<AnyElement>) -> Self {
        self.name_badges = badges;
        self
    }

    #[must_use]
    pub fn density(mut self, density: Density) -> Self {
        self.density = density;
        self
    }
}

impl RenderOnce for PlatformHero {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let d = self.density;

        let name_el = div()
            .font_family(body_family())
            .text_size(FONT_MD)
            .text_color(self.name_color)
            .child(self.name);
        let name_row: AnyElement = if self.name_badges.is_empty() {
            name_el.into_any_element()
        } else {
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(name_el)
                .children(self.name_badges)
                .into_any_element()
        };

        let info = div()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(2.0))
            .child(name_row)
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(self.description_color)
                    .child(self.description),
            );

        let mut root = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Md, d))
            .py(spacing(Spacing::Md, d))
            .px(px(18.0))
            .rounded(radius(Radius::Lg))
            .border(BORDER_THIN)
            .border_color(self.border)
            .bg(self.background)
            .child(self.tile)
            .child(info);

        if let Some(right) = self.right {
            root = root.child(right);
        }

        root
    }
}
