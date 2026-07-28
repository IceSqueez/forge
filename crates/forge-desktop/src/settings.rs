use std::sync::Arc;

use forge_components::{
    BORDER_THIN, BreadcrumbCrumb, Density, FONT_LG, FONT_MD, FONT_SM, FONT_XS, FONT_XXS,
    ForgePalette, Icon, OverlayPosition, Picker, PickerEvent, PickerItem, PickerLabels, Radius,
    Spacing, ThemeId, badge, body_family, card, field_hint, field_label, field_title,
    ghost_button_with_icon, icon, metric_card, mono_family, overlay, page_frame, primary_button,
    primary_button_with_icon, radius, set_body_family, set_mono_family, spacing, tr, with_alpha,
};
use forge_storage::{Language, SettingsRepo, reserved_keys};
use gpui::{
    AnyElement, ClickEvent, Context, Entity, FontWeight, Rgba, SharedString, Subscription, Window,
    div, prelude::*, px,
};

use crate::async_bridge::{self, ErrorSink};
use crate::presentation::{ActiveLanguage, ActivePresentation, Presentation};
use crate::runtime_handles::RuntimeHandles;
use crate::settings_audio::SettingsAudioView;
use crate::settings_scripting::SettingsScriptingView;
use crate::settings_shortcuts::SettingsShortcutsView;
use crate::settings_storage::SettingsStorageView;
use crate::settings_websocket::SettingsWebSocketView;

const RELEASES_URL: &str = concat!(env!("CARGO_PKG_REPOSITORY"), "/releases");

const RUST_VERSION: &str = "1.96.0";

const RECENT_RELEASES: [(&str, &str, &str); 3] = [
    ("v0.9.2", "Server panel, settings → websocket", "today"),
    ("v0.9.1", "OBS scene metadata, lock indicators", "3d ago"),
    ("v0.9.0", "TTS module GA, filters live preview", "1w ago"),
];

