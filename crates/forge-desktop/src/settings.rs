use forge_components::{
    BORDER_THIN, BreadcrumbCrumb, DEFAULT_BODY_FAMILY, DEFAULT_MONO_FAMILY, Density, FONT_LG,
    FONT_MD, FONT_SM, FONT_XS, FONT_XXS, ForgePalette, Icon, Radius, Spacing, ThemeId, badge,
    breadcrumb, card, ghost_button_with_icon, icon, metric_card, primary_button,
    primary_button_with_icon, radius, spacing, with_alpha,
};
use gpui::{
    AnyElement, ClickEvent, Context, FontWeight, Rgba, SharedString, Window, div, prelude::*, px,
};

use crate::presentation::{ActivePresentation, Presentation};

/// GitHub releases page opened by the Version pane's "Check for updates" affordance.
const RELEASES_URL: &str = concat!(env!("CARGO_PKG_REPOSITORY"), "/releases");

/// Toolchain the binary is built with, surfaced on the Diagnostics metric row.
/// A fixed string (no compile-time rustc probe) kept in step with
/// `rust-toolchain.toml`.
const RUST_VERSION: &str = "1.96.0";

/// Static recent-releases stub for the Version pane. A representative sample so
/// the card renders its real frame; the live list arrives once a release-notes
/// source reaches this screen (noted in UI_NOTES).
const RECENT_RELEASES: [(&str, &str, &str); 3] = [
    ("v0.9.2", "Server panel, settings → websocket", "today"),
    ("v0.9.1", "OBS scene metadata, lock indicators", "3d ago"),
    ("v0.9.0", "TTS module GA, filters live preview", "1w ago"),
];

/// The left-nav grouping of the twelve settings sections, mirroring the design's
/// PREFERENCES / ENGINE / ABOUT columns.
const NAV_GROUPS: [(&str, &[SettingsSection]); 3] = [
    (
        "PREFERENCES",
        &[
            SettingsSection::Appearance,
            SettingsSection::Language,
            SettingsSection::Shortcuts,
            SettingsSection::Notifications,
        ],
    ),
    (
        "ENGINE",
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
        "ABOUT",
        &[SettingsSection::Version, SettingsSection::Diagnostics],
    ),
];

/// One destination in the Settings screen's left section-nav. The active section
/// lives as a field on [`SettingsView`] rather than in the top-level router, so
/// navigating to Settings always opens at [`SettingsSection::Appearance`] and the
/// nav swaps the pane in place — matching the design's internal section state.
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
    /// Human-readable nav + breadcrumb label.
    fn label(self) -> &'static str {
        match self {
            SettingsSection::Appearance => "Appearance",
            SettingsSection::Language => "Language & region",
            SettingsSection::Shortcuts => "Shortcuts",
            SettingsSection::Notifications => "Notifications",
            SettingsSection::Audio => "Audio",
            SettingsSection::Scripting => "Scripting (Rhai)",
            SettingsSection::Queues => "Queues & threading",
            SettingsSection::Storage => "Storage & backups",
            SettingsSection::WebSocket => "WebSocket server",
            SettingsSection::Hotkeys => "Hotkeys",
            SettingsSection::Version => "Version & updates",
            SettingsSection::Diagnostics => "Logs & diagnostics",
        }
    }

    /// Nav glyph for the section.
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

    /// Stable element-id fragment for the nav button.
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

/// Title + subtitle shown on a theme card. The first theme is presented as the
/// product default; the others carry their upstream names.
fn theme_meta(theme: ThemeId) -> (&'static str, &'static str) {
    match theme {
        ThemeId::CatppuccinMocha => ("Default", "Mocha · dark"),
        ThemeId::TokyoNight => ("Tokyo Night", "Storm"),
        ThemeId::Latte => ("Catppuccin Latte", "Light mode"),
    }
}

