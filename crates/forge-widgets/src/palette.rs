use iced::Color;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ForgePalette {
    pub base: Color,
    pub shell: Color,
    pub elevated: Color,

    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_muted: Color,
    pub text_faint: Color,
    pub text_extreme_faint: Color,

    pub border_regular: Color,
    pub border_input: Color,
    pub border_active: Color,

    pub surface_overlay: Color,

    pub brand: Color,
    pub success: Color,
    pub warning: Color,
    pub info: Color,
    pub random: Color,
    pub bits: Color,
    pub accent_pink_light: Color,
    pub accent_teal: Color,
    pub disabled: Color,

    pub code_keyword: Color,
    pub code_fn: Color,
    pub code_str: Color,
    pub code_var: Color,
    pub code_comment: Color,
    pub code_num: Color,
}

const fn hex(r: u8, g: u8, b: u8) -> Color {
    Color {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a: 1.0,
    }
}

pub fn with_alpha(c: Color, alpha: f32) -> Color {
    Color { a: alpha, ..c }
}

pub fn bg_danger_soft(p: &ForgePalette) -> Color {
    with_alpha(p.random, 0.06)
}

pub fn bg_warn_soft(p: &ForgePalette) -> Color {
    with_alpha(p.warning, 0.06)
}

pub fn bd_warn_soft(p: &ForgePalette) -> Color {
    with_alpha(p.warning, 0.20)
}

pub fn bd_mauve_soft(p: &ForgePalette) -> Color {
    with_alpha(p.brand, 0.06)
}

pub const CATPPUCCIN_MOCHA: ForgePalette = ForgePalette {
    base: hex(0x18, 0x18, 0x25),
    shell: hex(0x11, 0x11, 0x1b),
    elevated: hex(0x1e, 0x1e, 0x2e),

    text_primary: hex(0xcd, 0xd6, 0xf4),
    text_secondary: hex(0xa6, 0xad, 0xc8),
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

    code_keyword: hex(0xcb, 0xa6, 0xf7),
    code_fn: hex(0x89, 0xdc, 0xeb),
    code_str: hex(0xa6, 0xe3, 0xa1),
    code_var: hex(0xfa, 0xb3, 0x87),
    code_comment: hex(0x6c, 0x70, 0x86),
    code_num: hex(0xfa, 0xb3, 0x87),
};

