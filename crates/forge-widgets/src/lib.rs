#![doc = "Pure design-system kit: Tier 1 widget builders + ForgePalette theme tokens."]

pub mod buttons;
pub mod cards;
pub mod inputs;
pub mod layout;
pub mod navigation;
pub mod onboarding;
pub mod palette;
pub mod sections;
pub mod status;
pub mod theme;
pub mod tokens;

pub use buttons::{
    destructive_button, ghost_button, icon_button, primary_button, primary_button_small,
    primary_button_with_icon_right, secondary_button,
};
pub use cards::{card, card_with_radius, hero_card, metric_card, stat_row};
pub use inputs::{search_input, select, text_input_field};
pub use layout::{breadcrumb, page_shell, title_bar, title_bar_with_logo, toolbar};
pub use navigation::{SIDEBAR_WIDTH, sidebar, sidebar_section, tree_node};
pub use onboarding::{
    BannerKind, PlatformCardProps, Severity, StepEntry, StepInfo, StepStatus, device_code_display,
    expiration_color_band, expiration_timer, live_status_banner, locale_tip_card,
    numbered_box_step, onboarding_footer, onboarding_step_header, onboarding_stepper,
    platform_picker_card,
};
pub use palette::{CATPPUCCIN_MOCHA, ForgePalette, LATTE, TOKYO_NIGHT};
pub use sections::{
    ToastVariant, counter_badge, empty_state, section_header, section_header_expandable,
    toast_banner,
};
pub use status::{StatusVariant, platform_badge, role_badge, status_dot, status_pill};
pub use theme::{catppuccin_mocha, latte, palette_for_theme, tokyo_night_storm};
pub use tokens::{Density, FontRole, Radius, Spacing, ThemeId, font, load_fonts, radius, spacing};
