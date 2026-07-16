use std::sync::Arc;

use forge_components::{
    BORDER_THIN, BreadcrumbCrumb, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_LG,
    FONT_MD, FONT_SM, FONT_XS, FONT_XXS, ForgePalette, Icon, Radius, Spacing, ThemeId, badge,
    breadcrumb, card, ghost_button_with_icon, icon, metric_card, primary_button,
    primary_button_with_icon, radius, spacing, tr, with_alpha,
};
use forge_storage::{Language, SettingsRepo};
use gpui::{
    AnyElement, ClickEvent, Context, Entity, FontWeight, Rgba, SharedString, Window, div,
    prelude::*, px,
};

use crate::presentation::{ActiveLanguage, ActivePresentation, Presentation};
use crate::runtime_handles::RuntimeHandles;
use crate::settings_audio::SettingsAudioView;
use crate::settings_hotkeys::SettingsHotkeysView;
use crate::settings_scripting::SettingsScriptingView;
use crate::settings_shortcuts::SettingsShortcutsView;
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
            SettingsSection::Hotkeys,
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
    Hotkeys,
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
            SettingsSection::Hotkeys => tr!("settings_nav_hotkeys"),
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
            SettingsSection::Hotkeys => Icon::Bolt,
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
            SettingsSection::Hotkeys => "hotkeys",
            SettingsSection::Version => "version",
            SettingsSection::Diagnostics => "diagnostics",
        }
    }
}

fn theme_meta(theme: ThemeId) -> (String, String) {
    match theme {
        ThemeId::CatppuccinMocha => (
            tr!("settings_theme_default"),
            format!("Mocha · {}", tr!("settings_theme_desc_dark")),
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
        ThemeId::CatppuccinMocha => "mocha",
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
    hotkeys: Entity<SettingsHotkeysView>,
}

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
        let hotkeys = cx.new(|cx| {
            SettingsHotkeysView::new(
                Arc::clone(&handles.backend),
                handles.rt_handle.clone(),
                handles.hotkey_client.clone(),
                cx,
            )
        });
        Self {
            section: SettingsSection::Appearance,
            handles,
            language: cx.global::<ActiveLanguage>().0,
            audio,
            scripting,
            websocket,
            shortcuts,
            hotkeys,
        }
    }

    fn select_section(&mut self, section: SettingsSection, cx: &mut Context<Self>) {
        self.section = section;
        cx.notify();
    }

    fn select_theme(&mut self, theme: ThemeId, cx: &mut Context<Self>) {
        let density = cx.density();
        cx.set_global(Presentation::new(theme, density));
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
        cx.notify();
    }

    fn check_for_updates(&mut self, cx: &mut Context<Self>) {
        cx.open_url(RELEASES_URL);
    }

    fn open_log_dir(&mut self, cx: &mut Context<Self>) {
        let dir = forge_platform_core::paths::data_dir().join("logs");
        cx.reveal_path(&dir);
    }

    fn render_header(&self, palette: &ForgePalette) -> impl IntoElement + use<> {
        let saved = div()
            .flex()
            .items_center()
            .gap(px(5.0))
            .child(icon(Icon::CircleCheck, FONT_XS, palette.success))
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_XS)
                    .text_color(palette.success)
                    .child(tr!("settings_ws_all_saved")),
            );
        breadcrumb(
            vec![
                BreadcrumbCrumb::leaf(tr!("settings_page_title")),
                BreadcrumbCrumb::leaf(self.section.label()),
            ],
            palette,
        )
        .right(saved)
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
                    .font_family(DEFAULT_MONO_FAMILY)
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
            SettingsSection::Storage => self.storage_pane(palette, density, cx),
            SettingsSection::Hotkeys => self.hotkeys.clone().into_any_element(),
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
                        .child(font_field(
                            tr!("settings_appearance_font_interface"),
                            DEFAULT_BODY_FAMILY,
                            palette,
                        ))
                        .child(font_field(
                            tr!("settings_appearance_font_monospace"),
                            DEFAULT_MONO_FAMILY,
                            palette,
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
                .font_family(DEFAULT_BODY_FAMILY)
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
                    .font_family(DEFAULT_BODY_FAMILY)
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
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_primary)
                    .child(label),
            )
            .child(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
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
            .font_family(DEFAULT_BODY_FAMILY)
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
            .font_family(DEFAULT_MONO_FAMILY)
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
                    .font_family(DEFAULT_BODY_FAMILY)
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
                    .font_family(DEFAULT_BODY_FAMILY)
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
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_MD)
                    .text_color(palette.text_primary)
                    .child("Forge"),
            )
            .child(
                div()
                    .font_family(DEFAULT_MONO_FAMILY)
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
                        .font_family(DEFAULT_BODY_FAMILY)
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
                    .font_family(DEFAULT_MONO_FAMILY)
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
            .font_family(DEFAULT_MONO_FAMILY)
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
                    .font_family(DEFAULT_BODY_FAMILY)
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

    fn storage_pane(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let db_path = forge_platform_core::paths::data_dir().join("forge.db");
        let backup_btn = primary_button(tr!("settings_storage_backup_btn"), palette).on_click(
            "settings-db-backup",
            cx.listener(|this, _: &ClickEvent, _, _| this.backup_db()),
        );

        let body = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(info_row(
                tr!("settings_storage_db_path_label"),
                db_path.display().to_string(),
                palette,
            ))
            .child(backup_btn)
            .child(field_hint(tr!("settings_storage_backup_hint"), palette));

        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, density))
            .child(pane_header(
                Icon::Folder,
                tr!("settings_storage_section_title"),
                palette,
            ))
            .child(card(body, palette))
            .into_any_element()
    }

    fn backup_db(&self) {
        let backend = Arc::clone(&self.handles.backend);
        self.handles.rt_handle.spawn(async move {
            let stamp = time::OffsetDateTime::now_utc().unix_timestamp();
            let path =
                forge_platform_core::paths::data_dir().join(format!("forge-backup-{stamp}.db"));
            match backend.export(&path).await {
                Ok(()) => tracing::info!(path = %path.display(), "DB backup created"),
                Err(e) => tracing::warn!(error = %e, "DB backup failed"),
            }
        });
    }
}