pub const TOKYO_NIGHT: ForgePalette = ForgePalette {
    base: hex(0x1a, 0x1b, 0x26),
    shell: hex(0x13, 0x14, 0x1f),
    elevated: hex(0x24, 0x28, 0x3b),

    text_primary: hex(0xc0, 0xca, 0xf5),
    text_secondary: hex(0x9a, 0xa5, 0xce),
    text_muted: hex(0x78, 0x7c, 0x99),
    text_faint: hex(0x56, 0x5f, 0x89),
    text_extreme_faint: hex(0x41, 0x44, 0x58),

    border_regular: hex(0x29, 0x2e, 0x42),
    border_input: hex(0x41, 0x44, 0x58),
    border_active: hex(0x7a, 0xa2, 0xf7),

    surface_overlay: hex(0x2f, 0x35, 0x49),

    brand: hex(0x7a, 0xa2, 0xf7),
    success: hex(0x9e, 0xce, 0x6a),
    warning: hex(0xe0, 0xaf, 0x68),
    info: hex(0x2a, 0xc3, 0xde),
    random: hex(0xf7, 0x76, 0x8e),
    bits: hex(0xff, 0x9e, 0x64),
    accent_pink_light: hex(0xbb, 0x9a, 0xf7),
    accent_teal: hex(0x73, 0xda, 0xca),
    disabled: hex(0x56, 0x5f, 0x89),

    code_keyword: hex(0xbb, 0x9a, 0xf7),
    code_fn: hex(0x7a, 0xa2, 0xf7),
    code_str: hex(0x9e, 0xce, 0x6a),
    code_var: hex(0xff, 0x9e, 0x64),
    code_comment: hex(0x56, 0x5f, 0x89),
    code_num: hex(0xff, 0x9e, 0x64),
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
    border_input: hex(0xac, 0xb0, 0xbe),
    border_active: hex(0x1e, 0x66, 0xf5),

    surface_overlay: hex(0xdc, 0xe0, 0xe8),

    brand: hex(0x1e, 0x66, 0xf5),
    success: hex(0x40, 0xa0, 0x2b),
    warning: hex(0xdf, 0x8e, 0x1d),
    info: hex(0x04, 0xa5, 0xe5),
    random: hex(0xd2, 0x0f, 0x39),
    bits: hex(0xfe, 0x64, 0x0b),
    accent_pink_light: hex(0xea, 0x76, 0xcb),
    accent_teal: hex(0x17, 0x9a, 0x99),
    disabled: hex(0xac, 0xb0, 0xbe),

    code_keyword: hex(0x88, 0x39, 0xef),
    code_fn: hex(0x04, 0xa5, 0xe5),
    code_str: hex(0x40, 0xa0, 0x2b),
    code_var: hex(0xfe, 0x64, 0x0b),
    code_comment: hex(0x9c, 0xa0, 0xb0),
    code_num: hex(0xfe, 0x64, 0x0b),
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_alpha_overrides_alpha_channel() {
        let opaque = CATPPUCCIN_MOCHA.random;
        let translucent = with_alpha(opaque, 0.06);
        assert_eq!(translucent.r, opaque.r);
        assert_eq!(translucent.g, opaque.g);
        assert_eq!(translucent.b, opaque.b);
        assert!((translucent.a - 0.06).abs() < 0.001);
    }

    #[test]
    fn bd_warn_soft_uses_warning_color_at_20_percent() {
        let c = bd_warn_soft(&CATPPUCCIN_MOCHA);
        assert!((c.a - 0.20).abs() < 0.001);
        assert_eq!(c.r, CATPPUCCIN_MOCHA.warning.r);
    }

    #[test]
    fn catppuccin_mocha_brand_is_lavender() {
        let p = CATPPUCCIN_MOCHA;
        assert!((p.brand.r - 0xcb as f32 / 255.0).abs() < 0.01);
        assert!((p.brand.g - 0xa6 as f32 / 255.0).abs() < 0.01);
        assert!((p.brand.b - 0xf7 as f32 / 255.0).abs() < 0.01);
    }

    #[test]
    fn tokyo_night_is_copy() {
        let a = TOKYO_NIGHT;
        let b = a;
        assert_eq!(a, b);
    }

    #[test]
    fn latte_base_is_light() {
        let p = LATTE;
        assert!(p.base.r > 0.9);
        assert!(p.base.g > 0.9);
        assert!(p.base.b > 0.9);
    }

    #[test]
    fn all_palettes_are_constructable() {
        let _m = CATPPUCCIN_MOCHA;
        let _t = TOKYO_NIGHT;
        let _l = LATTE;
    }

    #[test]
    fn surface_overlay_differs_from_border_regular_in_tokyo_night() {
        let p = TOKYO_NIGHT;
        assert_ne!(p.surface_overlay.r, p.border_regular.r);
    }

    #[test]
    fn surface_overlay_is_defined_for_all_palettes() {
        let _mo = CATPPUCCIN_MOCHA.surface_overlay;
        let _tn = TOKYO_NIGHT.surface_overlay;
        let _la = LATTE.surface_overlay;
    }

    #[test]
    fn code_colors_match_theme_css_per_theme() {
        let approx = |a: f32, b: u8| (a - b as f32 / 255.0).abs() < 0.01;

        let mo = CATPPUCCIN_MOCHA;
        assert!(
            approx(mo.code_keyword.r, 0xcb)
                && approx(mo.code_keyword.g, 0xa6)
                && approx(mo.code_keyword.b, 0xf7)
        );
        assert!(
            approx(mo.code_fn.r, 0x89) && approx(mo.code_fn.g, 0xdc) && approx(mo.code_fn.b, 0xeb)
        );
        assert!(
            approx(mo.code_str.r, 0xa6)
                && approx(mo.code_str.g, 0xe3)
                && approx(mo.code_str.b, 0xa1)
        );
        assert!(
            approx(mo.code_var.r, 0xfa)
                && approx(mo.code_var.g, 0xb3)
                && approx(mo.code_var.b, 0x87)
        );
        assert!(
            approx(mo.code_comment.r, 0x6c)
                && approx(mo.code_comment.g, 0x70)
                && approx(mo.code_comment.b, 0x86)
        );
        assert!(
            approx(mo.code_num.r, 0xfa)
                && approx(mo.code_num.g, 0xb3)
                && approx(mo.code_num.b, 0x87)
        );

        let tn = TOKYO_NIGHT;
        assert!(
            approx(tn.code_keyword.r, 0xbb)
                && approx(tn.code_keyword.g, 0x9a)
                && approx(tn.code_keyword.b, 0xf7)
        );
        assert!(
            approx(tn.code_fn.r, 0x7a) && approx(tn.code_fn.g, 0xa2) && approx(tn.code_fn.b, 0xf7)
        );
        assert!(
            approx(tn.code_str.r, 0x9e)
                && approx(tn.code_str.g, 0xce)
                && approx(tn.code_str.b, 0x6a)
        );
        assert!(
            approx(tn.code_var.r, 0xff)
                && approx(tn.code_var.g, 0x9e)
                && approx(tn.code_var.b, 0x64)
        );
        assert!(
            approx(tn.code_comment.r, 0x56)
                && approx(tn.code_comment.g, 0x5f)
                && approx(tn.code_comment.b, 0x89)
        );
        assert!(
            approx(tn.code_num.r, 0xff)
                && approx(tn.code_num.g, 0x9e)
                && approx(tn.code_num.b, 0x64)
        );

        let la = LATTE;
        assert!(
            approx(la.code_keyword.r, 0x88)
                && approx(la.code_keyword.g, 0x39)
                && approx(la.code_keyword.b, 0xef)
        );
        assert!(
            approx(la.code_fn.r, 0x04) && approx(la.code_fn.g, 0xa5) && approx(la.code_fn.b, 0xe5)
        );
        assert!(
            approx(la.code_str.r, 0x40)
                && approx(la.code_str.g, 0xa0)
                && approx(la.code_str.b, 0x2b)
        );
        assert!(
            approx(la.code_var.r, 0xfe)
                && approx(la.code_var.g, 0x64)
                && approx(la.code_var.b, 0x0b)
        );
        assert!(
            approx(la.code_comment.r, 0x9c)
                && approx(la.code_comment.g, 0xa0)
                && approx(la.code_comment.b, 0xb0)
        );
        assert!(
            approx(la.code_num.r, 0xfe)
                && approx(la.code_num.g, 0x64)
                && approx(la.code_num.b, 0x0b)
        );
    }
}
