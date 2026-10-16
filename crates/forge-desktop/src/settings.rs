use std::sync::Arc;

use forge_components::{
    BORDER_THIN, BreadcrumbCrumb, Density, FONT_MD, FONT_SM, FONT_XS, FONT_XXS, ForgePalette, Icon,
    Picker, PickerEvent, PickerItem, PickerLabels, Radius, Spacing, ThemeId, badge, body_family,
    card, dropdown, field_hint, field_label, field_title, ghost_button_with_icon, icon,
    mono_family, page_frame, radius, set_body_family, set_mono_family, setting_row, spacing,
    toggle, tr, with_alpha,
};
use forge_storage::{Language, SettingsRepo, reserved_keys};
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, FontWeight, Pixels, Rgba, SharedString,
    Subscription, Window, div, prelude::*, px,
};

use crate::async_bridge::{self, ErrorSink};
use crate::awake_state::AwakeState;
use crate::presentation::ActiveLanguage;
use crate::runtime_handles::RuntimeHandles;
use crate::settings_audio::SettingsAudioView;
use crate::settings_diagnostics::SettingsDiagnosticsView;
use crate::settings_pane_header::SettingsPaneHeader;
use crate::settings_scripting::SettingsScriptingView;
use crate::settings_shortcuts::SettingsShortcutsView;
use crate::settings_storage::SettingsStorageView;
use crate::settings_websocket::SettingsWebSocketView;
use forge_components::{ActivePresentation, Presentation};

const RELEASES_URL: &str = concat!(env!("CARGO_PKG_REPOSITORY"), "/releases");

pub const NAV_WIDTH: Pixels = px(210.0);
pub const PANE_MIN_WIDTH: Pixels = px(360.0);

const NAV_PAD_V: Pixels = px(12.0);
const NAV_PAD_H: Pixels = px(8.0);
const NAV_GROUP_GAP: Pixels = px(10.0);
const NAV_GROUP_LABEL_FONT: Pixels = px(10.0);
const NAV_GROUP_LABEL_PAD_V: Pixels = px(6.0);
const NAV_GROUP_LABEL_MB: Pixels = px(4.0);
const NAV_ROW_PAD_V: Pixels = px(7.0);
const NAV_ROW_PAD_H: Pixels = px(10.0);
const NAV_ROW_GAP: Pixels = px(10.0);
const NAV_ROW_MB: Pixels = px(2.0);
const NAV_ROW_FONT: Pixels = px(12.5);
const NAV_ROW_ICON: Pixels = px(14.0);
const PANE_PAD_V: Pixels = px(20.0);
const PANE_PAD_H: Pixels = px(24.0);

const NAV_GROUPS: [(&str, &[SettingsSection]); 3] = [
    (
        "settings_nav_group_preferences",
        &[
            SettingsSection::General,
            SettingsSection::Appearance,
            SettingsSection::Language,
            SettingsSection::Shortcuts,
        ],
    ),
    (
        "settings_nav_group_engine",
        &[
            SettingsSection::Audio,
            SettingsSection::Scripting,
            SettingsSection::Storage,
            SettingsSection::WebSocket,
        ],
    ),
    (
        "settings_nav_group_about",
        &[SettingsSection::Version, SettingsSection::Diagnostics],
    ),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsSection {
    General,
    Appearance,
    Language,
    Shortcuts,
    Audio,
    Scripting,
    Storage,
    WebSocket,
    Version,
    Diagnostics,
}

impl SettingsSection {
    pub(crate) fn label(self) -> String {
        match self {
            SettingsSection::General => tr!("settings_nav_general"),
            SettingsSection::Appearance => tr!("settings_nav_appearance"),
            SettingsSection::Language => tr!("settings_nav_language_region"),
            SettingsSection::Shortcuts => tr!("settings_nav_shortcuts"),
            SettingsSection::Audio => tr!("settings_nav_audio"),
            SettingsSection::Scripting => tr!("settings_scripting_title"),
            SettingsSection::Storage => tr!("settings_storage_section_title"),
            SettingsSection::WebSocket => tr!("settings_ws_title"),
            SettingsSection::Version => tr!("settings_version_title"),
            SettingsSection::Diagnostics => tr!("settings_diagnostics_section_title"),
        }
    }

    pub(crate) fn icon(self) -> Icon {
        match self {
            SettingsSection::General => Icon::Settings,
            SettingsSection::Appearance => Icon::Palette,
            SettingsSection::Language => Icon::Language,
            SettingsSection::Shortcuts => Icon::Keyboard,
            SettingsSection::Audio => Icon::Volume,
            SettingsSection::Scripting => Icon::Code,
            SettingsSection::Storage => Icon::Database,
            SettingsSection::WebSocket => Icon::Network,
            SettingsSection::Version => Icon::InfoCircle,
            SettingsSection::Diagnostics => Icon::Bug,
        }
    }

    fn key(self) -> &'static str {
        match self {
            SettingsSection::General => "general",
            SettingsSection::Appearance => "appearance",
            SettingsSection::Language => "language",
            SettingsSection::Shortcuts => "shortcuts",
            SettingsSection::Audio => "audio",
            SettingsSection::Scripting => "scripting",
            SettingsSection::Storage => "storage",
            SettingsSection::WebSocket => "websocket",
            SettingsSection::Version => "version",
            SettingsSection::Diagnostics => "diagnostics",
        }
    }

    pub(crate) fn from_key(key: &str) -> Option<Self> {
        NAV_GROUPS
            .iter()
            .flat_map(|(_, sections)| sections.iter())
            .copied()
            .find(|section| section.key() == key)
    }
}