const NAV_GROUPS: [(&str, &[SettingsSection]); 3] = [
    (
        "settings_nav_group_preferences",
        &[
            SettingsSection::Appearance,
            SettingsSection::Language,
            SettingsSection::Shortcuts,
            SettingsSection::Notifications,
        ],
    ),
    (
        "settings_nav_group_engine",
        &[
            SettingsSection::Audio,
            SettingsSection::Scripting,
            SettingsSection::Queues,
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
    Appearance,
    Language,
    Shortcuts,
    Notifications,
    Audio,
    Scripting,
    Queues,
    Storage,
    WebSocket,
    Version,
    Diagnostics,
}

impl SettingsSection {
    fn label(self) -> String {
        match self {
            SettingsSection::Appearance => tr!("settings_nav_appearance"),
            SettingsSection::Language => tr!("settings_nav_language_region"),
            SettingsSection::Shortcuts => tr!("settings_nav_shortcuts"),
            SettingsSection::Notifications => tr!("settings_nav_notifications"),
            SettingsSection::Audio => tr!("settings_nav_audio"),
            SettingsSection::Scripting => tr!("settings_scripting_title"),
            SettingsSection::Queues => tr!("settings_queues_section_title"),
            SettingsSection::Storage => tr!("settings_storage_section_title"),
            SettingsSection::WebSocket => tr!("settings_ws_title"),
            SettingsSection::Version => tr!("settings_version_title"),
            SettingsSection::Diagnostics => tr!("settings_diagnostics_section_title"),
        }
    }

    fn icon(self) -> Icon {
        match self {
            SettingsSection::Appearance => Icon::Photo,
            SettingsSection::Language => Icon::Globe,
            SettingsSection::Shortcuts => Icon::Keyboard,
            SettingsSection::Notifications => Icon::InfoCircle,
            SettingsSection::Audio => Icon::Volume,
            SettingsSection::Scripting => Icon::Terminal,
            SettingsSection::Queues => Icon::Notebook,
            SettingsSection::Storage => Icon::Folder,
            SettingsSection::WebSocket => Icon::Server,
            SettingsSection::Version => Icon::Diamond,
            SettingsSection::Diagnostics => Icon::Activity,
        }
    }

    fn key(self) -> &'static str {
        match self {
            SettingsSection::Appearance => "appearance",
            SettingsSection::Language => "language",
            SettingsSection::Shortcuts => "shortcuts",
            SettingsSection::Notifications => "notifications",
            SettingsSection::Audio => "audio",
            SettingsSection::Scripting => "scripting",
            SettingsSection::Queues => "queues",
            SettingsSection::Storage => "storage",
            SettingsSection::WebSocket => "websocket",
            SettingsSection::Version => "version",
            SettingsSection::Diagnostics => "diagnostics",
        }
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
    language: Language,
    audio: Entity<SettingsAudioView>,
    scripting: Entity<SettingsScriptingView>,
    websocket: Entity<SettingsWebSocketView>,
    shortcuts: Entity<SettingsShortcutsView>,
    storage: Entity<SettingsStorageView>,
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
    pub fn new(handles: Arc<RuntimeHandles>, cx: &mut Context<Self>) -> Self {
        let audio = cx.new(|cx| {
            SettingsAudioView::new(Arc::clone(&handles.backend), handles.rt_handle.clone(), cx)
        });
        let scripting = cx.new(|cx| {
            SettingsScriptingView::new(Arc::clone(&handles.backend), handles.rt_handle.clone(), cx)
        });
        let websocket = cx.new(|cx| {
            SettingsWebSocketView::new(
                Arc::clone(&handles.backend),
                handles.rt_handle.clone(),
                handles.server.clone(),
                cx,
            )
        });
        let shortcuts = cx.new(|cx| {
            SettingsShortcutsView::new(Arc::clone(&handles.backend), handles.rt_handle.clone(), cx)
        });
        let storage = cx.new(|cx| {
            SettingsStorageView::new(Arc::clone(&handles.backend), handles.rt_handle.clone(), cx)
        });
        Self {
            section: SettingsSection::Appearance,
            handles,
            language: cx.global::<ActiveLanguage>().0,
            audio,
            scripting,
            websocket,
            shortcuts,
            storage,
            font_picker: None,
        }
    }

    fn select_section(&mut self, section: SettingsSection, cx: &mut Context<Self>) {
        self.section = section;
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
            icon: Icon::Refresh,
        }];
        items.extend(
            cx.text_system()
                .all_font_names()
                .into_iter()
                .map(|name| PickerItem {
                    id: name.clone().into(),
                    label: name.into(),
                    sublabel: None,
                    icon: Icon::FileText,
                }),
        );

        let labels = PickerLabels {
            title: match target {
                FontTarget::Body => tr!("settings_appearance_font_picker_body"),
                FontTarget::Mono => tr!("settings_appearance_font_picker_mono"),
            }
            .into(),
            placeholder: tr!("settings_appearance_font_search").into(),
            empty: tr!("widget_picker_no_results").into(),
            loading: tr!("widget_picker_loading").into(),
            cancel: tr!("common_cancel").into(),
        };

        let picker = cx.new(|cx| Picker::new(labels, items, palette, cx));
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
        div().flex_1().child(field_label(palette, label, value))
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
        cx.notify();
    }

    fn select_language(&mut self, lang: Language, cx: &mut Context<Self>) {
        if self.language == lang {
            return;
        }
        self.language = lang;
        crate::i18n::install_language(lang);
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

    fn open_log_dir(&mut self, cx: &mut Context<Self>) {
        let dir = forge_platform_core::paths::data_dir().join("logs");
        cx.reveal_path(&dir);
    }

    fn render_status(&self, palette: &ForgePalette) -> impl IntoElement + use<> {
        div()
            .flex()
            .items_center()
            .gap(px(5.0))
            .child(icon(Icon::CircleCheck, FONT_XS, palette.success))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.success)
                    .child(tr!("widget_save_all_saved")),
            )
    }

    fn render_nav(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let mut list = div().flex().flex_col().gap(spacing(Spacing::Xxs, density));
        for (group, sections) in NAV_GROUPS {
            list = list.child(
                div()
                    .px(spacing(Spacing::Xs, density))
                    .pt(spacing(Spacing::Xs, density))
                    .pb(px(4.0))
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_faint)
                    .child(tr!(group)),
            );
            for &section in sections {
                list = list.child(self.nav_button(section, palette, density, cx));
            }
        }

        div()
            .id("settings-nav")
            .w(px(200.0))
            .h_full()
            .flex_shrink_0()
            .overflow_y_scroll()
            .bg(palette.shell)
            .border_r(BORDER_THIN)
            .border_color(palette.border_regular)
            .py(spacing(Spacing::Sm, density))
            .px(spacing(Spacing::Xs, density))
            .child(list)
    }

    fn nav_button(
        &self,
        section: SettingsSection,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let active = self.section == section;
        let button = if active {
            primary_button_with_icon(section.icon(), section.label(), palette)
        } else {
            ghost_button_with_icon(section.icon(), section.label(), palette)
        };
        button.full_width().density(density).on_click(
            section.key(),
            cx.listener(move |this, _: &ClickEvent, _, cx| this.select_section(section, cx)),
        )
    }

    fn render_pane(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let content = match self.section {
            SettingsSection::Appearance => self.appearance_pane(palette, density, cx),
            SettingsSection::Language => self.language_pane(palette, density, cx),
            SettingsSection::Shortcuts => self.shortcuts.clone().into_any_element(),
            SettingsSection::Audio => self.audio.clone().into_any_element(),
            SettingsSection::Scripting => self.scripting.clone().into_any_element(),
            SettingsSection::WebSocket => self.websocket.clone().into_any_element(),
            SettingsSection::Notifications => self.notifications_pane(palette, density),
            SettingsSection::Queues => self.queues_pane(palette, density),
            SettingsSection::Storage => self.storage.clone().into_any_element(),
            SettingsSection::Version => self.version_pane(palette, density, cx),
            SettingsSection::Diagnostics => self.diagnostics_pane(palette, density, cx),
        };

        div()
            .id("settings-pane")
            .flex_1()
            .h_full()
            .overflow_y_scroll()
            .bg(palette.base)
            .p(spacing(Spacing::Lg, density))
            .child(content)
    }

    fn appearance_pane(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let header = pane_header(Icon::LayoutGrid, tr!("settings_appearance_title"), palette);

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
            .gap(spacing(Spacing::Md, density))
            .child(header)
            .child(theme_block)
            .child(density_block)
            .child(fonts_block)
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
        let subtitle = div()
            .font_family(body_family())
            .text_size(FONT_SM)
            .text_color(palette.text_muted)
            .child(tr!("settings_language_subtitle"));

        let list = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(self.language_option_row("English", "en-US", Language::En, palette, cx))
            .child(self.language_option_row("Українська", "uk-UA", Language::Uk, palette, cx));

        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, density))
            .child(pane_header(
                Icon::Globe,
                tr!("settings_language_title"),
                palette,
            ))
            .child(subtitle)
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

        let mut releases = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(
                div()
                    .font_family(mono_family())
                    .text_size(FONT_XXS)
                    .text_color(palette.text_muted)
                    .child(tr!("settings_version_recent_releases")),
            );
        for (tag, summary, when) in RECENT_RELEASES {
            releases = releases.child(release_row(tag, summary, when, palette, density));
        }

        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, density))
            .child(pane_header(
                Icon::InfoCircle,
                tr!("settings_version_title"),
                palette,
            ))
            .child(card(identity, palette))
            .child(card(releases, palette))
            .into_any_element()
    }

    fn diagnostics_pane(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let version = env!("CARGO_PKG_VERSION");
        let log_dir = forge_platform_core::paths::data_dir().join("logs");
        let log_display = log_dir.display().to_string();

        let metric = |label: String, value: String| {
            div()
                .flex_1()
                .child(metric_card(label, value, None::<&str>, None, palette))
        };
        let metrics = div()
            .flex()
            .flex_row()
            .gap(spacing(Spacing::Sm, density))
            .child(metric(
                tr!("settings_about_build_label"),
                version.to_owned(),
            ))
            .child(metric(
                tr!("settings_about_rust_label"),
                RUST_VERSION.to_owned(),
            ))
            .child(metric(
                tr!("settings_about_os_label"),
                std::env::consts::OS.to_owned(),
            ));

        let path_box = div()
            .w_full()
            .px(spacing(Spacing::Sm, Density::Cozy))
            .py(px(7.0))
            .rounded(radius(Radius::Sm))
            .bg(palette.base)
            .border(BORDER_THIN)
            .border_color(palette.border_input)
            .font_family(mono_family())
            .text_size(FONT_XS)
            .text_color(palette.text_primary)
            .child(log_display);
        let open_btn = primary_button(tr!("settings_diagnostics_open_log_dir"), palette).on_click(
            "settings-open-logs",
            cx.listener(|this, _: &ClickEvent, _, cx| this.open_log_dir(cx)),
        );
        let logs_card = card(
            div()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Xs, density))
                .child(field_title(
                    tr!("settings_diagnostics_log_dir_label"),
                    palette,
                ))
                .child(path_box)
                .child(open_btn)
                .child(field_hint(
                    tr!("settings_diagnostics_log_dir_hint"),
                    palette,
                )),
            palette,
        );

        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, density))
            .child(pane_header(
                Icon::Activity,
                tr!("settings_diagnostics_section_title"),
                palette,
            ))
            .child(metrics)
            .child(logs_card)
            .into_any_element()
    }

    fn notifications_pane(&self, palette: &ForgePalette, density: Density) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, density))
            .child(pane_header(
                Icon::InfoCircle,
                tr!("settings_notifications_section_title"),
                palette,
            ))
            .child(card(
                div()
                    .font_family(body_family())
                    .text_size(FONT_SM)
                    .text_color(palette.text_muted)
                    .child(tr!("settings_notifications_hint")),
                palette,
            ))
            .into_any_element()
    }

    fn queues_pane(&self, palette: &ForgePalette, density: Density) -> AnyElement {
        let workers = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(0);

        let body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(info_row(
                tr!("settings_queues_workers_label"),
                workers.to_string(),
                palette,
            ))
            .child(field_hint(tr!("settings_queues_managed_hint"), palette));

        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, density))
            .child(pane_header(
                Icon::Notebook,
                tr!("settings_queues_section_title"),
                palette,
            ))
            .child(card(body, palette))
            .into_any_element()
    }
}