impl Render for SettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let header = self.render_header(&palette);
        let nav = self.render_nav(&palette, density, cx);
        let pane = self.render_pane(&palette, density, cx);

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(header)
            .child(
                div()
                    .w_full()
                    .flex_1()
                    .flex()
                    .flex_row()
                    .overflow_hidden()
                    .child(nav)
                    .child(pane),
            )
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
                .font_family(DEFAULT_BODY_FAMILY)
                .font_weight(FontWeight::MEDIUM)
                .text_size(FONT_LG)
                .text_color(palette.text_primary)
                .child(title),
        )
}

fn field_title(text: impl Into<SharedString>, palette: &ForgePalette) -> impl IntoElement {
    let text: SharedString = text.into();
    div()
        .font_family(DEFAULT_BODY_FAMILY)
        .font_weight(FontWeight::MEDIUM)
        .text_size(FONT_SM)
        .text_color(palette.text_primary)
        .child(text)
}

fn field_hint(text: impl Into<SharedString>, palette: &ForgePalette) -> impl IntoElement {
    let text: SharedString = text.into();
    div()
        .font_family(DEFAULT_BODY_FAMILY)
        .text_size(FONT_XS)
        .text_color(palette.text_muted)
        .child(text)
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
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_SM)
                .text_color(palette.text_primary)
                .child(label),
        )
        .child(
            div()
                .font_family(DEFAULT_MONO_FAMILY)
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

fn font_field(
    label: impl Into<SharedString>,
    family: &'static str,
    palette: &ForgePalette,
) -> impl IntoElement {
    let label: SharedString = label.into();
    div()
        .flex_1()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .child(
            div()
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(FONT_XXS)
                .text_color(palette.text_faint)
                .child(label),
        )
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .px(spacing(Spacing::Sm, Density::Cozy))
                .py(px(7.0))
                .rounded(radius(Radius::Sm))
                .bg(palette.base)
                .border(BORDER_THIN)
                .border_color(palette.border_input)
                .child(
                    div()
                        .font_family(DEFAULT_BODY_FAMILY)
                        .text_size(FONT_SM)
                        .text_color(palette.text_primary)
                        .child(family),
                )
                .child(icon(Icon::ChevronDown, FONT_XS, palette.text_faint)),
        )
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
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(FONT_XS)
                .text_color(palette.text_primary)
                .child(tag),
        )
        .child(
            div()
                .flex_1()
                .font_family(DEFAULT_BODY_FAMILY)
                .text_size(FONT_XS)
                .text_color(palette.text_muted)
                .child(summary),
        )
        .child(
            div()
                .font_family(DEFAULT_MONO_FAMILY)
                .text_size(FONT_XXS)
                .text_color(palette.text_faint)
                .child(when),
        )
}
