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

pub fn bg_danger_soft(p: &ForgePalette) -> Rgba {
    with_alpha(p.random, 0.06)
}

pub fn bg_warn_soft(p: &ForgePalette) -> Rgba {
    with_alpha(p.warning, 0.06)
}

pub fn bd_warn_soft(p: &ForgePalette) -> Rgba {
    with_alpha(p.warning, 0.20)
}

pub fn bd_mauve_soft(p: &ForgePalette) -> Rgba {
    with_alpha(p.brand, 0.06)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ThemeId {
    CatppuccinMocha,
    #[default]
    TokyoNight,
    Latte,
}

impl ThemeId {
    pub const ALL: [ThemeId; 3] = [
        ThemeId::CatppuccinMocha,
        ThemeId::TokyoNight,
        ThemeId::Latte,
    ];

    /// Persisted identifier - must stay stable across releases.
    pub fn storage_key(self) -> &'static str {
        match self {
            ThemeId::CatppuccinMocha => "catppuccin_mocha",
            ThemeId::TokyoNight => "tokyo_night",
            ThemeId::Latte => "latte",
        }
    }

    pub fn from_storage_key(key: &str) -> Option<ThemeId> {
        match key {
            "catppuccin_mocha" => Some(ThemeId::CatppuccinMocha),
            "tokyo_night" => Some(ThemeId::TokyoNight),
            "latte" => Some(ThemeId::Latte),
            _ => None,
        }
    }

    pub fn palette(self) -> ForgePalette {
        match self {
            ThemeId::CatppuccinMocha => CATPPUCCIN_MOCHA,
            ThemeId::TokyoNight => TOKYO_NIGHT,
            ThemeId::Latte => LATTE,
        }
    }
}

pub const CATPPUCCIN_MOCHA: ForgePalette = ForgePalette {
    base: hex(0x18, 0x18, 0x25),
    shell: hex(0x11, 0x11, 0x1b),
    elevated: hex(0x1e, 0x1e, 0x2e),

    text_primary: hex(0xcd, 0xd6, 0xf4),
    text_secondary: hex(0xba, 0xc2, 0xde),
    text_muted: hex(0x93, 0x99, 0xb2),
    text_faint: hex(0x6c, 0x70, 0x86),
    text_extreme_faint: hex(0x45, 0x47, 0x5a),

    border_regular: hex(0x31, 0x32, 0x44),
    border_input: hex(0x45, 0x47, 0x5a),
    border_active: hex(0xcb, 0xa6, 0xf7),

    surface_overlay: hex(0x31, 0x32, 0x44),

    brand: hex(0xcb, 0xa6, 0xf7),
    success: hex(0xa6, 0xe3, 0xa1),
    warning: hex(0xf9, 0xe2, 0xaf),
    info: hex(0x89, 0xdc, 0xeb),
    random: hex(0xf3, 0x8b, 0xa8),
    bits: hex(0xfa, 0xb3, 0x87),
    accent_pink_light: hex(0xf5, 0xc2, 0xe7),
    accent_teal: hex(0x94, 0xe2, 0xd5),
    disabled: hex(0x6c, 0x70, 0x86),

    platform_twitch: hex(0x91, 0x46, 0xff),
    platform_youtube: hex(0xff, 0x00, 0x00),
    platform_kick: hex(0x53, 0xfc, 0x18),

    code_keyword: hex(0xcb, 0xa6, 0xf7),
    code_fn: hex(0x89, 0xdc, 0xeb),
    code_str: hex(0xa6, 0xe3, 0xa1),
    code_var: hex(0xfa, 0xb3, 0x87),
    code_comment: hex(0x6c, 0x70, 0x86),
    code_num: hex(0xfa, 0xb3, 0x87),

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

    platform_twitch: hex(0x91, 0x46, 0xff),
    platform_youtube: hex(0xff, 0x00, 0x00),
    platform_kick: hex(0x53, 0xfc, 0x18),

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

    platform_twitch: hex(0x91, 0x46, 0xff),
    platform_youtube: hex(0xff, 0x00, 0x00),
    platform_kick: hex(0x53, 0xfc, 0x18),

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
