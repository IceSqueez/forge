use super::*;
use forge_components::Column;

impl VoiceAliasesView {
    fn strategy_banner(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> AnyElement {
        let density = cx.density();
        let current = self.strategy.loaded().copied();
        let strategy_segments = self.strategy_segments(current, palette, cx);
        let heading = self.strategy_heading(current, palette, cx);

        let row = div()
            .w_full()
            .flex()
            .items_center()
            .gap(scaled(BANNER_GAP, density))
            .child(icon(Icon::Wand, BANNER_ICON, palette.brand))
            .child(heading)
            .child(strategy_segments);
        let notice = self.strategy_notice(palette, density, cx);
        let body = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(scaled(BANNER_GAP, density))
            .child(row)
            .children(notice);

        div()
            .w_full()
            .px(scaled(PAGE_PAD_H, density))
            .py(scaled(PAGE_GAP_V, density))
            .child(
                card(body, palette)
                    .padding_xy(scaled(BANNER_PAD_V, density), scaled(BANNER_PAD_H, density))
                    .full_width(),
            )
            .into_any_element()
    }

    fn strategy_segments(
        &self,
        current: Option<StrategyChoice>,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let segments = StrategyChoice::ALL
            .into_iter()
            .map(|choice| {
                segment(
                    SharedString::from(format!("va-strat-{}", choice.key())),
                    choice.label(),
                    current == Some(choice),
                    cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.choose_strategy(choice, window, cx)
                    }),
                )
                .disabled(current.is_none())
            })
            .collect();
        let voice_popover = self.voice_picker.as_ref().map(|pending| {
            let view = cx.entity();
            dropdown(pending.picker.clone()).on_dismiss(move |_window, cx| {
                view.update(cx, |this, cx| this.close_voice_picker(cx));
            })
        });
        div()
            .relative()
            .child(segmented(segments, palette))
            .children(voice_popover)
            .into_any_element()
    }

    fn strategy_heading(
        &self,
        current: Option<StrategyChoice>,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let sublabel = match (current, &self.single_voice) {
            (Some(StrategyChoice::SingleVoice), Some(voice)) => tr!(
                "tts_aliases_strategy_single_current",
                voice = self.voice_label(voice)
            ),
            _ => tr!("tts_aliases_strategy_sublabel"),
        };

        div()
            .flex_1()
            .flex()
            .flex_col()
            .child(
                div()
                    .font_family(cx.body_family())
                    .font_weight(FontWeight::MEDIUM)
                    .text_size(BANNER_TITLE_FS)
                    .mb(BANNER_TITLE_MB)
                    .text_color(palette.text_primary)
                    .child(tr!("tts_aliases_strategy_label")),
            )
            .child(
                div()
                    .font_family(cx.body_family())
                    .text_size(META_FS)
                    .text_color(palette.text_muted)
                    .child(sublabel),
            )
            .into_any_element()
    }

    fn strategy_notice(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if let Some(reason) = self.strategy.failure() {
            Some(
                error_row_with_retry(
                    "va-strategy-retry",
                    tr!("tts_aliases_strategy_load_failed", reason = reason),
                    palette,
                    cx.listener(|this, _: &ClickEvent, _, cx| this.retry_strategy(cx)),
                    density,
                    cx,
                )
                .into_any_element(),
            )
        } else {
            self.strategy_error.as_ref().map(|reason| {
                error_row(
                    tr!("tts_aliases_strategy_save_failed", reason = reason.as_str()),
                    palette,
                    density,
                    cx,
                )
                .into_any_element()
            })
        }
    }

    fn toolbar(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let count = div()
            .font_family(cx.body_family())
            .text_size(META_FS)
            .text_color(palette.text_muted)
            .child(tr!(
                "tts_aliases_count",
                count = i64::try_from(self.total_count).unwrap_or(i64::MAX)
            ));

        let assign = primary_button_with_icon(Icon::Plus, tr!("tts_aliases_assign_btn"), palette)
            .on_click(
                "va-assign",
                cx.listener(|this, _: &ClickEvent, window, cx| this.open_assign(window, cx)),
            );

        let right = div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xs, density))
            .child(count)
            .child(assign);

        toolbar_row(
            div()
                .w(SEARCH_FIELD_WIDTH)
                .child(self.search.field().clone()),
            right,
        )
        .density(density)
        .px(scaled(PAGE_PAD_H, density))
        .pb(scaled(PAGE_GAP_V, density))
        .into_any_element()
    }

    fn table(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let columns = alias_table_columns();

        let hover_bg = with_alpha(palette.border_regular, ROW_HOVER_ALPHA);
        let mut frame = div()
            .w_full()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .border(BORDER_THIN)
            .border_color(palette.border_regular)
            .rounded(TABLE_RADIUS)
            .overflow_hidden();

        let shown = self.visible.len();
        if self.visible.is_empty() {
            let state = self.empty_table_state(palette, density, cx);
            let header_only = data_table(palette, columns, Vec::new())
                .density(density)
                .header_padding(scaled(HEADER_PAD_V, density), scaled(HEADER_PAD_H, density))
                .trailing_rule(false);
            frame = frame
                .child(header_only)
                .child(div().flex_1().min_h(px(0.0)).child(state));
        } else {
            let pal = *palette;
            let body = virtual_table(
                "va-table-scroll",
                palette,
                columns,
                shown,
                &self.table_scroll,
                density,
            )
            .header_padding(scaled(HEADER_PAD_V, density), scaled(HEADER_PAD_H, density))
            .row_padding(scaled(ROW_PAD_V, density), scaled(ROW_PAD_H, density))
            .row_hover(hover_bg)
            .trailing_rule(false)
            .build(
                move |this, ix, _window, cx| {
                    let index = this.visible[ix];
                    match this.aliases.get(index) {
                        Some(row) => this.alias_row(row, &pal, density, cx),
                        None => DataRow::new(Vec::new()),
                    }
                },
                cx,
            );
            frame = frame.child(body);
        }

        let footer = self.table_footer(shown, palette, density, cx);

        div()
            .w_full()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .px(scaled(PAGE_PAD_H, density))
            .pb(spacing(Spacing::Md, density))
            .child(frame)
            .child(footer)
            .into_any_element()
    }

    fn empty_table_state(
        &self,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let notice = load_notice(
            &self.list_load,
            LoadNotice {
                id: "va-load-retry",
                loading: tr!("tts_aliases_loading"),
                width: NoticeWidth::Pane,
            },
            cx.listener(|this, _: &ClickEvent, _, cx| this.retry_load(cx)),
            palette,
            density,
            cx,
        )
        .filter(|_| self.aliases.is_empty());
        notice.unwrap_or_else(|| {
            empty_state(tr!("tts_aliases_empty"), palette)
                .density(density)
                .into_any_element()
        })
    }

    fn table_footer(
        &self,
        shown: usize,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let auto = self.viewer_count.saturating_sub(self.total_count);
        div()
            .w_full()
            .pt(scaled(FOOTER_PAD_TOP, density))
            .font_family(cx.mono_family())
            .text_size(FONT_XXS)
            .text_color(palette.text_faint)
            .child(tr!(
                "tts_aliases_footer_caption",
                shown = i64::try_from(shown).unwrap_or(i64::MAX),
                total = i64::try_from(self.total_count).unwrap_or(i64::MAX),
                auto = i64::try_from(auto).unwrap_or(i64::MAX)
            ))
            .into_any_element()
    }

    fn alias_row(
        &self,
        row: &AliasRow,
        palette: &ForgePalette,
        density: Density,
        cx: &mut Context<Self>,
    ) -> DataRow {
        let muted = row.blocked;
        let (pitch_color, speed_color) = if muted {
            (palette.text_extreme_faint, palette.text_extreme_faint)
        } else {
            (palette.text_muted, palette.text_muted)
        };
        let pitch_cell = mono_cell(fmt_pitch(row.pitch_semitones, muted), pitch_color, cx);
        let speed_cell = mono_cell(fmt_rate(row.rate_multiplier, muted), speed_color, cx);

        DataRow::new(vec![
            alias_viewer_cell(row, palette, density, cx),
            alias_voice_cell(row, palette, density, cx),
            pitch_cell.into_any_element(),
            speed_cell.into_any_element(),
            self.alias_row_actions(row, palette, density, cx),
        ])
    }

    fn alias_row_actions(
        &self,
        row: &AliasRow,
        palette: &ForgePalette,
        density: Density,
        cx: &Context<Self>,
    ) -> AnyElement {
        let muted = row.blocked;
        let row_key: SharedString = row.id.as_str().to_owned().into();
        let preview_color = if muted {
            palette.text_extreme_faint
        } else {
            palette.success
        };
        let preview_alias = row.id.clone();
        let edit_alias = row.id.clone();
        let delete_alias = row.id.clone();
        let mut preview = div()
            .id((gpui::ElementId::from("va-preview"), row_key.clone()))
            .flex()
            .child(icon(Icon::PlayerPlay, ACTION_GLYPH, preview_color));
        if !muted {
            preview =
                preview
                    .cursor_pointer()
                    .key_activated(palette.text_primary)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.preview(&preview_alias, cx)
                    }));
        }
        let edit = div()
            .id((gpui::ElementId::from("va-edit"), row_key.clone()))
            .flex()
            .cursor_pointer()
            .key_activated(palette.text_primary)
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.open_edit(&edit_alias, window, cx)
            }))
            .child(icon(Icon::Pencil, ACTION_GLYPH, palette.text_faint));
        let delete = div()
            .id((gpui::ElementId::from("va-delete"), row_key.clone()))
            .flex()
            .cursor_pointer()
            .key_activated(palette.text_primary)
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.request_delete(delete_alias.clone(), cx)
            }))
            .child(icon(Icon::Trash, ACTION_GLYPH, palette.text_faint));
        div()
            .w_full()
            .flex()
            .items_center()
            .justify_end()
            .gap(spacing(Spacing::Sm, density))
            .child(preview)
            .child(edit)
            .child(delete)
            .into_any_element()
    }

    fn active_overlay(&self, palette: &ForgePalette, cx: &mut Context<Self>) -> Option<AnyElement> {
        if let Some(form) = self.form.as_ref() {
            Some(form.clone().into_any_element())
        } else {
            self.pending_delete
                .get()
                .map(|id| self.delete_confirm(id, palette, cx))
        }
    }

    fn delete_confirm(
        &self,
        id: &AliasId,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let viewer = self
            .row_by_id(id)
            .map(|a| a.viewer_name.clone())
            .unwrap_or_default();
        let message = tr!("tts_aliases_delete_body", viewer = viewer.as_str());

        let card = confirm_modal(
            tr!("tts_aliases_delete_title"),
            message,
            ConfirmTone::Destructive,
            palette,
        )
        .esc_hint(tr!("widget_confirm_esc_to_cancel"))
        .on_cancel(
            "va-delete-cancel",
            tr!("common_cancel"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_delete(cx)),
        )
        .on_confirm(
            "va-delete-confirm",
            tr!("common_delete"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_delete(cx)),
        );

        let view = cx.entity();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("va-delete-scrim", move |_window, cx| {
                view.update(cx, |this, cx| this.cancel_delete(cx));
            })
            .into_any_element()
    }
}

