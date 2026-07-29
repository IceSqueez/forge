use gpui::Rgba;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ForgePalette {
    pub base: Rgba,
    pub shell: Rgba,
    pub elevated: Rgba,

    pub text_primary: Rgba,
    pub text_secondary: Rgba,
    pub text_muted: Rgba,
    pub text_faint: Rgba,
    pub text_extreme_faint: Rgba,

    pub border_regular: Rgba,
    pub border_input: Rgba,
    pub border_active: Rgba,

    pub surface_overlay: Rgba,

    pub brand: Rgba,
    pub success: Rgba,
    pub warning: Rgba,
    pub info: Rgba,
    pub random: Rgba,
    pub bits: Rgba,
    pub accent_pink_light: Rgba,
    pub accent_teal: Rgba,
    pub disabled: Rgba,

    pub platform_twitch: Rgba,
    pub platform_youtube: Rgba,
    pub platform_kick: Rgba,

    pub code_keyword: Rgba,
    pub code_fn: Rgba,
    pub code_str: Rgba,
    pub code_var: Rgba,
    pub code_comment: Rgba,
    pub code_num: Rgba,

    pub scrim: Rgba,
}

const fn hex(r: u8, g: u8, b: u8) -> Rgba {
    Rgba {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a: 1.0,
    }
}

pub fn with_alpha(c: Rgba, alpha: f32) -> Rgba {
    Rgba { a: alpha, ..c }
}

