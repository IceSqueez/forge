use super::*;

impl DiscordScreenView {
    pub(super) fn render_hero(&self, palette: &ForgePalette, density: Density) -> AnyElement {
        let count = self.data.rows.len();
        let badge_slot = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(scaled(COUNT_GAP, density))
            .mr(spacing(Spacing::Xxs, density))
            .child(
                div()
                    .flex_none()
                    .size(COUNT_TILE)
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(COUNT_TILE_RADIUS)
                    .bg(palette.surface_overlay)
                    .child(icon(Icon::Variable, COUNT_GLYPH, palette.brand)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(COUNT_VALUE_FS)
                            .text_color(palette.text_primary)
                            .child(tr!(
                                "discord_hero_webhooks",
                                count = i64::try_from(count).unwrap_or(i64::MAX)
                            )),
                    )
                    .child(
                        div()
                            .font_family(mono_family())
                            .text_size(COUNT_SUB_FS)
                            .text_color(palette.text_faint)
                            .child(tr!("discord_hero_webhooks_sub")),
                    ),
            );

        let body = div()
            .w_full()
            .flex()
            .items_center()
            .gap(scaled(HERO_GAP, density))
            .child(
                div()
                    .flex_shrink_0()
                    .size(HERO_TILE)
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(HERO_TILE_RADIUS)
                    .bg(palette.surface_overlay)
                    .child(icon(Icon::BrandDiscord, HERO_GLYPH, palette.brand)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .font_family(body_family())
                            .text_size(HERO_TITLE_FS)
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(palette.text_primary)
                            .child(tr!("discord_hero_title")),
                    )
                    .child(
                        div()
                            .mt(HERO_BLURB_MT)
                            .font_family(body_family())
                            .text_size(FONT_XS)
                            .text_color(palette.text_muted)
                            .child(tr!("discord_hero_blurb")),
                    ),
            )
            .child(badge_slot);

        div()
            .w_full()
            .mb(scaled(HERO_MARGIN_B, density))
            .child(
                card(body, palette)
                    .padding_xy(scaled(HERO_PAD_V, density), scaled(HERO_PAD_H, density))
                    .full_width(),
            )
            .into_any_element()
    }

    pub(super) fn render_stats(&self, palette: &ForgePalette, density: Density) -> AnyElement {
        let health = self.data.health;

        let (latency_value, latency_hint) = match health.latency_p50_ms {
            Some(ms) => (
                tr!(
                    "discord_stat_latency_value",
                    ms = i64::try_from(ms).unwrap_or(i64::MAX)
                ),
                tr!(
                    "discord_stat_latency_hint",
                    count = i64::try_from(health.latency_samples).unwrap_or(i64::MAX)
                ),
            ),
            None => (NO_VALUE.to_owned(), tr!("discord_stat_no_sends")),
        };

        let (budget_value, budget_hint, budget_ink) = if health.rate_limit_total == 0 {
            (
                NO_VALUE.to_owned(),
                tr!("discord_stat_budget_unknown"),
                palette.text_primary,
            )
        } else {
            let exhausted = health.rate_limit_used >= health.rate_limit_total;
            (
                tr!(
                    "discord_stat_budget_value",
                    used = i64::try_from(health.rate_limit_used).unwrap_or(i64::MAX),
                    total = i64::try_from(health.rate_limit_total).unwrap_or(i64::MAX)
                ),
                tr!("discord_stat_budget_hint"),
                if exhausted {
                    palette.warning
                } else {
                    palette.text_primary
                },
            )
        };

        let (send_value, send_ink) = match health.last_send_ok {
            Some(true) => (tr!("discord_stat_send_ok"), palette.success),
            Some(false) => (tr!("discord_stat_send_failed"), palette.danger),
            None => (NO_VALUE.to_owned(), palette.text_primary),
        };
        let send_hint = match self.last_send_at() {
            Some(at) => fmt_display_relative(Some(at)),
            None => tr!("discord_stat_send_none"),
        };

        let errors = health.errors_last_hour;
        let errors_ink = if errors == 0 {
            palette.success
        } else {
            palette.danger
        };

        div()
            .w_full()
            .flex()
            .items_stretch()
            .gap(spacing(Spacing::Sm, density))
            .mb(scaled(STAT_MARGIN_B, density))
            .child(stat_card(
                tr!("discord_stat_latency"),
                latency_value,
                palette.text_primary,
                latency_hint,
                palette.text_faint,
                palette,
                density,
            ))
            .child(stat_card(
                tr!("discord_stat_budget"),
                budget_value,
                budget_ink,
                budget_hint,
                palette.text_faint,
                palette,
                density,
            ))
            .child(stat_card(
                tr!("discord_stat_send"),
                send_value,
                send_ink,
                send_hint,
                palette.text_faint,
                palette,
                density,
            ))
            .child(stat_card(
                tr!("discord_stat_errors"),
                errors.to_string(),
                errors_ink,
                tr!("discord_stat_errors_hint"),
                palette.text_faint,
                palette,
                density,
            ))
            .into_any_element()
    }

