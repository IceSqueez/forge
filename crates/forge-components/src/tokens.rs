use gpui::{Pixels, px};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Density {
    Compact,
    #[default]
    Cozy,
    Spacious,
}

impl Density {
    fn multiplier(self) -> f32 {
        match self {
            Density::Compact => 0.85,
            Density::Cozy => 1.0,
            Density::Spacious => 1.2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spacing {
    None,
    Xxs,
    Xs,
    Sm,
    Md,
    Lg,
}

impl Spacing {
    fn base_px(self) -> f32 {
        match self {
            Self::None => 0.0,
            Self::Xxs => 4.0,
            Self::Xs => 6.0,
            Self::Sm => 10.0,
            Self::Md => 16.0,
            Self::Lg => 24.0,
        }
    }
}

pub fn spacing(s: Spacing, d: Density) -> Pixels {
    px((s.base_px() * d.multiplier()).round())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Radius {
    Sm,
    Md,
    Lg,
    /// Oversized sentinel that forces a fully rounded pill/circle.
    Pill,
}

pub fn radius(r: Radius) -> Pixels {
    match r {
        Radius::Sm => px(6.0),
        Radius::Md => px(9.0),
        Radius::Lg => px(12.0),
        Radius::Pill => px(999.0),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalSize {
    Sm,
    Md,
    Lg,
}

pub fn modal_width(s: ModalSize) -> Pixels {
    match s {
        ModalSize::Sm => px(440.0),
        ModalSize::Md => px(540.0),
        ModalSize::Lg => px(640.0),
    }
}

pub const BORDER_THIN: Pixels = px(0.5);
pub const BORDER_ACCENT: Pixels = px(1.0);

pub const FONT_XXS: Pixels = px(10.5);
pub const FONT_XS: Pixels = px(12.0);
pub const FONT_SM: Pixels = px(14.0);
pub const FONT_MD: Pixels = px(16.0);
pub const FONT_LG: Pixels = px(18.0);

pub const DEFAULT_BODY_FAMILY: &str = "Inter";
pub const DEFAULT_MONO_FAMILY: &str = "JetBrains Mono";