impl Render for SettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let status = self.render_status(&palette);
        let nav = self.render_nav(&palette, density, cx);
        let pane = self.render_pane(&palette, density, cx);

        let body = div()
            .w_full()
            .flex_1()
            .flex()
            .flex_row()
            .overflow_hidden()
            .child(nav)
            .child(pane);

        let font_overlay = self.font_picker.as_ref().map(|pending| {
            let view = cx.entity();
            overlay(pending.picker.clone(), &palette)
                .position(OverlayPosition::Center)
                .on_dismiss("settings-font-picker-scrim", move |_window, cx| {
                    view.update(cx, |this, cx| this.close_font_picker(cx));
                })
                .into_any_element()
        });

        let frame = page_frame(
            vec![
                BreadcrumbCrumb::leaf(tr!("settings_page_title")),
                BreadcrumbCrumb::leaf(self.section.label()),
            ],
            &palette,
        )
        .header_right(status)
        .body(body);

        div()
            .size_full()
            .flex()
            .flex_col()
            .child(frame)
            .children(font_overlay)
    }
}

fn pane_header(
    glyph: Icon,
    title: impl Into<SharedString>,
    palette: &ForgePalette,
) -> impl IntoElement {
    let title: SharedString = title.into();
    div()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xs, Density::Cozy))
        .child(icon(glyph, px(18.0), palette.brand))
        .child(
            div()
                .font_family(body_family())
                .font_weight(FontWeight::MEDIUM)
                .text_size(FONT_LG)
                .text_color(palette.text_primary)
                .child(title),
        )
}