/// Label + one-line hint for a density option row.
fn density_meta(density: Density) -> (&'static str, &'static str) {
    match density {
        Density::Compact => ("Compact", "Tighter spacing, more on screen"),
        Density::Cozy => ("Cozy", "Balanced spacing (default)"),
        Density::Spacious => ("Spacious", "Roomier spacing across panels"),
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

/// The Settings screen view-entity: a breadcrumb header, a left section-nav over
/// three groups, and the active section's pane. Carries only the active section;
/// the theme / density it edits live in the presentation `Global` (the single
/// source of truth), so a switch re-themes the whole app and survives navigation.
pub struct SettingsView {
    section: SettingsSection,
}

impl SettingsView {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self {
            section: SettingsSection::Appearance,
        }
    }

    // --- handlers ---------------------------------------------------------

    fn select_section(&mut self, section: SettingsSection, cx: &mut Context<Self>) {
        self.section = section;
        cx.notify();
    }

    /// Swaps the active theme by replacing the presentation `Global`, preserving
    /// the current density. Replacing the global notifies its observers (the root
    /// shell), which repaints the whole view tree in the new palette.
    fn select_theme(&mut self, theme: ThemeId, cx: &mut Context<Self>) {
        let density = cx.density();
        cx.set_global(Presentation::new(theme, density));
        cx.notify();
    }

    /// Swaps the density likewise, preserving the current theme.
    fn select_density(&mut self, density: Density, cx: &mut Context<Self>) {
        let theme = cx.theme();
        cx.set_global(Presentation::new(theme, density));
        cx.notify();
    }

    /// Opens the releases page in the user's browser. A pure side effect through
    /// the platform shell — no view state changes, so no repaint.
    fn check_for_updates(&mut self, cx: &mut Context<Self>) {
        cx.open_url(RELEASES_URL);
    }

    /// Reveals the log directory in the OS file manager. Pure side effect; no
    /// repaint.
    fn open_log_dir(&mut self, cx: &mut Context<Self>) {
        let dir = forge_platform_core::paths::data_dir().join("logs");
        cx.reveal_path(&dir);
    }

    // --- header + nav -----------------------------------------------------

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
                    .child("All changes saved"),
            );
        breadcrumb(
            vec![
                BreadcrumbCrumb::leaf("Settings"),
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
                    .child(group),
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

    // --- pane routing -----------------------------------------------------

    fn render_pane(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let content = match self.section {
            SettingsSection::Appearance => self.appearance_pane(palette, density, cx),
            SettingsSection::Version => self.version_pane(palette, density, cx),
            SettingsSection::Diagnostics => self.diagnostics_pane(palette, density, cx),
            other => self.deferred_pane(other, palette, density),
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

    // --- pane: Appearance -------------------------------------------------

    fn appearance_pane(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let header = pane_header(Icon::LayoutGrid, "Appearance", palette);

        // Theme block.
        let mut theme_grid = div().flex().flex_row().gap(spacing(Spacing::Sm, density));
        let active = cx.theme();
        for theme in ThemeId::ALL {
            theme_grid = theme_grid.child(self.theme_card(theme, active == theme, palette, cx));
        }
        let theme_block = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .child(field_title("Theme", palette))
            .child(field_hint("How Forge should look", palette))
            .child(theme_grid);

        // Density block.
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
                .child(field_title("UI density", palette))
                .child(field_hint("Adjust spacing across panels", palette))
                .child(density_rows),
        );

        // Fonts block (display-only selects — a full font scan is deferred).
        let fonts_block = section_divider(palette, density).child(
            div()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Sm, density))
                .child(field_title("Fonts", palette))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap(spacing(Spacing::Sm, density))
                        .child(font_field("INTERFACE", DEFAULT_BODY_FAMILY, palette))
                        .child(font_field("MONOSPACE", DEFAULT_MONO_FAMILY, palette)),
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

        // A themed mini-app preview: a tinted sidebar of bars beside a content
        // area, all drawn from the previewed theme's own palette.
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
                "ACTIVE",
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

    // --- pane: Version ----------------------------------------------------

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
                        .child("Open-source · MIT OR Apache-2.0"),
                ),
            )
            .child(div().flex_1())
            .child(
                ghost_button_with_icon(Icon::Refresh, "Check for updates", palette).on_click(
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
                    .child("RECENT RELEASES"),
            );
        for (tag, summary, when) in RECENT_RELEASES {
            releases = releases.child(release_row(tag, summary, when, palette, density));
        }

        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, density))
            .child(pane_header(Icon::InfoCircle, "Version & updates", palette))
            .child(card(identity, palette))
            .child(card(releases, palette))
            .into_any_element()
    }

    // --- pane: Diagnostics ------------------------------------------------

    fn diagnostics_pane(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let version = env!("CARGO_PKG_VERSION");
        let log_dir = forge_platform_core::paths::data_dir().join("logs");
        let log_display = log_dir.display().to_string();

        let metric = |label: &'static str, value: String| {
            div()
                .flex_1()
                .child(metric_card(label, value, None::<&str>, None, palette))
        };
        let metrics = div()
            .flex()
            .flex_row()
            .gap(spacing(Spacing::Sm, density))
            .child(metric("Build", version.to_owned()))
            .child(metric("Rust", RUST_VERSION.to_owned()))
            .child(metric("OS", std::env::consts::OS.to_owned()));

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
        let open_btn = primary_button("Open log directory", palette).on_click(
            "settings-open-logs",
            cx.listener(|this, _: &ClickEvent, _, cx| this.open_log_dir(cx)),
        );
        let logs_card = card(
            div()
                .flex()
                .flex_col()
                .gap(spacing(Spacing::Xs, density))
                .child(field_title("Log directory", palette))
                .child(path_box)
                .child(open_btn)
                .child(field_hint("Runtime logs stream to this folder.", palette)),
            palette,
        );

        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, density))
            .child(pane_header(Icon::Activity, "Logs & diagnostics", palette))
            .child(metrics)
            .child(logs_card)
            .into_any_element()
    }

    // --- pane: deferred sections -----------------------------------------

    fn deferred_pane(
        &self,
        section: SettingsSection,
        palette: &ForgePalette,
        density: Density,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Md, density))
            .child(pane_header(section.icon(), section.label(), palette))
            .child(card(
                div()
                    .font_family(DEFAULT_BODY_FAMILY)
                    .text_size(FONT_SM)
                    .text_color(palette.text_muted)
                    .child("This section arrives in a later slice."),
                palette,
            ))
            .into_any_element()
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