    pub(super) fn render_bindings(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let hint = div()
            .font_family(mono_family())
            .text_size(SECTION_HINT_FS)
            .text_color(palette.text_faint)
            .child(tr!(
                "discord_section_bindings_count",
                count = i64::try_from(self.data.rows.len()).unwrap_or(i64::MAX)
            ))
            .into_any_element();

        let mut list = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density));
        if self.data.rows.is_empty() {
            let notice = load_notice(
                &self.data.load,
                LoadNotice {
                    id: "discord-webhooks-load",
                    loading: tr!("discord_bindings_loading"),
                    width: NoticeWidth::Pane,
                },
                cx.listener(|this, _: &ClickEvent, _, cx| this.retry_webhooks_load(cx)),
                palette,
                density,
            );
            list = list.child(notice.unwrap_or_else(|| {
                empty_row(tr!("discord_bindings_empty"), palette, density).into_any_element()
            }));
        }
        for (index, row) in self.data.rows.iter().enumerate() {
            list = list.child(self.render_binding(index, row, palette, cx));
        }
        list = list.child(self.render_add_bar(palette, cx));

        div()
            .w_full()
            .flex()
            .flex_col()
            .child(
                SectionLabel::new(tr!("discord_section_bindings"), palette)
                    .row()
                    .trailing(Some(hint)),
            )
            .child(list)
            .into_any_element()
    }

    pub(super) fn render_binding(
        &self,
        index: usize,
        row: &WebhookRow,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let density = cx.density();
        let summary = Self::summary_of(row);
        let summary_ink = if row.linked_actions.is_empty() {
            palette.text_faint
        } else {
            palette.text_muted
        };

        let body = div()
            .w_full()
            .flex()
            .items_center()
            .gap(scaled(ROW_GAP, density))
            .child(icon(Icon::Variable, ROW_GLYPH, row_accent(index, palette)))
            .child(
                div()
                    .flex_none()
                    .min_w(ROW_NAME_MIN_W)
                    .font_family(body_family())
                    .text_size(ROW_NAME_FS)
                    .text_color(palette.text_primary)
                    .child(row.name.clone()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_family(body_family())
                    .text_size(ROW_SUMMARY_FS)
                    .text_color(summary_ink)
                    .child(summary),
            )
            .child(self.render_row_menu(index, &row.name, palette, cx));

        div()
            .w_full()
            .child(
                card(body, palette)
                    .padding_xy(scaled(ROW_PAD_V, density), scaled(ROW_PAD_H, density))
                    .full_width(),
            )
            .into_any_element()
    }

    pub(super) fn render_row_menu(
        &self,
        index: usize,
        name: &str,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let open = self.row_menu.open.as_deref() == Some(name);
        let view = cx.entity();
        let edit_name = name.to_owned();
        let test_name = name.to_owned();
        let delete_name = name.to_owned();
        let toggle_name = name.to_owned();

        menu_button(Icon::DotsVertical, open, palette)
            .placement(MenuPlacement::BottomRight)
            .open_at(self.row_menu.click_pos)
            .items(vec![
                menu_item(
                    ("discord-menu-edit", index),
                    tr!("discord_menu_edit"),
                    cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.edit_webhook(edit_name.clone(), cx)
                    }),
                )
                .icon(Icon::Edit)
                .into(),
                menu_item(
                    ("discord-menu-test", index),
                    tr!("discord_menu_test"),
                    cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.test_webhook(test_name.clone(), cx)
                    }),
                )
                .disabled(self.data.testing_rows.contains(name))
                .icon(Icon::Send)
                .into(),
                menu_divider(),
                menu_item(
                    ("discord-menu-delete", index),
                    tr!("common_delete"),
                    cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.prompt_delete(delete_name.clone(), cx)
                    }),
                )
                .icon(Icon::Trash)
                .color(palette.danger)
                .into(),
            ])
            .on_toggle(
                ("discord-menu-trigger", index),
                cx.listener(move |this, event: &ClickEvent, _, cx| {
                    this.toggle_menu(&toggle_name, event.position(), cx)
                }),
            )
            .on_dismiss(move |_window, cx| {
                view.update(cx, |this, cx| this.close_menu(cx));
            })
            .into_any_element()
    }

    pub(super) fn render_add_bar(
        &self,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        pad_tile(
            "discord-add-webhook",
            icon(Icon::Plus, ADD_BAR_GLYPH, palette.brand),
            div()
                .flex()
                .items_center()
                .child(tr!("discord_add_binding")),
            palette,
        )
        .bar(palette)
        .title_color(palette.brand)
        .hover_border(palette.brand)
        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.open_add_modal(cx)))
        .into_any_element()
    }

    pub(super) fn render_posts(&self, palette: &ForgePalette, density: Density) -> AnyElement {
        let last = match self.last_send_at() {
            Some(at) => fmt_display_relative(Some(at)),
            None => tr!("discord_posts_never"),
        };
        let hint = div()
            .font_family(mono_family())
            .text_size(SECTION_HINT_FS)
            .text_color(palette.text_faint)
            .child(last)
            .into_any_element();

        let mut list = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(spacing(Spacing::Xs, density));
        if self.data.posts.is_empty() {
            list = list.child(empty_row(tr!("discord_posts_empty"), palette, density));
        }
        for post in &self.data.posts {
            list = list.child(render_post(post, palette, density));
        }

        div()
            .w_full()
            .flex()
            .flex_col()
            .child(
                SectionLabel::new(tr!("discord_section_posts"), palette)
                    .row()
                    .trailing(Some(hint)),
            )
            .child(list)
            .into_any_element()
    }

    pub(super) fn render_footer(&self, palette: &ForgePalette, density: Density) -> AnyElement {
        let (dot, right) = match self.data.health.last_send_ok {
            Some(true) => (palette.success, tr!("discord_footer_healthy")),
            Some(false) => (palette.danger, tr!("discord_footer_failing")),
            None => (palette.text_faint, tr!("discord_footer_idle")),
        };

        div()
            .w_full()
            .mt(FOOTER_MT)
            .flex()
            .items_center()
            .justify_between()
            .py(scaled(FOOTER_PAD_V, density))
            .px(scaled(FOOTER_PAD_H, density))
            .border_t(BORDER_THIN)
            .border_color(palette.surface_overlay)
            .bg(palette.shell)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, density))
                    .child(footer_text(
                        tr!(
                            "discord_footer_webhooks",
                            count = i64::try_from(self.data.rows.len()).unwrap_or(i64::MAX)
                        ),
                        palette,
                    ))
                    .child(footer_text(FOOTER_SEPARATOR.to_owned(), palette))
                    .child(footer_text(
                        tr!(
                            "discord_footer_linked",
                            count = i64::try_from(self.linked_total()).unwrap_or(i64::MAX)
                        ),
                        palette,
                    )),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(spacing(Spacing::Xs, density))
                    .child(status_dot(dot, FOOTER_DOT))
                    .child(footer_text(right, palette)),
            )
            .into_any_element()
    }

    pub(super) fn render_delete_confirm(
        &self,
        prompt: &DeletePrompt,
        palette: &ForgePalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let message = if prompt.linked == 0 {
            tr!("discord_confirm_delete_body")
        } else {
            tr!(
                "discord_confirm_delete_body_linked",
                count = i64::try_from(prompt.linked).unwrap_or(i64::MAX)
            )
        };
        let card = confirm_modal(
            tr!("discord_confirm_delete_title"),
            message,
            ConfirmTone::Destructive,
            palette,
        )
        .item_name(prompt.name.clone())
        .esc_hint(tr!("widget_confirm_esc_to_cancel"))
        .on_cancel(
            "discord-delete-cancel",
            tr!("common_cancel"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_delete(cx)),
        )
        .on_confirm(
            "discord-delete-confirm",
            tr!("common_delete"),
            cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_delete(cx)),
        );

        let weak = cx.entity().downgrade();
        overlay(card, palette)
            .position(OverlayPosition::Center)
            .on_dismiss("discord-delete-dismiss", move |_window, cx| {
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "the view may close before the result arrives"
                )]
                let _ = weak.update(cx, |this, cx| this.cancel_delete(cx));
            })
            .into_any_element()
    }
}