/// `None` for a name outside the shipped accent vocabulary, so a caller can render it as unknown
/// instead of silently painting it a color that means something else.
pub fn accent_swatch(name: &str, palette: &ForgePalette) -> Option<Rgba> {
    match name {
        "mauve" => Some(palette.brand),
        "sky" => Some(palette.info),
        "green" => Some(palette.success),
        "peach" => Some(palette.bits),
        "yellow" => Some(palette.warning),
        "red" => Some(palette.random),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformKind {
    Twitch,
    YouTube,
    Kick,
}

pub fn platform_color(kind: PlatformKind, palette: &ForgePalette) -> Rgba {
    match kind {
        PlatformKind::Twitch => palette.platform_twitch,
        PlatformKind::YouTube => palette.platform_youtube,
        PlatformKind::Kick => palette.platform_kick,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ThemeId {
    #[default]
    ForgeDefault,
    TokyoNight,
    Latte,
}

impl ThemeId {
    pub const ALL: [ThemeId; 3] = [ThemeId::ForgeDefault, ThemeId::TokyoNight, ThemeId::Latte];

    /// Persisted identifier - must stay stable across releases.
    pub fn storage_key(self) -> &'static str {
        match self {
            ThemeId::ForgeDefault => "forge_default",
            ThemeId::TokyoNight => "tokyo_night",
            ThemeId::Latte => "latte",
        }
    }

    pub fn from_storage_key(key: &str) -> Option<ThemeId> {
        match key {
            "forge_default" | "catppuccin_mocha" => Some(ThemeId::ForgeDefault),
            "tokyo_night" => Some(ThemeId::TokyoNight),
            "latte" => Some(ThemeId::Latte),
            _ => None,
        }
    }

    pub fn palette(self) -> ForgePalette {
        match self {
            ThemeId::ForgeDefault => FORGE_DEFAULT,
            ThemeId::TokyoNight => TOKYO_NIGHT,
            ThemeId::Latte => LATTE,
        }
    }
}

pub const FORGE_DEFAULT: ForgePalette = ForgePalette {
    base: hex(0x1a, 0x18, 0x25),
    shell: hex(0x13, 0x10, 0x20),
    elevated: hex(0x22, 0x1f, 0x30),

    text_primary: hex(0xf0, 0xee, 0xf8),
    text_secondary: hex(0xc9, 0xc4, 0xdd),
    text_muted: hex(0x8a, 0x86, 0xa3),
    text_faint: hex(0x6b, 0x68, 0x84),
    text_extreme_faint: hex(0x54, 0x4f, 0x6e),

    border_regular: hex(0x2d, 0x29, 0x40),
    border_input: hex(0x3a, 0x35, 0x52),
    border_active: hex(0xc9, 0xa6, 0xf0),

    surface_overlay: hex(0x2e, 0x29, 0x42),

    brand: hex(0xc9, 0xa6, 0xf0),
    success: hex(0x50, 0xfa, 0x7b),
    warning: hex(0xe0, 0xb8, 0x60),
    info: hex(0x8b, 0xe9, 0xfd),
    random: hex(0xdc, 0x64, 0x64),
    bits: hex(0xff, 0xb8, 0x6c),
    accent_pink_light: hex(0xff, 0x66, 0xd9),
    accent_teal: hex(0x76, 0xe0, 0xcc),
    disabled: hex(0x6b, 0x68, 0x84),

    platform_twitch: hex(0xc9, 0xa6, 0xf0),
    platform_youtube: hex(0xdc, 0x64, 0x64),
    platform_kick: hex(0x8b, 0xe9, 0xfd),

    code_keyword: hex(0xc9, 0xa6, 0xf0),
    code_fn: hex(0x8b, 0xe9, 0xfd),
    code_str: hex(0x50, 0xfa, 0x7b),
    code_var: hex(0xff, 0xb8, 0x6c),
    code_comment: hex(0x6b, 0x68, 0x84),
    code_num: hex(0xff, 0x66, 0xd9),

    scrim: Rgba {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.45,
    },
};

pub const TOKYO_NIGHT: ForgePalette = ForgePalette {
    base: hex(0x1a, 0x1b, 0x26),
    shell: hex(0x16, 0x16, 0x1e),
    elevated: hex(0x24, 0x28, 0x3b),

    text_primary: hex(0xc0, 0xca, 0xf5),
    text_secondary: hex(0xa9, 0xb1, 0xd6),
    text_muted: hex(0x78, 0x7c, 0x99),
    text_faint: hex(0x56, 0x5f, 0x89),
    text_extreme_faint: hex(0x41, 0x44, 0x58),

    border_regular: hex(0x29, 0x2e, 0x42),
    border_input: hex(0x3b, 0x42, 0x61),
    border_active: hex(0xbb, 0x9a, 0xf7),

    surface_overlay: hex(0x3b, 0x42, 0x61),

    brand: hex(0xbb, 0x9a, 0xf7),
    success: hex(0x9e, 0xce, 0x6a),
    warning: hex(0xe0, 0xaf, 0x68),
    info: hex(0x2a, 0xc3, 0xde),
    random: hex(0xf7, 0x76, 0x8e),
    bits: hex(0xff, 0x9e, 0x64),
    accent_pink_light: hex(0xbb, 0x9a, 0xf7),
    accent_teal: hex(0x73, 0xda, 0xca),
    disabled: hex(0x56, 0x5f, 0x89),

    platform_twitch: hex(0xbb, 0x9a, 0xf7),
    platform_youtube: hex(0xf7, 0x76, 0x8e),
    platform_kick: hex(0x2a, 0xc3, 0xde),

    code_keyword: hex(0xbb, 0x9a, 0xf7),
    code_fn: hex(0x7a, 0xa2, 0xf7),
    code_str: hex(0x9e, 0xce, 0x6a),
    code_var: hex(0xff, 0x9e, 0x64),
    code_comment: hex(0x56, 0x5f, 0x89),
    code_num: hex(0xff, 0x9e, 0x64),

    scrim: Rgba {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.50,
    },
};

pub const LATTE: ForgePalette = ForgePalette {
    base: hex(0xef, 0xf1, 0xf5),
    shell: hex(0xe6, 0xe9, 0xef),
    elevated: hex(0xff, 0xff, 0xff),

    text_primary: hex(0x4c, 0x4f, 0x69),
    text_secondary: hex(0x5c, 0x5f, 0x77),
    text_muted: hex(0x6c, 0x6f, 0x85),
    text_faint: hex(0x8c, 0x8f, 0xa8),
    text_extreme_faint: hex(0xac, 0xb0, 0xbe),

    border_regular: hex(0xcc, 0xd0, 0xda),
    border_input: hex(0xbc, 0xc0, 0xcc),
    border_active: hex(0x1e, 0x66, 0xf5),

    surface_overlay: hex(0xbc, 0xc0, 0xcc),

    brand: hex(0x1e, 0x66, 0xf5),
    success: hex(0x40, 0xa0, 0x2b),
    warning: hex(0xdf, 0x8e, 0x1d),
    info: hex(0x04, 0xa5, 0xe5),
    random: hex(0xd2, 0x0f, 0x39),
    bits: hex(0xfe, 0x64, 0x0b),
    accent_pink_light: hex(0xea, 0x76, 0xcb),
    accent_teal: hex(0x17, 0x9a, 0x99),
    disabled: hex(0xac, 0xb0, 0xbe),

    platform_twitch: hex(0x1e, 0x66, 0xf5),
    platform_youtube: hex(0xd2, 0x0f, 0x39),
    platform_kick: hex(0x04, 0xa5, 0xe5),

    code_keyword: hex(0x88, 0x39, 0xef),
    code_fn: hex(0x04, 0xa5, 0xe5),
    code_str: hex(0x40, 0xa0, 0x2b),
    code_var: hex(0xfe, 0x64, 0x0b),
    code_comment: hex(0x9c, 0xa0, 0xb0),
    code_num: hex(0xfe, 0x64, 0x0b),

    scrim: Rgba {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.35,
    },
};
