#![doc = "Pure design-system kit: Tier 1 widget builders + ForgePalette theme tokens."]

pub mod actions;
pub mod autocomplete_popup;
pub mod breadcrumb;
pub mod builtin;
pub mod builtin_header;
pub mod builtin_health;
pub mod builtin_quick_actions;
pub mod buttons;
pub mod cards;
pub mod charts;
pub mod chat;
pub mod chat_widget;
pub mod clip_card;
pub mod code_editor;
pub mod console;
pub mod data;
pub mod events;
pub mod font_catalog;
pub mod hover_popover;
pub mod icons;
pub mod inputs;
pub mod key_capture;
pub mod layout;
pub mod locale;
pub mod modal;
pub mod modals;
pub mod navigation;
pub mod output_device_picker;
pub mod palette;
pub mod picker;
pub mod platform_tile;
pub mod popover;
pub mod rhai_highlight;
pub mod script_editor_overlay;
pub mod script_editor_widget;
pub mod sections;
pub mod server;
pub mod side_sheet;
pub mod status;
pub mod tag_list_input;
pub mod theme;
pub mod toast;
pub mod toggle;
pub mod tokens;
pub mod volume_slider;

pub use actions::{
    NodeProps, NodeStatus, SubActionProps, TriggerCardProps, category_chip, sub_action_card,
    tree_node_with_status, variable_chip,
};
pub use autocomplete_popup::{
    AutocompletePopupMessage, AutocompletePopupState, autocomplete_popup, filter_candidates,
};
pub use breadcrumb::{BreadcrumbCrumb, breadcrumb};
pub use builtin::builtin_content_renderer;
pub use builtin_header::{HeaderCardParams, builtin_header_card};
pub use builtin_health::builtin_health_grid;
pub use builtin_quick_actions::{builtin_quick_actions_grid, builtin_quick_actions_grid_with_hint};
pub use buttons::{
    destructive_button, ghost_button, ghost_button_with_icon, icon_button, primary_button,
    primary_button_small, primary_button_with_icon, primary_button_with_icon_right,
    secondary_button,
};
pub use cards::{
    BigJumpCardProps, big_jump_card, card, card_with_radius, hero_card, metric_card, stat_row,
};
pub use charts::throughput_sparkline;
pub use chat::{
    BadgeKind, ChatBody, ChatRow, Platform, PlatformTarget, chip_bg, filter_chip, input_bar,
};
pub use chat_widget::ChatRowWidget;
pub use clip_card::{ClipCardData, clip_card};
pub use code_editor::{CodeEditorState, rhai_editor};
pub use console::{ConsoleLevel, ConsoleLine, console};
pub use data::{
    FooterProps, VariantKind, data_screen_footer, data_table, persistence_toggle_inline, type_pill,
    value_preview, variant_kind_color,
};
pub use events::{
    EventInspectorParams, EventRowData, causation_chip, color_for_source, event_inspector,
    event_row_observability, json_viewer, replay_button, source_badge, source_label,
};
pub use font_catalog::{FontFamily, enumerate_font_families};
pub use hover_popover::{HoverTarget, format_signature, hover_popover};
pub use icons::{Icon, tabler_icon};
pub use inputs::{
    input_padding, search_input, select, select_owned, text_input_field, text_input_field_submit,
};
pub use key_capture::{KeyCapture, chord_from_key, key_capture};
pub use layout::{app_footer, page_shell, title_bar, toolbar};
pub use locale::{
    ArgsBuilder, fmt_feed_time, fmt_number, fmt_relative_time, fmt_short_date, install_bundle,
    set_locale_id, tr_lookup,
};
pub use modal::{ModalProps, modal};
pub use modals::{
    BulletItem, BulletKind, ConfirmKind, ConfirmModalParams, ConfirmTone, TypeToConfirmModalParams,
    confirm_modal, type_to_confirm_modal,
};
pub use navigation::{NavItem, SIDEBAR_WIDTH, Sidebar, sidebar};
pub use output_device_picker::{DeviceLabel, output_device_picker};
pub use palette::{CATPPUCCIN_MOCHA, ForgePalette, LATTE, TOKYO_NIGHT};
pub use picker::{PickerItem, PickerModalProps, picker_modal};
pub use platform_tile::platform_identity_tile;
pub use popover::{
    MenuItem, MenuPlacement, RowAction, actionable_count, menu_button, menu_button_trigger,
    menu_panel, row_actions,
};
pub use rhai_highlight::{
    RhaiHighlighter, RhaiHighlighterSettings, RhaiTokenKind, scan_type_hint, tokenize_line,
};
pub use script_editor_overlay::ScriptEditorOverlay;
pub use script_editor_widget::{
    OverlayChoice, ScriptEditorWidgetMsg, ScriptEditorWidgetState, apply_autocomplete_insert,
    choose_overlay, prefix_under_cursor, script_editor_widget, should_trigger_autocomplete,
};
pub use sections::{
    BannerKind, DividerAxis, ToastVariant, counter_badge, divider, empty_state, inline_error,
    live_status_banner, section_header, section_header_expandable, toast_banner,
};
pub use server::{BindAddressCardParams, BindBadge, bearer_token_display, bind_address_card};
pub use side_sheet::{
    Easing, SheetAnimation, SheetHeader, SheetPosition, SheetWidth, SideSheet, SideSheetConfig,
};
pub use status::{StatusVariant, platform_badge, role_badge, status_dot, status_pill};
pub use tag_list_input::{TagListInputMessage, TagListInputState, tag_list_input};
pub use theme::{catppuccin_mocha, latte, palette_for_theme, tokyo_night_storm};
pub use toast::{Toast, ToastAction, ToastKind, ToastQueue, toast_viewport};
pub use toggle::{ToggleProps, toggle, toggle_switch};
pub use tokens::{
    DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FontRole, FontWeight, ModalSize, Radius,
    Spacing, ThemeId, font, font_weighted, install_density, install_font_override, load_fonts,
    modal_width, radius, sp, spacing, spf,
};
pub use volume_slider::volume_slider;