pub(super) fn render_post(
    post: &WebhookPost,
    palette: &ForgePalette,
    density: Density,
) -> AnyElement {
    let (dot, kind_ink) = if post.ok {
        (palette.success, palette.text_muted)
    } else {
        (palette.danger, palette.danger)
    };
    let kind = if post.had_embed {
        tr!("discord_post_kind_embed")
    } else {
        tr!("discord_post_kind_message")
    };

    let body = div()
        .w_full()
        .flex()
        .items_center()
        .gap(scaled(ROW_GAP, density))
        .child(status_dot(dot, ROW_DOT))
        .child(
            div()
                .flex_none()
                .min_w(ROW_NAME_MIN_W)
                .truncate()
                .font_family(mono_family())
                .text_size(ROW_NAME_FS)
                .text_color(palette.text_primary)
                .child(post.webhook_name.clone()),
        )
        .child(
            badge(
                palette.surface_overlay,
                kind_ink,
                kind,
                false,
                KIND_BADGE_FS,
            )
            .flex_none(),
        )
        .child(div().flex_1().min_w_0())
        .child(
            div()
                .flex_none()
                .font_family(mono_family())
                .text_size(POST_TIME_FS)
                .text_color(palette.text_faint)
                .child(fmt_display_relative(Some(post.sent_at))),
        );

    div()
        .w_full()
        .child(
            card(body, palette)
                .padding_xy(scaled(ROW_PAD_V, density), scaled(ROW_PAD_H, density))
                .full_width(),
        )
        .into_any_element()
}

pub(super) fn row_accent(index: usize, palette: &ForgePalette) -> Rgba {
    let wheel = [
        palette.danger,
        palette.brand,
        palette.warning,
        palette.success,
        palette.info,
        palette.accent_peach,
    ];
    wheel[index % wheel.len()]
}

pub(super) fn empty_row(
    message: String,
    palette: &ForgePalette,
    density: Density,
) -> impl IntoElement {
    div()
        .w_full()
        .py(scaled(EMPTY_PAD_V, density))
        .font_family(body_family())
        .text_size(FONT_XS)
        .text_color(palette.text_faint)
        .child(message)
}

pub(super) fn footer_text(text: String, palette: &ForgePalette) -> impl IntoElement {
    div()
        .font_family(mono_family())
        .text_size(FOOTER_FS)
        .text_color(palette.text_faint)
        .child(text)
}
