use std::sync::RwLock;

use gpui::{BoxShadow, Pixels, SharedString, hsla, point, px};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Density {
    Compact,
    #[default]
    Cozy,
    Spacious,
}

impl Density {
    pub fn storage_key(self) -> &'static str {
        match self {
            Density::Compact => "compact",
            Density::Cozy => "cozy",
            Density::Spacious => "spacious",
        }
    }

    pub fn from_storage_key(key: &str) -> Option<Density> {
        match key {
            "compact" => Some(Density::Compact),
            "cozy" => Some(Density::Cozy),
            "spacious" => Some(Density::Spacious),
            _ => None,
        }
    }

    fn multiplier(self) -> f32 {
        match self {
            Density::Compact => 0.85,
            Density::Cozy => 1.0,
            Density::Spacious => 1.2,
        }
    }

    pub fn tighter(self) -> Density {
        match self {
            Density::Compact | Density::Cozy => Density::Compact,
            Density::Spacious => Density::Cozy,
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
    scaled(px(s.base_px()), d)
}

pub fn scaled(value: Pixels, d: Density) -> Pixels {
    match d {
        Density::Cozy => value,
        Density::Compact | Density::Spacious => px((value.as_f32() * d.multiplier()).round()),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Radius {
    Sm,
    Md,
    Lg,
    Pill,
}

pub const fn radius(r: Radius) -> Pixels {
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

pub const fn modal_width(s: ModalSize) -> Pixels {
    match s {
        ModalSize::Sm => px(440.0),
        ModalSize::Md => px(540.0),
        ModalSize::Lg => px(640.0),
    }
}

pub const BORDER_THIN: Pixels = px(1.0);

pub const FONT_ROLE_BADGE: Pixels = px(8.5);
pub const FONT_BADGE: Pixels = px(9.0);
pub const FONT_SECTION_LABEL: Pixels = px(9.5);
pub const FONT_MICRO: Pixels = px(10.0);
pub const FONT_XXS: Pixels = px(10.5);
pub const FONT_META: Pixels = px(11.0);
pub const FONT_LABEL: Pixels = px(11.5);
pub const FONT_XS: Pixels = px(12.0);
pub const FONT_ROW: Pixels = px(12.5);
pub const FONT_FIELD: Pixels = px(13.0);
pub const FONT_SM: Pixels = px(14.0);
pub const FONT_HERO: Pixels = px(15.0);
pub const FONT_MD: Pixels = px(16.0);
pub const FONT_LG: Pixels = px(18.0);
pub const FONT_XL: Pixels = px(22.0);
pub const FONT_XXL: Pixels = px(24.0);

pub const ESCAPE_KEY: &str = "escape";
pub(crate) const ENTER_KEY: &str = "enter";
pub(crate) const SPACE_KEY: &str = "space";
pub const DRAG_SOURCE_OPACITY: f32 = 0.4;
pub const DISABLED_OPACITY: f32 = 0.5;
pub const ROW_OFF_OPACITY: f32 = 0.55;
pub const HOVER_FILL_ALPHA: f32 = 0.92;
pub const CTA_HOVER_ALPHA: f32 = 0.85;
pub const TINT_FILL_ALPHA: f32 = 0.10;
pub const TINT_STRONG_FILL_ALPHA: f32 = 0.12;
pub const TINT_BORDER_ALPHA: f32 = 0.30;
pub const ROW_HOVER_WASH_ALPHA: f32 = 0.06;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Elevation {
    Popover,
    Toast,
}

const ELEVATION_SHADOW_ALPHA: f32 = 0.4;

pub(crate) fn elevation_shadow(e: Elevation) -> Vec<BoxShadow> {
    let (offset_y, blur) = match e {
        Elevation::Popover => (px(10.0), px(30.0)),
        Elevation::Toast => (px(8.0), px(24.0)),
    };
    vec![BoxShadow {
        color: hsla(0.0, 0.0, 0.0, ELEVATION_SHADOW_ALPHA),
        offset: point(px(0.0), offset_y),
        blur_radius: blur,
        spread_radius: px(0.0),
        inset: false,
    }]
}

pub(crate) const DEFAULT_BODY_FAMILY: &str = "Inter";
pub(crate) const DEFAULT_MONO_FAMILY: &str = "JetBrains Mono";

static BODY_FAMILY_OVERRIDE: RwLock<Option<SharedString>> = RwLock::new(None);
static MONO_FAMILY_OVERRIDE: RwLock<Option<SharedString>> = RwLock::new(None);

pub fn body_family() -> SharedString {
    resolve_family(&BODY_FAMILY_OVERRIDE, DEFAULT_BODY_FAMILY)
}

pub fn mono_family() -> SharedString {
    resolve_family(&MONO_FAMILY_OVERRIDE, DEFAULT_MONO_FAMILY)
}

pub fn set_body_family(family: Option<SharedString>) {
    if let Ok(mut guard) = BODY_FAMILY_OVERRIDE.write() {
        *guard = family;
    }
}

pub fn set_mono_family(family: Option<SharedString>) {
    if let Ok(mut guard) = MONO_FAMILY_OVERRIDE.write() {
        *guard = family;
    }
}

fn resolve_family(slot: &RwLock<Option<SharedString>>, fallback: &'static str) -> SharedString {
    match slot.read() {
        Ok(guard) => guard
            .clone()
            .unwrap_or_else(|| SharedString::from(fallback)),
        Err(_) => SharedString::from(fallback),
    }
}

#[cfg(test)]
mod tests {
    use gpui::px;

    use super::{Density, Spacing, scaled, spacing};

    const ALL_DENSITIES: [Density; 3] = [Density::Compact, Density::Cozy, Density::Spacious];

    #[test]
    fn cozy_returns_every_value_unchanged_including_fractions() {
        for value in [0.0, 1.0, 2.0, 8.0, 10.5, 13.3, 100.0] {
            assert_eq!(scaled(px(value), Density::Cozy), px(value));
        }
    }

    #[test]
    fn compact_and_spacious_round_to_whole_pixels_across_the_scale_and_off_scale() {
        for (base, compact, spacious) in [
            (4.0, 3.0, 5.0),
            (6.0, 5.0, 7.0),
            (10.0, 9.0, 12.0),
            (16.0, 14.0, 19.0),
            (24.0, 20.0, 29.0),
            (8.0, 7.0, 10.0),
            (12.0, 10.0, 14.0),
            (14.0, 12.0, 17.0),
            (18.0, 15.0, 22.0),
        ] {
            assert_eq!(
                scaled(px(base), Density::Compact),
                px(compact),
                "compact {base}"
            );
            assert_eq!(
                scaled(px(base), Density::Spacious),
                px(spacious),
                "spacious {base}"
            );
        }
    }

    #[test]
    fn hairline_values_stay_put_in_every_density() {
        for density in ALL_DENSITIES {
            for value in [1.0, 2.0] {
                assert_eq!(scaled(px(value), density), px(value), "{density:?} {value}");
            }
        }
    }

    #[test]
    fn zero_stays_zero_in_every_density() {
        for density in ALL_DENSITIES {
            assert_eq!(scaled(px(0.0), density), px(0.0));
        }
    }

    #[test]
    fn spacing_is_the_scaled_base_of_each_step() {
        for step in [
            (Spacing::None, 0.0),
            (Spacing::Xxs, 4.0),
            (Spacing::Xs, 6.0),
            (Spacing::Sm, 10.0),
            (Spacing::Md, 16.0),
            (Spacing::Lg, 24.0),
        ] {
            for density in ALL_DENSITIES {
                assert_eq!(spacing(step.0, density), scaled(px(step.1), density));
            }
        }
    }

    #[test]
    fn tighter_steps_spacious_to_cozy_to_compact_and_stops_at_compact() {
        assert_eq!(Density::Spacious.tighter(), Density::Cozy);
        assert_eq!(Density::Cozy.tighter(), Density::Compact);
        assert_eq!(Density::Compact.tighter(), Density::Compact);
    }
}
