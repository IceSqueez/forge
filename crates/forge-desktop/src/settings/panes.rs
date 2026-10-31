use super::*;

impl SettingsView {
    pub(super) fn render_nav(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let density = cx.density();
        let mut list = div().flex().flex_col().gap(spacing(Spacing::Sm, density));
        for (group, sections) in NAV_GROUPS {
            let mut block = div().flex().flex_col().child(
                div()
                    .px(spacing(Spacing::Sm, density))
                    .py(spacing(Spacing::Xs, density))
                    .mb(spacing(Spacing::Xxs, density))
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
            .overflow_y_revealing_focus()
            .bg(palette.shell)
            .border_r(BORDER_THIN)
            .border_color(palette.border_regular)
            .py(scaled(NAV_PAD_V, density))
            .px(scaled(NAV_PAD_H, density))
            .child(list)
    }

    pub(super) fn nav_row(
        &self,
        section: SettingsSection,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let density = cx.density();
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
            .gap(spacing(Spacing::Sm, density))
            .px(spacing(Spacing::Sm, density))
            .py(scaled(NAV_ROW_PAD_V, density))
            .mb(NAV_ROW_MB)
            .rounded(radius(Radius::Sm))
            .cursor_pointer()
            .font_family(body_family())
            .text_size(NAV_ROW_FONT)
            .text_color(text_color)
            .child(icon(section.icon(), NAV_ROW_ICON, icon_color))
            .child(section.label())
            .key_activated(palette.text_primary)
            .on_click(
                cx.listener(move |this, _: &ClickEvent, _, cx| this.select_section(section, cx)),
            );
        if active {
            row.bg(palette.surface_overlay)
        } else {
            row.hover(move |style| style.bg(hover_bg))
        }
    }

    pub(super) fn render_pane(
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
            SettingsSection::Notifications => self.notifications.clone().into_any_element(),
            SettingsSection::Audio => self.audio.clone().into_any_element(),
            SettingsSection::Scripting => self.scripting.clone().into_any_element(),
            SettingsSection::WebSocket => self.websocket.clone().into_any_element(),
            SettingsSection::Storage => self.storage.clone().into_any_element(),
            SettingsSection::About => self.about_pane(palette, density, cx),
            SettingsSection::Diagnostics => self.diagnostics.clone().into_any_element(),
        };

        div()
            .id("settings-pane")
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .overflow_y_revealing_focus()
            .bg(palette.base)
            .py(scaled(PANE_PAD_V, density))
            .px(spacing(Spacing::Lg, density))
            .child(content)
    }

    pub(super) fn appearance_pane(
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
        let theme_block = setting_block(
            tr!("settings_appearance_theme_label"),
            Some(tr!("settings_appearance_theme_hint").into()),
            theme_grid,
            palette,
            density,
        );

        let accent_row = setting_row(
            tr!("settings_appearance_accent_label"),
            Some(tr!("settings_appearance_accent_hint").into()),
            self.accent_swatches(palette, cx),
            palette,
            density,
        );

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
        let density_block = setting_block(
            tr!("settings_appearance_density_label"),
            Some(tr!("settings_appearance_density_subtitle").into()),
            density_rows,
            palette,
            density,
        );

        let fonts_block = setting_block(
            tr!("settings_appearance_fonts_label"),
            None,
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
            palette,
            density,
        );

        div()
            .flex()
            .flex_col()
            .child(SettingsPaneHeader::new(SettingsSection::Appearance))
            .child(setting_sections(
                [
                    theme_block.into_any_element(),
                    accent_row.into_any_element(),
                    density_block.into_any_element(),
                    fonts_block.into_any_element(),
                ],
                palette,
                density,
            ))
            .into_any_element()
    }

    pub(super) fn accent_swatches(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let density = cx.density();
        let theme = cx.theme();
        let selected = cx.accent();
        let mut row = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(spacing(Spacing::Xs, density));
        for accent in AccentColor::ALL {
            let active = accent == selected;
            let size = if active {
                ACCENT_SWATCH_SELECTED
            } else {
                ACCENT_SWATCH
            };
            let mut swatch = div()
                .id(SharedString::from(format!(
                    "settings-accent-{}",
                    accent.storage_key()
                )))
                .flex_none()
                .size(size)
                .rounded_full()
                .bg(theme.accent(accent))
                .cursor_pointer()
                .key_activated(palette.text_primary)
                .on_click(
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.select_accent(accent, cx)),
                );
            if active {
                swatch = swatch
                    .border(ACCENT_SWATCH_RING)
                    .border_color(palette.text_primary);
            }
            row = row.child(swatch);
        }
        row
    }

