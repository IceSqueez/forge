#![doc = "Pure design-system kit: Tier 1 widget builders + ForgePalette theme tokens."]

pub mod actions;
pub mod buttons;
pub mod cards;
pub mod chat;
pub mod code_editor;
pub mod console;
pub mod data;
pub mod icons;
pub mod inputs;
pub mod integration;
pub mod layout;
pub mod navigation;
pub mod onboarding;
pub mod palette;
pub mod sections;
pub mod status;
pub mod theme;
pub mod tokens;

pub use actions::{
    ModalProps, NodeProps, NodeStatus, SubActionProps, ToggleProps, TriggerCardProps,
    category_chip, modal, sub_action_card, toggle, tree_node_with_status, variable_chip,
};
pub use buttons::{
    destructive_button, ghost_button, icon_button, primary_button, primary_button_small,
    primary_button_with_icon_right, secondary_button,
};
pub use cards::{card, card_with_radius, hero_card, metric_card, stat_row};
pub use chat::{
    BadgeKind, ChatBody, ChatRow, Platform, PlatformTarget, chat_row, filter_chip, input_bar,
};
pub use code_editor::{CodeEditorState, code_editor};
pub use console::{ConsoleLevel, ConsoleLine, console};
pub use data::{
    FooterProps, VariantKind, data_screen_footer, data_table, persistence_toggle_inline, type_pill,
    value_preview, variant_kind_color,
};
pub use icons::{
    BOOTSTRAP_FONT, BOOTSTRAP_FONT_BYTES, ICON_ACTIVITY, ICON_BROADCAST, ICON_CHAT,
    ICON_CHEVRON_DOWN, ICON_CHEVRON_RIGHT, ICON_CHEVRON_UP, ICON_CLOCK, ICON_DOWNLOAD,
    ICON_FILE_CODE, ICON_GEAR, ICON_GLOBE, ICON_GRID, ICON_HASH, ICON_HOME, ICON_JOURNAL,
    ICON_LIGHTNING, ICON_MUSIC_NOTE, ICON_PEOPLE, ICON_PLUS, ICON_SERVER, ICON_SPEAKER,
    ICON_TERMINAL,
};
pub use inputs::{search_input, select, text_input_field};
pub use integration::{
    HeaderCardParams, integration_content_renderer, integration_header_card,
    integration_health_grid,
};
pub use layout::{
    TitleBarV2, breadcrumb, page_shell, title_bar, title_bar_v2, title_bar_with_logo, toolbar,
};
pub use navigation::{
    NavChild, NavItem, SIDEBAR_WIDTH, SidebarV2, sidebar, sidebar_section, sidebar_v2, tree_node,
};
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