impl Render for VoiceAliasesView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = cx.palette();
        let density = cx.density();

        let banner = self.strategy_banner(&palette, cx);
        let toolbar = self.toolbar(&palette, density, cx);
        let table = self.table(&palette, density, cx);
        let overlay = self.active_overlay(&palette, cx);

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(palette.base)
            .child(banner)
            .child(toolbar)
            .child(table)
            .children(overlay)
    }
}

fn alias_table_columns() -> Vec<Column> {
    vec![
        column(
            tr!("tts_aliases_col_viewer"),
            ColumnWidth::Flex(VIEWER_GROW),
        ),
        column(tr!("tts_aliases_col_voice"), ColumnWidth::Flex(VOICE_GROW)),
        column(tr!("tts_aliases_col_pitch"), ColumnWidth::Flex(PITCH_GROW)),
        column(tr!("tts_aliases_col_speed"), ColumnWidth::Flex(SPEED_GROW)),
        column(
            tr!("tts_aliases_col_actions"),
            ColumnWidth::Fixed(ACTIONS_W),
        )
        .align_end(),
    ]
}

fn alias_viewer_cell(
    row: &AliasRow,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> AnyElement {
    let muted = row.blocked;
    let name_color = if muted {
        palette.text_muted
    } else {
        palette.text_primary
    };

    let initial = row
        .viewer_name
        .chars()
        .next()
        .unwrap_or('?')
        .to_uppercase()
        .next()
        .unwrap_or('?');
    let avatar_bg = if muted {
        palette.text_extreme_faint
    } else {
        hash_accent(&row.viewer_name, palette)
    };
    let avatar = avatar_tile(initial.to_string(), avatar_bg, palette)
        .size(AVATAR)
        .font(FONT_XXS)
        .mono();
    let mut viewer_inner = div()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xs, density))
        .child(avatar)
        .child(
            div()
                .min_w(px(0.0))
                .truncate()
                .font_family(cx.body_family())
                .font_weight(FontWeight::MEDIUM)
                .text_size(FONT_XS)
                .text_color(name_color)
                .child(row.viewer_name.clone()),
        );
    if let Some(kind) = row.platform.kind() {
        viewer_inner = viewer_inner.child(platform_badge(
            row.platform.label(),
            platform_color(kind, palette),
            palette,
            density,
            cx,
        ));
    }
    if muted {
        viewer_inner = viewer_inner.child(role_badge(
            tr!("tts_aliases_role_blocked"),
            palette.danger,
            palette,
        ));
    }
    viewer_inner.into_any_element()
}

