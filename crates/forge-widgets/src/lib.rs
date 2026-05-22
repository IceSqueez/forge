#![doc = "Pure design-system kit: Tier 1 widget builders + ForgePalette theme tokens."]

pub mod actions;
pub mod breadcrumb;
pub mod buttons;
pub mod cards;
pub mod charts;
pub mod chat;
pub mod clip_card;
pub mod code_editor;
pub mod console;
pub mod data;
pub mod events;
pub mod icons;
pub mod inputs;
pub mod integration;
pub mod layout;
pub mod navigation;
pub mod output_device_picker;
pub mod palette;
pub mod picker;
pub mod popover;
pub mod sections;
pub mod server;
pub mod status;
pub mod theme;
pub mod toast;
pub mod tokens;
pub mod volume_slider;

pub use actions::{
    ModalProps, NodeProps, NodeStatus, SubActionProps, ToggleProps, TriggerCardProps,
    category_chip, modal, sub_action_card, toggle, tree_node_with_status, variable_chip,
};
pub use breadcrumb::{BreadcrumbCrumb, breadcrumb};
pub use buttons::{
    destructive_button, ghost_button, ghost_button_with_icon, icon_button, primary_button,
    primary_button_small, primary_button_with_icon_right, secondary_button,
};
pub use cards::{
    BigJumpCardProps, big_jump_card, card, card_with_radius, hero_card, metric_card, stat_row,
};
pub use charts::throughput_sparkline;
pub use chat::{
    BadgeKind, ChatBody, ChatRow, Platform, PlatformTarget, chat_row, chat_row_cheer, chat_row_cmd,
    chat_row_msg, chat_row_raid, chat_row_sub, chip_bg, filter_chip, input_bar,
};
pub use clip_card::{ClipCardData, clip_card};
pub use code_editor::{CodeEditorState, code_editor};
pub use console::{ConsoleLevel, ConsoleLine, console};
pub use data::{
    FooterProps, VariantKind, data_screen_footer, data_table, persistence_toggle_inline, type_pill,
    value_preview, variant_kind_color,
};
pub use events::{
    EventInspectorParams, EventRowData, causation_chip, color_for_source, event_inspector,
    event_row_observability, json_viewer, replay_button, source_badge,
};
pub use icons::{Icon, tabler_icon};
pub use inputs::{input_padding, search_input, select, text_input_field};
pub use integration::{
    HeaderCardParams, integration_content_renderer, integration_header_card,
    integration_health_grid, integration_quick_actions_grid,
    integration_quick_actions_grid_with_hint,
};
pub use layout::{app_footer, page_shell, title_bar, toolbar};
pub use navigation::{NavChild, NavItem, SIDEBAR_WIDTH, Sidebar, sidebar};
pub use output_device_picker::{DeviceLabel, output_device_picker};
pub use palette::{CATPPUCCIN_MOCHA, ForgePalette, LATTE, TOKYO_NIGHT};
pub use picker::{PickerItem, PickerModalProps, picker_modal};
pub use popover::{MenuItem, MenuPlacement, RowAction, actionable_count, menu_button, row_actions};
pub use sections::{
    BannerKind, ToastVariant, counter_badge, empty_state, live_status_banner, section_header,
    section_header_expandable, toast_banner,
};
pub use server::{
    BindAddressCardParams, BindBadge, BulletItem, BulletKind, ClientRowData, ClientStatus,
    FileMime, OverlayEntry, OverlayFileListParams, OverlayKind, SubscriptionChipData,
    TypeToConfirmModalParams, bearer_token_display, bind_address_card, client_table_row,
    overlay_file_list, type_to_confirm_modal,
};
pub use status::{StatusVariant, platform_badge, role_badge, status_dot, status_pill};
pub use theme::{catppuccin_mocha, latte, palette_for_theme, tokyo_night_storm};
pub use toast::{Toast, ToastAction, ToastKind, ToastQueue, toast_viewport};
pub use tokens::{Density, FontRole, Radius, Spacing, ThemeId, font, load_fonts, radius, spacing};
pub use volume_slider::volume_slider;