fn info_row(
    label: impl Into<SharedString>,
    value: impl Into<SharedString>,
    palette: &ForgePalette,
) -> impl IntoElement {
    let label: SharedString = label.into();
    let value: SharedString = value.into();
    div()
        .flex()
        .items_center()
        .justify_between()
        .py(spacing(Spacing::Xs, Density::Cozy))
        .child(
            div()
                .font_family(body_family())
                .text_size(FONT_SM)
                .text_color(palette.text_primary)
                .child(label),
        )
        .child(
            div()
                .font_family(mono_family())
                .text_size(FONT_SM)
                .text_color(palette.text_muted)
                .child(value),
        )
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

fn release_row(
    tag: &'static str,
    summary: &'static str,
    when: &'static str,
    palette: &ForgePalette,
    density: Density,
) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Sm, density))
        .child(
            div()
                .w(px(60.0))
                .flex_shrink_0()
                .font_family(mono_family())
                .text_size(FONT_XS)
                .text_color(palette.text_primary)
                .child(tag),
        )
        .child(
            div()
                .flex_1()
                .font_family(body_family())
                .text_size(FONT_XS)
                .text_color(palette.text_muted)
                .child(summary),
        )
        .child(
            div()
                .font_family(mono_family())
                .text_size(FONT_XXS)
                .text_color(palette.text_faint)
                .child(when),
        )
}