fn theme_meta(theme: ThemeId) -> (String, String) {
    match theme {
        ThemeId::ForgeDefault => (
            "Forge Default".to_owned(),
            format!("Violet · {}", tr!("settings_theme_desc_dark")),
        ),
        ThemeId::TokyoNight => ("Tokyo Night".to_owned(), tr!("settings_theme_desc_storm")),
        ThemeId::Latte => (
            "Catppuccin Latte".to_owned(),
            tr!("settings_theme_desc_light_mode"),
        ),
    }
}

fn density_meta(density: Density) -> (String, String) {
    match density {
        Density::Compact => (
            tr!("settings_appearance_density_compact"),
            tr!("settings_appearance_density_compact_hint"),
        ),
        Density::Cozy => (
            tr!("settings_appearance_density_cozy"),
            tr!("settings_appearance_density_cozy_hint"),
        ),
        Density::Spacious => (
            tr!("settings_appearance_density_spacious"),
            tr!("settings_appearance_density_spacious_hint"),
        ),
    }
}

fn density_key(density: Density) -> &'static str {
    match density {
        Density::Compact => "compact",
        Density::Cozy => "cozy",
        Density::Spacious => "spacious",
    }
}

fn theme_key(theme: ThemeId) -> &'static str {
    match theme {
        ThemeId::ForgeDefault => "forge",
        ThemeId::TokyoNight => "tokyo",
        ThemeId::Latte => "latte",
    }
}

pub struct SettingsView {
    section: SettingsSection,
    handles: Arc<RuntimeHandles>,
    awake: Entity<AwakeState>,
    language: Language,
    audio: Entity<SettingsAudioView>,
    scripting: Entity<SettingsScriptingView>,
    websocket: Entity<SettingsWebSocketView>,
    shortcuts: Entity<SettingsShortcutsView>,
    storage: Entity<SettingsStorageView>,
    diagnostics: Entity<SettingsDiagnosticsView>,
    font_picker: Option<FontPicker>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FontTarget {
    Body,
    Mono,
}

struct FontPicker {
    picker: Entity<Picker>,
    target: FontTarget,
    _sub: Subscription,
}

const FONT_DEFAULT_ID: &str = "__forge_default_font__";

impl SettingsView {
    pub fn new(
        handles: Arc<RuntimeHandles>,
        awake: Entity<AwakeState>,
        preselect: Option<SettingsSection>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&awake, |_, _, cx| cx.notify()).detach();
        let section = preselect.unwrap_or(NAV_GROUPS[0].1[0]);
        let audio =
            cx.new(|cx| SettingsAudioView::new(&handles, section == SettingsSection::Audio, cx));
        let scripting = cx.new(|cx| {
            SettingsScriptingView::new(Arc::clone(&handles.backend), handles.rt_handle.clone(), cx)
        });
        let websocket = cx.new(|cx| {
            SettingsWebSocketView::new(
                Arc::clone(&handles.backend),
                handles.rt_handle.clone(),
                handles.server.clone(),
                handles.overlays.clone(),
                cx,
            )
        });
        let shortcuts = cx.new(|cx| {
            SettingsShortcutsView::new(
                Arc::clone(&handles.backend),
                handles.rt_handle.clone(),
                handles.hotkey_client.clone(),
                cx,
            )
        });
        let storage = cx.new(|cx| {
            SettingsStorageView::new(
                Arc::clone(&handles.backend),
                handles.rt_handle.clone(),
                handles.chat_history_retention.clone(),
                cx,
            )
        });
        let diagnostics = cx.new(|cx| {
            SettingsDiagnosticsView::new(
                handles.log_tail.clone(),
                Arc::clone(&handles.backend),
                handles.builtins.clone(),
                handles.rt_handle.clone(),
                section == SettingsSection::Diagnostics,
                cx,
            )
        });
        Self {
            section,
            handles,
            awake,
            language: cx.global::<ActiveLanguage>().0,
            audio,
            scripting,
            websocket,
            shortcuts,
            storage,
            diagnostics,
            font_picker: None,
        }
    }

