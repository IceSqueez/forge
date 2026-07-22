use gpui::{
    App, FontWeight, IntoElement, ParentElement, Pixels, RenderOnce, Rgba, SharedString, Styled,
    Window, div, px,
};

use crate::palette::ForgePalette;
use crate::tokens::{FONT_XXS, Radius, body_family, mono_family, radius};

pub fn hash_accent(name: &str, palette: &ForgePalette) -> Rgba {
    let idx = name
        .bytes()
        .fold(0u32, |acc, b| acc.wrapping_add(u32::from(b))) as usize
        % 8;
    [
        palette.brand,
        palette.success,
        palette.warning,
        palette.info,
        palette.random,
        palette.bits,
        palette.accent_pink_light,
        palette.accent_teal,
    ][idx]
}

#[derive(IntoElement)]
pub struct AvatarTile {
    letter: SharedString,
    bg: Rgba,
    fg: Rgba,
    size: Pixels,
    corner: Pixels,
    font: Pixels,
    mono: bool,
}

pub fn avatar_tile(
    letter: impl Into<SharedString>,
    bg: Rgba,
    palette: &ForgePalette,
) -> AvatarTile {
    AvatarTile {
        letter: letter.into(),
        bg,
        fg: palette.shell,
        size: px(24.0),
        corner: radius(Radius::Sm),
        font: FONT_XXS,
        mono: false,
    }
}

impl AvatarTile {
    pub fn size(mut self, size: Pixels) -> Self {
        self.size = size;
        self
    }

    pub fn corner(mut self, corner: Pixels) -> Self {
        self.corner = corner;
        self
    }

    pub fn font(mut self, font: Pixels) -> Self {
        self.font = font;
        self
    }

    pub fn mono(mut self) -> Self {
        self.mono = true;
        self
    }

    pub fn fg(mut self, fg: Rgba) -> Self {
        self.fg = fg;
        self
    }
}

impl RenderOnce for AvatarTile {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let family = if self.mono {
            mono_family()
        } else {
            body_family()
        };
        div()
            .flex_none()
            .size(self.size)
            .rounded(self.corner)
            .bg(self.bg)
            .flex()
            .items_center()
            .justify_center()
            .font_family(family)
            .font_weight(FontWeight::SEMIBOLD)
            .text_size(self.font)
            .text_color(self.fg)
            .child(self.letter)
    }
}