    pub(super) fn theme_card(
        &self,
        theme: ThemeId,
        active: bool,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let density = cx.density();
        let pal = theme.accented_palette(cx.accent());
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
                theme.storage_key()
            )))
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density))
            .p(scaled(px(12.0), density))
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(border_color)
            .bg(palette.elevated)
            .cursor_pointer()
            .key_activated(pal.text_primary)
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

    pub(super) fn density_row(
        &self,
        option: Density,
        active: bool,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let (label, hint) = density_meta(option);
        choice_row(
            SharedString::from(format!("settings-density-{}", option.storage_key())),
            label.into(),
            field_hint(hint, palette).into_any_element(),
            active,
            palette,
            cx.density(),
        )
        .key_activated(palette.text_primary)
        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.select_density(option, cx)))
    }

    pub(super) fn language_pane(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let list = div()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xxs, density))
            .child(self.language_option_row("English", "en-US", Language::En, palette, cx))
            .child(self.language_option_row("Українська", "uk-UA", Language::Uk, palette, cx));

        div()
            .flex()
            .flex_col()
            .child(
                SettingsPaneHeader::new(SettingsSection::Language)
                    .subtitle(tr!("settings_language_subtitle")),
            )
            .child(setting_groups(
                [
                    list.into_any_element(),
                    self.region.clone().into_any_element(),
                ],
                density,
            ))
            .into_any_element()
    }

    pub(super) fn language_option_row(
        &self,
        native_label: &'static str,
        bcp47: &'static str,
        lang: Language,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let density = cx.density();
        choice_row(
            SharedString::from(format!("settings-language-{lang}")),
            native_label.into(),
            field_hint(bcp47, palette).into_any_element(),
            self.language == lang,
            palette,
            density,
        )
        .key_activated(palette.text_primary)
        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.select_language(lang, cx)))
    }

    pub(super) fn about_pane(
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
                    .text_size(FONT_XXL)
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
                div()
                    .flex()
                    .flex_col()
                    .gap(spacing(Spacing::Xxs, density))
                    .child(name_line)
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(FONT_XS)
                            .text_color(palette.text_muted)
                            .child(tr!("settings_version_license")),
                    )
                    .children(status_line(&self.update_check, palette)),
            )
            .child(div().flex_1())
            .children(self.update_check.offers_release_page().then(|| {
                ghost_button_with_icon(
                    Icon::ExternalLink,
                    tr!("settings_version_open_releases"),
                    palette,
                )
                .on_click(
                    "settings-open-releases",
                    cx.listener(|this, _: &ClickEvent, _, cx| this.open_releases_page(cx)),
                )
            }))
            .child(
                ghost_button_with_icon(
                    Icon::Refresh,
                    tr!("settings_version_check_updates"),
                    palette,
                )
                .busy(self.update_check.is_checking())
                .on_click(
                    "settings-check-updates",
                    cx.listener(|this, _: &ClickEvent, _, cx| this.check_for_updates(cx)),
                ),
            );

        div()
            .flex()
            .flex_col()
            .child(SettingsPaneHeader::new(SettingsSection::About))
            .child(setting_groups(
                [
                    card(identity, palette).into_any_element(),
                    self.about.clone().into_any_element(),
                ],
                density,
            ))
            .into_any_element()
    }

    pub(super) fn general_pane(
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
            .child(setting_rows(palette).row(stay_awake_row).in_card(palette))
            .into_any_element()
    }
}