    fn stay_awake_enabled(&self, cx: &App) -> bool {
        self.awake
            .read(cx)
            .enabled()
            .unwrap_or(crate::stay_awake::ENABLED_BY_DEFAULT)
    }

    fn toggle_stay_awake(&mut self, cx: &mut Context<Self>) {
        let enabled = !self.stay_awake_enabled(cx);
        let stay_awake = self.handles.stay_awake.clone();
        self.handles.rt_handle.spawn(async move {
            stay_awake.set_enabled(enabled).await;
        });
        cx.notify();
    }

    fn select_section(&mut self, section: SettingsSection, cx: &mut Context<Self>) {
        self.section = section;
        let diagnostics_open = section == SettingsSection::Diagnostics;
        self.diagnostics.update(cx, |view, cx| {
            view.set_active(diagnostics_open, cx);
        });
        let audio_open = section == SettingsSection::Audio;
        self.audio.update(cx, |view, cx| {
            view.set_active(audio_open, cx);
        });
        cx.notify();
    }

    fn open_font_picker(
        &mut self,
        target: FontTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let palette = cx.palette();
        let default_label = match target {
            FontTarget::Body => tr!("settings_appearance_font_default_body"),
            FontTarget::Mono => tr!("settings_appearance_font_default_mono"),
        };
        let mut items = vec![PickerItem {
            id: FONT_DEFAULT_ID.into(),
            label: default_label.into(),
            sublabel: None,
            icon: None,
        }];
        items.extend(
            cx.text_system()
                .all_font_names()
                .into_iter()
                .map(|name| PickerItem {
                    id: name.clone().into(),
                    label: name.into(),
                    sublabel: None,
                    icon: None,
                }),
        );

        let labels = PickerLabels {
            placeholder: tr!("settings_appearance_font_search").into(),
            empty: tr!("widget_picker_no_results").into(),
            loading: tr!("widget_picker_loading").into(),
        };
        let current = match target {
            FontTarget::Body => body_family(),
            FontTarget::Mono => mono_family(),
        };

        let picker =
            cx.new(|cx| Picker::new(labels, items, palette, cx).with_current(Some(current)));
        let sub = cx.subscribe(&picker, Self::on_font_picker_event);
        picker.update(cx, |f, cx| f.focus(window, cx));
        self.font_picker = Some(FontPicker {
            picker,
            target,
            _sub: sub,
        });
        cx.notify();
    }

