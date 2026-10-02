use forge_types::Variant;

use crate::config::{
    DESIGN_HEIGHT, DESIGN_SIZE_MAX_PX, DESIGN_SIZE_MIN_PX, DESIGN_WIDTH, MARGIN_BOTTOM,
    MARGIN_LEFT, MARGIN_MAX_PERCENT, MARGIN_MIN_PERCENT, MARGIN_RIGHT, MARGIN_TOP,
};
use crate::descriptor::OverlayConfig;

const BROWSER_SOURCE_DEFAULT_WIDTH_PX: u32 = 800;
const BROWSER_SOURCE_DEFAULT_HEIGHT_PX: u32 = 600;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DesignSize {
    pub width: u32,
    pub height: u32,
}

impl DesignSize {
    pub const BROWSER_SOURCE_DEFAULT: Self = Self {
        width: BROWSER_SOURCE_DEFAULT_WIDTH_PX,
        height: BROWSER_SOURCE_DEFAULT_HEIGHT_PX,
    };

    pub fn read(config: &OverlayConfig, fallback: Self) -> Self {
        Self {
            width: bounded(config, DESIGN_WIDTH, DESIGN_SIZE_MIN_PX, DESIGN_SIZE_MAX_PX)
                .unwrap_or(fallback.width),
            height: bounded(
                config,
                DESIGN_HEIGHT,
                DESIGN_SIZE_MIN_PX,
                DESIGN_SIZE_MAX_PX,
            )
            .unwrap_or(fallback.height),
        }
    }

    pub fn write_into(self, config: &mut OverlayConfig) {
        config.insert(DESIGN_WIDTH.to_owned(), Variant::Int(i64::from(self.width)));
        config.insert(
            DESIGN_HEIGHT.to_owned(),
            Variant::Int(i64::from(self.height)),
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ContentMargins {
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
    pub left: u32,
}

impl ContentMargins {
    pub const NONE: Self = Self {
        top: 0,
        right: 0,
        bottom: 0,
        left: 0,
    };

    pub const fn even(percent: u32) -> Self {
        Self {
            top: percent,
            right: percent,
            bottom: percent,
            left: percent,
        }
    }

    pub fn read(config: &OverlayConfig, fallback: Self) -> Self {
        let side = |key: &str, otherwise: u32| {
            bounded(config, key, MARGIN_MIN_PERCENT, MARGIN_MAX_PERCENT).unwrap_or(otherwise)
        };
        Self {
            top: side(MARGIN_TOP, fallback.top),
            right: side(MARGIN_RIGHT, fallback.right),
            bottom: side(MARGIN_BOTTOM, fallback.bottom),
            left: side(MARGIN_LEFT, fallback.left),
        }
    }

    pub fn write_into(self, config: &mut OverlayConfig) {
        for (key, percent) in [
            (MARGIN_TOP, self.top),
            (MARGIN_RIGHT, self.right),
            (MARGIN_BOTTOM, self.bottom),
            (MARGIN_LEFT, self.left),
        ] {
            config.insert(key.to_owned(), Variant::Int(i64::from(percent)));
        }
    }
}

fn bounded(config: &OverlayConfig, key: &str, min: i64, max: i64) -> Option<u32> {
    let stored = config.get(key).and_then(Variant::as_int)?.clamp(min, max);
    u32::try_from(stored).ok()
}
