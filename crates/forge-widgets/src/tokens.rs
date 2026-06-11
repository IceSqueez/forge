use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use iced::{Font, font};

pub const DEFAULT_BODY_FAMILY: &str = "Inter";
pub const DEFAULT_MONO_FAMILY: &str = "JetBrains Mono";

thread_local! {
    static ACTIVE_DENSITY: Cell<Density> = const { Cell::new(Density::Cozy) };
    static BODY_FAMILY_OVERRIDE: Cell<Option<&'static str>> = const { Cell::new(None) };
    static MONO_FAMILY_OVERRIDE: Cell<Option<&'static str>> = const { Cell::new(None) };
    static LEAKED_FAMILY_NAMES: RefCell<HashMap<String, &'static str>> = RefCell::new(HashMap::new());
}

/// Replaces the active density for this thread. Per-thread, like the locale bundle — iced's
/// view loop runs on the main thread, so installing once there covers every token call site.
pub fn install_density(density: Density) {
    ACTIVE_DENSITY.with(|cell| cell.set(density));
}

fn active_density() -> Density {
    ACTIVE_DENSITY.with(Cell::get)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ThemeId {
    #[default]
    CatppuccinMocha,
    TokyoNight,
    Latte,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Density {
    Compact,
    #[default]
    Cozy,
    Spacious,
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
            Self::Xs => 8.0,
            Self::Sm => 12.0,
            Self::Md => 16.0,
            Self::Lg => 24.0,
        }
    }
}

pub fn spacing(s: Spacing, d: Density) -> u16 {
    let multiplier = match d {
        Density::Compact => 0.85,
        Density::Cozy => 1.0,
        Density::Spacious => 1.2,
    };
    (s.base_px() * multiplier).round() as u16
}

pub fn sp(token: Spacing) -> u16 {
    spacing(token, active_density())
}

pub fn spf(token: Spacing) -> f32 {
    f32::from(spacing(token, active_density()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Radius {
    Sm,
    Md,
    Lg,
    /// Returns a large sentinel so iced renders a fully rounded pill/circle.
    Pill,
}

pub fn radius(r: Radius) -> f32 {
    match r {
        Radius::Sm => 7.0,
        Radius::Md => 8.0,
        Radius::Lg => 12.0,
        Radius::Pill => 999.0,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalSize {
    Sm,
    Md,
    Lg,
}

pub fn modal_width(s: ModalSize) -> f32 {
    match s {
        ModalSize::Sm => 440.0,
        ModalSize::Md => 540.0,
        ModalSize::Lg => 640.0,
    }
}

pub const BORDER_THIN: f32 = 0.5;
pub const BORDER_ACCENT: f32 = 1.0;

pub const FONT_XS: f32 = 12.0;
pub const FONT_SM: f32 = 14.0;
pub const FONT_MD: f32 = 16.0;
pub const FONT_LG: f32 = 18.0;

pub const FONT_DEVICE_CODE: f32 = 28.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FontRole {
    Body,
    Monospace,
}

/// Replaces the active family for one role on this thread; `None` restores the bundled default.
pub fn install_font_override(role: FontRole, family: Option<&str>) {
    let name = family.map(leak_family_name);
    match role {
        FontRole::Body => BODY_FAMILY_OVERRIDE.with(|cell| cell.set(name)),
        FontRole::Monospace => MONO_FAMILY_OVERRIDE.with(|cell| cell.set(name)),
    }
}

// Why: iced font families are `&'static str`; the cache bounds the leak to one
// allocation per distinct family name per thread.
fn leak_family_name(name: &str) -> &'static str {
    LEAKED_FAMILY_NAMES.with(|cache| {
        let mut cache = cache.borrow_mut();
        match cache.get(name) {
            Some(leaked) => leaked,
            None => {
                let leaked: &'static str = Box::leak(name.to_owned().into_boxed_str());
                cache.insert(name.to_owned(), leaked);
                leaked
            }
        }
    })
}

/// Caller must invoke `load_fonts()` at startup; otherwise iced falls back to system fonts.
pub fn font(role: FontRole) -> Font {
    match role {
        FontRole::Body => Font::with_name(
            BODY_FAMILY_OVERRIDE
                .with(Cell::get)
                .unwrap_or(DEFAULT_BODY_FAMILY),
        ),
        FontRole::Monospace => Font {
            family: font::Family::Name(
                MONO_FAMILY_OVERRIDE
                    .with(Cell::get)
                    .unwrap_or(DEFAULT_MONO_FAMILY),
            ),
            weight: font::Weight::Normal,
            stretch: font::Stretch::Normal,
            style: font::Style::Normal,
        },
    }
}

/// Returns bundled Inter + JetBrains Mono font bytes; iced loads these so the
/// UI looks consistent across systems regardless of installed fonts.
pub fn load_fonts() -> Vec<std::borrow::Cow<'static, [u8]>> {
    vec![
        std::borrow::Cow::Borrowed(include_bytes!("../assets/fonts/Inter-Regular.ttf")),
        std::borrow::Cow::Borrowed(include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf")),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spacing_density_orders_compact_lt_cozy_lt_spacious() {
        for s in [Spacing::Xs, Spacing::Sm, Spacing::Md, Spacing::Lg] {
            assert!(spacing(s, Density::Compact) < spacing(s, Density::Cozy));
            assert!(spacing(s, Density::Cozy) < spacing(s, Density::Spacious));
        }
    }
}