    fn on_font_picker_event(
        &mut self,
        _picker: Entity<Picker>,
        event: &PickerEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            PickerEvent::Selected(id) => self.pick_font(id.clone(), cx),
            PickerEvent::Cancelled => self.close_font_picker(cx),
        }
    }

    fn pick_font(&mut self, id: SharedString, cx: &mut Context<Self>) {
        let Some(pending) = self.font_picker.take() else {
            return;
        };
        let family: Option<SharedString> = if id.as_ref() == FONT_DEFAULT_ID {
            None
        } else {
            Some(id)
        };
        match pending.target {
            FontTarget::Body => set_body_family(family.clone()),
            FontTarget::Mono => set_mono_family(family.clone()),
        }

        let backend = Arc::clone(&self.handles.backend) as Arc<dyn SettingsRepo>;
        let persisted = family.map(|f| f.to_string());
        let target = pending.target;
        async_bridge::report_failure(
            &self.handles.rt_handle,
            async move {
                match target {
                    FontTarget::Body => backend.set_font_body(persisted).await,
                    FontTarget::Mono => backend.set_font_mono(persisted).await,
                }
            },
            ErrorSink::Toast,
            tr!("settings_appearance_font_persist_failed"),
            cx,
        );

        cx.refresh_windows();
        cx.notify();
    }

    fn close_font_picker(&mut self, cx: &mut Context<Self>) {
        self.font_picker = None;
        cx.notify();
    }

    fn font_field(
        &self,
        label: SharedString,
        target: FontTarget,
        current: SharedString,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let element_id = match target {
            FontTarget::Body => "font-field-body",
            FontTarget::Mono => "font-field-mono",
        };
        let preview_family = current.clone();
        let value = div()
            .id(element_id)
            .cursor_pointer()
            .flex()
            .items_center()
            .justify_between()
            .px(spacing(Spacing::Sm, Density::Cozy))
            .py(px(7.0))
            .rounded(radius(Radius::Sm))
            .bg(palette.base)
            .border(BORDER_THIN)
            .border_color(palette.border_input)
            .hover(|s| s.border_color(palette.border_active))
            .child(
                div()
                    .font_family(preview_family)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(current),
            )
            .child(icon(Icon::ChevronDown, FONT_XS, palette.text_faint))
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.open_font_picker(target, window, cx)
            }));
        let popover = self
            .font_picker
            .as_ref()
            .filter(|pending| pending.target == target)
            .map(|pending| {
                let view = cx.entity();
                dropdown(pending.picker.clone()).on_dismiss(move |_window, cx| {
                    view.update(cx, |this, cx| this.close_font_picker(cx));
                })
            });
        let field = div().relative().w_full().child(value).children(popover);
        div()
            .flex_1()
            .min_w(px(0.0))
            .child(field_label(palette, label, field))
    }

    fn select_theme(&mut self, theme: ThemeId, cx: &mut Context<Self>) {
        let density = cx.density();
        cx.set_global(Presentation::new(theme, density));

        let backend = Arc::clone(&self.handles.backend) as Arc<dyn SettingsRepo>;
        let key = theme.storage_key().to_owned();
        async_bridge::report_failure(
            &self.handles.rt_handle,
            async move { backend.set_theme(&key).await },
            ErrorSink::Toast,
            tr!("settings_theme_persist_failed"),
            cx,
        );
        cx.refresh_windows();
        cx.notify();
    }

    fn select_language(&mut self, lang: Language, cx: &mut Context<Self>) {
        if self.language == lang {
            return;
        }
        self.language = lang;
        crate::i18n::install_language(lang);
        self.handles
            .anonymous_donor
            .store(tr!(crate::i18n::ANONYMOUS_DONOR_KEY));
        cx.set_global(ActiveLanguage(lang));

        let backend = Arc::clone(&self.handles.backend) as Arc<dyn SettingsRepo>;
        self.handles.rt_handle.spawn(async move {
            if let Err(e) = backend.set_language(lang).await {
                tracing::warn!(error = %e, "failed to persist language selection");
            }
        });

        cx.refresh_windows();
        cx.notify();
    }

    fn select_density(&mut self, density: Density, cx: &mut Context<Self>) {
        let theme = cx.theme();
        cx.set_global(Presentation::new(theme, density));

        let backend = Arc::clone(&self.handles.backend) as Arc<dyn SettingsRepo>;
        let key = density.storage_key().to_owned();
        async_bridge::report_failure(
            &self.handles.rt_handle,
            async move { backend.set_string(reserved_keys::DENSITY, &key).await },
            ErrorSink::Toast,
            tr!("settings_density_persist_failed"),
            cx,
        );
        cx.refresh_windows();
        cx.notify();
    }

    fn check_for_updates(&mut self, cx: &mut Context<Self>) {
        async_bridge::open_external(
            &self.handles.rt_handle,
            RELEASES_URL,
            ErrorSink::Toast,
            tr!("settings_check_updates_failed"),
            cx,
        );
    }

    fn render_nav(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let mut list = div().flex().flex_col().gap(NAV_GROUP_GAP);
        for (group, sections) in NAV_GROUPS {
            let mut block = div().flex().flex_col().child(
                div()
                    .px(NAV_ROW_PAD_H)
                    .py(NAV_GROUP_LABEL_PAD_V)
                    .mb(NAV_GROUP_LABEL_MB)
                    .font_family(mono_family())
                    .text_size(NAV_GROUP_LABEL_FONT)
                    .text_color(palette.text_faint)
                    .child(tr!(group)),
            );
            for &section in sections {
                block = block.child(self.nav_row(section, palette, cx));
            }
            list = list.child(block);
        }

        div()
            .id("settings-nav")
            .w(NAV_WIDTH)
            .min_w(NAV_WIDTH)
            .max_w(NAV_WIDTH)
            .h_full()
            .flex_shrink_0()
            .overflow_y_scroll()
            .bg(palette.shell)
            .border_r(BORDER_THIN)
            .border_color(palette.border_regular)
            .py(NAV_PAD_V)
            .px(NAV_PAD_H)
            .child(list)
    }

    fn nav_row(
        &self,
        section: SettingsSection,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let active = self.section == section;
        let (text_color, icon_color) = if active {
            (palette.text_primary, palette.brand)
        } else {
            (palette.text_secondary, palette.text_secondary)
        };
        let hover_bg = palette.elevated;
        let row = div()
            .id(SharedString::from(format!(
                "settings-nav-{}",
                section.key()
            )))
            .flex()
            .items_center()
            .gap(NAV_ROW_GAP)
            .px(NAV_ROW_PAD_H)
            .py(NAV_ROW_PAD_V)
            .mb(NAV_ROW_MB)
            .rounded(radius(Radius::Sm))
            .cursor_pointer()
            .font_family(body_family())
            .text_size(NAV_ROW_FONT)
            .text_color(text_color)
            .child(icon(section.icon(), NAV_ROW_ICON, icon_color))
            .child(section.label())
            .on_click(
                cx.listener(move |this, _: &ClickEvent, _, cx| this.select_section(section, cx)),
            );
        if active {
            row.bg(palette.surface_overlay)
        } else {
            row.hover(move |style| style.bg(hover_bg))
        }
    }

    fn render_pane(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let content = match self.section {
            SettingsSection::General => self.general_pane(palette, density, cx),
            SettingsSection::Appearance => self.appearance_pane(palette, density, cx),
            SettingsSection::Language => self.language_pane(palette, density, cx),
            SettingsSection::Shortcuts => self.shortcuts.clone().into_any_element(),
            SettingsSection::Audio => self.audio.clone().into_any_element(),
            SettingsSection::Scripting => self.scripting.clone().into_any_element(),
            SettingsSection::WebSocket => self.websocket.clone().into_any_element(),
            SettingsSection::Storage => self.storage.clone().into_any_element(),
            SettingsSection::Version => self.version_pane(palette, density, cx),
            SettingsSection::Diagnostics => self.diagnostics.clone().into_any_element(),
        };

        div()
            .id("settings-pane")
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .overflow_y_scroll()
            .bg(palette.base)
            .py(PANE_PAD_V)
            .px(PANE_PAD_H)
            .child(content)
    }

    fn appearance_pane(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut theme_grid = div().flex().flex_row().gap(spacing(Spacing::Sm, density));
        let active = cx.theme();
        for theme in ThemeId::ALL {
            theme_grid = theme_grid.child(self.theme_card(theme, active == theme, palette, cx));
        }
        let theme_block = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(field_title(tr!("settings_appearance_theme_label"), palette))
            .child(field_hint(tr!("settings_appearance_theme_hint"), palette))
            .child(theme_grid);

        let mut density_rows = div().flex().flex_col().gap(spacing(Spacing::Xxs, density));
        let current_density = cx.density();
        for option in [Density::Compact, Density::Cozy, Density::Spacious] {
            density_rows = density_rows.child(self.density_row(
                option,
                current_density == option,
                palette,
                cx,
            ));
        }
        let density_block = section_divider(palette, density).child(
            div()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Xs, density))
                .child(field_title(
                    tr!("settings_appearance_density_label"),
                    palette,
                ))
                .child(field_hint(
                    tr!("settings_appearance_density_subtitle"),
                    palette,
                ))
                .child(density_rows),
        );

        let fonts_block = section_divider(palette, density).child(
            div()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Sm, density))
                .child(field_title(tr!("settings_appearance_fonts_label"), palette))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap(spacing(Spacing::Sm, density))
                        .child(self.font_field(
                            tr!("settings_appearance_font_interface").into(),
                            FontTarget::Body,
                            body_family(),
                            palette,
                            cx,
                        ))
                        .child(self.font_field(
                            tr!("settings_appearance_font_monospace").into(),
                            FontTarget::Mono,
                            mono_family(),
                            palette,
                            cx,
                        )),
                ),
        );

        div()
            .flex()
            .flex_col()
            .child(SettingsPaneHeader::new(SettingsSection::Appearance))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(spacing(Spacing::Md, density))
                    .child(theme_block)
                    .child(density_block)
                    .child(fonts_block),
            )
            .into_any_element()
    }

    fn theme_card(
        &self,
        theme: ThemeId,
        active: bool,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let pal = theme.palette();
        let (title, subtitle) = theme_meta(theme);
        let border_color = if active {
            palette.brand
        } else {
            palette.border_regular
        };

        let bar = |height: f32, width_frac: gpui::DefiniteLength, color: Rgba| {
            div().h(px(height)).w(width_frac).rounded(px(2.0)).bg(color)
        };
        let sidebar = div()
            .w(px(46.0))
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .p(px(6.0))
            .bg(pal.shell)
            .border_r(BORDER_THIN)
            .border_color(pal.border_regular)
            .child(bar(5.0, gpui::relative(1.0), pal.brand))
            .child(bar(
                5.0,
                gpui::relative(0.85),
                with_alpha(pal.text_muted, 0.4),
            ))
            .child(bar(
                5.0,
                gpui::relative(0.7),
                with_alpha(pal.text_muted, 0.4),
            ))
            .child(div().flex_1())
            .child(bar(
                4.0,
                gpui::relative(0.6),
                with_alpha(pal.text_muted, 0.3),
            ));
        let content_area = div()
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .flex()
            .flex_col()
            .gap(px(5.0))
            .p(px(8.0))
            .child(bar(
                8.0,
                gpui::relative(0.5),
                with_alpha(pal.text_muted, 0.6),
            ))
            .child(bar(
                4.0,
                gpui::relative(0.8),
                with_alpha(pal.text_muted, 0.3),
            ))
            .child(div().flex_1())
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(5.0))
                    .child(bar(14.0, gpui::relative(0.4), pal.brand))
                    .child(bar(
                        14.0,
                        gpui::relative(0.3),
                        with_alpha(pal.text_muted, 0.2),
                    )),
            )
            .child(bar(
                6.0,
                gpui::relative(0.7),
                with_alpha(pal.text_muted, 0.3),
            ));
        let preview = div()
            .h(px(100.0))
            .w_full()
            .flex()
            .flex_row()
            .overflow_hidden()
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(pal.border_regular)
            .bg(pal.base)
            .child(sidebar)
            .child(content_area);

        let mut footer = div().flex().items_center().justify_between().child(
            div()
                .font_family(body_family())
                .font_weight(FontWeight::MEDIUM)
                .text_size(FONT_SM)
                .text_color(palette.text_primary)
                .child(title),
        );
        if active {
            footer = footer.child(badge(
                palette.surface_overlay,
                palette.brand,
                tr!("settings_theme_active"),
                true,
                FONT_XXS,
            ));
        }

        div()
            .id(SharedString::from(format!(
                "settings-theme-{}",
                theme_key(theme)
            )))
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(6.0))
            .p(px(12.0))
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(border_color)
            .bg(palette.elevated)
            .cursor_pointer()
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.select_theme(theme, cx)))
            .child(preview)
            .child(footer)
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(subtitle),
            )
    }

    fn density_row(
        &self,
        density: Density,
        active: bool,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let (label, hint) = density_meta(density);
        let labels = div()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(label),
            )
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(hint),
            );

        let mut row = div()
            .id(SharedString::from(format!(
                "settings-density-{}",
                density_key(density)
            )))
            .flex()
            .items_center()
            .justify_between()
            .px(spacing(Spacing::Sm, Density::Cozy))
            .py(px(8.0))
            .rounded(radius(Radius::Sm))
            .cursor_pointer()
            .on_click(
                cx.listener(move |this, _: &ClickEvent, _, cx| this.select_density(density, cx)),
            )
            .child(labels);
        if active {
            row = row
                .bg(with_alpha(palette.brand, 0.12))
                .border(BORDER_THIN)
                .border_color(with_alpha(palette.brand, 0.5))
                .child(icon(Icon::CircleCheck, FONT_SM, palette.brand));
        }
        row
    }

    fn language_pane(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let list = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(self.language_option_row("English", "en-US", Language::En, palette, cx))
            .child(self.language_option_row("Українська", "uk-UA", Language::Uk, palette, cx));

        div()
            .flex()
            .flex_col()
            .child(
                SettingsPaneHeader::new(SettingsSection::Language)
                    .subtitle(tr!("settings_language_subtitle")),
            )
            .child(list)
            .into_any_element()
    }

    fn language_option_row(
        &self,
        native_label: &'static str,
        bcp47: &'static str,
        lang: Language,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let is_selected = self.language == lang;

        let chip = div()
            .px(px(8.0))
            .py(px(3.0))
            .rounded(radius(Radius::Sm))
            .bg(palette.surface_overlay)
            .font_family(mono_family())
            .text_size(FONT_XS)
            .text_color(palette.text_primary)
            .child(bcp47);

        let mut row = div()
            .id(SharedString::from(format!("settings-language-{lang}")))
            .flex()
            .items_center()
            .justify_between()
            .w_full()
            .py(px(10.0))
            .rounded(radius(Radius::Sm))
            .cursor_pointer()
            .on_click(
                cx.listener(move |this, _: &ClickEvent, _, cx| this.select_language(lang, cx)),
            )
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(native_label),
            )
            .child(chip);
        if is_selected {
            row = row
                .bg(with_alpha(palette.brand, 0.12))
                .border(BORDER_THIN)
                .border_color(with_alpha(palette.brand, 0.5));
        }
        row
    }

    fn version_pane(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let version = env!("CARGO_PKG_VERSION");

        let tile = div()
            .w(px(48.0))
            .h(px(48.0))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(11.0))
            .bg(palette.brand)
            .child(
                div()
                    .font_family(body_family())
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(px(24.0))
                    .text_color(palette.base)
                    .child("F"),
            );
        let name_line = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_MD)
                    .text_color(palette.text_primary)
                    .child("Forge"),
            )
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(format!("v{version}")),
            );
        let identity = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Md, density))
            .child(tile)
            .child(
                div().flex().flex_col().gap(px(4.0)).child(name_line).child(
                    div()
                        .font_family(body_family())
                        .text_size(FONT_XS)
                        .text_color(palette.text_muted)
                        .child(tr!("settings_version_license")),
                ),
            )
            .child(div().flex_1())
            .child(
                ghost_button_with_icon(
                    Icon::Refresh,
                    tr!("settings_version_check_updates"),
                    palette,
                )
                .on_click(
                    "settings-check-updates",
                    cx.listener(|this, _: &ClickEvent, _, cx| this.check_for_updates(cx)),
                ),
            );

        div()
            .flex()
            .flex_col()
            .child(SettingsPaneHeader::new(SettingsSection::Version))
            .child(card(identity, palette))
            .into_any_element()
    }

    fn general_pane(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let stay_awake_row = setting_row(
            tr!("settings_stay_awake_label"),
            Some(tr!("settings_stay_awake_hint").into()),
            toggle(self.stay_awake_enabled(cx), palette).on_click(
                "settings-general-stay-awake",
                cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_stay_awake(cx)),
            ),
            palette,
            density,
        );

        div()
            .flex()
            .flex_col()
            .child(SettingsPaneHeader::new(SettingsSection::General))
            .child(card(stay_awake_row, palette))
            .into_any_element()
    }
}

impl Render for SettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let nav = self.render_nav(&palette, cx);
        let pane = self.render_pane(&palette, density, cx);

        let body = div()
            .w_full()
            .flex_1()
            .flex()
            .flex_row()
            .overflow_hidden()
            .child(nav)
            .child(pane);

        let frame = page_frame(
            vec![
                BreadcrumbCrumb::leaf(tr!("settings_page_title")),
                BreadcrumbCrumb::leaf(self.section.label()),
            ],
            &palette,
        )
        .body(body);

        div().size_full().flex().flex_col().child(frame)
    }
}

fn section_divider(palette: &ForgePalette, density: Density) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Sm, density))
        .pt(spacing(Spacing::Md, density))
        .border_t(BORDER_THIN)
        .border_color(palette.border_regular)
}
