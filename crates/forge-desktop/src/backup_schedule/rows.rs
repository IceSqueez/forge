use super::*;

impl BackupScheduleView {
    pub(super) fn schedule_row(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> SettingRow {
        let line = StatusLine::of(&self.status);
        let settings = self.settings.loaded();
        let change = settings.and_then(|s| s.key.as_ref()).map(|_| {
            ghost_button(tr!("backup_schedule_change_btn"), palette).on_click(
                "backup-schedule-change-passphrase",
                cx.listener(|this, _: &ClickEvent, _, cx| this.change_passphrase(cx)),
            )
        });
        let enabled = settings.is_some_and(|s| s.enabled);
        let switch = toggle(enabled, palette);
        let switch = if settings.is_some() {
            div().child(switch.on_click(
                "backup-schedule-enabled",
                cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_enabled(cx)),
            ))
        } else {
            div().opacity(DISABLED_OPACITY).child(switch)
        };
        let controls = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Sm, density))
            .children(change)
            .child(switch);
        let row = setting_row(
            tr!("backup_schedule_label"),
            Some(line.text().into()),
            controls,
            palette,
            density,
        );
        if line.is_failure() {
            row.hint_color(palette.danger)
        } else {
            row
        }
    }

    pub(super) fn number_row(
        &self,
        number: ScheduleNumber,
        palette: &ForgePalette,
        density: Density,
    ) -> Option<SettingRow> {
        let field = self.numbers.iter().find(|f| f.number == number)?;
        let hint = if field.invalid {
            number.range_error()
        } else {
            number.hint()
        };
        let mut control = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(
                div()
                    .w(NUMBER_INPUT_W)
                    .min_w(NUMBER_INPUT_W)
                    .max_w(NUMBER_INPUT_W)
                    .child(field.input.clone()),
            )
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_muted)
                    .child(number.unit(field.value)),
            );
        if !self.settings.is_loaded() {
            control = control.opacity(DISABLED_OPACITY);
        }
        let row = setting_row(
            number.label(),
            Some(SharedString::from(hint)),
            control,
            palette,
            density,
        );
        Some(if field.invalid {
            row.hint_color(palette.danger)
        } else {
            row
        })
    }

    pub(super) fn category_item(
        &self,
        category: BackupCategory,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let fixed = include_setting(category).is_none();
        let settings = self.settings.loaded();
        let checked = fixed || settings.is_some_and(|s| s.included.contains(&category));
        let label_color = if fixed {
            palette.text_muted
        } else {
            palette.text_primary
        };
        let mut item = div()
            .id(SharedString::from(format!(
                "backup-schedule-category-{category:?}"
            )))
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .py(spacing(Spacing::Xxs, density))
            .px(spacing(Spacing::Xs, density))
            .rounded(radius(Radius::Sm))
            .child(checkbox(checked, palette).disabled(fixed))
            .child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(label_color)
                    .child(category_label(category)),
            );
        if fixed {
            item = item.child(
                div()
                    .font_family(body_family())
                    .text_size(FONT_XS)
                    .text_color(palette.text_faint)
                    .child(tr!("backup_schedule_category_database_reason")),
            );
        } else if settings.is_none() {
            item = item.opacity(DISABLED_OPACITY);
        } else {
            let hover_bg = palette.base;
            item = item
                .cursor_pointer()
                .hover(move |style| style.bg(hover_bg))
                .key_activated(palette.text_primary)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.toggle_category(category, cx)
                }));
        }
        item
    }

    pub(super) fn contents_row(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> SettingRow {
        let mut items = div()
            .w_full()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(spacing(Spacing::Xs, density));
        for category in BackupCategory::ALL {
            items = items.child(self.category_item(category, palette, density, cx));
        }
        setting_row(
            tr!("backup_schedule_contents_label"),
            Some(tr!("backup_schedule_contents_hint").into()),
            items,
            palette,
            density,
        )
        .stacked()
    }

    pub(super) fn folder_row(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> SettingRow {
        let settings = self.settings.loaded();
        let destination = settings.and_then(|s| s.destination.clone());
        let has_destination = destination.is_some();
        let shown = match settings {
            Some(_) => destination
                .unwrap_or_else(|| backups_dir(&self.data_dir).to_string_lossy().into_owned()),
            None => UNKNOWN_VALUE.to_owned(),
        };
        let path_box = div()
            .flex_1()
            .min_w(px(0.0))
            .overflow_hidden()
            .text_ellipsis()
            .py(spacing(Spacing::Xs, density))
            .px(spacing(Spacing::Sm, density))
            .rounded(radius(Radius::Md))
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .bg(palette.shell)
            .font_family(mono_family())
            .text_size(FONT_XS)
            .text_color(palette.text_primary)
            .child(shown);
        let choose = ghost_button_with_icon(
            Icon::FolderOpen,
            tr!("backup_schedule_folder_choose"),
            palette,
        )
        .disabled(settings.is_none())
        .on_click(
            "backup-schedule-folder-choose",
            cx.listener(|this, _: &ClickEvent, _, cx| this.choose_folder(cx)),
        );
        let reset = has_destination.then(|| {
            ghost_button(tr!("backup_schedule_folder_reset"), palette).on_click(
                "backup-schedule-folder-reset",
                cx.listener(|this, _: &ClickEvent, _, cx| this.reset_folder(cx)),
            )
        });
        let control = div()
            .w_full()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(path_box)
            .child(choose)
            .children(reset);
        setting_row(
            tr!("backup_schedule_folder_label"),
            Some(tr!("backup_schedule_folder_hint").into()),
            control,
            palette,
            density,
        )
        .stacked()
    }

    pub(super) fn storage_used_row(&self, palette: &ForgePalette, density: Density) -> SettingRow {
        let mut items = div()
            .w_full()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(spacing(Spacing::Sm, density));
        match &self.status.storage_used {
            Some(used) => {
                for (part, bytes) in storage_parts(used) {
                    items = items.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(spacing(Spacing::Xs, density))
                            .child(
                                div()
                                    .font_family(body_family())
                                    .text_size(FONT_XS)
                                    .text_color(palette.text_muted)
                                    .child(part.label()),
                            )
                            .child(
                                div()
                                    .font_family(mono_family())
                                    .text_size(FONT_XS)
                                    .text_color(palette.text_primary)
                                    .child(fmt_bytes(bytes)),
                            ),
                    );
                }
            }
            None => {
                items = items.child(
                    div()
                        .font_family(mono_family())
                        .text_size(FONT_XS)
                        .text_color(palette.text_muted)
                        .child(tr!("backup_schedule_storage_used_unknown")),
                );
            }
        }
        setting_row(
            tr!("backup_schedule_storage_used_label"),
            Some(tr!("backup_schedule_storage_used_hint").into()),
            items,
            palette,
            density,
        )
        .stacked()
    }
}