// --- shared pane primitives ----------------------------------------------

/// A pane title: the section glyph over an `FONT_LG` heading.
fn pane_header(glyph: Icon, title: &'static str, palette: &ForgePalette) -> impl IntoElement {
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

/// A form-block heading (`FONT_SM`, primary ink, medium weight).
fn field_title(text: &'static str, palette: &ForgePalette) -> impl IntoElement {
    div()
        .font_family(DEFAULT_BODY_FAMILY)
        .font_weight(FontWeight::MEDIUM)
        .text_size(FONT_SM)
        .text_color(palette.text_primary)
        .child(text)
}

/// A muted one-line hint below a field title.
fn field_hint(text: &'static str, palette: &ForgePalette) -> impl IntoElement {
    div()
        .font_family(DEFAULT_BODY_FAMILY)
        .text_size(FONT_XS)
        .text_color(palette.text_muted)
        .child(text)
}

/// A column wrapper that opens with a hairline top divider + top padding, the way
/// the design separates Appearance's stacked blocks.
fn section_divider(palette: &ForgePalette, density: Density) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .gap(spacing(Spacing::Sm, density))
        .pt(spacing(Spacing::Md, density))
        .border_t(BORDER_THIN)
        .border_color(palette.border_regular)
}

/// A display-only font picker: an uppercase mono field label over a bordered box
/// naming the active family. The real family selector lands with the font-scan
/// capability (noted in UI_NOTES).
fn font_field(
    label: &'static str,
    family: &'static str,
    palette: &ForgePalette,
) -> impl IntoElement {
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

/// One release line: a fixed-width mono version tag, the summary, and a right
/// aligned relative date.
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