fn alias_voice_cell(
    row: &AliasRow,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> AnyElement {
    if row.blocked {
        div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .child(icon(Icon::VolumeOff, ENGINE_GLYPH, palette.danger))
            .child(
                div()
                    .font_family(cx.mono_family())
                    .text_size(VOICE_FS)
                    .text_color(palette.text_faint)
                    .child(tr!("tts_aliases_never_speak")),
            )
            .into_any_element()
    } else {
        let (glyph, glyph_color) = engine_visual(&row.engine_id, palette);
        div()
            .flex()
            .items_center()
            .gap(spacing(Spacing::Xxs, density))
            .child(icon(glyph, ENGINE_GLYPH, glyph_color))
            .child(
                div()
                    .min_w(px(0.0))
                    .truncate()
                    .font_family(cx.mono_family())
                    .text_size(VOICE_FS)
                    .text_color(palette.text_primary)
                    .child(format!("{} · {}", row.engine_label, row.voice_id)),
            )
            .into_any_element()
    }
}

fn mono_cell(value: String, color: Rgba, cx: &App) -> impl IntoElement {
    div()
        .font_family(cx.mono_family())
        .text_size(META_FS)
        .text_color(color)
        .child(value)
}

fn role_badge(
    label: impl Into<SharedString>,
    color: Rgba,
    palette: &ForgePalette,
) -> impl IntoElement {
    badge(palette.surface_overlay, color, label, true, ROLE_BADGE_FS)
}

fn platform_badge(
    label: String,
    dot: Rgba,
    palette: &ForgePalette,
    density: Density,
    cx: &App,
) -> impl IntoElement {
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(spacing(Spacing::Xxs, density))
        .py(scaled(PLATFORM_BADGE_PAD_V, density))
        .px(spacing(Spacing::Xs, density))
        .rounded(radius(Radius::Pill))
        .bg(palette.surface_overlay)
        .child(status_dot(dot, PLATFORM_DOT))
        .child(
            div()
                .font_family(cx.mono_family())
                .text_size(ROLE_BADGE_FS)
                .text_color(palette.text_muted)
                .child(label),
        )
}

fn engine_visual(engine_id: &str, palette: &ForgePalette) -> (Icon, Rgba) {
    match engine_id {
        forge_tts_core::ENGINE_ID_PIPER => (Icon::Cpu, palette.success),
        forge_tts_core::ENGINE_ID_ESPEAK_NG
        | forge_tts_core::ENGINE_ID_SAPI
        | forge_tts_core::ENGINE_ID_AVSPEECH_STORED_AS_NSSPEECH => {
            (Icon::Terminal, palette.success)
        }
        forge_tts_core::ENGINE_ID_ELEVENLABS => (Icon::Microphone2, palette.theme_brand),
        forge_tts_core::ENGINE_ID_POLLY => (Icon::BrandAws, palette.accent_peach),
        forge_tts_core::ENGINE_ID_AZURE => (Icon::Cloud, palette.info),
        forge_tts_core::ENGINE_ID_OPENAI => (Icon::Bolt, palette.accent_teal),
        _ => (Icon::Cloud, palette.text_muted),
    }
}

fn fmt_pitch_amount(semitones: f32) -> String {
    format!("{semitones:.2}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
}

fn fmt_pitch(value: Option<f32>, blocked: bool) -> String {
    if blocked {
        return "-".to_owned();
    }
    match value {
        Some(p) if p >= 0.0 => format!("+{} st", fmt_pitch_amount(p)),
        Some(p) => format!("{} st", fmt_pitch_amount(p)),
        None => "0 st".to_owned(),
    }
}

fn fmt_rate(value: Option<f32>, blocked: bool) -> String {
    if blocked {
        return "-".to_owned();
    }
    value
        .map(|r| format!("{r:.1}x"))
        .unwrap_or_else(|| "1.0x".to_owned())
}
